//! Active Learning Intent 仓储（REAL LEARNING ENGINE V1 · §4 / §5 锁定契约）。
//!
//! 职责边界：
//! - 本仓储只负责 `active_learning_intent` 的**读写与归属校验**，不做任何决策；
//! - 它**不是**第二套推荐引擎，也**不**决定「今天学什么」——那是 Decision Engine 的职责；
//! - 所有写路径强制 profile 归属校验：`learning_item_id` / `goal_id` 必须属于同一档案，
//!   否则返回 typed error，绝不接受跨档案引用（与 `cognitive/mod.rs` 的边界纪律同源）。
//!
//! # §4 的语义是「整体替换」，不是「字段合并」
//!
//! `set_active_intent` 走 `INSERT ... ON CONFLICT(profile_id) DO UPDATE`，
//! `DO UPDATE SET` 覆盖**全部**业务字段（含 `created_at`）。因此：
//!
//! ```text
//! 上一份 intent 的 free_text / goal_id / domain 不会"残留"到新 intent 里。
//! ```
//!
//! 调用方必须一次给全，未给的字段就是 `NULL`。这是刻意的：让「当前意图」
//! 永远只有一个可解释的来源，而不是新旧字段的叠加态。
//!
//! # §5 的过期语义
//!
//! - V1 最长 12 小时；调用方可请求更短，**不可**请求更长。
//! - `expires_at <= now` → `get_active_intent` 返回 `None`。
//! - 过期**不产生** failure / skip / 兴趣衰减 / LearningMoment（§5 / §50）。
//!   过期只是「不再是当前意图」，不是一次负面信号。

use rusqlite::{params, Connection};

/// §3 锁定值域：`mode`。
pub const MODES: [&str; 3] = ["autopilot", "copilot", "direct"];

/// §3 锁定值域：`domain`（`NULL` 亦合法，表示尚未判定领域）。
pub const DOMAINS: [&str; 5] = [
    "generic",
    "english",
    "mathematics",
    "computer_science_408",
    "programming",
];

/// §3 锁定值域：`source`。
pub const SOURCES: [&str; 4] = ["command_bar", "today_choice", "journey", "material"];

/// §5 锁定：V1 意图最长生命周期 = 12 小时。
pub const MAX_INTENT_LIFETIME_MINUTES: i64 = 12 * 60;

/// §3 当前意图行（每档案恰好一行，因为 `profile_id` 是主键）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActiveLearningIntent {
    pub profile_id: i64,
    pub mode: String,
    pub domain: Option<String>,
    pub learning_item_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub free_text: Option<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
}

/// `set_active_intent` 的入参。
///
/// `requested_lifetime_minutes = None` → 采用 §5 的 12 小时上限。
#[derive(Debug, Clone)]
pub struct SetActiveIntentParams {
    pub profile_id: i64,
    pub mode: String,
    pub domain: Option<String>,
    pub learning_item_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub free_text: Option<String>,
    pub source: String,
    pub requested_lifetime_minutes: Option<i64>,
}

/// 意图写路径的 typed error code。
///
/// §9 / §14 要求「非法转换 / 复用冲突返回 typed error」；本层先把同一纪律
/// 应用到意图写路径：调用方拿到的是**稳定字符串码**，而不是需要解析的中文错误文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentErrorCode {
    ProfileNotFound,
    LearningItemNotInProfile,
    GoalNotInProfile,
    InvalidMode,
    InvalidDomain,
    InvalidSource,
    IntentLifetimeNotPositive,
    IntentLifetimeExceedsMax,
    Db,
}

impl IntentErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::LearningItemNotInProfile => "LEARNING_ITEM_NOT_IN_PROFILE",
            Self::GoalNotInProfile => "GOAL_NOT_IN_PROFILE",
            Self::InvalidMode => "INVALID_MODE",
            Self::InvalidDomain => "INVALID_DOMAIN",
            Self::InvalidSource => "INVALID_SOURCE",
            Self::IntentLifetimeNotPositive => "INTENT_LIFETIME_NOT_POSITIVE",
            Self::IntentLifetimeExceedsMax => "INTENT_LIFETIME_EXCEEDS_MAX",
            Self::Db => "DB_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentError {
    pub code: IntentErrorCode,
    pub message: String,
}

