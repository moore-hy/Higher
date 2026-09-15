//! M5-E — 返回结果：**确定性**的短故事片段 / 收藏 / 场景记忆。
//!
//! 硬规则：
//!
//! - 同一 `seed` + 同一档位 + 同一主题 → **恒得同一结果**（§M5-E / CW-06）；
//! - 结果**只**存在于 companion 侧：写 `companion_memories`，**不**写任何学习表、
//!   **不**改掌握度、**不**给学习增益（§M5-E「No power equipment / No pay-to-win
//!   learning bonus」）；
//! - 只有文本，没有二进制资产（§M4-C 结尾）。
//!
//! 主题（§M5-D）只改变**风味**：故事与收藏的措辞，不改变任何数值。

use crate::companion::readiness::normalize_theme;
use crate::companion::types::{
    ExpeditionReadiness, THEME_ELECTRONICS, THEME_ENGLISH, THEME_MATH, THEME_PROGRAMMING,
};

/// 一次返回带来的确定性结果（落 `companion_memories`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryFragment {
    pub kind: String,
    pub title: String,
    pub body: String,
}

/// 收藏/记忆的 kind（有限枚举）。
pub const MEMORY_KIND_RETURN: &str = "expedition_return";

/// 收藏物品名池（按主题风味）。
fn collectible_pool(theme: &str) -> &'static [&'static str] {
    match theme {
        THEME_ENGLISH => &[
            "一枚刻着陌生词根的碎石",
            "一页被风翻开的旧词典残页",
            "一枚念起来很顺的小铃",
            "一张写满插画的明信片",
        ],
        THEME_MATH => &[
            "一颗天然六棱的小水晶",
            "一枚纹路对称的贝壳",
            "一段弯曲得很有规律的藤蔓",
            "一颗总是滚成同一种轨迹的石子",
        ],
        THEME_PROGRAMMING => &[
            "一枚咬合得很整齐的齿轮",
            "一段打结又解开的绳环",
            "一颗表面有刻痕的黑色石子",
            "一小卷绕得极整的细线",
        ],
        THEME_ELECTRONICS => &[
            "一片闪着微弱红光的小金属片",
            "一小段会共鸣的空心细管",
            "一颗吸住了细沙的磁石",
            "一枚薄得透光的小圆片",
        ],
        // General：不假装知道领域（§M5-D：无法高置信推断 → General）
        _ => &[
            "一颗被水磨圆的小石头",
            "一片叶脉很清楚的落叶",
            "一小截带着松香的木枝",
            "一粒混着细沙的透明碎石",
        ],
    }
}

/// 场景记忆正文池（与档位相关：走得越久，回来的描述越远）。
fn memory_body_pool(tier: ExpeditionReadiness) -> &'static [&'static str] {
    match tier {
        ExpeditionReadiness::ReadyLong => &[
            "它走了很远。回来时身上带着远处才有的凉气，趴在门口好一会儿没动。",
            "这一趟跨过了好几道坡。它没多说什么，只是把带回来的东西放在你手边。",
            "很久。你几乎以为它不回来了 —— 然后门响了一下。",
        ],
        ExpeditionReadiness::ReadyMedium => &[
            "它去了一整个下午能走到的边界，回来时有点喘。",
            "路程刚好够它想起点什么。它把东西放下，靠在你旁边。",
            "它绕了一圈，把路上看见的都记住了。",
        ],
        // READY_SHORT（以及任何不认识的档位，防御性收敛到最短档）
        _ => &[
            "它没走远，就在屋子边上转了一圈。",
            "很快。它回来时甚至没出汗。",
            "短的一趟，但它还是带了东西给你。",
        ],
    }
}

/// 确定性生成一次返回结果。
///
/// `seed` 由远征创建时算定（`profile_id + started_at + tier + duration`），
/// 因此重放同一条远征记录恒得同一故事 —— 这是 CW-06 的依据。
pub fn fragment(seed: i64, tier: ExpeditionReadiness, theme: &str) -> StoryFragment {
    let theme = normalize_theme(theme);
    // 用不同偏移量取两个独立的变体，避免 title/body 恒同时变化。
    let items = collectible_pool(theme);
    let ti = ((seed % items.len() as i64) + items.len() as i64) as usize % items.len();
    let bodies = memory_body_pool(tier);
    let bi = (((seed / 7) % bodies.len() as i64) + bodies.len() as i64) as usize % bodies.len();

    StoryFragment {
        kind: MEMORY_KIND_RETURN.to_string(),
        title: items[ti].to_string(),
        body: bodies[bi].to_string(),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::companion::types::THEME_GENERAL;

    #[test]
    fn same_seed_same_fragment() {
        for seed in [0i64, 1, 42, 999_999] {
            let a = fragment(seed, ExpeditionReadiness::ReadyMedium, THEME_MATH);
            let b = fragment(seed, ExpeditionReadiness::ReadyMedium, THEME_MATH);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn negative_seed_is_handled() {
        // 防御性：seed 理论上恒非负，但绝不 panic / 越界。
        let f = fragment(-5, ExpeditionReadiness::ReadyShort, THEME_GENERAL);
        assert!(!f.title.is_empty());
        assert!(!f.body.is_empty());
    }

    #[test]
    fn theme_changes_flavor_only() {
        let a = fragment(3, ExpeditionReadiness::ReadyShort, THEME_ENGLISH);
        let b = fragment(3, ExpeditionReadiness::ReadyShort, THEME_MATH);
        assert_ne!(a.title, b.title, "不同主题应给不同风味");
        assert_eq!(a.body, b.body, "档位相同 → 场景记忆正文相同（只有风味变）");
        assert_eq!(a.kind, b.kind);
    }

    #[test]
    fn unknown_theme_falls_back_to_general() {
        let a = fragment(11, ExpeditionReadiness::ReadyShort, "Klingon");
        let b = fragment(11, ExpeditionReadiness::ReadyShort, THEME_GENERAL);
        assert_eq!(a, b);
    }

    #[test]
    fn long_trip_reads_differently_from_short_trip() {
        let s = fragment(5, ExpeditionReadiness::ReadyShort, THEME_GENERAL);
        let l = fragment(5, ExpeditionReadiness::ReadyLong, THEME_GENERAL);
        assert_ne!(s.body, l.body);
    }
}
