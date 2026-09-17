//! HIGHER COGNITIVE CORE V1.2 §9 — Learning Moment 领域（精确契约）。
//!
//! Learning Moment 是 Higher 认知闭环里**唯一的学习行为事实入口**：
//!
//! ```text
//! LEARNING EXPERIENCE → LEARNING MOMENTS → EVIDENCE → LEARNER MODEL UPDATE → NEXT ACTION
//! ```
//!
//! 硬规则（任务书 §9 锁定，全部在本文件强制执行）：
//!
//! 1. **profile 归属**：插入前对每一个被引用的 session / learning_item / goal
//!    做 profile 归属校验；**跨档案引用一律拒绝**（不依赖 DB 的 FK 兜底，
//!    因为 FK 只保证「父行存在」，不保证「父行属于同一档案」）。
//! 2. **append-oriented**：本模块**不提供** `update_learning_moment` 通用命令。
//!    历史行是既成事实。唯一允许的演变是「新写一条 moment」。
//! 3. `tutor_observed` 可以产生 attempt / question / confusion / hint / interest 信号，
//!    但**绝不允许**写出 `*_success` 作为权威证据（§10：LLM 文本永远不是 HIGH 证据）。
//! 4. `system_derived` 可以派生确定性事实，但**必须在 `metadata_json` 中保留 provenance**。
//! 5. `metadata_json` 必须在**写入 DB 之前**被解析校验通过（畸形 JSON 直接拒绝）。
//! 6. `unknown` **永不**被编码为 `failure`：证据缺失用「没有这条 moment」表达，
//!    或用 `result = NULL` 表达「发生了行为但没有结论」。
//!
//! 本模块**不引用**任何 LLM / provider / agent 符号。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Learning Moment 类型（序列化为 snake_case；与 DB `moment_type` 文本一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum LearningMomentType {
    RecallAttempt,
    RecallSuccess,
    RecallPartial,
    RecallFailure,
    HintRequested,
    HintUsed,
    ExplanationAttempt,
    ExplanationSuccess,
    PracticeAttempt,
    PracticeSuccess,
    PracticeFailure,
    ErrorDetected,
    ErrorCorrected,
    TransferAttempt,
    TransferSuccess,
    TransferFailure,
    QuestionAsked,
    ConfusionDetected,
    InterestSignal,
    ManualNote,
}

/// 全部 20 个 moment 类型（稳定顺序；用于遍历与导出）。
pub const ALL_MOMENT_TYPES: [LearningMomentType; 20] = [
    LearningMomentType::RecallAttempt,
    LearningMomentType::RecallSuccess,
    LearningMomentType::RecallPartial,
    LearningMomentType::RecallFailure,
    LearningMomentType::HintRequested,
    LearningMomentType::HintUsed,
    LearningMomentType::ExplanationAttempt,
    LearningMomentType::ExplanationSuccess,
    LearningMomentType::PracticeAttempt,
    LearningMomentType::PracticeSuccess,
    LearningMomentType::PracticeFailure,
    LearningMomentType::ErrorDetected,
    LearningMomentType::ErrorCorrected,
    LearningMomentType::TransferAttempt,
    LearningMomentType::TransferSuccess,
    LearningMomentType::TransferFailure,
    LearningMomentType::QuestionAsked,
    LearningMomentType::ConfusionDetected,
    LearningMomentType::InterestSignal,
    LearningMomentType::ManualNote,
];

