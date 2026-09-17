//! HOTFIX-01 FIX H —— 命令栏自由文本 → `ActiveLearningIntent` 的**确定性**捕获。
//!
//! # 为什么必须是确定性的
//!
//! 一个 `ActiveLearningIntent` 的决策优先级很高：它会改写 Decision Engine 的
//! `user_target` / `user_named_domain` / mode，进而影响接下来编排什么、学什么。
//! 用 LLM 去猜「这句话是不是想学习」，等于把「今天学什么」交给一个不可复现、
//! 不可审计、且会随模型版本漂移的东西。所以这里是一条**纯函数**规则链：
//! 同样的输入永远得到同样的输出，且每一步都能说清为什么。
//!
//! HOTFIX-01 明令：**不得**用 LLM 绕过这条保守的确定性规则。
//!
//! # 唯一政策：宁可漏判，不可误判
//!
//! ```text
//! prefer FALSE NEGATIVE  over  FALSE POSITIVE
//! ```
//!
//! 漏判的代价是「这次没有主动意图，用户再说一次就行」；
//! 误判的代价是「用户只是抱怨了一句『我数学学得很差』，应用却把它记成
//! 『现在想学数学』，然后开始给他安排数学训练」。后者是**替用户决定**，
//! 而且用户很难意识到发生了什么。两者不对称，所以规则一律向保守一侧倒。
//!
//! # 判定顺序（H1 锁定 + O2 M0 收窄）
//!
//! ```text
//! 1. 否定 / 非意图守卫
//! 1b. 通用求助守卫（O2 M0 新增）
//! 2. 显式 AUTOPILOT 短语
//! 3. 领域词 + 当前学习意图动词
//! 4. 不写
//! ```
//!
//! 顺序本身就是语义：`我不想学数学` 同时命中了否定词和领域词，
//! 但因为第 1 步先跑，结果是**不写**。
//!
//! # O2 M0 —— 为什么必须收窄「看」
//!
//! 收窄前 `看` 与 `学` 并列在 [`VERB_TOKENS_ZH`] 里，于是
//! `帮我看看这段代码为什么报错` 会因为「含领域词 `代码` + 含动词 `看`」
//! 被判成 `COPILOT + Programming`。这是**误判**：用户在求助排错，
//! 不是在说「我现在想学编程」。一旦写进 `ActiveLearningIntent`，
//! 它会改写 Decision Engine 的 `user_target` / mode，开始给用户安排编程训练 ——
//! 又一次「替用户决定」。
//!
//! 两处修改：
//!
//! 1. `看` 从 [`VERB_TOKENS_ZH`] 移除 —— 它不再是一个学习意图动词。
//! 2. 新增 [`ASSISTANCE_TOKENS`] 通用求助守卫，位于否定守卫之后、
//!    AUTOPILOT 之前：命中即**不写**，原因码
//!    [`REASON_ASSISTANCE_NOT_INTENT`]。
//!
//! 代价是明确的、也是刻意的：`我想学数学，顺便帮我看看这段代码`
//! 这类「真意图 + 求助」混合句会被整体判为不写。按唯一政策
//! `false negative > false positive`，这个代价必须付 ——
//! 漏判只是「用户再说一次」，误判是「应用擅自开始安排训练」。
//!
//! # 它绝不产生学习事实
//!
//! 本模块是**纯函数**，不碰数据库。写意图是调用方的事，而写意图
//! **不产生** LearningMoment / Evidence / 掌握度更新 ——
//! 「用户说他打算学数学」不是「用户学会了数学」。

use super::learning_domain::LearningDomain;

/// 捕获结果。`None` 表示**不写意图**（这是最常见、也是最安全的结果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedIntent {
    /// 用户把「学什么」的决策权交给了 Higher（H2）。
    Autopilot,
    /// 用户点名了一个领域，并且表达了**当前**的学习意图（H3 + H4）。
    Copilot(LearningDomain),
}

/// 一次捕获的完整结果：结论 + **为什么**。
///
/// 带上原因码是刻意的（§50「明确的『没有发生』优于沉默」）：
/// 「没有写意图」可能是「没有领域词」、也可能是「命中了否定守卫」，
/// 这两种情况在审计与排障时完全不同，不能都表现成一个 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentCapture {
    pub intent: Option<CapturedIntent>,
    /// 稳定原因码，永不为空。
    pub reason: &'static str,
}

