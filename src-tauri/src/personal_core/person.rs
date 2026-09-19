//! HIGHER PERSONAL CORE — PERSON STATE V1 / KNOW ME（A2-3 §17–§28）。
//!
//! # 一句话
//!
//! ```text
//! A2-1 / A2-2 让 Higher 知道「什么算真的学会了」
//! A2-3   让 Higher 第一次有「我」—— 而不是「当前 StudyProfile」
//! ```
//!
//! # 它是投影，不是第二真相库（§18）
//!
//! ```text
//! LearningMoment ─┐
//! Goal ───────────┤
//! Task ───────────┼→ Person State Projection（只读，不 INSERT）
//! Session ────────┤
//! Personalization ┘
//! ```
//!
//! 禁止：`CREATE TABLE person_state` / `personal_state` / `life_event`。
//! 事实**留在原系统**，这里只是把它们读出来、按来源类别打上标签。
//!
//! # 来源类别必须透明（§20）
//!
//! ```text
//! ConfirmedByUser  用户明确告诉 Higher（例如他创建了「英语四级」这个目标）
//! Observed         Higher 从真实系统行为看到（例如 learning_moments 里真有记录）
//! Inferred         AI / 规则推断（例如 personalization 摘要）
//! Unknown          没有足够证据
//! ```
//!
//! # 三条不能跨越的边界
//!
//! ```text
//! ① LocalPerson != StudyProfile（§22：一个人有多个档案）
//! ② 档案 A 的学习证据不得污染档案 B 的学习能力状态
//! ③ 软记忆（AI 抽取/推断）永不晋升为 Observed 真相（§21）
//! ```

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

// ============================ 来源类别（§20 锁定词表） ============================

/// Higher 对用户的每一条理解**必须**属于这四类中的一类。
///
/// 它是**分类量**，不是分数 —— 与 `EvidenceAuthority` 同理：
/// 一个数字必然要在某处把「用户说的」和「我看到的」压成可比量，那处就是漏洞。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    /// 用户明确告诉 Higher（创建目标、确认偏好、自报状态）。
    ConfirmedByUser,
    /// Higher 从**真实系统行为**看到（canonical 表里真有这一行）。
    Observed,
    /// AI / 规则推断。**永不**等于 Observed。
    Inferred,
    /// 没有足够证据。**绝不**用推断填补（§25）。
    Unknown,
}

impl SourceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmedByUser => "confirmed_by_user",
            Self::Observed => "observed",
            Self::Inferred => "inferred",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "confirmed_by_user" => Some(Self::ConfirmedByUser),
            "observed" => Some(Self::Observed),
            "inferred" => Some(Self::Inferred),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

// ============================ 一条「已知」 ============================

/// §26：重要结论必须能回答「为什么 Higher 这么认为」。
///
/// 因此每一条都带 `reason` + `source_class` + `evidence_refs`。
/// V1 的 UI 不必漂亮，但**数据结构必须保留**这三项 —— 它是以后
/// `Today → Why?` 的基础。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct KnownText {
    pub value: Option<String>,
    pub source_class: SourceClass,
    /// 人话：为什么是这个值 / 为什么没有值。
    pub reason: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct KnownCount {
    pub value: Option<i64>,
    pub source_class: SourceClass,
    pub reason: String,
    pub evidence_refs: Vec<String>,
}

impl KnownText {
    /// 没有足够证据 —— **绝不**用推断填补。
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            source_class: SourceClass::Unknown,
            reason: reason.into(),
            evidence_refs: Vec::new(),
        }
    }
    pub fn observed(
        value: impl Into<String>,
        reason: impl Into<String>,
        refs: Vec<String>,
    ) -> Self {
        Self {
            value: Some(value.into()),
            source_class: SourceClass::Observed,
            reason: reason.into(),
            evidence_refs: refs,
        }
    }
    pub fn confirmed(
        value: impl Into<String>,
        reason: impl Into<String>,
        refs: Vec<String>,
    ) -> Self {
        Self {
            value: Some(value.into()),
            source_class: SourceClass::ConfirmedByUser,
            reason: reason.into(),
            evidence_refs: refs,
        }
    }
    pub fn inferred(
        value: impl Into<String>,
        reason: impl Into<String>,
        refs: Vec<String>,
    ) -> Self {
        Self {
            value: Some(value.into()),
            source_class: SourceClass::Inferred,
            reason: reason.into(),
            evidence_refs: refs,
        }
    }
}

