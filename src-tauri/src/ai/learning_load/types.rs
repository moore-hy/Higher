//! DEV-0077.4-A · Learning Load Evidence Layer — 类型定义（§七-§十/§十九/§二五/
//! §二六/§三十/§三五/§三七/§三八/§四九/§五九）。
//!
//! 最高原则（§二）：Evidence ≠ Judgment ≠ Planning ≠ Mutation。
//! 本模块全部结构为**运行时计算**的只读证据快照（§四：NO MIGRATION，
//! 禁止新增任何 learning_load/pace/difficulty 持久表）。

// =============== §七：LearningLoadEvidence（顶层） ===============

/// 一次 Evidence Build 的完整快照（纯读取产物，禁止回写）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LearningLoadEvidence {
    pub profile_id: i64,
    /// 构建时刻（本地日期 YYYY-MM-DD + UTC 时间，仅溯源用）。
    pub generated_at: String,
    /// §八：统计窗口（7/14/30 天；只是统计窗口，不是 week goal）。
    pub windows: EvidenceWindows,
    /// §九：每个 LearningItem 一条 Unit Evidence。
    pub units: Vec<LearningUnitEvidence>,
    /// §二十五：Subject（顶层知识节点）级 Pace。
    pub subject_pace: Vec<SubjectPaceEvidence>,
    /// §二十三：Global Pace（全 profile 样本池）。
    pub global_pace: PaceEvidence,
    /// §三十四/§三十五：容量证据（stated 与 observed 严格分离，绝不合并）。
    pub capacity: CapacityEvidence,
    /// §三十七：数据量总览。
    pub data_summary: LearningEvidenceSummary,
    /// §四十九：关联冲突与数据质量计数。
    pub conflicts: EvidenceConflictSummary,
    /// §三十二/§三十三：Mastery 证据可用性。
    pub mastery: MasteryEvidence,
    /// §三十八：Profile 级整体证据质量（enum + 事实 reasons）。
    pub evidence_quality: QualityAssessment,
}

// =============== §八：Evidence Windows ===============

#[derive(Debug, Clone, serde::Serialize)]
pub struct EvidenceWindows {
    pub short_days: i64,  // 7
    pub medium_days: i64, // 14
    pub long_days: i64,   // 30
}

impl Default for EvidenceWindows {
    fn default() -> Self {
        Self { short_days: 7, medium_days: 14, long_days: 30 }
    }
}

// =============== §十：EvidenceRef（证据溯源） ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceType {
    Task,
    StudySession,
    Evaluation,
    Feedback,
    LearningItem,
    Mastery,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EvidenceRef {
    pub source_type: EvidenceSourceType,
    pub entity_id: i64,
    pub occurred_at: Option<String>,
    pub summary: String,
}

// =============== §五十九：EvidenceConfidence ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    Unknown,
    Low,
    Medium,
    High,
}

