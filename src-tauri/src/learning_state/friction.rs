//! M2 — LEARNING FRICTION V1（学习摩擦：一个**窄**的、基于证据的重投影）。
//!
//! ## 它是什么 / 不是什么
//!
//! 它检测的是「同一个知识点在滚动窗口里**反复失败/部分失败**」这一件可验证事实，
//! 并据此调整**支持的轻重**与重试节奏。它**不是**：
//!
//! - 不是「疼痛评分」，也不做任何医学/心理上的度量（故代码用 `LearningFrictionState`）；
//! - 不推断人格 / 智力 / 天赋 —— 永远不产出「你不擅长 X」「你记忆力差」这类结论；
//! - 不发明证据：没有记录的东西（提示使用次数、延迟、脑内状态）**一律不假装存在**。
//!
//! ## 唯一输入
//!
//! ```text
//! evaluations（trust_state = 'trusted'，learning_item_id 非空，滚动窗口内）
//!   → 失败 / 部分失败 / 连续失败 / 明确的重试模式
//! micro_learning_events（done/partial，同主体）
//!   → 仅作为 secondary context，**绝不**独立提升摩擦
//! ```
//!
//! **Absence of Evidence means unknown, not success/failure**：无证据 → `Unknown`，
//! 既不算成功也不算失败。
//!
//! ## 锁定策略（§M2-D，deterministic，0 LLM）
//!
//! ```text
//! unknown / low  → support 0：自由回忆 / 自我解释
//! medium         → support 1：缩小范围 + 一个线索
//! high           → support 2：候选/引导（先识别，再自由回忆）+ 冷却，禁止连续锤击
//! ```
//!
//! ## 读取范围与隔离
//!
//! 只读（仅 SELECT）；全部按 `profile_id` 过滤（profile scoped）；
//! 只影响**当前摩擦主体**，其它学习项恒为 support 0（不污染无关项目）。

use crate::learning_state::types::{FrictionLevel, FrictionSignal, LearningFrictionState};
use crate::repository::evaluation::EvaluationRepository;
use crate::repository::micro_learning_event::MicroLearningEventRepository;
use rusqlite::Connection;
use std::collections::BTreeMap;

/// §M2-B：可信失败/部分失败的滚动窗口（天）。
pub const FRICTION_WINDOW_DAYS: i64 = 14;

/// §M2-F：High 摩擦主体的冷却时长（分钟）。冷却期内**不得**反复锤击同一项。
pub const FRICTION_COOLDOWN_MINUTES: i64 = 60;

/// Micro secondary context 的时间窗（小时）。
pub const FRICTION_MICRO_CONTEXT_HOURS: i64 = 24;

/// 达到 High 所需的窗口内可信失败数。
pub const FAILED_FOR_HIGH: i64 = 2;
/// 达到 High 所需的「连续可信失败」数。
pub const CONSECUTIVE_FOR_HIGH: i64 = 2;
/// 构成「明确的重试模式」所需的失败 + 部分失败总数。
pub const RETRY_FOR_GROUNDED_RETRY: i64 = 2;
/// 仅靠部分失败达到 Medium 所需数量。
pub const PARTIAL_FOR_MEDIUM: i64 = 2;

/// 信号 code（稳定字符串；UI 只做展示映射，不参与判断）。
pub const SIGNAL_TRUSTED_FAILED: &str = "trusted_failed_evaluations";
pub const SIGNAL_TRUSTED_PARTIAL: &str = "trusted_partial_evaluations";
pub const SIGNAL_CONSECUTIVE_FAILURES: &str = "consecutive_grounded_failures";
pub const SIGNAL_GROUNDED_RETRY: &str = "explicit_grounded_retry";
pub const SIGNAL_MICRO_CONTEXT: &str = "micro_retry_context_secondary";

/// §M2-D 支持等级（0..2）。
pub const SUPPORT_FREE_RECALL: u8 = 0;
pub const SUPPORT_ONE_CUE: u8 = 1;
pub const SUPPORT_GUIDED: u8 = 2;

/// §M2-E：support 0（与自由回忆同一件事，故不额外附加文案）。
pub const SUPPORT_0_TEXT: &str = "不看笔记，先回忆关键结论。";
/// §M2-E：support 1 —— 诚实可执行回退：缩小范围（**不**承诺并不存在的关键词提示）。
pub const SUPPORT_1_TEXT: &str = "先把范围缩小到要点，再尝试自由回忆。";
/// §M2-E：support 2 —— 诚实可执行回退：先识别关键结论 + 冷却，
/// **不**承诺并不存在的「候选 / 上下文」。
pub const SUPPORT_2_TEXT: &str = "先识别关键结论再自由回忆，中间可稍微冷却、不要连续硬磕。";