impl KnownCount {
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            source_class: SourceClass::Unknown,
            reason: reason.into(),
            evidence_refs: Vec::new(),
        }
    }
    pub fn observed(value: i64, reason: impl Into<String>, refs: Vec<String>) -> Self {
        Self {
            value: Some(value),
            source_class: SourceClass::Observed,
            reason: reason.into(),
            evidence_refs: refs,
        }
    }
}

// ============================ 域视图 ============================

/// §19 的 `learning`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct LearningView {
    /// 当前焦点（最近真的产生过学习事实的学习项）。
    pub current_focus: KnownText,
    /// 客观回忆状态 —— **只有**权威准入的证据才能给出（A2-1）。
    pub recall_state: KnownText,
    pub application_state: KnownText,
    /// 权威已验证证据条数（读侧闸门放行的那些）。
    pub verified_evidence_count: KnownCount,
    pub last_activity_at: KnownText,
}

/// §19 的 `execution`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ExecutionView {
    pub tasks_done_today: KnownCount,
    pub open_tasks: KnownCount,
}

/// §19 的 `goals`。目标是**用户创建的** → `ConfirmedByUser`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct GoalView {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub source_class: SourceClass,
    pub reason: String,
    /// A2-4 §31：Goal Mode。**绝不**按标题猜 —— 没有结构化来源就是 `Unclassified`。
    pub goal_mode: super::capability::GoalModeResolution,
}

/// §19 的 `time`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct TimeView {
    pub minutes_today: KnownCount,
}

/// §21：软记忆**只**能落在 `soft_context`，且一律 `Inferred`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SoftContextView {
    pub summary: KnownText,
    /// 明确告诉 UI：这是软记忆，不是客观真相。
    pub note: String,
}

/// §23 Body V1 —— 这轮**不接**手表 / Health Connect。
///
/// 没有真实数据就是 `Unknown`；用户自报才是 `SelfReported`（映射到
/// `ConfirmedByUser`）。**绝不**把「昨晚没睡好」自动升级成
///「恢复度医学偏低」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct BodyStateV1 {
    pub sleep: KnownText,
    pub energy: KnownText,
    pub stress: KnownText,
    pub mood: KnownText,
    pub recovery: KnownText,
}

/// §22：LocalPerson 层可以看到多个 workspace 的**高层摘要**。
///
/// 刻意**只**放高层信息（名字 + 目标数）。每个工作区的学习能力状态仍由
/// **它自己的**档案证据算出 —— 跨档案可见 ≠ 跨档案污染。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct WorkspaceSummary {
    pub profile_id: i64,
    pub name: String,
    pub goal_count: i64,
}

/// §26：一条明确的「我不知道」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct UnknownItem {
    pub domain: String,
    pub label: String,
    pub reason: String,
}

// ============================ PersonStateSnapshot ============================

/// §19 的最小结构。刻意**不**预建 50 个域。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct PersonStateSnapshot {
    pub as_of: String,
    /// `study_profile` —— 本快照的学习/执行/时间部分是**档案级**的。
    ///
    /// 它**不是** person 级。person 级的信息只有 `workspaces` / `soft_context` / `body`。
    pub scope: String,
    pub profile_id: i64,
    pub profile_name: String,
    /// §22：这个人在这台机器上一共有几个档案（LocalPerson 层事实）。
    pub person_profile_count: i64,

    pub learning: LearningView,
    pub execution: ExecutionView,
    pub goals: Vec<GoalView>,
    pub time: TimeView,
    pub soft_context: SoftContextView,
    pub body: BodyStateV1,
    /// A2-4 §33：Capability 投影（**只读**，不反写 Learner Model，不新建能力分表）。
    pub capability: super::capability::CapabilityView,
    pub workspaces: Vec<WorkspaceSummary>,
    pub unknowns: Vec<UnknownItem>,
}

/// 档案级作用域标记（§22：`LocalPerson != StudyProfile`）。
pub const SCOPE_STUDY_PROFILE: &str = "study_profile";

