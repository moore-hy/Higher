//! M4-E — 对白策略：**本地确定性模板**（0 Cloud）。
//!
//! 硬规则（§M4-E）：
//!
//! - 常规 companion 对白 = 本地确定性模板；
//! - 变体由**稳定 seed/context** 选出，**不是**不受控的随机刷屏；
//! - 以下场景**绝不**调用 Cloud：`hello` / `welcome back` / `micro complete` /
//!   `session complete` / `expedition return`。
//!
//! 文案纪律（与 §M3-D / §M2-G 同源）：
//!
//! - 只陈述可验证事实，不做羞辱式播报；
//! - 卡住时给**陪伴与坚持**，不是「你怎么又错了」；
//! - 谢绝学习时立刻接受、零内疚、不追第二次（§M4-G）。

use crate::companion::deterministic::pick_index;
use crate::companion::types::{CompanionDialogue, DialogueEvent};

/// 每个事件的模板变体（≥3 条，避免可感知的复读）。
fn variants(event: DialogueEvent) -> &'static [&'static str] {
    match event {
        DialogueEvent::ReturnAfterBreak => &[
            "你回来啦。我把这段路记下来了。",
            "好一阵没见了。我还在，东西都没动。",
            "你回来了。我先陪你待一会儿。",
        ],
        DialogueEvent::FirstVisitToday => &[
            "今天也见面了。",
            "新的一天。我在。",
            "早。今天从哪儿开始都行。",
        ],
        DialogueEvent::MicroComplete => &[
            "刚才那一下就记住了。",
            "记下了。这一小步算数。",
            "嗯，这段我已经收好了。",
        ],
        DialogueEvent::SessionComplete => &[
            "这一段时间很扎实，我都看着。",
            "完成了一段。休息一下吧。",
            "刚才那段很稳，我记下来了。",
        ],
        DialogueEvent::DifficultAttempt => &[
            "这个点你回来试了好几次了。不急。",
            "还没通，但你没放下它 —— 这本身就难得。",
            "它现在难，不代表它会一直难。",
        ],
        DialogueEvent::ExpeditionStart => &[
            "我出发了，回来给你带点东西。",
            "去看看。很快回来。",
            "这段路我熟，放心。",
        ],
        DialogueEvent::ExpeditionReturn => &[
            "我回来了，带了点小东西。",
            "路上捡到一样东西，给你。",
            "回来了。这次走得挺远。",
        ],
        DialogueEvent::Recovery => &[
            "最近的节奏有点紧。先歇一歇也行。",
            "我看到你有点累了。不催你。",
            "慢一点没关系，我在旁边。",
        ],
        DialogueEvent::DeclineLearning => &[
            "好，那就不做。我就陪你待着。",
            "行，今天先这样。",
            "收到，不勉强。",
        ],
    }
}

/// 确定性选出一条对白。
///
/// `context` 由调用方给出（例如 `local_date`、远征 theme、tier），
/// 参与哈希 → 不同上下文/不同日子会自然换语气，但**同输入恒同输出**。
pub fn dialogue(profile_id: i64, event: DialogueEvent, context: &[&str]) -> CompanionDialogue {
    let pool = variants(event);
    let mut parts: Vec<String> = vec![
        format!("profile:{}", profile_id),
        format!("event:{}", event.as_str()),
    ];
    parts.extend(context.iter().map(|c| (*c).to_string()));
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();

    let idx = pick_index(&refs, pool.len());
    CompanionDialogue {
        event: event.as_str().to_string(),
        variant: idx as i64,
        text: pool[idx].to_string(),
    }
}

/// §M4-G 的主动学习邀请文案（确定性；来源动作由 canonical 学习状态给出）。
///
/// 风格参照任务书示例「都来了，要不要顺手做一个很小的？」，但**不重复同一句**。
pub fn nudge_text(profile_id: i64, action_title: &str, context: &[&str]) -> String {
    const POOL: [&str; 4] = [
        "都来了，要不要顺手做一个很小的？",
        "既然来了 —— 有一个很小的，要试试吗？",
        "在的话，顺手推进一点点也好。",
        "要不要先做那个最小的？做完就随你。",
    ];
    let mut parts: Vec<String> = vec![format!("profile:{}", profile_id), "nudge".to_string()];
    parts.extend(context.iter().map(|c| (*c).to_string()));
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    let idx = pick_index(&refs, POOL.len());
    // 明确引用 canonical 动作的标题：Companion 不自己编学习任务。
    format!("{}（{}）", POOL[idx], action_title)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn dialogue_is_deterministic_for_same_input() {
        let a = dialogue(7, DialogueEvent::ExpeditionReturn, &["2026-09-16"]);
        let b = dialogue(7, DialogueEvent::ExpeditionReturn, &["2026-09-16"]);
        assert_eq!(a.text, b.text);
        assert_eq!(a.variant, b.variant);
        assert_eq!(a.event, "expedition_return");
    }

    #[test]
    fn every_event_has_multiple_variants() {
        for e in [
            DialogueEvent::ReturnAfterBreak,
            DialogueEvent::FirstVisitToday,
            DialogueEvent::MicroComplete,
            DialogueEvent::SessionComplete,
            DialogueEvent::DifficultAttempt,
            DialogueEvent::ExpeditionStart,
            DialogueEvent::ExpeditionReturn,
            DialogueEvent::Recovery,
            DialogueEvent::DeclineLearning,
        ] {
            assert!(
                variants(e).len() >= 3,
                "§M4-E：每个事件都需要多个变体以避免可感知复读（{:?}）",
                e
            );
        }
    }

    #[test]
    fn different_profiles_spread_across_variants() {
        let picked: std::collections::BTreeSet<i64> = (1..60)
            .map(|p| dialogue(p, DialogueEvent::FirstVisitToday, &["2026-09-16"]).variant)
            .collect();
        assert!(
            picked.len() >= 2,
            "变体必须真的会变化（不是恒选第 0 条）"
        );
    }
}
