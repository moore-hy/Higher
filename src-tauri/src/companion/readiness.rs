//! M5-C — 远征就绪度 + M5-D 主题：**纯函数式、确定性、0 LLM** 策略。
//!
//! ## §M5-C 就绪度
//!
//! 就绪度由 M3 `MeaningfulLearningContribution` 派生。**它不是余额**：
//!
//! - 没有 `energy balance` / `+N energy` / `spend N energy` / `learning coins` /
//!   `fuel wallet` 这些面向用户的概念；
//! - 内部有界数值只作为**实现细节**用于派生就绪度（任务书明确允许）。
//!
//! 锁定映射（`today_total` 已由 M3 封顶 = 40）：
//!
//! ```text
//! < 5    → NOT_READY    （远征不可用）
//! >= 5   → READY_SHORT  （20m）
//! >= 15  → READY_MEDIUM （20m / 60m）
//! >= 30  → READY_LONG   （20m / 60m / 3h）
//! ```
//!
//! **绝不**从以下来源产生就绪度（§M5-C 末段，逐条由结构保证）：
//! 打开 App 时长 / 点宠物 / 空转时长 / skipped Micro / 后台常驻 / 无验证的计时器时间。
//! 这些都**不会**提升 M3 `today_total`（见 `learning_state::contribution`），
//! 因此在此处天然无法抬高就绪度。
//!
//! ## 结算（settle）
//!
//! 「开始一次远征」即按**一条**确定性策略结算当前机会：
//! 存在**未收口**的远征（running 或 ready 未收取）时，就绪度被占位为 `NOT_READY`。
//! 这样无需任何余额列，也无需后台 tick。
//!
//! ## §M5-D 主题
//!
//! 只在能从既有学习内容**高置信**推断时才用具体主题，否则 `General`。
//! 主题只改变故事/收藏风味，**不**改变掌握度、**不**给学习增益。

use crate::companion::types::{
    ExpeditionReadiness, THEMES, THEME_ELECTRONICS, THEME_ENGLISH, THEME_GENERAL, THEME_MATH,
    THEME_PROGRAMMING,
};

/// `today_total >= 5` → READY_SHORT
pub const READINESS_SHORT_MIN: i64 = 5;
/// `today_total >= 15` → READY_MEDIUM
pub const READINESS_MEDIUM_MIN: i64 = 15;
/// `today_total >= 30` → READY_LONG
pub const READINESS_LONG_MIN: i64 = 30;

/// 主题推断时最多考察的学习项数量（RAM-light，且不引入全表扫描）。
pub const THEME_SCAN_LIMIT: usize = 200;

/// §M5-C：由 M3 贡献派生就绪度（纯函数）。
pub fn readiness_from_contribution(today_total: i64) -> ExpeditionReadiness {
    if today_total >= READINESS_LONG_MIN {
        ExpeditionReadiness::ReadyLong
    } else if today_total >= READINESS_MEDIUM_MIN {
        ExpeditionReadiness::ReadyMedium
    } else if today_total >= READINESS_SHORT_MIN {
        ExpeditionReadiness::ReadyShort
    } else {
        ExpeditionReadiness::NotReady
    }
}

/// §M7 / P0-01：就绪度消费水位线（纯函数）。
///
/// 给定「当前学习日累计贡献」「上一次出发所消耗的学习日与当时快照值」，返回
/// **尚未被当前远征机会兑现**的贡献量，用以派生就绪度。
///
/// 语义：
/// - 无任何消费纪录（`consumed_local_date = None`）→ 全额派生（v033 存量数据的安全默认）。
/// - 同一学习日 → `max(0, today_total - consumed_total)`：已兑现的证据不再重复生成就绪度。
/// - 跨学习日 → 全额派生：`today_total` 本身已按本地学习日重新归零，绝不拿今天与昨天旧水位相减。
///
/// 这是「最小前向安全」机制：**只做减法、永不为负、绝不建模余额/币/燃料钱包**。
pub fn unconsumed_contribution(
    today_total: i64,
    consumed_local_date: &Option<String>,
    consumed_total: i64,
    current_local_date: &str,
) -> i64 {
    match consumed_local_date {
        None => today_total,
        Some(date) if date == current_local_date => (today_total - consumed_total).max(0),
        Some(_) => today_total,
    }
}

/// §M5-C：结算后的**有效**就绪度。
///
/// 只要还有未收口的远征（进行中，或已完成但未收取），就绪度即被占位为 `NOT_READY`
/// —— 「开始远征结算了当前的机会」，且不需要任何余额列。
pub fn effective_readiness(
    derived: ExpeditionReadiness,
    has_open_expedition: bool,
) -> ExpeditionReadiness {
    if has_open_expedition {
        ExpeditionReadiness::NotReady
    } else {
        derived
    }
}

