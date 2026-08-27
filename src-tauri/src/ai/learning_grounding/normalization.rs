//! DEV-0077.4-A.1 · 保守 Normalization（§二十二）。
//!
//! 只做：trim / 连续空白压缩 / ASCII case fold。
//! 禁止语义改写（「极限」→「函数极限」不允许）。

/// §二十二：normalize 名称（唯一性语义 = Profile + Resolved Parent + Normalized Name）。
pub fn normalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_ws = false;
    for ch in name.trim().chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            // ASCII case fold（仅 ASCII；CJK 等保持原样）
            if ch.is_ascii_alphabetic() {
                out.extend(ch.to_lowercase());
            } else {
                out.push(ch);
            }
            in_ws = false;
        }
    }
    out.trim().to_string()
}

/// ref_key 规范化（trim + ASCII lower；ref_key 供系统使用，不展示给用户 §九十八）。
pub fn normalize_ref_key(key: &str) -> String {
    key.trim().to_ascii_lowercase()
}