/// §M2-G：允许对用户说的话（只陈述事实 + 下一步，不做羞辱式播报）。
pub const FRICTION_NOTICE_TITLE: &str = "这个点最近反复卡住了。";
pub const FRICTION_NOTICE_BODY: &str = "这次换一种更轻的方式。";

/// 生产入口：构建本档案的摩擦投影（只读 / deterministic / 0 LLM）。
pub fn build_friction_state(
    conn: &Connection,
    profile_id: i64,
) -> Result<LearningFrictionState, String> {
    let rows = EvaluationRepository::new(conn)
        .list_trusted_in_window(profile_id, FRICTION_WINDOW_DAYS, FRICTION_COOLDOWN_MINUTES)
        .map_err(|e| e.to_string())?;

    // 按学习项聚合（BTreeMap 保证与查询顺序无关的确定性遍历基准）。
    let mut acc: BTreeMap<i64, SubjectAccum> = BTreeMap::new();
    for r in &rows {
        let a = acc.entry(r.learning_item_id).or_default();
        match r.outcome.as_str() {
            "failed" => {
                a.failed += 1;
                // 行序 = occurred_at DESC ⇒ 头部连续段就是「最近连续失败」。
                if !a.consecutive_broken {
                    a.consecutive_failed += 1;
                }
                if a.latest_failed_at.is_none() {
                    a.latest_failed_at = Some(r.occurred_at.clone());
                }
                if a.cooldown_until.is_none() {
                    // 冷却截止与失败时间出自同一行（同一时间口径）。
                    a.cooldown_until = r.cooldown_candidate.clone();
                }
            }
            "partial" => {
                a.partial += 1;
                a.consecutive_broken = true;
            }
            // passed / unrated：都终止「连续失败」，且不抬高摩擦。
            _ => a.consecutive_broken = true,
        }
        if a.latest_at.is_none() {
            a.latest_at = Some(r.occurred_at.clone());
        }
        if a.label.is_none() {
            let t = r.title.trim();
            if !t.is_empty() {
                a.label = Some(t.to_string());
            }
        }
    }

    if acc.is_empty() {
        // 无任何可信证据 → Unknown（既不是成功，也不是失败）。
        return Ok(LearningFrictionState::unknown());
    }

    // 确定性选取「当前摩擦主体」：
    // 等级 → 失败数 → 连续失败 → 部分失败 → 最近时间 → id 升序
    let subject_id = select_friction_subject(&acc).expect("acc 非空：前面已 early-return");
    let a = &acc[&subject_id];
    let level = level_of(a);

    // ---- 信号（稳定顺序；Micro 类恒为非权威）----
    let mut signals: Vec<FrictionSignal> = Vec::new();
    if a.failed > 0 {
        signals.push(FrictionSignal {
            code: SIGNAL_TRUSTED_FAILED.to_string(),
            count: a.failed,
            latest_at: a.latest_failed_at.clone(),
            authoritative: true,
        });
    }
    if a.partial > 0 {
        signals.push(FrictionSignal {
            code: SIGNAL_TRUSTED_PARTIAL.to_string(),
            count: a.partial,
            latest_at: None,
            authoritative: true,
        });
    }
    if a.consecutive_failed >= CONSECUTIVE_FOR_HIGH {
        signals.push(FrictionSignal {
            code: SIGNAL_CONSECUTIVE_FAILURES.to_string(),
            count: a.consecutive_failed,
            latest_at: a.latest_failed_at.clone(),
            authoritative: true,
        });
    }
    if a.failed + a.partial >= RETRY_FOR_GROUNDED_RETRY {
        signals.push(FrictionSignal {
            code: SIGNAL_GROUNDED_RETRY.to_string(),
            count: a.failed + a.partial,
            latest_at: a.latest_at.clone(),
            authoritative: true,
        });
    }
    let micro_retries = MicroLearningEventRepository::new(conn)
        .count_done_partial_for_subject(profile_id, subject_id, FRICTION_MICRO_CONTEXT_HOURS)
        .map_err(|e| e.to_string())?;
    if micro_retries > 0 {
        // secondary context：`authoritative = false`，绝不独立提升等级。
        signals.push(FrictionSignal {
            code: SIGNAL_MICRO_CONTEXT.to_string(),
            count: micro_retries,
            latest_at: None,
            authoritative: false,
        });
    }

    Ok(LearningFrictionState {
        level,
        subject_learning_item_id: Some(subject_id),
        subject_label: a.label.clone(),
        signals,
        recommended_support_level: level.support_level(),
        // §M2-F：只有 High 才携带冷却（低摩擦不该被限制重试）。
        cooldown_until: if level == FrictionLevel::High {
            a.cooldown_until.clone()
        } else {
            None
        },
    })
}