/// 主题关键词表（大小写不敏感的子串匹配）。
///
/// 刻意**保守**：只在明确命中时才给具体主题，否则 `General`（§M5-D
/// 「Do not call Cloud just to classify a simple theme」）。
const THEME_KEYWORDS: [(&str, &[&str]); 4] = [
    (
        THEME_ENGLISH,
        &[
            "english",
            "英语",
            "英文",
            "单词",
            "词汇",
            "vocab",
            "grammar",
            "语法",
            "听力",
            "listening",
            "翻译",
            "translation",
            "reading",
            "阅读",
            "toefl",
            "ielts",
            "cet",
            "四级",
            "六级",
            "专四",
            "专八",
        ],
    ),
    (
        THEME_MATH,
        &[
            "math",
            "数学",
            "代数",
            "algebra",
            "几何",
            "geometry",
            "微积分",
            "calculus",
            "概率",
            "统计",
            "statistics",
            "probability",
            "线性代数",
            "矩阵",
        ],
    ),
    (
        THEME_PROGRAMMING,
        &[
            "编程",
            "代码",
            "programming",
            "code",
            "rust",
            "python",
            "java",
            "javascript",
            "typescript",
            "算法",
            "algorithm",
            "数据结构",
            "git",
            "sql",
            "编译器",
        ],
    ),
    (
        THEME_ELECTRONICS,
        &[
            "电子",
            "electronics",
            "电路",
            "circuit",
            "模电",
            "数电",
            "单片机",
            "mcu",
            "fpga",
            "信号",
            "放大器",
            "电阻",
            "电容",
            "verilog",
            "spice",
            "编码器",
        ],
    ),
];

/// §M5-D：从学习项名称推断主题（**最高命中数**；并列 → `THEMES` 的固定顺序）。
///
/// 无法高置信推断 → `General`。
pub fn infer_theme(item_names: &[String]) -> &'static str {
    let mut hits: Vec<(usize, &'static str)> = Vec::new();
    for (theme, keywords) in THEME_KEYWORDS.iter() {
        let mut n = 0usize;
        for name in item_names.iter().take(THEME_SCAN_LIMIT) {
            let lower = name.to_lowercase();
            if keywords.iter().any(|k| lower.contains(&k.to_lowercase())) {
                n += 1;
            }
        }
        hits.push((n, theme));
    }
    let best = hits
        .iter()
        .max_by(|a, b| {
            // 命中数降序；并列时保持 THEME_KEYWORDS 的固定顺序（稳定）。
            a.0.cmp(&b.0)
        })
        .copied()
        .unwrap_or((0, THEME_GENERAL));
    if best.0 == 0 {
        THEME_GENERAL
    } else {
        best.1
    }
}

/// 主题必须落在有限枚举内（防御性：落库前校验）。
pub fn normalize_theme(raw: &str) -> &'static str {
    THEMES
        .iter()
        .find(|t| **t == raw)
        .copied()
        .unwrap_or(THEME_GENERAL)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn readiness_thresholds_are_locked() {
        assert_eq!(
            readiness_from_contribution(0),
            ExpeditionReadiness::NotReady
        );
        assert_eq!(
            readiness_from_contribution(4),
            ExpeditionReadiness::NotReady
        );
        assert_eq!(
            readiness_from_contribution(5),
            ExpeditionReadiness::ReadyShort
        );
        assert_eq!(
            readiness_from_contribution(14),
            ExpeditionReadiness::ReadyShort
        );
        assert_eq!(
            readiness_from_contribution(15),
            ExpeditionReadiness::ReadyMedium
        );
        assert_eq!(
            readiness_from_contribution(29),
            ExpeditionReadiness::ReadyMedium
        );
        assert_eq!(
            readiness_from_contribution(30),
            ExpeditionReadiness::ReadyLong
        );
        assert_eq!(
            readiness_from_contribution(40),
            ExpeditionReadiness::ReadyLong
        );
    }

    #[test]
    fn open_expedition_settles_readiness() {
        assert_eq!(
            effective_readiness(ExpeditionReadiness::ReadyLong, true),
            ExpeditionReadiness::NotReady
        );
        assert_eq!(
            effective_readiness(ExpeditionReadiness::ReadyLong, false),
            ExpeditionReadiness::ReadyLong
        );
    }

    #[test]
    fn readiness_availability_is_locked() {
        assert!(ExpeditionReadiness::NotReady
            .available_durations()
            .is_empty());
        assert_eq!(
            ExpeditionReadiness::ReadyShort.available_durations(),
            vec![20 * 60]
        );
        assert_eq!(
            ExpeditionReadiness::ReadyMedium.available_durations(),
            vec![20 * 60, 60 * 60]
        );
        assert_eq!(
            ExpeditionReadiness::ReadyLong.available_durations(),
            vec![20 * 60, 60 * 60, 3 * 60 * 60]
        );
        assert!(ExpeditionReadiness::ReadyShort.allows(20 * 60));
        assert!(!ExpeditionReadiness::ReadyShort.allows(60 * 60));
        assert!(!ExpeditionReadiness::ReadyLong.allows(90));
    }

    #[test]
    fn theme_inference_is_conservative() {
        assert_eq!(infer_theme(&[]), THEME_GENERAL);
        assert_eq!(infer_theme(&["随便什么".to_string()]), THEME_GENERAL);
        assert_eq!(infer_theme(&["英语四级词汇".to_string()]), THEME_ENGLISH);
        assert_eq!(infer_theme(&["线性代数".to_string()]), THEME_MATH);
        assert_eq!(infer_theme(&["Rust 编程".to_string()]), THEME_PROGRAMMING);
        assert_eq!(infer_theme(&["电路分析".to_string()]), THEME_ELECTRONICS);
        // 多数票优先（英文命中 2 项 vs 数学 1 项）
        assert_eq!(
            infer_theme(&[
                "英语单词".to_string(),
                "听力训练".to_string(),
                "数学".to_string()
            ]),
            THEME_ENGLISH
        );
    }

    #[test]
    fn unknown_theme_normalizes_to_general() {
        assert_eq!(normalize_theme("Klingon"), THEME_GENERAL);
        assert_eq!(normalize_theme(THEME_MATH), THEME_MATH);
    }
}