impl LearningMomentType {
    /// DB 文本（snake_case）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecallAttempt => "recall_attempt",
            Self::RecallSuccess => "recall_success",
            Self::RecallPartial => "recall_partial",
            Self::RecallFailure => "recall_failure",
            Self::HintRequested => "hint_requested",
            Self::HintUsed => "hint_used",
            Self::ExplanationAttempt => "explanation_attempt",
            Self::ExplanationSuccess => "explanation_success",
            Self::PracticeAttempt => "practice_attempt",
            Self::PracticeSuccess => "practice_success",
            Self::PracticeFailure => "practice_failure",
            Self::ErrorDetected => "error_detected",
            Self::ErrorCorrected => "error_corrected",
            Self::TransferAttempt => "transfer_attempt",
            Self::TransferSuccess => "transfer_success",
            Self::TransferFailure => "transfer_failure",
            Self::QuestionAsked => "question_asked",
            Self::ConfusionDetected => "confusion_detected",
            Self::InterestSignal => "interest_signal",
            Self::ManualNote => "manual_note",
        }
    }

    /// 从 DB 文本解析。未知文本 → `None`（调用方决定如何处理；**绝不**默认成 failure）。
    pub fn parse(raw: &str) -> Option<Self> {
        ALL_MOMENT_TYPES.iter().copied().find(|t| t.as_str() == raw)
    }

    /// 成功类 moment（其权威性受 §10 约束）。
    pub fn is_success(self) -> bool {
        matches!(
            self,
            Self::RecallSuccess
                | Self::ExplanationSuccess
                | Self::PracticeSuccess
                | Self::TransferSuccess
        )
    }

    /// 失败类 moment。
    pub fn is_failure(self) -> bool {
        matches!(
            self,
            Self::RecallFailure | Self::PracticeFailure | Self::TransferFailure
        )
    }

    /// 部分类 moment。
    pub fn is_partial(self) -> bool {
        matches!(self, Self::RecallPartial)
    }

    /// 由类型本身确定的结果（`result` 字段缺省时的确定性推导）。
    ///
    /// **只有**这三类有内在结果；其余类型的 `result` 保持 `None`
    /// ——这正是「unknown 不被编码为 failure」的表达方式。
    pub fn intrinsic_result(self) -> Option<&'static str> {
        if self.is_success() {
            Some("success")
        } else if self.is_failure() {
            Some("failure")
        } else if self.is_partial() {
            Some("partial")
        } else {
            None
        }
    }
}

/// Moment 来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum MomentSourceType {
    Session,
    Micro,
    Evaluation,
    UserExplicit,
    TutorObserved,
    SystemDerived,
    Imported,
}

pub const ALL_SOURCE_TYPES: [MomentSourceType; 7] = [
    MomentSourceType::Session,
    MomentSourceType::Micro,
    MomentSourceType::Evaluation,
    MomentSourceType::UserExplicit,
    MomentSourceType::TutorObserved,
    MomentSourceType::SystemDerived,
    MomentSourceType::Imported,
];

impl MomentSourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Micro => "micro",
            Self::Evaluation => "evaluation",
            Self::UserExplicit => "user_explicit",
            Self::TutorObserved => "tutor_observed",
            Self::SystemDerived => "system_derived",
            Self::Imported => "imported",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ALL_SOURCE_TYPES.iter().copied().find(|t| t.as_str() == raw)
    }

    /// §10：tutor（LLM）观察与导入标注在质量阶梯上最多是 LOW / MEDIUM。
    pub fn is_non_authoritative(self) -> bool {
        matches!(self, Self::TutorObserved | Self::Imported)
    }
}

/// 证据质量（§10 阶梯）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceQuality {
    Low,
    Medium,
    High,
}

impl EvidenceQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    /// §10 / §13：只有 medium/high 证据可以推进 FSRS 状态或学习模型状态。
    pub fn is_trusted(self) -> bool {
        matches!(self, Self::Medium | Self::High)
    }
}

/// 用户自报置信度（用于 §11 Calibration 轴；**不是**证据质量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    Low,
    Medium,
    High,
}

impl EvidenceConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// 可序列化 DTO（字段与顺序精确对应任务书 §9）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct LearningMoment {
    pub id: i64,
    pub profile_id: i64,
    pub session_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub moment_type: LearningMomentType,
    pub occurred_at: String,
    pub source_type: MomentSourceType,
    pub source_id: Option<String>,
    pub result: Option<String>,
    pub hint_level: Option<i64>,
    pub confidence: Option<EvidenceConfidence>,
    pub evidence_quality: EvidenceQuality,
    pub metadata_json: serde_json::Value,
    pub created_at: String,
}

/// 写入用的新 moment（无 `id` / `created_at`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct NewLearningMoment {
    pub profile_id: i64,
    pub session_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub moment_type: LearningMomentType,
    pub occurred_at: String,
    pub source_type: MomentSourceType,
    pub source_id: Option<String>,
    pub result: Option<String>,
    pub hint_level: Option<i64>,
    pub confidence: Option<EvidenceConfidence>,
    pub evidence_quality: EvidenceQuality,
    pub metadata_json: serde_json::Value,
}

impl NewLearningMoment {
    /// 便捷构造：最少必填项 + 空 metadata。
    pub fn new(
        profile_id: i64,
        moment_type: LearningMomentType,
        occurred_at: impl Into<String>,
        source_type: MomentSourceType,
        evidence_quality: EvidenceQuality,
    ) -> Self {
        Self {
            profile_id,
            session_id: None,
            learning_item_id: None,
            goal_id: None,
            moment_type,
            occurred_at: occurred_at.into(),
            source_type,
            source_id: None,
            result: None,
            hint_level: None,
            confidence: None,
            evidence_quality,
            metadata_json: serde_json::json!({}),
        }
    }