#[derive(Debug, Default, Clone)]
struct SubjectAccum {
    label: Option<String>,
    failed: i64,
    partial: i64,
    consecutive_failed: i64,
    /// 最近连续失败段是否已被打断（遍历 DESC 时遇到首个非 failed）。
    consecutive_broken: bool,
    latest_failed_at: Option<String>,
    cooldown_until: Option<String>,
    latest_at: Option<String>,
}

/// §M2-D 锁定阈值 → 等级。
///
/// 注意：`partial` 只可能把等级推到 `Medium`；`High` 需要**真实失败**
/// （或连续失败）—— 这保证「部分失败」不会被当成「反复卡住」。
fn level_of(a: &SubjectAccum) -> FrictionLevel {
    if a.failed >= FAILED_FOR_HIGH || a.consecutive_failed >= CONSECUTIVE_FOR_HIGH {
        FrictionLevel::High
    } else if a.failed >= 1 || a.partial >= PARTIAL_FOR_MEDIUM {
        FrictionLevel::Medium
    } else {
        // 有可信证据但既无失败也无（足量）部分失败 → low（不是 unknown）。
        FrictionLevel::Low
    }
}

/// §M2-D：确定性选取「当前摩擦主体」的**单一显式比较器**。
///
/// 方向全部就地写明，**不使用** final `.reverse()` —— 后者会把已经反向的 tie-break
/// 连同主排序一起翻转，导致「更少失败 / 更旧证据 / 更大 id」被优先，违背锁定意图。
///
/// 顺序（从高到低）：等级 DESC → 失败数 DESC → 连续失败 DESC → 部分失败 DESC
/// → 最近时间 DESC → id ASC。
fn select_friction_subject(acc: &BTreeMap<i64, SubjectAccum>) -> Option<i64> {
    let mut subjects: Vec<i64> = acc.keys().copied().collect();
    subjects.sort_by(|x, y| {
        let ax = &acc[x];
        let ay = &acc[y];
        level_of(ay)
            .rank()
            .cmp(&level_of(ax).rank())
            .then_with(|| ay.failed.cmp(&ax.failed))
            .then_with(|| ay.consecutive_failed.cmp(&ax.consecutive_failed))
            .then_with(|| ay.partial.cmp(&ax.partial))
            .then_with(|| ay.latest_at.cmp(&ax.latest_at))
            .then_with(|| x.cmp(&y))
    });
    subjects.into_iter().next()
}

/// §M2-D / §M2-E：把 support level 叠加到既有 0-LLM `prompt_variant` 上。
///
/// support 0 **原样返回** base —— 这样「无摩擦」路径与既有行为逐字节一致，
/// 不会因为引入 M2 而改变任何既有推荐语义。
pub fn support_prompt_variant(base: &str, support: u8) -> String {
    if support == SUPPORT_FREE_RECALL {
        base.to_string()
    } else {
        format!("{}+support{}", base, support)
    }
}

