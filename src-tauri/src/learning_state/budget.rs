//! HIGHER CLOSED LOOP V1 — PHASE 3：Time Budget。
//!
//! 规则（任务书 PHASE 3）：
//! - 只支持有限时间档：30 秒 / 3 分钟 / 10 分钟 / 25 分钟；
//! - **30 秒不得创建普通 StudySession**，只能返回 `micro_action`；
//! - 普通学习动作必须满足 `estimated_minutes <= available_minutes`；
//! - 完整任务过长时只能返回 `entry_slice`，**不得伪造任务已经完成**。

/// 30 秒档：micro action 专用（不落 StudySession）。
pub const BUDGET_KEY_30S: &str = "30s";
pub const BUDGET_KEY_3M: &str = "3m";
pub const BUDGET_KEY_10M: &str = "10m";
pub const BUDGET_KEY_25M: &str = "25m";

/// 有限时间档。刻意不是任意整数——避免 UI 出现无意义的自定义时长。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeBudget {
    Seconds30,
    Min3,
    Min10,
    Min25,
}

impl TimeBudget {
    pub const ALL: [TimeBudget; 4] = [
        TimeBudget::Seconds30,
        TimeBudget::Min3,
        TimeBudget::Min10,
        TimeBudget::Min25,
    ];

    /// 稳定 key（IPC + 单测）。
    pub fn key(self) -> &'static str {
        match self {
            Self::Seconds30 => BUDGET_KEY_30S,
            Self::Min3 => BUDGET_KEY_3M,
            Self::Min10 => BUDGET_KEY_10M,
            Self::Min25 => BUDGET_KEY_25M,
        }
    }

    /// 档位分钟数（30 秒档 = 0，表示“不足一分钟”）。
    pub fn minutes(self) -> i64 {
        match self {
            Self::Seconds30 => 0,
            Self::Min3 => 3,
            Self::Min10 => 10,
            Self::Min25 => 25,
        }
    }

    /// 是否只能执行 micro action。
    pub fn is_micro(self) -> bool {
        matches!(self, Self::Seconds30)
    }

    /// M0-A：30 秒档**拿不到 grounded Micro** 时的降级档。
    ///
    /// 30 秒不是合法正式学时档（`apply_budget` 会把它钳成 0 分钟，产生无法执行的
    /// 「0 分钟普通动作」）。因此当 Micro unavailable 时，按 §M1-E「3 分钟快速学习恒可用」
    /// 退回最小真实学时档，而**不是**伪造一个没有来源的 micro。
    pub fn normal_fallback(self) -> TimeBudget {
        match self {
            Self::Seconds30 => Self::Min3,
            other => other,
        }
    }

    /// 解析时间档；非法输入必须显式失败（不静默降级）。
    pub fn parse(key: &str) -> Result<Self, String> {
        match key.trim().to_ascii_lowercase().as_str() {
            "30s" | "30秒" => Ok(Self::Seconds30),
            "3m" | "3分钟" => Ok(Self::Min3),
            "10m" | "10分钟" => Ok(Self::Min10),
            "25m" | "25分钟" => Ok(Self::Min25),
            other => Err(format!(
                "不支持的时间档：{}（仅允许 30s / 3m / 10m / 25m）",
                other
            )),
        }
    }
}

// M0-A：30 秒档的候选**必须**来自真实 grounding 来源（`learning_state::micro::generate_candidates`）。
//
// 这里刻意**不再**保留任何「两布尔启发式伪造 Micro」的降级函数：
// 旧 `pick_micro_action(has_risk_signal, has_material)` 会在没有任何真实来源时
// 凭 recovery / review 状态凭空造出一个 `MicroActionKind`，产出「micro action only」
// 但 `micro_action = None` 的不可执行结果 —— 这正是 M0-A 明令删除的生产兜底。
// 无 grounded 候选时的唯一合法语义是 `Micro unavailable`，退回普通 NextAction。
