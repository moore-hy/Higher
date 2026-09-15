//! M4 — 确定性工具：**唯一**的「稳定伪随机」来源。
//!
//! Companion 的变体选择（对白 / 故事 / 收藏 / seed）**必须**可复算：
//! 同一档案 + 同一事件 + 同一学习日 → 恒得同一结果。
//! 因此本模块只提供纯函数式哈希，**不**使用 `rand`、系统时间或任何全局状态。
//!
//! 算法：FNV-1a 64bit（实现自包含，不引入依赖；跨平台稳定）。
//! 返回 `i64`（去掉符号位）以便直接落库为 `companion_expeditions.seed`。

/// FNV-1a 64bit。
pub fn stable_hash(parts: &[&str]) -> i64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h: u64 = OFFSET;
    for (i, p) in parts.iter().enumerate() {
        // 段间插入分隔符，避免 ["ab","c"] 与 ["a","bc"] 碰撞。
        if i > 0 {
            h ^= 0x1f;
            h = h.wrapping_mul(PRIME);
        }
        for b in p.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
    }
    // 清掉符号位：保证跨平台一致的**非负**整数。
    (h & 0x7fff_ffff_ffff_ffff) as i64
}

/// 在 `len` 个变体里确定性地选一个（`len == 0` → 0）。
pub fn pick_index(parts: &[&str], len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (stable_hash(parts) % len as i64) as usize
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_order_sensitive() {
        assert_eq!(stable_hash(&["a", "b"]), stable_hash(&["a", "b"]));
        assert_ne!(stable_hash(&["a", "b"]), stable_hash(&["b", "a"]));
        // 分隔符防碰撞
        assert_ne!(stable_hash(&["ab", "c"]), stable_hash(&["a", "bc"]));
    }

    #[test]
    fn hash_is_non_negative() {
        for i in 0..500 {
            let s = format!("profile:{}", i);
            assert!(stable_hash(&[s.as_str()]) >= 0);
        }
    }

    #[test]
    fn pick_index_is_in_range_and_deterministic() {
        for i in 0..200 {
            let s = format!("e{}", i);
            let a = pick_index(&[s.as_str()], 3);
            let b = pick_index(&[s.as_str()], 3);
            assert_eq!(a, b);
            assert!(a < 3);
        }
        assert_eq!(pick_index(&["x"], 0), 0);
    }
}