// ============================ 投影（只读） ============================

/// 一次返回 V1 所需快照（§24：不做 N+1 IPC）。
///
/// # 它不写任何东西
///
/// 全函数只有 `SELECT`。A23-09 在源码层与运行时双重锁定这一点。
pub fn project_person_state(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
) -> Result<PersonStateSnapshot, String> {
    let profile_name: String = conn
        .query_row(
            "SELECT name FROM study_profiles WHERE id = ?1",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("read study profile failed: {e}"))?
        .ok_or_else(|| format!("study profile {profile_id} not found"))?;

    let person_profile_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM study_profiles", [], |r| r.get(0))
        .map_err(|e| format!("count study profiles failed: {e}"))?;

    let goals = project_goals(conn, profile_id)?;
    let (learning, capability) = project_learning(conn, profile_id, now_utc)?;
    let execution = project_execution(conn, profile_id, now_utc)?;
    let time = project_time(conn, profile_id, now_utc)?;
    let soft_context = project_soft_context(conn, profile_id)?;
    let body = project_body();
    let workspaces = project_workspaces(conn)?;
    // 先算 unknowns，再搬走各视图 —— 避免借用已移动的值。
    let unknowns = collect_unknowns(&learning, &body, &soft_context);

    Ok(PersonStateSnapshot {
        as_of: now_utc.to_string(),
        scope: SCOPE_STUDY_PROFILE.to_string(),
        profile_id,
        profile_name,
        person_profile_count,
        learning,
        execution,
        goals,
        time,
        soft_context,
        body,
        capability,
        workspaces,
        unknowns,
    })
}

// ---------------- goals ----------------