/// 没有命中任何规则（最常见的结果）。
pub const REASON_NO_MATCH: &str = "no_deterministic_match";
/// 命中了否定守卫（H5）。
pub const REASON_NEGATION: &str = "negation_guard";
/// 命中了「能力 / 历史陈述」守卫（H6）。
pub const REASON_NOT_CURRENT_INTENT: &str = "history_or_ability_is_not_current_intent";
/// 命中了通用求助守卫（O2 M0）。
pub const REASON_ASSISTANCE_NOT_INTENT: &str = "generic_assistance_request_is_not_learning_intent";
/// 命中了显式安排短语（H2）。
pub const REASON_AUTOPILOT: &str = "explicit_arrangement_phrase";
/// 领域词 + 当前学习意图动词（H3 + H4）。
pub const REASON_DOMAIN_INTENT: &str = "domain_with_current_learning_intent";

// ============================ H5 否定守卫 ============================

/// 显式否定短语。命中**任意**一个 → 不写意图，即使领域词与学习动词都在。
///
/// 前 12 条是 HOTFIX-01 H5 逐字给出的；后面几条是同义的保守补充
/// （H5 的措辞是 "including but not limited to"）。补充的原则同样是
/// 「宁可漏判」：多一条否定词最多让一次真意图被忽略，少一条却会让
/// 「我不想看数学」被记成「现在想学数学」。
const NEGATION_TOKENS: [&str; 18] = [
    "不想学",
    "不学",
    "别安排",
    "不要安排",
    "不想复习",
    "不想练",
    "先别学",
    "暂时不学",
    "今天不学",
    "现在不学",
    "不用安排",
    "别让我学",
    // —— 同义补充 ——
    "不想看",
    "不想做",
    "不要学",
    "别看",
    "别学了",
    "先不学",
];

// ============================ H6 能力 / 历史守卫 ============================

/// 过去时间标记。出现即说明这句话在讲**过去**，而不是当前意图。
const HISTORY_TOKENS: [&str; 7] = ["以前", "之前", "曾经", "过去", "原来", "早就", "当年"];

/// 能力 / 水平评价。出现即说明这句话在讲**能力**，而不是当前意图。
///
/// 例：`我数学学得很差` —— 它同时含有领域词（数学）与学习动词（学），
/// 若不拦住，就会变成 `COPILOT + Mathematics`。而用户只是在陈述一件事，
/// 并没有要求现在开始学。
const ABILITY_TOKENS: [&str; 9] = [
    "很差",
    "很不好",
    "不好",
    "太差",
    "比较差",
    "有点差",
    "基础差",
    "不行",
    "跟不上",
];

/// 完成体标记（「学过 / 复习过」）。它描述的是**已经发生过**的事。
const PERFECTIVE_TOKENS: [&str; 7] = ["学过", "复习过", "练过", "做过", "看过", "背过", "读过"];

// ============================ O2 M0 通用求助守卫 ============================

/// 通用求助 / 排障动词。命中**任意**一个 → 不写意图。
///
/// 这些词描述的是「帮我把眼前这个东西弄好」，不是「我现在想学某领域」。
/// 它们与领域词高度共现（`代码` / `算法` / `英语`），所以误判风险最大：
///
/// ```text
/// 帮我看看这段代码为什么报错  -> 领域词 `代码`，收窄前会被误判为 COPILOT + Programming
/// 帮我改一下 Python 代码       -> 领域词 `python` / `代码`
/// 解释一下这个算法             -> 与 `数学`/`408` 共现时同理
/// 帮我看看英语翻译             -> 领域词 `英语`
/// ```
///
/// 逐字取自 O2 §11 的 MUST-NOT 清单。`看` 同时从动词表移除（见 [`VERB_TOKENS_ZH`]），
/// 这里保留它是为了让「看不是学习动词」这条规则在代码里**显式可读、可直接断言**，
/// 而不是靠「动词表里恰好没有它」这种隐式事实。
const ASSISTANCE_TOKENS: [&str; 12] = [
    "看",
    "看看",
    "帮我看",
    "解释",
    "说明",
    "修",
    "修改",
    "改",
    "调试",
    "debug",
    "排查",
    "报错",
];

// ============================ H2 显式安排短语 ============================

/// 用户**显式**把「学什么」的决定权交出来。
///
/// 命中即 `Autopilot`，并且优先级高于「领域词 + 动词」：`数学你来安排`
/// 里虽然点明了领域，但用户明确委托了选择权，所以是 Autopilot 而不是
/// `Copilot + Mathematics`（H1 的 `数学你来安排` 例子）。
const AUTOPILOT_TOKENS: [&str; 5] = [
    "帮我安排",
    "你来安排",
    "按我的状态安排",
    "不知道学什么",
    "你决定",
];

// ============================ H3 领域词 ============================

