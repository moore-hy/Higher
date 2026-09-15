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

/// micro action 建议（30 秒档）：短 Recall / 重新解释一个概念 / 查看一个关键错误。
/// 只返回**可执行的最小动作**，绝不创建 StudySession。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroActionKind {
    ShortRecall,
    ReexplainConcept,
    ReviewKeyError,
}

impl MicroActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShortRecall => "short_recall",
            Self::ReexplainConcept => "reexplain_concept",
            Self::ReviewKeyError => "review_key_error",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ShortRecall => "30 秒回顾一个关键点",
            Self::ReexplainConcept => "30 秒用自己的话解释一个概念",
            Self::ReviewKeyError => "30 秒看一个最近的关键错误",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Self::ShortRecall => "时间不足一分钟，只看一条你已经学过的东西，不新增学习记录。",
            Self::ReexplainConcept => "时间不足一分钟，用复述代替阅读，不新增学习记录。",
            Self::ReviewKeyError => "时间不足一分钟，先看一个已记录的错误，不新增学习记录。",
        }
    }
}

/// 30 秒档的 deterministic 选择：有风险信号 → 先看关键错误；有可复述材料 → 复述；
/// 否则 → 短 Recall。同一输入恒返回同一结果（无随机、无 LLM）。
pub fn pick_micro_action(has_risk_signal: bool, has_reviewable_material: bool) -> MicroActionKind {
    if has_risk_signal {
        MicroActionKind::ReviewKeyError
    } else if has_reviewable_material {
        MicroActionKind::ReexplainConcept
    } else {
        MicroActionKind::ShortRecall
    }
}