fn project_goals(conn: &Connection, profile_id: i64) -> Result<Vec<GoalView>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, status FROM goals
             WHERE profile_id = ?1
             ORDER BY id DESC",
        )
        .map_err(|e| format!("prepare goals failed: {e}"))?;
    let rows = stmt
        .query_map([profile_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("query goals failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, status) = row.map_err(|e| format!("read goal failed: {e}"))?;
        // A2-4 §31：Goal Mode **只**看结构化来源。`goals` 表目前没有这种字段，
        // 因此恒为 Unclassified —— 「考研」不会被猜成 Exam。
        let goal_mode = super::capability::resolve_goal_mode(&name, None);
        out.push(GoalView {
            id,
            name,
            status,
            // 目标是**用户自己建的** —— 这是「用户明确告诉 Higher」的最直接形态。
            source_class: SourceClass::ConfirmedByUser,
            reason: "这个目标是你自己创建的".to_string(),
            goal_mode,
        });
    }
    Ok(out)
}

// ---------------- learning ----------------

/// 学习域：**只有**权威准入的证据才允许给出客观状态（A2-1）。
/// 返回 `(learning, capability)` —— capability 由**同一次** Learner Model 投影得出，
/// 因此不产生第二次查询（§24：不做 N+1）。
fn project_learning(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
) -> Result<(LearningView, super::capability::CapabilityView), String> {
    // 最近真的产生过学习事实的学习项（档案内）。
    let focus: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT li.id, li.name, MAX(m.occurred_at)
             FROM learning_moments m
             JOIN learning_items li ON li.id = m.learning_item_id
             WHERE m.profile_id = ?1 AND m.learning_item_id IS NOT NULL
             GROUP BY li.id
             ORDER BY MAX(m.occurred_at) DESC, li.id DESC
             LIMIT 1",
            [profile_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("read learning focus failed: {e}"))?;

    let last_activity: Option<String> = conn
        .query_row(
            "SELECT MAX(occurred_at) FROM learning_moments WHERE profile_id = ?1",
            [profile_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("read last activity failed: {e}"))?
        .flatten();

    let (focus_view, recall_view, application_view, capability) = match focus {
        Some((item_id, name, _at)) => {
            let refs = vec![format!("learning_item:{item_id}")];
            let (recall, application, state) =
                learner_states_for_item(conn, profile_id, item_id, now_utc)?;
            // A2-4 §33：Capability 是 Learner Model 的**只读投影**。
            let capability = match state {
                Some(s) => super::capability::project_capability(&s),
                None => super::capability::empty_capability(),
            };
            (
                KnownText::observed(name, "这是最近真的产生过学习事实的学习项", refs),
                recall,
                application,
                capability,
            )
        }
        None => (
            KnownText::unknown("这个档案里还没有任何学习事实"),
            KnownText::unknown("没有学习证据，因此不判断回忆状态"),
            KnownText::unknown("没有学习证据，因此不判断应用状态"),
            super::capability::empty_capability(),
        ),
    };

    let last_activity_view = match last_activity {
        Some(at) => KnownText::observed(
            at,
            "最近一次学习事实的发生时间",
            vec!["learning_moments".to_string()],
        ),
        None => KnownText::unknown("没有任何学习事实"),
    };

    let verified = count_verified_evidence(conn, profile_id)?;
    let verified_view = if verified > 0 {
        KnownCount::observed(
            verified,
            "读侧闸门放行的权威已验证证据条数",
            vec!["learning_moments".to_string()],
        )
    } else {
        KnownCount::observed(
            0,
            "目前还没有任何**被真实验证过**的学习证据",
            vec!["learning_moments".to_string()],
        )
    };

    Ok((
        LearningView {
            current_focus: focus_view,
            recall_state: recall_view,
            application_state: application_view,
            verified_evidence_count: verified_view,
            last_activity_at: last_activity_view,
        },
        capability,
    ))
}

fn learner_states_for_item(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    now_utc: &str,
) -> Result<
    (
        KnownText,
        KnownText,
        Option<crate::cognitive::learner_model::LearnerItemStateV2>,
    ),
    String,
> {
    use crate::cognitive::learner_model::{
        project_learner_item_state, FrictionBand, LearnerProjectionInput, MemoryUnitSummary,
    };
    use crate::cognitive::learning_moment::{LearningMoment, MomentSourceType};

    // 只取**本档案本学习项**的 moment —— 跨档案不影响这里（§22）。
    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, learning_item_id, moment_type, occurred_at, source_type,
                    source_id, result, hint_level, evidence_quality, metadata_json, created_at
             FROM learning_moments
             WHERE profile_id = ?1 AND learning_item_id = ?2
             ORDER BY occurred_at DESC, id DESC
             LIMIT 200",
        )
        .map_err(|e| format!("prepare moments failed: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![profile_id, item_id], |r| {
            Ok(LearningMomentLite {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                learning_item_id: r.get(2)?,
                moment_type: r.get(3)?,
                occurred_at: r.get(4)?,
                source_type: r.get(5)?,
                source_id: r.get(6)?,
                result: r.get(7)?,
                hint_level: r.get(8)?,
                evidence_quality: r.get(9)?,
                metadata_json: r.get(10)?,
            })
        })
        .map_err(|e| format!("query moments failed: {e}"))?;

    let mut moments: Vec<LearningMoment> = Vec::new();
    for row in rows {
        let lite = row.map_err(|e| format!("read moment failed: {e}"))?;
        if let Some(m) = lite.into_moment() {
            moments.push(m);
        }
    }
    if moments.is_empty() {
        return Ok((
            KnownText::unknown("没有学习证据，因此不判断回忆状态"),
            KnownText::unknown("没有学习证据，因此不判断应用状态"),
            None,
        ));
    }

    let state = project_learner_item_state(&LearnerProjectionInput {
        profile_id,
        learning_item_id: item_id,
        moments_desc: moments,
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: now_utc.to_string(),
    });

    let refs = vec![format!("learning_item:{item_id}")];
    let recall = KnownText::observed(
        recall_label(state.recall_state),
        "由**权威准入**的学习证据投影出的客观回忆状态",
        refs.clone(),
    );
    let application = KnownText::observed(
        application_label(state.application_state),
        "由**权威准入**的学习证据投影出的客观应用状态",
        refs,
    );
    Ok((recall, application, Some(state)))
}

/// `RecallState` → 人话。刻意**不改** `learner_model`（那是冻结的客观投影）。
fn recall_label(s: crate::cognitive::learner_model::RecallState) -> &'static str {
    use crate::cognitive::learner_model::RecallState as R;
    match s {
        R::Unknown => "还不知道",
        R::Fragile => "还不稳",
        R::Prompted => "需要提示",
        R::Independent => "能独立回忆",
    }
}