/// 领域词表（H3 逐字）。ASCII 词按**词边界**匹配，避免 `rust` 命中 `trust`。
const DOMAIN_TOKENS: [(&str, LearningDomain); 26] = [
    // English
    ("英语", LearningDomain::English),
    ("英文", LearningDomain::English),
    ("english", LearningDomain::English),
    ("cet4", LearningDomain::English),
    ("cet-4", LearningDomain::English),
    ("cet6", LearningDomain::English),
    ("四级", LearningDomain::English),
    ("六级", LearningDomain::English),
    // Mathematics
    ("数学", LearningDomain::Mathematics),
    ("高数", LearningDomain::Mathematics),
    ("线代", LearningDomain::Mathematics),
    ("概率", LearningDomain::Mathematics),
    ("math", LearningDomain::Mathematics),
    ("mathematics", LearningDomain::Mathematics),
    // ComputerScience408
    ("408", LearningDomain::ComputerScience408),
    ("数据结构", LearningDomain::ComputerScience408),
    ("操作系统", LearningDomain::ComputerScience408),
    ("组成原理", LearningDomain::ComputerScience408),
    ("计算机组成", LearningDomain::ComputerScience408),
    ("计算机网络", LearningDomain::ComputerScience408),
    // Programming
    ("编程", LearningDomain::Programming),
    ("代码", LearningDomain::Programming),
    ("programming", LearningDomain::Programming),
    ("coding", LearningDomain::Programming),
    ("rust", LearningDomain::Programming),
    ("python", LearningDomain::Programming),
];

/// 语言类领域词（单独一张表，因为它们的匹配方式与上面 26 条不同：
/// `java` 是 `javascript` 的前缀，必须靠词边界区分）。
const LANGUAGE_TOKENS: [(&str, LearningDomain); 5] = [
    ("java", LearningDomain::Programming),
    ("c++", LearningDomain::Programming),
    ("javascript", LearningDomain::Programming),
    ("typescript", LearningDomain::Programming),
    ("c#", LearningDomain::Programming),
];

// ============================ H4 当前学习意图动词 ============================

/// 中文动词。
///
/// O2 M0：`看` 已从本表**移除**。它是最典型的通用求助动词
/// （`帮我看看这段代码为什么报错` / `帮我看看英语翻译`），
/// 把它当学习意图动词会产生「用户求助排错 → 应用开始安排训练」的误判。
/// 参见 [`ASSISTANCE_TOKENS`]。
///
/// 保留的动词都要求用户表达**自己的学习行为**：学习 / 复习 / 练习 / 做题 / 练 / 学。
/// `学` 仍刻意放在最后 —— 它是最短、最容易误伤的一个，而且**必须**在领域词被剔除
/// 之后才允许参与判定（见 `capture_intent`）。
const VERB_TOKENS_ZH: [&str; 6] = ["学习", "复习", "练习", "做题", "练", "学"];

/// 英文动词。
const VERB_TOKENS_EN: [&str; 4] = ["study", "learn", "review", "practice"];

// ============================ 匹配原语 ============================

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

/// 拉丁词匹配：要求两侧都不是字母/数字。
///
/// 为什么需要它：`trust` 含有 `rust`、`javascript` 含有 `java`。
/// 用裸 `contains` 会让「I don't trust this」被记成「想学编程」。
fn contains_latin(haystack_lower: &str, token: &str) -> bool {
    let bytes = haystack_lower.as_bytes();
    let mut from = 0usize;
    while from < haystack_lower.len() {
        let Some(rel) = haystack_lower[from..].find(token) else {
            return false;
        };
        let start = from + rel;
        let end = start + token.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        // 拉丁 token 全为 ASCII，因此 `start + 1` 一定是字符边界。
        from = start + 1;
    }
    false
}

/// 统一的 token 命中：含非 ASCII 字符的 token 用子串匹配（中文没有词边界），
/// 纯 ASCII 的 token 用词边界匹配。
fn hits(haystack_lower: &str, token: &str) -> bool {
    if token.is_ascii() {
        contains_latin(haystack_lower, token)
    } else {
        haystack_lower.contains(token)
    }
}

fn any_hit(haystack_lower: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|t| hits(haystack_lower, t))
}

/// 命中任一领域词，返回第一个命中的领域。
///
/// 顺序即优先级：中文/精确词先于语言前缀词。这里**不**做「多领域取交集」
/// 之类的聪明事 —— 一次说两个领域属于歧义，而歧义按 H1 一律不写。
fn match_domain(haystack_lower: &str) -> Option<(LearningDomain, &'static str)> {
    for (token, domain) in DOMAIN_TOKENS {
        if hits(haystack_lower, token) {
            return Some((domain, token));
        }
    }
    for (token, domain) in LANGUAGE_TOKENS {
        if hits(haystack_lower, token) {
            return Some((domain, token));
        }
    }
    None
}