    /// 便捷构造：绑定学习项。
    pub fn for_item(mut self, learning_item_id: i64) -> Self {
        self.learning_item_id = Some(learning_item_id);
        self
    }

    /// 便捷构造：带 hint level。
    pub fn with_hint(mut self, hint_level: i64) -> Self {
        self.hint_level = Some(hint_level);
        self
    }

    /// 便捷构造：带用户自报置信度（Calibration 轴输入）。
    pub fn with_confidence(mut self, c: EvidenceConfidence) -> Self {
        self.confidence = Some(c);
        self
    }

    /// 便捷构造：带 metadata。
    pub fn with_metadata(mut self, v: serde_json::Value) -> Self {
        self.metadata_json = v;
        self
    }
}

/// 允许出现在 `result` 列的取值。**`unknown` 不在其中**
/// ——「未知」不能用一个值来表达，只能由「没有这条记录」或 `result = NULL` 表达。
pub const ALLOWED_RESULTS: [&str; 3] = ["success", "partial", "failure"];

// ============================ 归属校验 ============================

/// 校验某个被引用的父行存在，且属于指定 profile。
///
/// `table` 只接受本模块内部传入的**字面量**（无外部输入拼进 SQL）。
fn assert_owned(
    conn: &Connection,
    table: &'static str,
    id: i64,
    profile_id: i64,
    label: &str,
) -> Result<(), String> {
    let sql = format!("SELECT profile_id FROM {table} WHERE id = ?1");
    let found: Option<Option<i64>> =
        match conn.query_row(&sql, params![id], |row| row.get::<_, Option<i64>>(0)) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };

    match found {
        None => Err(format!("{label}不存在（id={id}）")),
        // profile_id 为 NULL 表示「无归属」（历史遗留孤儿行）——同样不接受，
        // 因为 §9 要求「归属被检查」，NULL 不构成任何档案的归属。
        Some(None) => Err(format!("{label}没有档案归属（id={id}），拒绝写入")),
        Some(Some(p)) if p != profile_id => Err(format!(
            "跨档案引用被拒绝：{label} id={id} 属于 profile {p}，不是 profile {profile_id}"
        )),
        Some(Some(_)) => Ok(()),
    }
}