fn application_label(s: crate::cognitive::learner_model::ApplicationState) -> &'static str {
    use crate::cognitive::learner_model::ApplicationState as A;
    match s {
        A::Unknown => "还不知道",
        A::Guided => "需要引导",
        A::Independent => "能独立应用",
    }
}

/// 统计读侧闸门放行的权威已验证证据条数。
fn count_verified_evidence(conn: &Connection, profile_id: i64) -> Result<i64, String> {
    use crate::cognitive::learning_moment::{LearningMoment, MomentSourceType};
    use crate::personal_core::adapters::learning::authority_for_learning_moment;

    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, learning_item_id, moment_type, occurred_at, source_type,
                    source_id, result, hint_level, evidence_quality, metadata_json
             FROM learning_moments
             WHERE profile_id = ?1
             ORDER BY id DESC
             LIMIT 500",
        )
        .map_err(|e| format!("prepare moments failed: {e}"))?;
    let rows = stmt
        .query_map([profile_id], |r| {
            Ok(LearningMomentLite {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                learning_item_id: r.get(2)?,
                moment_type: r.get(3)?,
                occurred_at: r.get(4)?,
                source_type: r.get(5)?,
                source_id: r.get(6)?,
                result: r.get(7)?,
                hint_level: r.get(8)?,
                evidence_quality: r.get(9)?,
                metadata_json: r.get(10)?,
            })
        })
        .map_err(|e| format!("query moments failed: {e}"))?;

    let mut count = 0i64;
    for row in rows {
        let lite = row.map_err(|e| format!("read moment failed: {e}"))?;
        if let Some(m) = lite.into_moment() {
            if authority_for_learning_moment(&m).is_verified() {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// 从 `learning_moments` 行重建 `LearningMoment`（解析失败 → 跳过，绝不猜）。
struct LearningMomentLite {
    id: i64,
    profile_id: i64,
    learning_item_id: Option<i64>,
    moment_type: String,
    occurred_at: String,
    source_type: String,
    source_id: Option<String>,
    result: Option<String>,
    hint_level: Option<i64>,
    evidence_quality: String,
    metadata_json: String,
}

impl LearningMomentLite {
    fn into_moment(self) -> Option<crate::cognitive::learning_moment::LearningMoment> {
        use crate::cognitive::learning_moment::{
            EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType,
        };
        let moment_type = LearningMomentType::parse(&self.moment_type)?;
        let source_type = MomentSourceType::parse(&self.source_type)?;
        let evidence_quality = EvidenceQuality::parse(&self.evidence_quality)?;
        let metadata_json: serde_json::Value = serde_json::from_str(&self.metadata_json).ok()?;
        Some(LearningMoment {
            id: self.id,
            profile_id: self.profile_id,
            session_id: None,
            learning_item_id: self.learning_item_id,
            goal_id: None,
            moment_type,
            occurred_at: self.occurred_at.clone(),
            source_type,
            source_id: self.source_id,
            result: self.result,
            hint_level: self.hint_level,
            confidence: None,
            evidence_quality,
            metadata_json,
            created_at: self.occurred_at,
        })
    }
}

// ---------------- execution ----------------

fn project_execution(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
) -> Result<ExecutionView, String> {
    let today = &now_utc[..10.min(now_utc.len())];
    let done: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE profile_id = ?1 AND status = 'done'
               AND archived_at IS NULL",
            [profile_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("count done tasks failed: {e}"))?;
    let open: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE profile_id = ?1 AND status != 'done'
               AND archived_at IS NULL",
            [profile_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("count open tasks failed: {e}"))?;

    Ok(ExecutionView {
        tasks_done_today: KnownCount::observed(
            done,
            format!("这个档案里已完成任务数（截至 {today}）"),
            vec!["tasks".to_string()],
        ),
        open_tasks: KnownCount::observed(open, "未完成任务数", vec!["tasks".to_string()]),
    })
}

// ---------------- time ----------------

fn project_time(conn: &Connection, profile_id: i64, now_utc: &str) -> Result<TimeView, String> {
    let today = &now_utc[..10.min(now_utc.len())];
    let seconds: Option<i64> = conn
        .query_row(
            "SELECT SUM(duration_seconds) FROM study_sessions
             WHERE profile_id = ?1 AND date(started_at) = date(?2)",
            rusqlite::params![profile_id, today],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("sum session seconds failed: {e}"))?
        .flatten();

    let view = match seconds {
        Some(s) => KnownCount::observed(
            s / 60,
            format!("{today} 真实记录的学习时长"),
            vec!["study_sessions".to_string()],
        ),
        None => KnownCount::observed(
            0,
            format!("{today} 还没有任何学习会话记录"),
            vec!["study_sessions".to_string()],
        ),
    };
    Ok(TimeView {
        minutes_today: view,
    })
}

// ---------------- soft context ----------------

/// §21：`PersonalizationProfile` / AI 抽取上下文**只**能成为 `soft_context`，
/// 且一律 `Inferred`。它**永不**晋升为 `Observed`。
fn project_soft_context(conn: &Connection, profile_id: i64) -> Result<SoftContextView, String> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT md_content, structured_json FROM personalization_profiles
             WHERE profile_id = ?1
             ORDER BY version DESC, id DESC
             LIMIT 1",
            [profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("read personalization failed: {e}"))?;

    let summary = match row {
        Some((md, _)) if md.as_deref().unwrap_or_default().trim().len() > 0 => KnownText::inferred(
            truncate(md.as_deref().unwrap_or_default().trim(), 240),
            "来自个性化档案（AI/规则抽取），**不是**客观观测",
            vec!["personalization_profiles".to_string()],
        ),
        _ => KnownText::unknown("还没有个性化档案"),
    };

    Ok(SoftContextView {
        summary,
        note: "这一栏是软记忆：它可能不准确，也永远不会自动变成「我确认的事实」。".to_string(),
    })
}

// ---------------- body ----------------

/// §23：本轮**不接**手表 / Health Connect / 健康数据源。
///
/// 因此 V1 恒为 `Unknown`。这不是「以后再填」，而是**现在真的没有数据来源**。
/// 用户自报走 `ConfirmedByUser`（V1 还没有自报入口，因此这里不会出现）。
fn project_body() -> BodyStateV1 {
    let reason = "没有真实身体数据来源（本轮不接手表 / Health Connect）";
    BodyStateV1 {
        sleep: KnownText::unknown(reason),
        energy: KnownText::unknown(reason),
        stress: KnownText::unknown(reason),
        mood: KnownText::unknown(reason),
        recovery: KnownText::unknown(reason),
    }
}

// ---------------- workspaces ----------------

/// §22：LocalPerson 层的高层摘要。
///
/// 每个档案的目标数由**它自己的**行算出 —— 跨档案可见的是摘要，
/// 不是别人的学习证据。
fn project_workspaces(conn: &Connection) -> Result<Vec<WorkspaceSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name,
                    (SELECT COUNT(*) FROM goals g WHERE g.profile_id = p.id)
             FROM study_profiles p
             ORDER BY p.id",
        )
        .map_err(|e| format!("prepare workspaces failed: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("query workspaces failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (profile_id, name, goal_count) =
            row.map_err(|e| format!("read workspace failed: {e}"))?;
        out.push(WorkspaceSummary {
            profile_id,
            name,
            goal_count,
        });
    }
    Ok(out)
}

// ---------------- unknowns ----------------

/// §25：把「我不知道」显式列出来 —— UI 必须真的显示它们。
fn collect_unknowns(
    learning: &LearningView,
    body: &BodyStateV1,
    soft: &SoftContextView,
) -> Vec<UnknownItem> {
    let mut out = Vec::new();
    for (domain, label, known) in [
        ("learning", "当前焦点", &learning.current_focus),
        ("learning", "回忆状态", &learning.recall_state),
        ("learning", "应用状态", &learning.application_state),
        ("body", "睡眠", &body.sleep),
        ("body", "精力", &body.energy),
        ("body", "压力", &body.stress),
        ("body", "情绪", &body.mood),
        ("body", "恢复", &body.recovery),
        ("soft_context", "个性化摘要", &soft.summary),
    ] {
        if known.source_class == SourceClass::Unknown {
            out.push(UnknownItem {
                domain: domain.to_string(),
                label: label.to_string(),
                reason: known.reason.clone(),
            });
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}