/// 命中**全部**领域词（用于「一次点了两个领域 → 歧义 → 不写」）。
fn all_domains(haystack_lower: &str) -> Vec<LearningDomain> {
    let mut found: Vec<LearningDomain> = Vec::new();
    for (token, domain) in DOMAIN_TOKENS.iter().chain(LANGUAGE_TOKENS.iter()) {
        if hits(haystack_lower, token) && !found.contains(domain) {
            found.push(*domain);
        }
    }
    found
}

/// 把命中的领域词从文本里剔除，再找动词。
///
/// # 为什么必须剔除
///
/// `数学` 这个**领域词本身含有动词字 `学`**。若不剔除，
/// 输入 `数学`（两个字，没有任何学习请求）会因为「含有 `学`」而被判成
/// `COPILOT + Mathematics` —— 一个典型的假阳性。
/// 剔除之后 `我现在想学数学` → 余下 `我现在想学` → 命中 `学` ✓，
/// 而 `数学` → 余下空串 → 不命中 ✓。
fn strip_domains(haystack_lower: &str) -> String {
    let mut out = haystack_lower.to_string();
    for (token, _) in DOMAIN_TOKENS.iter().chain(LANGUAGE_TOKENS.iter()) {
        if token.is_ascii() {
            // ASCII token 不做整词删除：`python` 删掉后剩 ` 我学过一点` 也能判；
            // 而误删（例如把 `trust` 里的 `rust` 删掉）只会让动词判定更保守，
            // 不影响安全性。
            out = out.replace(token, " ");
        } else {
            out = out.replace(token, "");
        }
    }
    out
}

// ============================ 唯一入口 ============================

/// 确定性捕获：这段文本表达了**当前**学习意图吗？
///
/// 判定顺序严格按 H1。返回的 [`IntentCapture`] 同时给出原因码，
/// 便于审计「为什么没写」。
///
/// 本函数是**纯函数**：不读库、不写库、不产生任何学习事实。
pub fn capture_intent(text: &str) -> IntentCapture {
    let haystack = text.trim().to_lowercase();

    // ---- 1. 否定 / 非意图守卫（最高优先级）----
    if haystack.is_empty() {
        return IntentCapture {
            intent: None,
            reason: REASON_NO_MATCH,
        };
    }
    if any_hit(&haystack, &NEGATION_TOKENS) {
        return IntentCapture {
            intent: None,
            reason: REASON_NEGATION,
        };
    }
    // 通用求助 / 排障请求：讲的是「帮我弄好眼前这个东西」，不是当前学习意图（O2 M0）。
    if any_hit(&haystack, &ASSISTANCE_TOKENS) {
        return IntentCapture {
            intent: None,
            reason: REASON_ASSISTANCE_NOT_INTENT,
        };
    }
    // 能力 / 历史 / 完成体陈述：讲的是**过去或水平**，不是当前请求（H6）。
    if any_hit(&haystack, &HISTORY_TOKENS)
        || any_hit(&haystack, &ABILITY_TOKENS)
        || any_hit(&haystack, &PERFECTIVE_TOKENS)
    {
        return IntentCapture {
            intent: None,
            reason: REASON_NOT_CURRENT_INTENT,
        };
    }

    // ---- 2. 显式安排短语 → AUTOPILOT ----
    if any_hit(&haystack, &AUTOPILOT_TOKENS) {
        return IntentCapture {
            intent: Some(CapturedIntent::Autopilot),
            reason: REASON_AUTOPILOT,
        };
    }

    // ---- 3. 领域词 + 当前学习意图动词 → COPILOT + domain ----
    //
    // 一次点名两个不同领域属于歧义 → 不写（H1：ambiguous → NO INTENT WRITE）。
    let domains = all_domains(&haystack);
    if domains.len() > 1 {
        return IntentCapture {
            intent: None,
            reason: REASON_NO_MATCH,
        };
    }
    if let Some((domain, _)) = match_domain(&haystack) {
        let remainder = strip_domains(&haystack);
        if any_hit(&remainder, &VERB_TOKENS_ZH) || any_hit(&remainder, &VERB_TOKENS_EN) {
            return IntentCapture {
                intent: Some(CapturedIntent::Copilot(domain)),
                reason: REASON_DOMAIN_INTENT,
            };
        }
    }

    // ---- 4. 不写 ----
    IntentCapture {
        intent: None,
        reason: REASON_NO_MATCH,
    }
}

/// 只要结论、不要原因码时的便捷封装。
pub fn capture_learning_intent_from_text(text: &str) -> Option<CapturedIntent> {
    capture_intent(text).intent
}
