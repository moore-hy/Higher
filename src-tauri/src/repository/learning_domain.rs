//! Learning Domain 仓储与解析链（REAL LEARNING ENGINE V1 · §6）。
//!
//! # 职责
//!
//! 1. 读写 `learning_items.domain` / `goals.domain`（v040 新增的可空列）；
//! 2. 实现 §6 锁定的**领域解析顺序**。
//!
//! # 为什么单独一个模块，而不是改 `Goal` / `LearningItem` 结构体
//!
//! 两个既有仓储都用**显式列清单**投影（`GOAL_COLS` 与 learning_item 的五处
//! 内联 SELECT）。把 `domain` 塞进结构体意味着同时改动这些列清单与行映射器，
//! 影响所有既有读路径 —— 收益为零，回归风险不为零。
//! 因此领域走独立访问器：**领域真相仍然只在 v040 的两个列里**，
//! 不存在第二份领域状态，只是读取入口更窄。
//!
//! # §6 解析顺序（锁定）
//!
//! ```text
//! 1 ActiveLearningIntent.domain
//! 2 learning_item.domain
//! 3 goal.domain
//! 4 imported document source domain   ← PACK B / W5，本包不实现
//! 5 Generic
//! ```
//!
//! 第 4 步在 PACK A 中**显式缺席**：`document_sources` 表属于 v042（PACK B）。
//! 这里不伪造一个空实现，而是把它作为 [`DomainResolutionSource`] 的一个
//! **保留变体**列出来 —— 这样 PACK B 接线时只需替换一处，且审计时一眼可见
//! 「该步骤在 PACK A 尚未生效」。
//!
//! # 第 1 步的边界（本实现的解释，已记入账本）
//!
//! 当前意图的 `domain` 只在**它确实指向本次解析的学习项**时生效，即：
//!
//! ```text
//! intent.learning_item_id IS NULL            → 该意图面向整个档案，适用
//! intent.learning_item_id == 解析目标         → 直接适用
//! intent.learning_item_id == 另一个学习项      → **不适用**，继续向下解析
//! ```
//!
//! 理由：意图指向 B 时把 A 的领域染成 B 的领域，会让领域变成跨项污染源，
//! 违背 §50「一个用户动作 ≠ 两个学习事实」的同族纪律。

use rusqlite::{params, Connection};

use crate::cognitive::learning_domain::{LearningDomain, FALLBACK_DOMAIN};
use crate::repository::active_learning_intent::ActiveLearningIntentRepository;

/// 领域解析的**来源**，使 §6 的顺序可被测试与审计，而不是只能看到最终值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainResolutionSource {
    ActiveLearningIntent,
    LearningItem,
    Goal,
    /// §6 第 4 步：由导入文档来源提供。**PACK A 不产生此值**（表属于 v042 / PACK B）。
    ImportedDocumentSource,
    /// §6 第 5 步：解析链走到底仍无显式领域。
    Fallback,
}

impl DomainResolutionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ActiveLearningIntent => "active_learning_intent",
            Self::LearningItem => "learning_item",
            Self::Goal => "goal",
            Self::ImportedDocumentSource => "imported_document_source",
            Self::Fallback => "fallback",
        }
    }
}

/// 解析结果：值 + 来源。只返回 `LearningDomain` 会丢掉「这个值有多可信」，
/// 而 §6 的整个意义就在于「显式确认优先于兜底」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainResolution {
    pub domain: LearningDomain,
    pub source: DomainResolutionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainErrorCode {
    ProfileNotFound,
    LearningItemNotInProfile,
    GoalNotInProfile,
    InvalidDomain,
    StoredDomainUnrecognized,
    Db,
}

impl DomainErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::LearningItemNotInProfile => "LEARNING_ITEM_NOT_IN_PROFILE",
            Self::GoalNotInProfile => "GOAL_NOT_IN_PROFILE",
            Self::InvalidDomain => "INVALID_DOMAIN",
            Self::StoredDomainUnrecognized => "STORED_DOMAIN_UNRECOGNIZED",
            Self::Db => "DB_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError {
    pub code: DomainErrorCode,
    pub message: String,
}

