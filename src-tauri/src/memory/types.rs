//! HIGHER COGNITIVE CORE V1.2 §13 — Memory 类型（**不引用 fsrs**）。
//!
//! 本文件刻意**不** import `fsrs` crate：FSRS 类型只在 `memory/engine.rs` 出现
//! （ME-10 结构性断言）。这里只定义 Higher 自己的领域类型与可以脱离
//! 排程库独立测试的**评分映射规则**。
//!
//! `retrievability` 是缓存展示/决策值，**FSRS 才是排程真相**。

use serde::{Deserialize, Serialize};

/// 默认期望保留率（§13 锁定）。
pub const DEFAULT_DESIRED_RETENTION: f64 = 0.90;

/// 允许作为整条 MemoryUnit 的记忆种类（§13 锁定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Vocabulary,
    Definition,
    Formula,
    Fact,
    Distinction,
    ProtocolField,
    ShortAnswer,
}

pub const ALL_MEMORY_KINDS: [MemoryKind; 7] = [
    MemoryKind::Vocabulary,
    MemoryKind::Definition,
    MemoryKind::Formula,
    MemoryKind::Fact,
    MemoryKind::Distinction,
    MemoryKind::ProtocolField,
    MemoryKind::ShortAnswer,
];

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Definition => "definition",
            Self::Formula => "formula",
            Self::Fact => "fact",
            Self::Distinction => "distinction",
            Self::ProtocolField => "protocol_field",
            Self::ShortAnswer => "short_answer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ALL_MEMORY_KINDS.iter().copied().find(|k| k.as_str() == raw)
    }
}

/// **不允许**作为整条 MemoryUnit 的能力（§13 锁定）。
///
/// 这些是**能力（ability）**，不是可间隔复习的记忆条目；把它们塞进一条
/// MemoryUnit 会伪造「用 FSRS 排程数学解题能力」这种不成立的命题。
pub const NON_MEMORY_UNIT_ABILITIES: [&str; 5] = [
    "mathematical_problem_solving",
    "programming_ability",
    "writing_ability",
    "oral_fluency",
    "complex_transfer",
];

/// 校验一个 `memory_kind` 文本是否被允许作为 MemoryUnit。
pub fn is_allowed_memory_kind(raw: &str) -> bool {
    MemoryKind::parse(raw).is_some()
}

/// 校验一个文本是否是被明确禁止的「能力」标识。
pub fn is_forbidden_ability(raw: &str) -> bool {
    NON_MEMORY_UNIT_ABILITIES.contains(&raw)
}

/// FSRS 评分档（任务书 §13 锁定的四种；`Easy` 在 V1 **永不自动推断**）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRating {
    Again,
    Hard,
    Good,
    Easy,
}

pub const ALL_REVIEW_RATINGS: [ReviewRating; 4] = [
    ReviewRating::Again,
    ReviewRating::Hard,
    ReviewRating::Good,
    ReviewRating::Easy,
];

impl ReviewRating {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Hard => "hard",
            Self::Good => "good",
            Self::Easy => "easy",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ALL_REVIEW_RATINGS
            .iter()
            .copied()
            .find(|r| r.as_str() == raw)
    }

    /// §13：`Easy` **永不**由自动推断产生。
    pub fn is_auto_inferable(self) -> bool {
        !matches!(self, Self::Easy)
    }
}

/// 可序列化的排程状态快照（stability / difficulty）。
///
/// 这是 `memory_units.fsrs_state_json` 与 `memory_reviews.state_*_json` 的载荷。
/// 定义在 types.rs 是为了让 DB 层不需要感知 fsrs 类型。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SchedulingState {
    pub stability: f64,
    pub difficulty: f64,
}

impl SchedulingState {
    pub fn new(stability: f64, difficulty: f64) -> Self {
        Self {
            stability,
            difficulty,
        }
    }
}

/// MemoryUnit（当前排程状态缓存；**不是**排程真相）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryUnit {
    pub id: i64,
    pub profile_id: i64,
    pub linked_learning_item_id: i64,
    pub memory_key: String,
    pub memory_kind: MemoryKind,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub retrievability: Option<f64>,
    pub last_review_at: Option<String>,
    pub next_review_at: Option<String>,
    pub desired_retention: f64,
    pub review_count: i64,
    pub lapse_count: i64,
    pub fsrs_state_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryUnit {
    /// 是否已经完成过至少一次复习（§11 Stability 轴用到）。
    pub fn has_completed_review(&self) -> bool {
        self.review_count > 0 && self.last_review_at.is_some()
    }

    /// 缓存/派生 retrievability 是否低于期望保留率。
    ///
    /// `None`（未知）**不**算高风险：证据缺失不等于风险。
    pub fn below_desired_retention(&self) -> bool {
        match self.retrievability {
            Some(r) => r < self.desired_retention,
            None => false,
        }
    }

    /// 在给定时刻是否已到期。
    pub fn is_due_at(&self, now_utc: &str) -> bool {
        match &self.next_review_at {
            Some(next) => normalize_utc(next) <= normalize_utc(now_utc),
            None => false,
        }
    }

    /// §13：高风险 = 缓存/派生 retrievability 低于期望保留率，**或**已到期。
    pub fn is_high_risk_at(&self, now_utc: &str) -> bool {
        self.below_desired_retention() || self.is_due_at(now_utc)
    }
}

