//! HIGHER PERSONAL CORE — GOAL MODE + CAPABILITY CONTRACT V1（A2-4 §30–§35）。
//!
//! # 这一包**只**建契约
//!
//! ```text
//! 不做完整 Exam Planner
//! 不做完整 Growth Planner
//! 不做能力分 / 技能百分比持久化
//! ```
//!
//! # §32：Exam 与 Growth 的语义锁定
//!
//! ```text
//! Exam    优化「deadline 前结果」
//!         score / coverage / question weight / mock result / time remaining
//! Growth  优化「长期真实能力」
//!         understand / recall / apply / independent / debug / build / transfer / retain
//! ```
//!
//! 这两个**未来必须走不同 Planner**。本包只冻结词汇与判定边界。
//!
//! # §31：绝不按标题猜
//!
//! ```text
//! 「考研」 → Exam     ← 禁止（那是猜）
//! 没有结构化来源 → Unclassified
//! ```
//!
//! # §33：只有已有 Evidence 才允许投影
//!
//! ```text
//! Recall / Apply / Independent / Transfer / Retain   可由既有 Learner Model 诚实映射
//! Understand / Debug / Build                         恒为 Unknown（本轮没有证据）
//! ```
//!
//! 禁止为了 UI 完整而写 `Debug = 50%` 或 `Build = Beginner`。
//!
//! # §34：不建立第二真相
//!
//! ```text
//! Learner Model / Evidence  →  Capability Projection
//! ```
//!
//! V1 **不**新建 `capability_scores` / `skill_percentages`，也**不**反写
//! Learner Model。

use serde::{Deserialize, Serialize};

use super::person::SourceClass;

// ============================ §31 Goal Mode ============================

/// §31 的正式词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum GoalMode {
    /// 优化「deadline 前结果」。
    Exam,
    /// 优化「长期真实能力」。
    Growth,
    /// 生活类目标。
    Life,
    /// 维持类目标。
    Maintenance,
    /// **没有结构化来源**。这是默认，不是失败。
    Unclassified,
}

impl GoalMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exam => "exam",
            Self::Growth => "growth",
            Self::Life => "life",
            Self::Maintenance => "maintenance",
            Self::Unclassified => "unclassified",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "exam" => Some(Self::Exam),
            "growth" => Some(Self::Growth),
            "life" => Some(Self::Life),
            "maintenance" => Some(Self::Maintenance),
            "unclassified" => Some(Self::Unclassified),
            _ => None,
        }
    }

    /// §32：优化目标。这两个未来必须走**不同** Planner。
    pub fn optimizes(self) -> Option<&'static str> {
        match self {
            Self::Exam => Some("deadline 前的结果"),
            Self::Growth => Some("长期真实能力"),
            Self::Life => Some("生活执行"),
            Self::Maintenance => Some("维持既有水平"),
            Self::Unclassified => None,
        }
    }
}

/// §31 的解析结果（带「为什么」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GoalModeResolution {
    pub mode: GoalMode,
    pub source_class: SourceClass,
    pub reason: String,
    /// 该模式关注什么（§32）。Unclassified → 空。
    pub focuses_on: Vec<String>,
}

impl GoalModeResolution {
    fn unclassified(reason: impl Into<String>) -> Self {
        Self {
            mode: GoalMode::Unclassified,
            source_class: SourceClass::Unknown,
            reason: reason.into(),
            focuses_on: Vec::new(),
        }
    }
}

/// §31：**唯一**的 Goal Mode 判定入口。
///
/// # 它为什么恒为 `Unclassified`
///
/// 当前 `goals` 表**没有**任何 mode / kind 结构化字段。按标题猜（「考研」→ Exam）
/// 正是 §31 明令禁止的事 —— 那是把推断包装成事实。
///
/// 因此本函数只看**结构化来源**（目前没有），拿不到就诚实返回 `Unclassified`。
/// 将来有了结构化字段，在这里加分支 —— 判定逻辑**只此一处**。
pub fn resolve_goal_mode(goal_name: &str, structured_mode: Option<&str>) -> GoalModeResolution {
    match structured_mode.and_then(GoalMode::parse) {
        Some(mode) => GoalModeResolution {
            mode,
            source_class: SourceClass::ConfirmedByUser,
            reason: "目标自带结构化模式字段".to_string(),
            focuses_on: focuses_for(mode).into_iter().map(str::to_string).collect(),
        },
        None => GoalModeResolution::unclassified(format!(
            "没有结构化来源可以判定「{goal_name}」是考试型还是成长型 —— 不按标题猜测"
        )),
    }
}

