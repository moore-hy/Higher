//! M4 — COMPANION SKILL V1：类型定义。
//!
//! Companion 是**独立产品子系统**（架构：`src-tauri/src/companion/`），
//! **不藏在 Today 组件里**，也不持有任何学习真相：
//!
//! | Companion 拥有 | Companion **不**拥有（从 canonical 系统读） |
//! |---|---|
//! | identity / personality seed | task truth |
//! | current behavior state | learning mastery truth |
//! | world / expedition state | today learning minutes truth |
//! | companion memories | evaluation truth |
//! | bounded dialogue summary | next action ranking truth |
//! | collectibles / story fragments | |
//! | interaction cooldowns | |
//!
//! 因此本模块的所有结构体里**不会**出现 task / mastery / minutes / evaluation
//! 的字段：那些只能通过 `learning_state` 读取（见 `service::build_companion_state`）。

use serde::{Deserialize, Serialize};

/// 稳定身份 key（不是显示名）。V1 只有一个原型身份。
pub const COMPANION_ID: &str = "haven-companion";

/// 允许的性格原型（有限枚举，**不**扩张成几十种情绪）。
pub const ARCHETYPES: [&str; 3] = ["sprout-guide", "quiet-scholar", "trail-scout"];

/// M5-B：V1 只开放三档远征时长（秒）。
pub const EXPEDITION_SHORT_SECONDS: i64 = 20 * 60;
pub const EXPEDITION_MEDIUM_SECONDS: i64 = 60 * 60;
pub const EXPEDITION_LONG_SECONDS: i64 = 3 * 60 * 60;

/// 全部允许的远征时长（升序）。
pub const EXPEDITION_DURATIONS: [i64; 3] = [
    EXPEDITION_SHORT_SECONDS,
    EXPEDITION_MEDIUM_SECONDS,
    EXPEDITION_LONG_SECONDS,
];

/// M4-G：同一个「来访」内最多一次主动学习邀请；超过该间隔视为新的一次来访。
pub const NUDGE_VISIT_GAP_MINUTES: i64 = 180;

/// `current_scene` 的 V1 取值（自由文本列，保留后续扩展空间）。
pub const SCENE_HOME: &str = "home";
pub const SCENE_WILDS: &str = "wilds";

// =============== 行为状态机（§M4-B） ===============

/// §M4-B 的**确定性**基础状态机（7 个状态，刻意不做几十种情绪）。
///
/// 迁移只由可验证输入决定：远征状态 / 近期有意义学习 / 近期返回 /
/// recovery 状态 / 近期互动 / 距上次访问的时间。**不需要 LLM**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorState {
    Idle,
    Curious,
    Resting,
    Expedition,
    Returning,
    Celebrating,
    Recovery,
}

impl BehaviorState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Curious => "curious",
            Self::Resting => "resting",
            Self::Expedition => "expedition",
            Self::Returning => "returning",
            Self::Celebrating => "celebrating",
            Self::Recovery => "recovery",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "idle" => Self::Idle,
            "curious" => Self::Curious,
            "resting" => Self::Resting,
            "expedition" => Self::Expedition,
            "returning" => Self::Returning,
            "celebrating" => Self::Celebrating,
            "recovery" => Self::Recovery,
            _ => return None,
        })
    }
}

// =============== 远征就绪度（§M5-C） ===============

/// §M5-C 的**派生**就绪状态。
///
/// 它**不是**余额，也不存在「能量 / 学习币 / 燃料钱包」这些概念：
/// 它只表达「此刻可以出发去多久」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpeditionReadiness {
    NotReady,
    ReadyShort,
    ReadyMedium,
    ReadyLong,
}