/// 一次复习的不可变账本行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryReview {
    pub id: i64,
    pub profile_id: i64,
    pub memory_unit_id: i64,
    pub learning_moment_id: Option<i64>,
    pub rating: ReviewRating,
    pub reviewed_at: String,
    pub elapsed_days: i64,
    pub scheduled_days: i64,
    pub state_before_json: serde_json::Value,
    pub state_after_json: serde_json::Value,
    pub created_at: String,
}

/// 新建 MemoryUnit 的输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct NewMemoryUnit {
    pub profile_id: i64,
    pub linked_learning_item_id: i64,
    pub memory_key: String,
    pub memory_kind: MemoryKind,
    /// 缺省 `DEFAULT_DESIRED_RETENTION`。
    pub desired_retention: Option<f64>,
}

impl NewMemoryUnit {
    pub fn new(
        profile_id: i64,
        linked_learning_item_id: i64,
        memory_key: impl Into<String>,
        memory_kind: MemoryKind,
    ) -> Self {
        Self {
            profile_id,
            linked_learning_item_id,
            memory_key: memory_key.into(),
            memory_kind,
            desired_retention: None,
        }
    }

    pub fn effective_desired_retention(&self) -> f64 {
        self.desired_retention.unwrap_or(DEFAULT_DESIRED_RETENTION)
    }
}

/// 记忆压力状态（§13 锁定分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressureStatus {
    /// 一条 MemoryUnit 都没有 —— **不是**「一切正常」，而是「暂无证据」。
    Insufficient,
    Calm,
    Watch,
    High,
}

impl MemoryPressureStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insufficient => "insufficient",
            Self::Calm => "calm",
            Self::Watch => "watch",
            Self::High => "high",
        }
    }

    /// 是否已有足够数据可以展示任何数字（UI §24「Memory card」必须遵守）。
    pub fn is_available(self) -> bool {
        !matches!(self, Self::Insufficient)
    }
}

/// 记忆压力投影（§13 锁定字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryPressure {
    pub total_units: i64,
    pub due_count: i64,
    pub high_risk_count: i64,
    pub next_due_at: Option<String>,
    pub oldest_due_at: Option<String>,
    pub status: MemoryPressureStatus,
}

impl MemoryPressure {
    /// 无任何 MemoryUnit 时的空白投影（UI 必须据此渲染「记忆节奏正在建立」）。
    pub fn insufficient() -> Self {
        Self {
            total_units: 0,
            due_count: 0,
            high_risk_count: 0,
            next_due_at: None,
            oldest_due_at: None,
            status: MemoryPressureStatus::Insufficient,
        }
    }

    /// §13 锁定分类（顺序即优先级）。
    pub fn classify(
        total_units: i64,
        due_count: i64,
        high_risk_count: i64,
    ) -> MemoryPressureStatus {
        if total_units == 0 {
            return MemoryPressureStatus::Insufficient;
        }
        if due_count >= 3 || high_risk_count >= 3 {
            return MemoryPressureStatus::High;
        }
        if high_risk_count == 0 && due_count == 0 {
            return MemoryPressureStatus::Calm;
        }
        if (1..=2).contains(&due_count) {
            return MemoryPressureStatus::Watch;
        }
        // due_count == 0 但存在 1..=2 个高风险：仍需关注，但不构成 high。
        if high_risk_count > 0 {
            return MemoryPressureStatus::Watch;
        }
        MemoryPressureStatus::Calm
    }
}

/// 到期队列的一行（Memory 页 §25 使用；只含**真实**字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct DueMemoryUnit {
    pub unit: MemoryUnit,
    /// 学习项展示名（**非权威**展示名；来源真相永远是 `learning_item_id`）。
    pub learning_item_label: Option<String>,
    /// 逾期天数（正数 = 已逾期；0 = 今天到期；负数 = 尚未到期）。
    pub overdue_days: i64,
    /// 当前状态（new / due / stable），由 §11 Stability 轴口径决定。
    pub status: String,
}

/// 归一 UTC 文本（与 `learning_state::types::normalize_utc` 同口径，本地实现避免跨层耦合）。
pub fn normalize_utc(raw: &str) -> String {
    raw.trim()
        .trim_end_matches('Z')
        .trim_end_matches('z')
        .replace('T', " ")
}