fn focuses_for(mode: GoalMode) -> Vec<&'static str> {
    match mode {
        GoalMode::Exam => vec![
            "score",
            "coverage",
            "question weight",
            "mock result",
            "time remaining",
        ],
        GoalMode::Growth => vec![
            "understand",
            "recall",
            "apply",
            "independent",
            "debug",
            "build",
            "transfer",
            "retain",
        ],
        GoalMode::Life => vec!["execution", "habit continuity"],
        GoalMode::Maintenance => vec!["retention", "pace"],
        GoalMode::Unclassified => Vec::new(),
    }
}

// ============================ §33 Capability Axis ============================

/// §33 的 8 条轴（冻结词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAxis {
    Understand,
    Recall,
    Apply,
    Independent,
    Debug,
    Build,
    Transfer,
    Retain,
}

impl CapabilityAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Understand => "understand",
            Self::Recall => "recall",
            Self::Apply => "apply",
            Self::Independent => "independent",
            Self::Debug => "debug",
            Self::Build => "build",
            Self::Transfer => "transfer",
            Self::Retain => "retain",
        }
    }
}

/// §33 全部 8 条轴（锁定顺序）。
pub const ALL_CAPABILITY_AXES: [CapabilityAxis; 8] = [
    CapabilityAxis::Understand,
    CapabilityAxis::Recall,
    CapabilityAxis::Apply,
    CapabilityAxis::Independent,
    CapabilityAxis::Debug,
    CapabilityAxis::Build,
    CapabilityAxis::Transfer,
    CapabilityAxis::Retain,
];

/// §33：**只能**由既有 Learner Model 证据诚实映射的轴。
///
/// 其余轴在 V1 恒为 `Unknown` —— 不是「以后再补」，而是**现在没有证据**。
pub const PROJECTABLE_AXES: [CapabilityAxis; 5] = [
    CapabilityAxis::Recall,
    CapabilityAxis::Apply,
    CapabilityAxis::Independent,
    CapabilityAxis::Transfer,
    CapabilityAxis::Retain,
];

/// 这条轴在 V1 是否**有**可投影的既有证据。
///
/// UI 可以用它解释「为什么这一栏是空的」，而不是自己去猜。
pub fn is_projectable(axis: CapabilityAxis) -> bool {
    PROJECTABLE_AXES.contains(&axis)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct CapabilityAxisState {
    pub axis: CapabilityAxis,
    /// 值。`None` = Unknown —— **绝不**用 0 或 "beginner" 顶替。
    pub value: Option<String>,
    pub source_class: SourceClass,
    pub reason: String,
    pub evidence_refs: Vec<String>,
}

/// §33 / §34：Capability 投影（**只读**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct CapabilityView {
    pub learning_item_id: Option<i64>,
    pub axes: Vec<CapabilityAxisState>,
    /// §35：明确告诉 UI 这是投影，不是新真相。
    pub note: String,
}