/// 校验学习档案本身存在。
fn assert_profile_exists(conn: &Connection, profile_id: i64) -> Result<(), String> {
    let found: Option<i64> = match conn.query_row(
        "SELECT id FROM study_profiles WHERE id = ?1",
        params![profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };
    match found {
        Some(_) => Ok(()),
        None => Err(format!("学习档案不存在（id={profile_id}）")),
    }
}

// ============================ 写校验（纯函数，便于测试） ============================

/// 在触碰 DB 之前完成的全部契约校验。
///
/// 全部为**纯**规则：不读 DB、不依赖时间。这样 `unknown is not failure` /
/// `tutor_observed 不得授权权威成功` / `metadata_json 必须合法` 三条硬规则
/// 都可以被独立单测覆盖。
pub fn validate_new_moment(m: &NewLearningMoment) -> Result<(), String> {
    // (1) occurred_at 必须是明确的时刻
    if m.occurred_at.trim().is_empty() {
        return Err("occurred_at 不能为空".to_string());
    }

    // (2) metadata_json 必须在写 DB 之前解析并校验（畸形 → 直接拒绝）
    if !m.metadata_json.is_object() {
        return Err("metadata_json 必须是 JSON 对象".to_string());
    }

    // (3) result 取值白名单；"unknown" 不是合法结果
    if let Some(r) = &m.result {
        if !ALLOWED_RESULTS.contains(&r.as_str()) {
            return Err(format!(
                "result 取值非法：{r}（只允许 success / partial / failure；unknown 必须用 result=NULL 表达）"
            ));
        }
        // (3b) result 必须与 moment_type 的内在语义一致，禁止互相矛盾
        if let Some(intrinsic) = m.moment_type.intrinsic_result() {
            if r != intrinsic {
                return Err(format!(
                    "result 与 moment_type 矛盾：{} 的内在结果是 {intrinsic}，收到 {r}",
                    m.moment_type.as_str()
                ));
            }
        }
    }

    // (4) hint_level 非负
    if let Some(h) = m.hint_level {
        if h < 0 {
            return Err("hint_level 不能为负".to_string());
        }
    }

    // (5) tutor_observed 不得写出权威成功证据（§9 / §10）
    if m.source_type == MomentSourceType::TutorObserved && m.moment_type.is_success() {
        return Err(format!(
            "tutor_observed 不得写入权威成功证据（moment_type={}）",
            m.moment_type.as_str()
        ));
    }

    // (6) tutor_observed / imported 也不得声称 HIGH 质量（LLM 文本永远不是 HIGH 证据）
    if m.source_type.is_non_authoritative() && m.evidence_quality == EvidenceQuality::High {
        return Err(format!(
            "{} 不得声明 high 证据质量（§10：LLM 文本/导入标注永远不是 HIGH 证据）",
            m.source_type.as_str()
        ));
    }

    // (7) system_derived 必须保留 provenance
    if m.source_type == MomentSourceType::SystemDerived {
        let has_provenance = m
            .metadata_json
            .as_object()
            .map(|o| !o.is_empty() && o.contains_key("provenance"))
            .unwrap_or(false);
        if !has_provenance {
            return Err("system_derived 必须在 metadata_json 中保留 provenance（§9）".to_string());
        }
    }

    Ok(())
}

/// 归一化：把 `result` 填充为 moment_type 的内在结果（若调用方未给出）。
fn normalized_result(m: &NewLearningMoment) -> Option<String> {
    match &m.result {
        Some(r) => Some(r.clone()),
        None => m.moment_type.intrinsic_result().map(|s| s.to_string()),
    }
}

// ============================ 领域 API（唯一写入口） ============================

/// 写入一条 Learning Moment。
///
/// 顺序（任一失败即整体失败，不留半条数据）：
/// 1. 档案存在性；
/// 2. 纯规则校验；
/// 3. 每一个被引用父行的 profile 归属校验；
/// 4. INSERT。
pub fn record_learning_moment(
    conn: &Connection,
    m: NewLearningMoment,
) -> Result<LearningMoment, String> {
    assert_profile_exists(conn, m.profile_id)?;
    validate_new_moment(&m)?;

    if let Some(sid) = m.session_id {
        assert_owned(conn, "study_sessions", sid, m.profile_id, "学习会话")?;
    }
    if let Some(iid) = m.learning_item_id {
        assert_owned(conn, "learning_items", iid, m.profile_id, "学习项")?;
    }
    if let Some(gid) = m.goal_id {
        assert_owned(conn, "goals", gid, m.profile_id, "目标")?;
    }

    let result = normalized_result(&m);
    let metadata_text = m.metadata_json.to_string();

    conn.execute(
        "INSERT INTO learning_moments
            (profile_id, session_id, learning_item_id, goal_id, moment_type, occurred_at,
             source_type, source_id, result, hint_level, confidence, evidence_quality, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            m.profile_id,
            m.session_id,
            m.learning_item_id,
            m.goal_id,
            m.moment_type.as_str(),
            m.occurred_at,
            m.source_type.as_str(),
            m.source_id,
            result,
            m.hint_level,
            m.confidence.map(|c| c.as_str()),
            m.evidence_quality.as_str(),
            metadata_text,
        ],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    get_learning_moment(conn, id)?.ok_or_else(|| "写入后读取失败".to_string())
}

/// 按 id 读取（只读诊断用）。
pub fn get_learning_moment(conn: &Connection, id: i64) -> Result<Option<LearningMoment>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, session_id, learning_item_id, goal_id, moment_type,
                    occurred_at, source_type, source_id, result, hint_level, confidence,
                    evidence_quality, metadata_json, created_at
             FROM learning_moments WHERE id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(row) => Ok(Some(row_to_moment(row)?)),
        None => Ok(None),
    }
}

/// 某个学习项下的 moments（新→旧；`limit` <= 0 时返回空）。
pub fn list_learning_moments_for_item(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    limit: i64,
) -> Result<Vec<LearningMoment>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, session_id, learning_item_id, goal_id, moment_type,
                    occurred_at, source_type, source_id, result, hint_level, confidence,
                    evidence_quality, metadata_json, created_at
             FROM learning_moments
             WHERE profile_id = ?1 AND learning_item_id = ?2
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![profile_id, learning_item_id, limit],
            row_to_moment_raw,
        )
        .map_err(|e| e.to_string())?;
    collect_moments(rows)
}

/// 某档案最近 moments（新→旧）。`learning_item_id IS NULL` 的档案级 moment 也在内。
pub fn list_recent_learning_moments(
    conn: &Connection,
    profile_id: i64,
    limit: i64,
) -> Result<Vec<LearningMoment>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, session_id, learning_item_id, goal_id, moment_type,
                    occurred_at, source_type, source_id, result, hint_level, confidence,
                    evidence_quality, metadata_json, created_at
             FROM learning_moments
             WHERE profile_id = ?1
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, limit], row_to_moment_raw)
        .map_err(|e| e.to_string())?;
    collect_moments(rows)
}