/// §M2-E：support > 0 时替换为更轻的引导文案；support 0 返回 `None`
/// （表示「沿用动作自身模板」，不额外改写）。
pub fn support_instruction(support: u8, subject: &str) -> Option<String> {
    match support {
        SUPPORT_FREE_RECALL => None,
        // §M2-E：诚实回退 —— 缩小范围，绝不承诺并不存在的关键词提示 / 候选 / 上下文。
        SUPPORT_ONE_CUE => Some(if subject.is_empty() {
            SUPPORT_1_TEXT.to_string()
        } else {
            format!("先把范围缩小到「{}」的要点，再尝试自由回忆。", subject)
        }),
        // §M2-E：诚实回退 —— 先识别关键结论再自由回忆，并给出冷却提示（反锤击）。
        _ => Some(if subject.is_empty() {
            SUPPORT_2_TEXT.to_string()
        } else {
            format!(
                "关于「{}」，先识别关键结论再自由回忆，中间可稍微冷却、不要连续硬磕。",
                subject
            )
        }),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn acc(failed: i64, partial: i64, consecutive: i64) -> SubjectAccum {
        SubjectAccum {
            failed,
            partial,
            consecutive_failed: consecutive,
            ..Default::default()
        }
    }

    #[test]
    fn level_thresholds_are_locked() {
        assert_eq!(level_of(&acc(0, 0, 0)), FrictionLevel::Low);
        assert_eq!(level_of(&acc(1, 0, 1)), FrictionLevel::Medium);
        assert_eq!(level_of(&acc(0, 2, 0)), FrictionLevel::Medium);
        assert_eq!(level_of(&acc(2, 0, 2)), FrictionLevel::High);
        assert_eq!(level_of(&acc(1, 1, 2)), FrictionLevel::High);
    }

    #[test]
    fn support_level_mapping_is_locked() {
        assert_eq!(FrictionLevel::Unknown.support_level(), 0);
        assert_eq!(FrictionLevel::Low.support_level(), 0);
        assert_eq!(FrictionLevel::Medium.support_level(), 1);
        assert_eq!(FrictionLevel::High.support_level(), 2);
    }

    #[test]
    fn support_zero_keeps_base_variant_byte_identical() {
        assert_eq!(
            support_prompt_variant("retry_recent_error.last_step", 0),
            "retry_recent_error.last_step"
        );
        assert_eq!(
            support_prompt_variant("retry_recent_error.last_step", 1),
            "retry_recent_error.last_step+support1"
        );
        assert_eq!(support_instruction(0, "优先编码器"), None);
    }

    #[test]
    fn unrelated_subject_never_inherits_support() {
        let st = LearningFrictionState {
            level: FrictionLevel::High,
            subject_learning_item_id: Some(7),
            subject_label: Some("优先编码器".to_string()),
            signals: Vec::new(),
            recommended_support_level: 2,
            cooldown_until: Some("2099-01-01 00:00:00".to_string()),
        };
        // 同一主体 → 继承
        assert_eq!(st.support_level_for(Some(7)), 2);
        assert!(st.is_subject_in_cooldown(Some(7), "2026-01-01 00:00:00"));
        // 无关主体 → 恒 0，且不受冷却影响（不污染）
        assert_eq!(st.support_level_for(Some(8)), 0);
        assert_eq!(st.support_level_for(None), 0);
        assert!(!st.is_subject_in_cooldown(Some(8), "2026-01-01 00:00:00"));
    }

    #[test]
    fn cooldown_expires_deterministically() {
        let st = LearningFrictionState {
            level: FrictionLevel::High,
            subject_learning_item_id: Some(7),
            subject_label: None,
            signals: Vec::new(),
            recommended_support_level: 2,
            cooldown_until: Some("2026-09-16 02:00:00".to_string()),
        };
        // SQLite 格式
        assert!(st.is_cooldown_active("2026-09-16 01:59:59"));
        assert!(!st.is_cooldown_active("2026-09-16 02:00:00"));
        assert!(!st.is_cooldown_active("2026-09-16 02:00:01"));
        // chrono 格式（date::now_utc()）必须与 SQLite 格式等价 ——
        // 直接字符串比较会因 `'T' > ' '` 得出相反结论，故这条是防回归的关键断言。
        assert!(st.is_cooldown_active("2026-09-16T01:59:59Z"));
        assert!(!st.is_cooldown_active("2026-09-16T02:00:01Z"));
    }

    // ---- P1-01：摩擦主体选取的每一级 tie-break 都必须方向正确（无 final .reverse()）----
    #[test]
    fn friction_subject_selection_tie_breaks() {
        use std::collections::BTreeMap;

        // 等级优先：High 压过 Low。
        // Low 的正确构造 = failed=0 且 partial<PARTIAL_FOR_MEDIUM；
        // 注意 failed>=FAILED_FOR_HIGH(2) 即 High —— 夹具曾误用 failed=9 当 Low。
        let mut a = BTreeMap::new();
        a.insert(
            1,
            SubjectAccum {
                failed: 0,
                partial: 1,
                ..Default::default()
            },
        ); // Low
        a.insert(
            2,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 2,
                ..Default::default()
            },
        ); // High
        assert_eq!(select_friction_subject(&a), Some(2), "higher level wins");

        // 同等级（Medium）→ 失败数多者优先。
        // Medium 内 failed 只能取 0（需 partial>=PARTIAL_FOR_MEDIUM）或 1：
        // failed>=2 即升 High，无法用 failed=1 vs 3 构造同等级（旧夹具错误）。
        let mut b = BTreeMap::new();
        b.insert(
            1,
            SubjectAccum {
                failed: 0,
                partial: 2,
                ..Default::default()
            },
        ); // Medium（failed=0，partial 达 Medium 阈）
        b.insert(
            2,
            SubjectAccum {
                failed: 1,
                ..Default::default()
            },
        ); // Medium
        assert_eq!(select_friction_subject(&b), Some(2), "more failures wins");

        // 同等级同失败 → 连续失败多者优先
        let mut c = BTreeMap::new();
        c.insert(
            1,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 1,
                ..Default::default()
            },
        );
        c.insert(
            2,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 2,
                ..Default::default()
            },
        );
        assert_eq!(
            select_friction_subject(&c),
            Some(2),
            "more consecutive failures wins"
        );

        // 同等级同失败同连续 → 部分失败多者优先
        let mut d = BTreeMap::new();
        d.insert(
            1,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 1,
                partial: 1,
                ..Default::default()
            },
        );
        d.insert(
            2,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 1,
                partial: 3,
                ..Default::default()
            },
        );
        assert_eq!(select_friction_subject(&d), Some(2), "more partial wins");

        // 同等级同失败同连续同部分 → 最近时间（latest_at DESC）优先
        let mut e = BTreeMap::new();
        e.insert(
            1,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 1,
                partial: 1,
                latest_at: Some("2026-01-01 00:00:00".to_string()),
                ..Default::default()
            },
        );
        e.insert(
            2,
            SubjectAccum {
                failed: 2,
                consecutive_failed: 1,
                partial: 1,
                latest_at: Some("2026-01-02 00:00:00".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(select_friction_subject(&e), Some(2), "newer latest_at wins");

        // 全部相同 → 最小 id 优先（id ASC）
        let mut f = BTreeMap::new();
        f.insert(
            5,
            SubjectAccum {
                failed: 1,
                ..Default::default()
            },
        );
        f.insert(
            3,
            SubjectAccum {
                failed: 1,
                ..Default::default()
            },
        );
        assert_eq!(
            select_friction_subject(&f),
            Some(3),
            "smaller id wins on full tie"
        );
    }

    // ---- P1-02：support 文案必须诚实，不得承诺不存在的线索/候选/上下文 ----
    #[test]
    fn support0_keeps_instruction_unchanged() {
        // FR-S01：support 0 不改写 instruction
        assert_eq!(support_instruction(0, "优先编码器"), None);
    }

    #[test]
    fn support1_is_honest_fallback_without_missing_cue() {
        // FR-S02 / FR-S04：support1 提供诚实回退，且不承诺缺失的「提示/候选/上下文」
        let s = support_instruction(1, "优先编码器").expect("support1 yields text");
        assert!(
            !s.contains("提示"),
            "must not claim a missing keyword hint: {}",
            s
        );
        assert!(
            !s.contains("候选"),
            "must not claim missing candidates: {}",
            s
        );
        assert!(
            !s.contains("上下文"),
            "must not claim missing context: {}",
            s
        );
        assert!(
            s.contains("优先编码器"),
            "should reference the grounded subject: {}",
            s
        );
    }

    #[test]
    fn support2_is_honest_fallback_without_missing_cue() {
        // FR-S03 / FR-S04：support2 提供诚实回退，且不承诺缺失的「候选/上下文」
        let s = support_instruction(2, "优先编码器").expect("support2 yields text");
        assert!(
            !s.contains("候选"),
            "must not claim missing candidates: {}",
            s
        );
        assert!(
            !s.contains("上下文"),
            "must not claim missing context: {}",
            s
        );
        assert!(
            s.contains("优先编码器"),
            "should reference the grounded subject: {}",
            s
        );
    }

    #[test]
    fn support_with_empty_subject_falls_back_to_const_text() {
        // 无 grounded 主体时退回诚实常量文案（仍不承诺缺失线索）
        let s1 = support_instruction(1, "").expect("support1 yields text");
        assert_eq!(s1, SUPPORT_1_TEXT);
        let s2 = support_instruction(2, "").expect("support2 yields text");
        assert_eq!(s2, SUPPORT_2_TEXT);
    }
}