impl EvidenceConfidence {
    /// §五十九：按样本数定级（可被 outlier 比例降级）。
    pub fn from_samples(n: usize) -> Self {
        match n {
            0 => Self::Unknown,
            1..=2 => Self::Low,
            3..=4 => Self::Medium,
            _ => Self::High,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

// =============== §十九：PaceEvidence ===============

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PaceEvidence {
    /// §五十六：usable pace sample 数（completed Task + valid estimate +
    /// >=1 completed Session + actual>0；pending 不算）。
    pub sample_count: usize,
    pub estimated_minutes_total: i64,
    pub actual_minutes_total: i64,
    /// §二十：样本级 ratio 的中位数（优先于总量比；超长任务不得支配结果）。
    pub median_ratio: Option<f64>,
    /// 非 outlier 样本 ratio 的算术均值（参考值）。
    pub mean_ratio: Option<f64>,
    /// §二十二：校准输出——0 样本→1.0；<3 样本不信任 unit ratio→1.0；
    /// >=3 → clamp(median, 0.67, 1.75)。本阶段只输出，不改 Planner。
    pub calibrated_ratio: f64,
    pub confidence: PaceConfidence,
    /// §二十一：ratio < 0.25 或 > 4.0 的样本数（统计保护，非业务判断；
    /// 事实保留，仅校准排除）。
    pub outlier_count: usize,
}

/// §五十九：Pace 专属置信度（Unknown/Low/Medium/High + outlier 降级）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaceConfidence {
    #[default]
    Unknown,
    Low,
    Medium,
    High,
}

impl PaceConfidence {
    pub fn from_samples(n: usize) -> Self {
        match EvidenceConfidence::from_samples(n) {
            EvidenceConfidence::Unknown => Self::Unknown,
            EvidenceConfidence::Low => Self::Low,
            EvidenceConfidence::Medium => Self::Medium,
            EvidenceConfidence::High => Self::High,
        }
    }
    /// outlier 占比 > 30% → 降一级（§五十九）。
    pub fn downgrade_by_outliers(self, samples: usize, outliers: usize) -> Self {
        if samples == 0 {
            return self;
        }
        if (outliers as f64 / samples as f64) > 0.3 {
            match self {
                Self::High => Self::Medium,
                Self::Medium => Self::Low,
                Self::Low => Self::Low,
                Self::Unknown => Self::Unknown,
            }
        } else {
            self
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

// =============== §二十五：SubjectPaceEvidence ===============

#[derive(Debug, Clone, serde::Serialize)]
pub struct SubjectPaceEvidence {
    /// 顶层知识节点 id（树无法确定 → None + subject_name="unknown"，§二十四）。
    pub subject_item_id: Option<i64>,
    pub subject_name: String,
    pub pace: PaceEvidence,
    pub unit_count: usize,
}

// =============== §二十六：EvaluationEvidence ===============

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EvaluationEvidence {
    pub count: i64,
    pub latest_outcome: Option<String>,
    pub passed_count: i64,
    pub partial_count: i64,
    pub failed_count: i64,
    /// 已评定（非 unrated）总数。
    pub rated_count: i64,
    /// §二十八：最近一次有效 score/max_score 比值（0..1）。
    pub recent_score_ratio: Option<f64>,
    pub latest_at: Option<String>,
}

// =============== §二十九/§三十：FeedbackEvidenceSummary ===============

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedbackEvidenceItem {
    pub id: i64,
    pub feedback_type: String,
    pub title: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FeedbackEvidenceSummary {
    pub count: i64,
    pub weakness_count: i64,
    pub error_count: i64,
    pub blocker_count: i64,
    pub observation_count: i64,
    /// §三十：最多 10 条（禁止把完整历史塞进 AI Context）。
    pub recent_items: Vec<FeedbackEvidenceItem>,
}

// =============== §三十五：CapacityEvidence ===============

/// 用户自述（stated）与观测（observed）容量**严格分列**，绝不合并成一个字段。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CapacityEvidence {
    /// §三十四：stated = 规划时用户确认的每日可用分钟（最近 Blueprint
    /// structured_json.daily_available_minutes；无 Blueprint → None）。
    pub stated_daily_minutes: Option<i64>,
    /// §三十六：观测容量按 local study day 聚合；calendar = 总分钟/日历天数，
    /// active_day = 总分钟/实际学习天数（两者都重要，不得互相覆盖）。
    pub observed_daily_minutes_7d: Option<i64>,
    pub observed_daily_minutes_14d: Option<i64>,
    pub observed_daily_minutes_30d: Option<i64>,
    pub active_study_days_30d: i64,
    /// §三十六：active-day 口径（30 天窗口）。
    pub active_day_average_minutes_30d: Option<i64>,
}

// =============== §三十二/§三十三：MasteryEvidence ===============

/// Unit 级 mastery 直接读 learning_items.mastery_status（真实值，§三十二：
/// Evaluation 不得变成 Mastery Truth）；此处为 Profile 级 MasteryAssessment
/// 可用性（goal/period 维度的最新可信评估）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MasteryEvidence {
    /// §九十六：稳定 Repository 是否可用（当前 = repository::mastery 存在 → true）。
    pub available: bool,
    /// 最新一条 assessment（scored）摘要；无 → None。
    pub latest_score: Option<i64>,
    pub latest_confidence: Option<String>,
    pub latest_period: Option<String>,
    /// NOT AVAILABLE AS STABLE EVIDENCE 时此字段说明原因（§三十三）。
    pub note: String,
}

// =============== §三十七：LearningEvidenceSummary ===============

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LearningEvidenceSummary {
    pub learning_item_count: usize,
    pub linked_task_count: i64,
    pub unlinked_task_count: i64,
    pub linked_session_count: i64,
    pub unlinked_session_count: i64,
    pub evaluation_count: i64,
    pub feedback_count: i64,
    pub pace_sample_count: usize,
    pub observed_study_minutes_30d: i64,
    /// §五十七：completed Task 无任何 Session（数据质量问题，不进 pace）。
    pub completed_without_session_count: i64,
    /// §五十八：Session 有 Task 但 Task 无 estimate（进 actual，不进 calibration）。
    pub session_without_estimate_count: i64,
}

// =============== §四十九：EvidenceConflictSummary ===============

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EvidenceConflictSummary {
    /// §四十七：Session snapshot learning_item_id ≠ Task.learning_item_id。
    pub session_task_learning_item_conflicts: usize,
    /// 引用指向不存在/他 profile 的 LearningItem。
    pub missing_learning_items: usize,
    /// estimated_minutes 非法（0 或负；CHECK 1..1440 之外的历史脏数据）。
    pub invalid_task_estimates: usize,
    /// duration_seconds 非法（<=0）或异常（>18h，§十五标记不删）。
    pub invalid_session_durations: usize,
}

// =============== §三十八：EvidenceQuality ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceQuality {
    Insufficient,
    Low,
    Medium,
    High,
}

impl EvidenceQuality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Insufficient => "insufficient",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// §三十八：不给虚假的 87.4 分——enum + 事实性 reasons（§四十：禁止人格评价）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct QualityAssessment {
    pub quality: EvidenceQuality,
    pub reasons: Vec<String>,
}

// =============== §九：LearningUnitEvidence ===============

#[derive(Debug, Clone, serde::Serialize)]
pub struct LearningUnitEvidence {
    pub learning_item_id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub goal_id: Option<i64>,
    /// §二十七/§三十二：真实当前值（learning_items.mastery_status）。
    pub mastery_status: String,

    // ---- Task（§十一：仅 learning_item_id != NULL 进入 Unit）----
    pub task_count: i64,
    pub completed_task_count: i64,
    pub pending_task_count: i64,
    pub planned_minutes: i64,

    // ---- Session（§十三/§十四：completed & duration>0；round 到分钟）----
    pub session_count: i64,
    pub actual_minutes: i64,

    // ---- Pace（§十六-§十八：样本级 ratio；effort 与 calibration 分离）----
    pub pace: PaceEvidence,

    // ---- Evaluation（§二十六）----
    pub evaluation: EvaluationEvidence,

    // ---- Feedback（§二十九）----
    pub feedback: FeedbackEvidenceSummary,

    // ---- Quality（§三十八-§三十九）----
    pub evidence_quality: QualityAssessment,
    /// 代表性证据溯源（上限 20 条，防 Context 膨胀）。
    pub evidence_refs: Vec<EvidenceRef>,
}