/// 某学习项下的最近 N 条 moments，**按时间升序**返回（投影内部用）。
pub fn list_item_moments_ascending(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    limit_desc_first: i64,
) -> Result<Vec<LearningMoment>, String> {
    let mut v =
        list_learning_moments_for_item(conn, profile_id, learning_item_id, limit_desc_first)?;
    v.reverse();
    Ok(v)
}

/// 统计某档案在某时间窗内的 moments 数量。
pub fn count_moments_since(
    conn: &Connection,
    profile_id: i64,
    since_utc: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1 AND occurred_at >= ?2",
        params![profile_id, since_utc],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

/// 按类型统计（Progress 页 Quality 轴：recall success/partial/failure、hint usage）。
pub fn count_moments_by_type_since(
    conn: &Connection,
    profile_id: i64,
    since_utc: &str,
) -> Result<Vec<(LearningMomentType, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT moment_type, COUNT(*) FROM learning_moments
             WHERE profile_id = ?1 AND occurred_at >= ?2
             GROUP BY moment_type ORDER BY moment_type ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, since_utc], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        let (raw, n) = r.map_err(|e| e.to_string())?;
        if let Some(t) = LearningMomentType::parse(&raw) {
            out.push((t, n));
        }
        // 未知文本静默跳过：绝不猜测其语义（fail-safe，不伪造）。
    }
    Ok(out)
}

/// 出现过的**本地日期**数量（Progress 的 active days；Fluency 轴要求 >= 2 个本地日期）。
///
/// `occurred_at` 是 UTC 文本，`offset_hours` 为本地时区偏移（本项目默认 +8）。
pub fn count_distinct_local_dates_since(
    conn: &Connection,
    profile_id: i64,
    since_utc: &str,
    offset_hours: i64,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(DISTINCT date(datetime(occurred_at), ?3 || ' hours'))
         FROM learning_moments WHERE profile_id = ?1 AND occurred_at >= ?2",
        params![profile_id, since_utc, format!("{offset_hours:+}")],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

// ============================ 行映射 ============================

fn row_to_moment(row: &rusqlite::Row<'_>) -> Result<LearningMoment, String> {
    let moment_type_raw: String = row.get(5).map_err(|e| e.to_string())?;
    let source_type_raw: String = row.get(7).map_err(|e| e.to_string())?;
    let quality_raw: String = row.get(12).map_err(|e| e.to_string())?;
    let confidence_raw: Option<String> = row.get(11).map_err(|e| e.to_string())?;
    let metadata_raw: String = row.get(13).map_err(|e| e.to_string())?;

    let moment_type = LearningMomentType::parse(&moment_type_raw)
        .ok_or_else(|| format!("未知 moment_type：{moment_type_raw}"))?;
    let source_type = MomentSourceType::parse(&source_type_raw)
        .ok_or_else(|| format!("未知 source_type：{source_type_raw}"))?;
    let evidence_quality = EvidenceQuality::parse(&quality_raw)
        .ok_or_else(|| format!("未知 evidence_quality：{quality_raw}"))?;
    let confidence = match confidence_raw {
        Some(ref c) => {
            Some(EvidenceConfidence::parse(c).ok_or_else(|| format!("未知 confidence：{c}"))?)
        }
        None => None,
    };

    // DB 里的行都是本模块写入的合法 JSON；解析失败按空对象处理，
    // 绝不因为展示层问题丢掉证据本身。
    let metadata_json: serde_json::Value =
        serde_json::from_str(&metadata_raw).unwrap_or_else(|_| serde_json::json!({}));

    Ok(LearningMoment {
        id: row.get(0).map_err(|e| e.to_string())?,
        profile_id: row.get(1).map_err(|e| e.to_string())?,
        session_id: row.get(2).map_err(|e| e.to_string())?,
        learning_item_id: row.get(3).map_err(|e| e.to_string())?,
        goal_id: row.get(4).map_err(|e| e.to_string())?,
        moment_type,
        occurred_at: row.get(6).map_err(|e| e.to_string())?,
        source_type,
        source_id: row.get(8).map_err(|e| e.to_string())?,
        result: row.get(9).map_err(|e| e.to_string())?,
        hint_level: row.get(10).map_err(|e| e.to_string())?,
        confidence,
        evidence_quality,
        metadata_json,
        created_at: row.get(14).map_err(|e| e.to_string())?,
    })
}

fn row_to_moment_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningMoment> {
    row_to_moment(row).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })
}

fn collect_moments<F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<LearningMoment>, String>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<LearningMoment>,
{
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}
