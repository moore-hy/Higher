//! Session Note 结构化工具（DEV-0024 / BATCH-03）。
//!
//! LearningDocument：study_sessions.note 持久层继续为 TEXT，内容为两种形态：
//! - 旧形态：任意纯文本（完全向后兼容）
//! - 新形态（v2）：`{"v":2,"blocks":[{"t":"text","c":"…"},{"t":"image"|"video"|"drawing","a":<attachment_id>,"n":"文件名"}]}`
//!
//! 本模块提供：
//! - 解析（parse）：任意 note → blocks；非法/旧文本 → 单 text 块
//! - 纯文本提取（plain_text）：供 AI Context / Review 摘要（绝不把 JSON/token 发给模型）
//! - 文本字数（text_len）：Knowledge 学习记录的“笔记字数”
//! - 媒体 id 提取（media_ids）：统计图片/附件数量

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum NoteBlock {
    Text { c: String },
    Image { a: i64, #[serde(default)] n: String },
    Video { a: i64, #[serde(default)] n: String },
    Drawing { a: i64, #[serde(default)] n: String },
}

impl NoteBlock {
    pub fn attachment_id(&self) -> Option<i64> {
        match self {
            NoteBlock::Text { .. } => None,
            NoteBlock::Image { a, .. } | NoteBlock::Video { a, .. } | NoteBlock::Drawing { a, .. } => Some(*a),
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            NoteBlock::Text { .. } => "text",
            NoteBlock::Image { .. } => "image",
            NoteBlock::Video { .. } => "video",
            NoteBlock::Drawing { .. } => "drawing",
        }
    }
}

#[derive(Debug, Deserialize)]
struct V2Doc {
    v: u32,
    blocks: Vec<NoteBlock>,
}

/// 解析 note → blocks（任何输入都安全返回；旧纯文本 = 单 text 块）。
pub fn parse_blocks(note: Option<&str>) -> Vec<NoteBlock> {
    let note = match note {
        Some(n) if !n.trim().is_empty() => n.trim(),
        _ => return Vec::new(),
    };
    if !note.starts_with('{') {
        return vec![NoteBlock::Text { c: note.to_string() }];
    }
    match serde_json::from_str::<V2Doc>(note) {
        Ok(doc) if doc.v == 2 => doc.blocks,
        _ => vec![NoteBlock::Text { c: note.to_string() }],
    }
}

/// 用户可读纯文本（AI Context / 摘要；媒体块替换为「[图片]/[视频]/[画图]」占位）。
pub fn plain_text(note: Option<&str>) -> String {
    parse_blocks(note)
        .iter()
        .map(|b| match b {
            NoteBlock::Text { c } => c.clone(),
            NoteBlock::Image { .. } => "[图片]".to_string(),
            NoteBlock::Video { .. } => "[视频]".to_string(),
            NoteBlock::Drawing { .. } => "[画图]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 文本字数（不含媒体块）。
pub fn text_len(note: Option<&str>) -> usize {
    parse_blocks(note)
        .iter()
        .map(|b| match b {
            NoteBlock::Text { c } => c.trim().chars().count(),
            _ => 0,
        })
        .sum()
}

/// 媒体统计：图片数（image+drawing）与视频数。
pub fn media_counts(note: Option<&str>) -> (usize, usize) {
    let mut img = 0usize;
    let mut vid = 0usize;
    for b in parse_blocks(note) {
        match b.kind() {
            "image" | "drawing" => img += 1,
            "video" => vid += 1,
            _ => {}
        }
    }
    (img, vid)
}

/// 媒体附件 id 列表。
pub fn media_ids(note: Option<&str>) -> Vec<i64> {
    parse_blocks(note).iter().filter_map(|b| b.attachment_id()).collect()
}

/// 序列化 blocks → note（v2 JSON）。空 blocks → None 语义由调用方处理（存空串）。
pub fn serialize_blocks(blocks: &[NoteBlock]) -> String {
    serde_json::json!({ "v": 2, "blocks": blocks }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_plain_text_is_single_block() {
        let blocks = parse_blocks(Some("旧笔记\n第二行"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(plain_text(Some("旧笔记")), "旧笔记");
        assert_eq!(text_len(Some("你好世界")), 4);
    }

    #[test]
    fn v2_roundtrip_and_helpers() {
        let blocks = vec![
            NoteBlock::Text { c: "第一段".into() },
            NoteBlock::Image { a: 7, n: "a.png".into() },
            NoteBlock::Text { c: "第二段".into() },
            NoteBlock::Video { a: 8, n: "b.mp4".into() },
            NoteBlock::Drawing { a: 9, n: "c.png".into() },
        ];
        let s = serialize_blocks(&blocks);
        assert!(s.starts_with("{\"blocks\""));
        let parsed = parse_blocks(Some(&s));
        assert_eq!(parsed.len(), 5);
        assert_eq!(plain_text(Some(&s)), "第一段\n[图片]\n第二段\n[视频]\n[画图]");
        assert_eq!(text_len(Some(&s)), 6);
        assert_eq!(media_counts(Some(&s)), (2, 1));
        assert_eq!(media_ids(Some(&s)), vec![7, 8, 9]);
    }

    #[test]
    fn malformed_json_falls_back_to_text() {
        let blocks = parse_blocks(Some("{\"v\":2,\"blocks\":\"不是数组\"}"));
        assert_eq!(blocks.len(), 1); // 整体当文本
        assert!(plain_text(None).is_empty());
    }
}