impl ExpeditionReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotReady => "NOT_READY",
            Self::ReadyShort => "READY_SHORT",
            Self::ReadyMedium => "READY_MEDIUM",
            Self::ReadyLong => "READY_LONG",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "NOT_READY" => Self::NotReady,
            "READY_SHORT" => Self::ReadyShort,
            "READY_MEDIUM" => Self::ReadyMedium,
            "READY_LONG" => Self::ReadyLong,
            _ => return None,
        })
    }

    /// 该就绪度下可选的时长（升序）。`NOT_READY` → 空（远征不可用）。
    pub fn available_durations(self) -> Vec<i64> {
        match self {
            Self::NotReady => Vec::new(),
            Self::ReadyShort => vec![EXPEDITION_SHORT_SECONDS],
            Self::ReadyMedium => vec![EXPEDITION_SHORT_SECONDS, EXPEDITION_MEDIUM_SECONDS],
            Self::ReadyLong => EXPEDITION_DURATIONS.to_vec(),
        }
    }

    /// 该就绪度是否允许某个**合法**时长。
    pub fn allows(self, duration_seconds: i64) -> bool {
        self.available_durations().contains(&duration_seconds)
    }
}

// =============== 远征状态 ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpeditionStatus {
    Running,
    Ready,
    Collected,
}

impl ExpeditionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Collected => "collected",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "running" => Self::Running,
            "ready" => Self::Ready,
            "collected" => Self::Collected,
            _ => return None,
        })
    }
}

// =============== 持久化结构（与 v033 一一对应） ===============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionProfile {
    pub id: i64,
    pub profile_id: i64,
    pub companion_id: String,
    pub archetype: String,
    pub nickname: Option<String>,
    pub personality_seed: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionWorldState {
    pub id: i64,
    pub profile_id: i64,
    /// 持久化的就绪度（§M5-C：派生状态的落库快照）。
    pub expedition_readiness: ExpeditionReadiness,
    pub readiness_updated_at: Option<String>,
    pub current_scene: String,
    pub current_behavior: BehaviorState,
    pub last_interaction_at: Option<String>,
    pub last_nudge_at: Option<String>,
    /// §M7 / P0-01：就绪度消费水位线（与学习日绑定，绝不构成钱包/余额）。
    ///
    /// `consumed_local_date` = 上一次「出发远征」所消耗的本地学习日；
    /// `consumed_contribution_total` = 该次出发时 `today_total` 的快照值。
    /// 二者共同表示「哪些已有贡献已被当前远征机会兑现」：
    /// 只有**同一学习日**、且数值更大的新贡献，才能在收取之后重新生成就绪度。
    pub consumed_local_date: Option<String>,
    pub consumed_contribution_total: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionExpedition {
    pub id: i64,
    pub profile_id: i64,
    pub status: ExpeditionStatus,
    pub started_at: String,
    pub duration_seconds: i64,
    pub finished_at: Option<String>,
    pub readiness_tier_at_start: ExpeditionReadiness,
    pub seed: i64,
    pub theme: String,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionMemory {
    pub id: i64,
    pub profile_id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub source_type: Option<String>,
    pub source_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionEvent {
    pub id: i64,
    pub profile_id: i64,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

// =============== 对话（§M4-E：本地确定性模板，0 Cloud） ===============

/// §M4-E 的事件集合（有限枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogueEvent {
    ReturnAfterBreak,
    FirstVisitToday,
    MicroComplete,
    SessionComplete,
    DifficultAttempt,
    ExpeditionStart,
    ExpeditionReturn,
    Recovery,
    DeclineLearning,
}

impl DialogueEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReturnAfterBreak => "return_after_break",
            Self::FirstVisitToday => "first_visit_today",
            Self::MicroComplete => "micro_complete",
            Self::SessionComplete => "session_complete",
            Self::DifficultAttempt => "difficult_attempt",
            Self::ExpeditionStart => "expedition_start",
            Self::ExpeditionReturn => "expedition_return",
            Self::Recovery => "recovery",
            Self::DeclineLearning => "decline_learning",
        }
    }
}