impl DomainError {
    pub fn new(code: DomainErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn db(err: rusqlite::Error) -> Self {
        Self::new(DomainErrorCode::Db, err.to_string())
    }
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for DomainError {}

/// 把数据库里的字符串还原为领域。非法值不静默降级为 `Generic` ——
/// 那会把「数据损坏」伪装成「未确认」，两者是完全不同的处理路径。
fn parse_stored(raw: &str) -> Result<LearningDomain, DomainError> {
    LearningDomain::parse(raw).ok_or_else(|| {
        DomainError::new(
            DomainErrorCode::StoredDomainUnrecognized,
            format!("数据库中的 domain 值不在 §6 词表内：{raw}"),
        )
    })
}

fn verify_profile_exists(conn: &Connection, profile_id: i64) -> Result<(), DomainError> {
    let found: Option<i64> = match conn.query_row(
        "SELECT id FROM study_profiles WHERE id = ?1",
        params![profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(DomainError::db(e)),
    };
    match found {
        Some(_) => Ok(()),
        None => Err(DomainError::new(
            DomainErrorCode::ProfileNotFound,
            format!("学习档案不存在（id={profile_id}）"),
        )),
    }
}

// ============================ 读 ============================

/// 读取学习项的领域（`None` = 尚未显式确认，**不是** `generic`）。
pub fn get_learning_item_domain(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<Option<LearningDomain>, DomainError> {
    let row: Option<Option<String>> = match conn.query_row(
        "SELECT domain FROM learning_items WHERE id = ?1 AND profile_id = ?2",
        params![learning_item_id, profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(DomainError::db(e)),
    };
    match row {
        None => Err(DomainError::new(
            DomainErrorCode::LearningItemNotInProfile,
            format!("学习项不存在或不属于该档案（item={learning_item_id}, profile={profile_id}）"),
        )),
        Some(None) => Ok(None),
        Some(Some(raw)) => parse_stored(&raw).map(Some),
    }
}

/// 读取目标的领域。
pub fn get_goal_domain(
    conn: &Connection,
    profile_id: i64,
    goal_id: i64,
) -> Result<Option<LearningDomain>, DomainError> {
    let row: Option<Option<String>> = match conn.query_row(
        "SELECT domain FROM goals WHERE id = ?1 AND profile_id = ?2",
        params![goal_id, profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(DomainError::db(e)),
    };
    match row {
        None => Err(DomainError::new(
            DomainErrorCode::GoalNotInProfile,
            format!("目标不存在或不属于该档案（goal={goal_id}, profile={profile_id}）"),
        )),
        Some(None) => Ok(None),
        Some(Some(raw)) => parse_stored(&raw).map(Some),
    }
}

// ============================ 写 ============================

/// 显式确认学习项的领域。`None` 表示**撤销确认**，回到「尚未知道」。
///
/// 本函数**不做任何推测**：调用方给什么就写什么，不给就写 `NULL`（§6）。
pub fn set_learning_item_domain(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    domain: Option<LearningDomain>,
) -> Result<(), DomainError> {
    verify_profile_exists(conn, profile_id)?;
    let affected = conn
        .execute(
            "UPDATE learning_items SET domain = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND profile_id = ?3",
            params![domain.map(|d| d.as_str()), learning_item_id, profile_id],
        )
        .map_err(DomainError::db)?;
    if affected == 0 {
        return Err(DomainError::new(
            DomainErrorCode::LearningItemNotInProfile,
            format!("学习项不存在或不属于该档案（item={learning_item_id}, profile={profile_id}）"),
        ));
    }
    Ok(())
}

/// 显式确认目标的领域。`NULL` 的语义与 [`set_learning_item_domain`] 相同。
pub fn set_goal_domain(
    conn: &Connection,
    profile_id: i64,
    goal_id: i64,
    domain: Option<LearningDomain>,
) -> Result<(), DomainError> {
    verify_profile_exists(conn, profile_id)?;
    let affected = conn
        .execute(
            "UPDATE goals SET domain = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND profile_id = ?3",
            params![domain.map(|d| d.as_str()), goal_id, profile_id],
        )
        .map_err(DomainError::db)?;
    if affected == 0 {
        return Err(DomainError::new(
            DomainErrorCode::GoalNotInProfile,
            format!("目标不存在或不属于该档案（goal={goal_id}, profile={profile_id}）"),
        ));
    }
    Ok(())
}

// ============================ §6 解析链 ============================

/// 按 §6 顺序解析某学习项的领域。
///
/// 解析**永不失败于「没有领域」**：走到底就是 `Generic`（[`FALLBACK_DOMAIN`]）。
/// 只有「档案/学习项不存在」或「数据库损坏」才是错误。
pub fn resolve_domain_for_item(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    now_utc: &str,
) -> Result<DomainResolution, DomainError> {
    // 学习项必须存在且归属正确 —— 这是解析的前提，不是解析的一步。
    let item_row: Option<(Option<String>, Option<i64>)> = match conn.query_row(
        "SELECT domain, goal_id FROM learning_items WHERE id = ?1 AND profile_id = ?2",
        params![learning_item_id, profile_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(DomainError::db(e)),
    };
    let (item_domain_raw, item_goal_id) = item_row.ok_or_else(|| {
        DomainError::new(
            DomainErrorCode::LearningItemNotInProfile,
            format!("学习项不存在或不属于该档案（item={learning_item_id}, profile={profile_id}）"),
        )
    })?;

    // ---- 1) ActiveLearningIntent.domain（仅当意图确实覆盖本学习项）----
    let intent_repo = ActiveLearningIntentRepository::new(conn);
    let intent = intent_repo
        .get_active_intent(profile_id, now_utc)
        .map_err(|e| DomainError::new(DomainErrorCode::Db, e.message))?;
    if let Some(intent) = intent {
        let covers_this_item = match intent.learning_item_id {
            None => true,
            Some(target) => target == learning_item_id,
        };
        if covers_this_item {
            if let Some(raw) = intent.domain.as_deref() {
                return Ok(DomainResolution {
                    domain: parse_stored(raw)?,
                    source: DomainResolutionSource::ActiveLearningIntent,
                });
            }
        }
    }

    // ---- 2) learning_item.domain ----
    if let Some(raw) = item_domain_raw.as_deref() {
        return Ok(DomainResolution {
            domain: parse_stored(raw)?,
            source: DomainResolutionSource::LearningItem,
        });
    }

    // ---- 3) goal.domain ----
    if let Some(goal_id) = item_goal_id {
        let goal_domain: Option<Option<String>> = match conn.query_row(
            "SELECT domain FROM goals WHERE id = ?1 AND profile_id = ?2",
            params![goal_id, profile_id],
            |r| r.get(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(DomainError::db(e)),
        };
        if let Some(Some(raw)) = goal_domain {
            return Ok(DomainResolution {
                domain: parse_stored(&raw)?,
                source: DomainResolutionSource::Goal,
            });
        }
    }

    // ---- 4) imported document source domain ----
    // PACK A / W2 不实现：`document_sources` 属于 v042（PACK B / W5）。
    // 这里刻意**不写占位查询**，让该步骤在 PACK A 的缺席是显式且可审计的。

    // ---- 5) Generic ----
    Ok(DomainResolution {
        domain: FALLBACK_DOMAIN,
        source: DomainResolutionSource::Fallback,
    })
}
