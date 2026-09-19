//! HIGHER PERSONAL CORE — `EvidenceScope` V1（A2-1 §8）。
//!
//! # 它解决的问题
//!
//! 「这条证据属于谁 / 属于哪个范围」过去是**隐式**的：`learning_moments.profile_id`
//! 只是一个列，跨档案安全性靠每个 SQL 各自记得带 `WHERE profile_id = ?1`。
//! 一旦证据被投影成 DTO 传给上层，`profile_id` 就被丢掉，于是「同一份长相的证据」
//! 在两个档案里无法区分。A2-1 把它做成**类型**。
//!
//! # 硬规则（§8 锁定）
//!
//! ```text
//! 1. StudyProfile != Person
//! 2. 绝不引入 person_id = profile_id
//! 3. 每一个工作区/实体变体都自带归属信息
//! 4. 不改变任何 DB schema（本模块不落表、不迁移）
//! 5. LocalPerson = 本机 Higher 安装的拥有者，**不是**云账号
//! ```
//!
//! # 为什么 `LocalPerson` 不是 `StudyProfile`
//!
//! `LocalPerson` 代表「这台机器上这个人」。学习档案（`StudyProfile`）是他在 Higher 里
//! 划分出来的**学习工作区**（§25：StudyProfile 概念上就是 Learning Workspace）。
//! 一个人可以有很多工作区，一个工作区里没有任何「人」的属性。
//! 因此 `LocalPerson` **不带** `profile_id` —— 它不是「profile_id = NULL 的档案」，
//! 而是一个不同的种类。把两者混为一谈会让「档案隔离」退化成一个可空列。

use serde::{Deserialize, Serialize};

/// 证据的作用域（**自带归属**，序列化稳定 snake_case）。
///
/// 每个变体都携带它所属的 `profile_id`，唯一例外是 [`EvidenceScope::LocalPerson`]，
/// 它按定义不属于任何档案。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceScope {
    /// 本机 Higher 安装的拥有者。**没有** `profile_id` —— 它不是档案。
    LocalPerson,
    /// 一个学习工作区（既有 `study_profiles`，概念上是 Learning Workspace）。
    StudyProfile { profile_id: i64 },
    /// 某个档案下的一个学习项。
    LearningItem {
        profile_id: i64,
        learning_item_id: i64,
    },
    /// 某个档案下的一个目标。
    Goal { profile_id: i64, goal_id: i64 },
    /// 某个档案下的一个任务。
    Task { profile_id: i64, task_id: i64 },
    /// 某个档案下的一个学习会话。
    Session { profile_id: i64, session_id: i64 },
    /// 某个档案下的一次训练运行。
    TrainingRun {
        profile_id: i64,
        training_run_id: i64,
    },
}

/// 全部 7 个作用域变体（稳定顺序；用于遍历与测试穷尽性）。
pub const ALL_SCOPE_KINDS: [&str; 7] = [
    "local_person",
    "study_profile",
    "learning_item",
    "goal",
    "task",
    "session",
    "training_run",
];

impl EvidenceScope {
    /// 变体的稳定文本（snake_case）。
    pub fn kind(self) -> &'static str {
        match self {
            Self::LocalPerson => "local_person",
            Self::StudyProfile { .. } => "study_profile",
            Self::LearningItem { .. } => "learning_item",
            Self::Goal { .. } => "goal",
            Self::Task { .. } => "task",
            Self::Session { .. } => "session",
            Self::TrainingRun { .. } => "training_run",
        }
    }

    /// 该作用域所属的档案。
    ///
    /// `LocalPerson` 返回 `None` —— **这不是**「属于 0 号档案」，而是「不属于任何档案」。
    /// 调用方若需要一个档案，必须显式处理 `None`，不得用 `unwrap_or(0)` 兜底。
    pub fn owner_profile_id(self) -> Option<i64> {
        match self {
            Self::LocalPerson => None,
            Self::StudyProfile { profile_id }
            | Self::LearningItem { profile_id, .. }
            | Self::Goal { profile_id, .. }
            | Self::Task { profile_id, .. }
            | Self::Session { profile_id, .. }
            | Self::TrainingRun { profile_id, .. } => Some(profile_id),
        }
    }

    /// 两个作用域是否属于**同一个档案**。
    ///
    /// ```text
    /// LocalPerson vs 任何 StudyProfile  ->  false（§8：LocalPerson != StudyProfile）
    /// profile 1 vs profile 1            ->  true
    /// profile 1 vs profile 2            ->  false（即便其它 id 完全相同）
    /// ```
    ///
    /// 这条判定是「档案隔离」的机器可执行表达：长相一样但档案不同的两条证据，
    /// 它们**不是**同一条证据。
    pub fn same_profile_owner(self, other: Self) -> bool {
        match (self.owner_profile_id(), other.owner_profile_id()) {
            (Some(a), Some(b)) => a == b,
            // 任何一侧是 LocalPerson → 不是「同一个档案」。
            _ => false,
        }
    }

    /// `self` 是否**包含** `other`（归属层级）。
    ///
    /// ```text
    /// LocalPerson         ⊇ 一切
    /// StudyProfile{p}     ⊇ 该档案下的 LearningItem / Goal / Task / Session / TrainingRun
    /// 实体级作用域        ⊇ 只有它自己
    /// ```
    ///
    /// 跨档案一律 `false`：`StudyProfile{1}` 不包含 `LearningItem{profile_id: 2, ..}`，
    /// 即便 id 数字相同。
    pub fn contains(self, other: Self) -> bool {
        if self == other {
            return true;
        }
        match self {
            Self::LocalPerson => true,
            Self::StudyProfile { profile_id } => other.owner_profile_id() == Some(profile_id),
            // 实体级作用域只容纳自己（上面已判定相等）。
            Self::LearningItem { .. }
            | Self::Goal { .. }
            | Self::Task { .. }
            | Self::Session { .. }
            | Self::TrainingRun { .. } => false,
        }
    }
}