/// 一条确定性对白（变体由稳定 seed 选出，不是不受控的随机刷屏）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionDialogue {
    pub event: String,
    pub variant: i64,
    pub text: String,
}

// =============== 交互（§M4-D） ===============

/// 允许的 companion 交互类型（有限枚举；拒绝未知输入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// 打招呼（首次进入 / 回来）
    Greet,
    /// 点一下宠物（**不产生任何学习收益**，§M3-A / §M5-C）
    Pet,
    /// 鼓励一下
    Cheer,
    /// §M4-G：谢绝学习邀请 —— 立刻接受、不内疚、同一来访不再二次邀请
    DeclineNudge,
}

impl InteractionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Greet => "greet",
            Self::Pet => "pet",
            Self::Cheer => "cheer",
            Self::DeclineNudge => "decline_nudge",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "greet" => Ok(Self::Greet),
            "pet" => Ok(Self::Pet),
            "cheer" => Ok(Self::Cheer),
            "decline_nudge" => Ok(Self::DeclineNudge),
            other => Err(format!(
                "未知的 companion 交互：{}（仅允许 greet / pet / cheer / decline_nudge）",
                other
            )),
        }
    }
}

// =============== 对外 DTO ===============

/// §M4-D `get_companion_state(profile_id)` 的返回。
///
/// 注意：这里**没有**任何学习真相字段 —— 学习信息一律由独立的
/// `learning_state` 命令提供（Companion 只读取，不复制）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionState {
    pub profile_id: i64,
    pub profile: CompanionProfile,
    pub world: CompanionWorldState,
    /// 有效行为状态（§M4-B；可能与 `world.current_behavior` 不同 —— 落库的是上次观测结果）
    pub behavior: BehaviorState,
    /// 有效就绪度（§M5-C；本次派生结果）
    pub readiness: ExpeditionReadiness,
    /// 该就绪度下可选的远征时长（秒，升序；NOT_READY → 空）
    pub available_durations: Vec<i64>,
    /// 正在进行（未到 finished_at）的远征
    pub open_expedition: Option<CompanionExpedition>,
    /// 已完成、等待收取的远征
    pub ready_expedition: Option<CompanionExpedition>,
    pub memory_count: i64,
    /// 本次交互/进入时的对白（确定性模板）
    pub dialogue: CompanionDialogue,
    /// §M4-G：此刻是否允许发出主动学习邀请（每次来访最多一次）
    pub nudge_available: bool,
}

/// §M4-G 的主动学习邀请：**来源必须是 canonical 学习状态**。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionNudge {
    pub text: String,
    /// canonical NextLearningAction 的动作类型（Companion **不**自己排序学习任务）
    pub action_type: String,
    pub reason_code: String,
    /// 该动作的展示标题（来自 canonical NextAction，不是 Companion 编的）
    pub title: String,
    pub estimated_minutes: i64,
    /// canonical 建议时长（可选）
    pub suggested_minutes: Option<i64>,
}

/// §M5-E 返回事件：确定性（同一 seed + 同一档位 → 同一故事与收藏）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionReturn {
    pub expedition: CompanionExpedition,
    pub memory: CompanionMemory,
    pub dialogue: CompanionDialogue,
    /// §M5-F：收取之后**最多一条**学习邀请（来自 canonical 学习状态）
    pub nudge: Option<CompanionNudge>,
}

/// §M5-D：主题只改变故事/收藏的**风味**，不改变掌握度、不给学习增益。
pub const THEME_ENGLISH: &str = "English";
pub const THEME_MATH: &str = "Math";
pub const THEME_PROGRAMMING: &str = "Programming";
pub const THEME_ELECTRONICS: &str = "Electronics";
pub const THEME_GENERAL: &str = "General";

/// 允许的主题（有限枚举）。
pub const THEMES: [&str; 5] = [
    THEME_ENGLISH,
    THEME_MATH,
    THEME_PROGRAMMING,
    THEME_ELECTRONICS,
    THEME_GENERAL,
];