impl IntentError {
    pub fn new(code: IntentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn db(err: rusqlite::Error) -> Self {
        Self::new(IntentErrorCode::Db, err.to_string())
    }
}

impl std::fmt::Display for IntentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for IntentError {}

/// §5：`expires_at <= now` → 过期。
///
/// 两侧均为 SQLite `datetime('now')` 产出的 `YYYY-MM-DD HH:MM:SS`（UTC），
/// 该格式下字典序与时间序一致。
pub fn is_expired(expires_at: &str, now_utc: &str) -> bool {
    expires_at <= now_utc
}

const INTENT_COLUMNS: &str = "profile_id, mode, domain, learning_item_id, goal_id, free_text,
     source, created_at, updated_at, expires_at";

fn row_to_intent(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActiveLearningIntent> {
    Ok(ActiveLearningIntent {
        profile_id: row.get(0)?,
        mode: row.get(1)?,
        domain: row.get(2)?,
        learning_item_id: row.get(3)?,
        goal_id: row.get(4)?,
        free_text: row.get(5)?,
        source: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        expires_at: row.get(9)?,
    })
}

fn query_intent(
    conn: &Connection,
    profile_id: i64,
) -> Result<Option<ActiveLearningIntent>, IntentError> {
    let sql = format!("SELECT {INTENT_COLUMNS} FROM active_learning_intent WHERE profile_id = ?1");
    match conn.query_row(&sql, params![profile_id], row_to_intent) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(IntentError::db(e)),
    }
}

// ============================ 值域校验 ============================

fn validate_mode(mode: &str) -> Result<(), IntentError> {
    if MODES.contains(&mode) {
        Ok(())
    } else {
        Err(IntentError::new(
            IntentErrorCode::InvalidMode,
            format!("mode 非法：{mode}（允许：{}）", MODES.join(" / ")),
        ))
    }
}

fn validate_domain(domain: Option<&str>) -> Result<(), IntentError> {
    match domain {
        None => Ok(()),
        Some(d) if DOMAINS.contains(&d) => Ok(()),
        Some(d) => Err(IntentError::new(
            IntentErrorCode::InvalidDomain,
            format!("domain 非法：{d}（允许：{}）", DOMAINS.join(" / ")),
        )),
    }
}

fn validate_source(source: &str) -> Result<(), IntentError> {
    if SOURCES.contains(&source) {
        Ok(())
    } else {
        Err(IntentError::new(
            IntentErrorCode::InvalidSource,
            format!("source 非法：{source}（允许：{}）", SOURCES.join(" / ")),
        ))
    }
}

/// §5：解析生命周期。缺省 → 12 小时上限；更短允许；更长拒绝。
fn resolve_lifetime_minutes(requested: Option<i64>) -> Result<i64, IntentError> {
    match requested {
        None => Ok(MAX_INTENT_LIFETIME_MINUTES),
        Some(m) if m <= 0 => Err(IntentError::new(
            IntentErrorCode::IntentLifetimeNotPositive,
            format!("意图生命周期必须为正数分钟（收到 {m}）"),
        )),
        Some(m) if m > MAX_INTENT_LIFETIME_MINUTES => Err(IntentError::new(
            IntentErrorCode::IntentLifetimeExceedsMax,
            format!(
                "意图生命周期不得超过 {} 分钟（12 小时，§5）；收到 {m}",
                MAX_INTENT_LIFETIME_MINUTES
            ),
        )),
        Some(m) => Ok(m),
    }
}

// ============================ 归属校验 ============================

fn verify_profile_exists(conn: &Connection, profile_id: i64) -> Result<(), IntentError> {
    let found: Option<i64> = match conn.query_row(
        "SELECT id FROM study_profiles WHERE id = ?1",
        params![profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(IntentError::db(e)),
    };
    match found {
        Some(_) => Ok(()),
        None => Err(IntentError::new(
            IntentErrorCode::ProfileNotFound,
            format!("学习档案不存在（id={profile_id}）"),
        )),
    }
}

fn verify_learning_item_owner(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<(), IntentError> {
    let owner: Option<i64> = match conn.query_row(
        "SELECT profile_id FROM learning_items WHERE id = ?1",
        params![learning_item_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(IntentError::db(e)),
    };
    match owner {
        Some(o) if o == profile_id => Ok(()),
        _ => Err(IntentError::new(
            IntentErrorCode::LearningItemNotInProfile,
            format!(
                "学习项不存在或不属于该档案（learning_item={learning_item_id}, profile={profile_id}）"
            ),
        )),
    }
}

/// `goals.profile_id` 自 v005 起存在但**可为 NULL**（历史行）。
/// 按 §4「verify goal.profile_id == profile_id」，NULL 视为**不属于**任何档案，
/// 绝不当成通配符放行。
fn verify_goal_owner(conn: &Connection, profile_id: i64, goal_id: i64) -> Result<(), IntentError> {
    let owner: Option<Option<i64>> = match conn.query_row(
        "SELECT profile_id FROM goals WHERE id = ?1",
        params![goal_id],
        |r| r.get::<_, Option<i64>>(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(IntentError::db(e)),
    };
    match owner {
        Some(Some(o)) if o == profile_id => Ok(()),
        _ => Err(IntentError::new(
            IntentErrorCode::GoalNotInProfile,
            format!("目标不存在或不属于该档案（goal={goal_id}, profile={profile_id}）"),
        )),
    }
}

// ============================ §5 DIRECT consumption ============================

/// 清除某档案的当前意图（§5 的「explicit clear」）。
///
/// 本函数**不自行开启事务**，供 W4 `create_training_run` 在同一事务内调用，
/// 以满足 §5「DIRECT intent 的清除必须与 TrainingRun 创建同事务」。
pub fn clear_active_intent_in_tx(conn: &Connection, profile_id: i64) -> Result<bool, IntentError> {
    let affected = conn
        .execute(
            "DELETE FROM active_learning_intent WHERE profile_id = ?1",
            params![profile_id],
        )
        .map_err(IntentError::db)?;
    Ok(affected > 0)
}

pub struct ActiveLearningIntentRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ActiveLearningIntentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// §4 `set_active_intent` —— **原子**整体替换。
    ///
    /// ```text
    /// BEGIN TRANSACTION
    ///   verify profile exists
    ///   if learning_item_id != NULL: verify learning_item.profile_id == profile_id
    ///   if goal_id != NULL:          verify goal.profile_id == profile_id
    ///   INSERT ... ON CONFLICT(profile_id) DO UPDATE SET (全部业务字段)
    /// COMMIT
    /// ```
    ///
    /// 任一步失败 → 事务未提交即析构回滚，**不会**留下半更新的意图。
    pub fn set_active_intent(
        &self,
        p: SetActiveIntentParams,
    ) -> Result<ActiveLearningIntent, IntentError> {
        // 1) 值域校验先行：让调用方拿到 typed error，而不是裸 CHECK 约束失败文本。
        validate_mode(&p.mode)?;
        validate_domain(p.domain.as_deref())?;
        validate_source(&p.source)?;
        let lifetime_minutes = resolve_lifetime_minutes(p.requested_lifetime_minutes)?;

        // 2) BEGIN TRANSACTION
        let tx = self.conn.unchecked_transaction().map_err(IntentError::db)?;

        // 3) + 4) 归属校验
        verify_profile_exists(&tx, p.profile_id)?;
        if let Some(item_id) = p.learning_item_id {
            verify_learning_item_owner(&tx, p.profile_id, item_id)?;
        }
        if let Some(goal_id) = p.goal_id {
            verify_goal_owner(&tx, p.profile_id, goal_id)?;
        }

        // 5) 整体替换式 upsert。created_at / updated_at / expires_at 全部由
        //    同一语句内的 'now' 计算：SQLite 保证一条语句内 'now' 恒定，
        //    因此 expires_at 严格等于「本次 created_at + lifetime」。
        let modifier = format!("+{lifetime_minutes} minutes");
        tx.execute(
            "INSERT INTO active_learning_intent
                 (profile_id, mode, domain, learning_item_id, goal_id, free_text, source,
                  created_at, updated_at, expires_at)
             VALUES
                 (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                  datetime('now'), datetime('now'), datetime('now', ?8))
             ON CONFLICT(profile_id) DO UPDATE SET
                 mode             = excluded.mode,
                 domain           = excluded.domain,
                 learning_item_id = excluded.learning_item_id,
                 goal_id          = excluded.goal_id,
                 free_text        = excluded.free_text,
                 source           = excluded.source,
                 created_at       = excluded.created_at,
                 updated_at       = excluded.updated_at,
                 expires_at       = excluded.expires_at",
            params![
                p.profile_id,
                p.mode,
                p.domain,
                p.learning_item_id,
                p.goal_id,
                p.free_text,
                p.source,
                modifier,
            ],
        )
        .map_err(IntentError::db)?;

        // 6) 回读：返回的是**数据库里真实的那一行**，不是入参回显。
        let stored = query_intent(&tx, p.profile_id)?.ok_or_else(|| {
            IntentError::new(
                IntentErrorCode::Db,
                "意图 upsert 后回读失败（本应恰好一行）".to_string(),
            )
        })?;

        // 7) COMMIT
        tx.commit().map_err(IntentError::db)?;
        Ok(stored)
    }

    /// §5 `get_active_intent` —— 过期即 `None`。
    ///
    /// 过期意图**不得**影响 Decision Engine；这里是最前端的拦截点。
    /// 返回 `None` 表示「当前没有意图」，**不是**一次失败或跳过。
    pub fn get_active_intent(
        &self,
        profile_id: i64,
        now_utc: &str,
    ) -> Result<Option<ActiveLearningIntent>, IntentError> {
        let found = query_intent(self.conn, profile_id)?;
        Ok(found.filter(|intent| !is_expired(&intent.expires_at, now_utc)))
    }

    /// 读取原始行（**含已过期**），仅供诊断/审计使用。
    ///
    /// 业务路径一律使用 [`Self::get_active_intent`]。
    pub fn get_raw_intent(
        &self,
        profile_id: i64,
    ) -> Result<Option<ActiveLearningIntent>, IntentError> {
        query_intent(self.conn, profile_id)
    }

    /// 显式清除当前意图（§5：COPILOT / AUTOPILOT 可保留至过期或显式清除）。
    pub fn clear_active_intent(&self, profile_id: i64) -> Result<bool, IntentError> {
        let tx = self.conn.unchecked_transaction().map_err(IntentError::db)?;
        let cleared = clear_active_intent_in_tx(&tx, profile_id)?;
        tx.commit().map_err(IntentError::db)?;
        Ok(cleared)
    }
}