/// §33 —— 由**既有** Learner Model 状态投影 Capability。
///
/// # 它绝不做什么
///
/// ```text
/// 不新建 capability_scores      不持久化任何能力分
/// 不反写 Learner Model          单向投影
/// 不为 UI 完整而伪造 Debug/Build
/// ```
pub fn project_capability(
    state: &crate::cognitive::learner_model::LearnerItemStateV2,
) -> CapabilityView {
    use crate::cognitive::learner_model::{
        ApplicationState as App, FluencyState as Fl, RecallState as R, TransferState as T,
    };

    let item_id = state.learning_item_id;
    let refs = vec![format!("learning_item:{item_id}")];

    // §33：完全没有证据 → **全部** Unknown。绝不是「平均一下给个中间值」。
    if state.lacks_evidence() {
        return CapabilityView {
            learning_item_id: Some(item_id),
            axes: all_unknown("这个学习项还没有任何学习证据"),
            note: "Capability 是 Learner Model / Evidence 的投影，不是第二份真相。".to_string(),
        };
    }

    let unknown = |axis: CapabilityAxis, reason: &str| CapabilityAxisState {
        axis,
        value: None,
        source_class: SourceClass::Unknown,
        reason: reason.to_string(),
        evidence_refs: Vec::new(),
    };
    let observed = |axis: CapabilityAxis, value: &str, reason: &str| CapabilityAxisState {
        axis,
        value: Some(value.to_string()),
        source_class: SourceClass::Observed,
        reason: reason.to_string(),
        evidence_refs: refs.clone(),
    };

    let axes = vec![
        // Understand：§33 的诚实映射清单里**没有**它 → 恒 Unknown。
        unknown(
            CapabilityAxis::Understand,
            "本轮没有可支撑「理解」的独立证据",
        ),
        observed(
            CapabilityAxis::Recall,
            recall_label(state.recall_state),
            "由客观回忆状态投影",
        ),
        observed(
            CapabilityAxis::Apply,
            application_label(state.application_state),
            "由客观应用状态投影",
        ),
        observed(
            CapabilityAxis::Independent,
            if state.application_state == App::Independent {
                "independent"
            } else {
                "not_yet_independent"
            },
            "由客观应用状态是否为 Independent 投影",
        ),
        // Debug / Build：§33 明确点名 —— 恒 Unknown。
        unknown(CapabilityAxis::Debug, "本轮没有排错证据"),
        unknown(CapabilityAxis::Build, "本轮没有从零构建证据"),
        observed(
            CapabilityAxis::Transfer,
            transfer_label(state.transfer_state),
            "由客观迁移状态投影",
        ),
        observed(
            CapabilityAxis::Retain,
            fluency_label(state.fluency_state),
            "由客观流畅度状态投影",
        ),
    ];

    CapabilityView {
        learning_item_id: Some(item_id),
        axes,
        note: "Capability 是 Learner Model / Evidence 的投影，不是第二份真相。".to_string(),
    }
}

/// §34：没有焦点学习项 / 没有档案时的 Capability。
///
/// 全轴 `Unknown` —— **不是**全 0，也不是 "beginner"。
pub fn empty_capability() -> CapabilityView {
    CapabilityView {
        learning_item_id: None,
        axes: all_unknown("还没有可投影的学习项"),
        note: "Capability 是 Learner Model / Evidence 的投影，不是第二份真相。".to_string(),
    }
}

fn all_unknown(reason: &str) -> Vec<CapabilityAxisState> {
    ALL_CAPABILITY_AXES
        .iter()
        .map(|axis| CapabilityAxisState {
            axis: *axis,
            value: None,
            source_class: SourceClass::Unknown,
            reason: reason.to_string(),
            evidence_refs: Vec::new(),
        })
        .collect()
}

fn recall_label(s: crate::cognitive::learner_model::RecallState) -> &'static str {
    use crate::cognitive::learner_model::RecallState as R;
    match s {
        R::Unknown => "unknown",
        R::Fragile => "fragile",
        R::Prompted => "prompted",
        R::Independent => "independent",
    }
}

fn application_label(s: crate::cognitive::learner_model::ApplicationState) -> &'static str {
    use crate::cognitive::learner_model::ApplicationState as A;
    match s {
        A::Unknown => "unknown",
        A::Guided => "guided",
        A::Independent => "independent",
    }
}

fn transfer_label(s: crate::cognitive::learner_model::TransferState) -> &'static str {
    use crate::cognitive::learner_model::TransferState as T;
    match s {
        T::Unknown => "unknown",
        T::Attempted => "attempted",
        T::Partial => "partial",
        T::Independent => "independent",
    }
}

fn fluency_label(s: crate::cognitive::learner_model::FluencyState) -> &'static str {
    use crate::cognitive::learner_model::FluencyState as F;
    match s {
        F::Unknown => "unknown",
        F::Slow => "slow",
        F::Functional => "functional",
        F::Fluent => "fluent",
    }
}
