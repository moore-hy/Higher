//! NIGHT SHIFT O2 · M2/M4 —— 文档解析**边界**（不是解析器实现）。
//!
//! # 这里定义什么，不定义什么
//!
//! 定义的是**契约**：一份文件进来，出去的是一棵确定性的章节树 + 一串 chunk。
//! 不定义的是**怎么解析**：Higher 不写 PDF / DOCX / PPTX / OCR 解析器（O2 §7.1）。
//! 真正的解析由成熟运行时（Docling）完成，这里只放一个薄适配器接口。
//!
//! # 为什么解析必须是一个可替换的边界
//!
//! 导入生命周期的正确性（状态机、事务边界、失败不留半成品）与**解析器是谁无关**。
//! 把两者绑在一起，就再也无法在没有 Docling 的机器上验证生命周期。
//! 因此 [`DocumentParser`] 是 trait，Docling 只是它的一个实现；
//! 集成测试可以注入一个确定性假解析器，验证的是**真实**的生命周期代码路径。
//!
//! # ordinal 由解析器给出
//!
//! [`ParsedChunk::ordinal`] 与 [`ParsedSection::ordinal`] 是**输入**而不是
//! 由仓储生成：同一份材料 + 同一个 parser 版本必须得到同一串序号。
//! 这是 `Context Compiler` 稳定排序（相关度 → doc → revision → section → chunk）
//! 能够成立的前提。

use serde::{Deserialize, Serialize};

/// 解析产物：章节树 + chunk 列表，**全部** ordinal 显式。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub sections: Vec<ParsedSection>,
    pub chunks: Vec<ParsedChunk>,
    /// 解析器身份（写入 `document_revisions.parser_name`）。
    pub parser_name: String,
    /// 解析器版本（写入 `document_revisions.parser_version`，未知时为 `None`）。
    pub parser_version: Option<String>,
}

/// 一个章节。`parent_index` 是**同一 `sections` 数组内**的下标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSection {
    pub title: Option<String>,
    pub ordinal: i64,
    /// `None` = 顶层章节。
    pub parent_index: Option<usize>,
}

/// 一个 chunk。`section_index` 是**同一 `sections` 数组内**的下标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedChunk {
    pub ordinal: i64,
    pub text: String,
    pub section_index: Option<usize>,
}

/// 解析失败的**分类**。
///
/// 三类必须分开，因为它们的**可恢复性不同**：
///
/// ```text
/// RuntimeUnavailable  运行时不在（可恢复：配置/安装后重试）-> DOCLING_UNAVAILABLE
/// Unsupported         运行时明确不支持这种输入（换文件）    -> UNSUPPORTED_INPUT
/// Failed              运行时报错（重试可能有用）            -> PARSER_FAILED
/// ```
///
/// 把三者压成一个「解析失败」会让 UI 只能给出一句无用的报错，
/// 也让 O2-18「Docling 不可用时是可恢复状态」无法被断言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFailure {
    RuntimeUnavailable(String),
    Unsupported(String),
    Failed(String),
}

impl ParseFailure {
    /// 稳定错误码（写入 `document_ingestion_jobs.error_code`）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::RuntimeUnavailable(_) => "DOCLING_UNAVAILABLE",
            Self::Unsupported(_) => "UNSUPPORTED_INPUT",
            Self::Failed(_) => "PARSER_FAILED",
        }
    }

    /// 人话细节（写入 `document_ingestion_jobs.error_detail`）。
    pub fn detail(&self) -> String {
        match self {
            Self::RuntimeUnavailable(m) | Self::Unsupported(m) | Self::Failed(m) => m.clone(),
        }
    }

    /// 是否值得重试。
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::RuntimeUnavailable(_) => true,
            Self::Unsupported(_) => false,
            Self::Failed(_) => true,
        }
    }
}

impl std::fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.detail())
    }
}

/// 文档解析边界。
///
/// 实现者**不得**：写数据库、产生学习事实、安装依赖、访问网络。
/// 它只负责「字节 + 文件名 → 结构」。
pub trait DocumentParser {
    /// 稳定名称（写入 `document_revisions.parser_name`）。
    fn name(&self) -> String;

    /// 版本（可未知）。
    fn version(&self) -> Option<String>;

    /// 解析一份文件。
    fn parse(&self, file_name: &str, bytes: &[u8]) -> Result<ParsedDocument, ParseFailure>;
}

/// 运行时缺失时的**占位解析器**。
///
/// 存在的唯一目的：让导入生命周期在**没有 Docling 的机器上照常跑完一遍**，
/// 留下一条 `Failed` + `DOCLING_UNAVAILABLE` + 可重试的作业记录，
/// 而不是让调用方拿到一个没有任何事实痕迹的裸错误。
///
/// 它**不是**解析器实现，也永远不会「退化成」一个自研解析器 ——
/// 它只会失败，且失败得可恢复（O2-18）。
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableParser;

impl UnavailableParser {
    pub fn new() -> Self {
        Self
    }
}

impl DocumentParser for UnavailableParser {
    fn name(&self) -> String {
        "docling".to_string()
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        Err(ParseFailure::RuntimeUnavailable(
            "Docling runtime is not installed; install it and retry the ingestion".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // O2-18 的纯函数侧：运行时缺失必须是**可恢复**的稳定错误码。
    #[test]
    fn unavailable_parser_is_recoverable_docling_unavailable() {
        let p = UnavailableParser::new();
        let err = p.parse("x.pdf", b"whatever").unwrap_err();
        assert_eq!(err.code(), "DOCLING_UNAVAILABLE");
        assert!(err.is_recoverable());
        assert_eq!(p.name(), "docling");
    }
}
