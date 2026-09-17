//! HIGHER COGNITIVE CORE V1.2 §6 / §15 — Domain Packs。
//!
//! 领域包把「学习项的状态」映射为**可执行且可解释的训练协议链**。
//! 它刻意只做三件事：
//!
//! 1. 声明该领域的**能力轴**；
//! 2. 声明「状态 → 协议链」的**显式映射**（不是模型自由发挥）；
//! 3. 声明该领域的**禁止项**（如 408 纯理论条目不得进入独立实现）。
//!
//! 概念上复用既有 Higher Skill 基础设施，**不新建插件系统**（§15）。
//! **不发布受版权保护的教材/课程内容。**

pub mod computer_science_408;
pub mod english;
pub mod mathematics;

use serde::{Deserialize, Serialize};

use crate::cognitive::protocol::ProtocolId;

/// 一条协议链 + 它为什么被选中（人话理由，UI 可直接展示）。
///
/// `protocols` 是 `&'static [ProtocolId]`（静态链），因此只实现 `Serialize`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ts_rs::TS)]
pub struct ProtocolChain {
    pub protocols: &'static [ProtocolId],
    /// 该链的适用条件（稳定 code，便于 UI 与测试断言）。
    pub condition: &'static str,
}

impl ProtocolChain {
    pub const fn new(protocols: &'static [ProtocolId], condition: &'static str) -> Self {
        Self {
            protocols,
            condition,
        }
    }

    pub fn first(&self) -> Option<ProtocolId> {
        self.protocols.first().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.protocols.is_empty()
    }
}

/// 领域包种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum DomainPack {
    English,
    Mathematics,
    ComputerScience408,
}

impl DomainPack {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::Mathematics => "mathematics",
            Self::ComputerScience408 => "computer_science_408",
        }
    }

    /// 该领域包对应的协议域（与 `ProtocolDomain` 对齐）。
    pub fn protocol_domain(self) -> crate::cognitive::protocol::ProtocolDomain {
        use crate::cognitive::protocol::ProtocolDomain;
        match self {
            Self::English => ProtocolDomain::English,
            Self::Mathematics => ProtocolDomain::Mathematics,
            Self::ComputerScience408 => ProtocolDomain::ComputerScience408,
        }
    }

    /// 该领域的能力轴（稳定顺序）。
    pub fn capability_axes(self) -> &'static [&'static str] {
        match self {
            Self::English => english::CAPABILITY_AXES,
            Self::Mathematics => mathematics::CAPABILITY_AXES,
            Self::ComputerScience408 => computer_science_408::CAPABILITY_AXES,
        }
    }

    /// 由文本解析（未知 → `None`，**不**猜测领域）。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "english" => Some(Self::English),
            "mathematics" => Some(Self::Mathematics),
            "computer_science_408" => Some(Self::ComputerScience408),
            _ => None,
        }
    }
}
