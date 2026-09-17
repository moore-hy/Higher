//! Learning Domain 词表（REAL LEARNING ENGINE V1 · §3 / §6）。
//!
//! # 为什么需要一个独立于 `ProtocolDomain` 的类型
//!
//! 两个词表**故意不相等**：
//!
//! ```text
//! LearningDomain（本文件）   5 值：generic / english / mathematics /
//!                                  computer_science_408 / programming
//! ProtocolDomain（§14 注册表）4 值：generic / english / mathematics /
//!                                  computer_science_408
//! ```
//!
//! - `LearningDomain` 是**存储与意图**的词表：它描述「这个东西属于哪个领域」，
//!   由 §3 的 `active_learning_intent.domain`、§6 的 `learning_items.domain` /
//!   `goals.domain`、§26 的 `document_sources.domain` 共同使用。
//! - `ProtocolDomain` 是**协议选择**的词表：它描述「哪些训练协议可以服务这个领域」，
//!   由 Cognitive Core V1.2 §14 的协议注册表拥有。
//!
//! 把两者合成一个类型会立刻制造两个问题：要么存储被迫丢掉 `programming`
//! （违反 §6 锁定 schema），要么协议注册表被迫多出一个没有任何协议声明的领域
//! （违反 V1.2 契约）。因此这里保持两个类型，并在**唯一一个显式函数**里做桥接，
//! 而不是散落各处的隐式转换。
//!
//! # 桥接是 OWNER 批准的**临时协议适配器**
//!
//! 见 [`LearningDomain::to_protocol_domain`]。`programming` 在协议注册表中
//! 没有对应值 —— 这是 §6 词表与 V1.2 注册表之间一个**真实存在的落差**，
//! 不是实现疏漏。HOTFIX-01 FIX I 已由 Owner 明确批准：Real Learning Engine V1
//! 期间，`LearningDomain::Programming` **临时复用**现成的编码类协议族
//! （即 `ProtocolDomain::ComputerScience408`）。
//!
//! 这个批准**只**覆盖「协议选择」，不覆盖任何语义等同：
//!
//! ```text
//! Programming != CS408
//! Programming mastery != CS408 mastery
//! Programming goal != CS408 goal
//! ```
//!
//! 存储侧始终是 `programming`（§6 schema 未变），协议注册表也**不**扩张。
//! 一旦 Owner 给注册表补上编程专属协议，本函数是**唯一**需要改的地方。

use serde::{Deserialize, Serialize};

use super::protocol::ProtocolDomain;

/// §3 / §6 锁定的五值领域词表。
///
/// `as_str` 的返回值与数据库 `CHECK` 约束**逐字一致**；
/// 两者不一致会立刻被 `db_level_check_constraints_*` 这类测试抓住。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningDomain {
    Generic,
    English,
    Mathematics,
    ComputerScience408,
    /// §6 词表独有：协议注册表目前**没有**对应领域。
    Programming,
}

impl LearningDomain {
    /// 全部五值（顺序 = §3 / §6 的书写顺序）。
    pub const ALL: [LearningDomain; 5] = [
        LearningDomain::Generic,
        LearningDomain::English,
        LearningDomain::Mathematics,
        LearningDomain::ComputerScience408,
        LearningDomain::Programming,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::English => "english",
            Self::Mathematics => "mathematics",
            Self::ComputerScience408 => "computer_science_408",
            Self::Programming => "programming",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.as_str() == raw)
    }

    /// 人话领域名（UI 展示用；不编造数据）。
    pub fn display_name_zh(self) -> &'static str {
        match self {
            Self::Generic => "通用",
            Self::English => "英语",
            Self::Mathematics => "数学",
            Self::ComputerScience408 => "计算机 408",
            Self::Programming => "编程",
        }
    }

    /// 领域 → 协议选择词表的桥接。**OWNER 批准的临时协议适配器（FIX I）。**
    ///
    /// 四个同名值直接对应。`programming` 在 V1.2 协议注册表中不存在，
    /// 这里映射到 [`ProtocolDomain::ComputerScience408`]，依据是**可核验的**：
    /// 注册表中全部四个编码类协议（`coding_trace` / `coding_completion` /
    /// `debugging` / `independent_build`）都只声明 `computer_science_408`
    /// 作为领域（`independent_build` 另加 `generic`）。也就是说，
    /// 「编程」这一语义在协议层此刻**确实**由 CS408 承载。
    ///
    /// # 这个映射**不**声称任何语义等同（FIX I）
    ///
    /// ```text
    /// Programming != CS408
    /// Programming mastery != CS408 mastery
    /// Programming goal != CS408 goal
    /// ```
    ///
    /// 它只是一个**协议适配器**：让编程领域的学习也能拿到一套可执行的训练协议。
    /// 存储侧（`learning_items.domain` / `goals.domain` / 意图 domain）始终写
    /// `programming`，协议注册表**不**扩张，掌握度也仍然按各自的记忆单元计算。
    ///
    /// 因此这里不再有「待 Owner 确认」的状态 —— Owner 已批准（HOTFIX-01 FIX I），
    /// 且批准的范围恰好就是「临时复用现有协议族」。
    pub fn to_protocol_domain(self) -> ProtocolDomain {
        match self {
            Self::Generic => ProtocolDomain::Generic,
            Self::English => ProtocolDomain::English,
            Self::Mathematics => ProtocolDomain::Mathematics,
            Self::ComputerScience408 | Self::Programming => ProtocolDomain::ComputerScience408,
        }
    }
}

impl std::fmt::Display for LearningDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// §6 的兜底值：解析链走到底仍未显式确认 → `Generic`。
///
/// 兜底**不是**「猜成通用」，而是「明确地不知道具体领域」。
pub const FALLBACK_DOMAIN: LearningDomain = LearningDomain::Generic;
