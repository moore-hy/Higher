//! NIGHT SHIFT O2 · M4 —— Docling 真实运行时的**薄适配器**（不是解析器）。
//!
//! # 这一层到底做了什么
//!
//! 只做三件事：
//!
//! ```text
//! 1. 找到成熟运行时（隔离 venv 里的 python + 官方 docling 包）
//! 2. 把文件交给它解析（子进程 + 一行 JSON 契约）
//! 3. 把 JSON 投影成 Higher 的 ParsedDocument
//! ```
//!
//! 它**不**解析 PDF / DOCX / PPTX，**不**做 OCR，**不**做版面分析 ——
//! 那些是 docling 的职责（O2 §7.1：不得自研富文档解析器）。
//!
//! # 为什么是子进程
//!
//! Higher 是 Rust/Tauri 进程，没有内嵌 Python。成熟解析能力是一个 Python 包。
//! 在这个边界上，子进程 + 文本契约是**最薄**的实现：运行时可以整体替换
//! （升级版本、换镜像、换机器）而 Higher 一行不用改；解析崩溃也带不走 Higher。
//!
//! # 复用既有 `DoclingRuntime`，不新建第二个运行时概念
//!
//! `crate::runtime::DoclingRuntime` 已经定义了「Docling 是否在位」的契约
//! （`RuntimeAdapter` / `RuntimeHealth` / `endpoint_present`）。
//! 本模块**不重写**那套判定，而是把发现的解释器路径喂给它 ——
//! 于是「运行时健康」在整个应用里只有一个真相源。
//!
//! # 不可用时必须是一条**可恢复**的路
//!
//! 运行时不在 → [`ParseFailure::RuntimeUnavailable`]（错误码 `DOCLING_UNAVAILABLE`，
//! `is_recoverable() == true`）。作业被标成 `Failed`，**没有任何半成品结构**，
//! 材料本身毫发无损。用户装好运行时后重试即可 —— 这就是 O2-18。
//! 这里**绝不**退回一个自研解析器。

use std::path::{Path, PathBuf};
use std::process::Command;

use super::parser::{DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection};

/// 本夜锁定的运行时目录名（O2 §6）。
pub const RUNTIME_DIR_NAME: &str = "docling-2.73.0-o2";

/// 本夜锁定的候选版本（写入 `document_revisions.parser_version` 兜底用）。
pub const PINNED_VERSION: &str = "2.73.0";

/// 环境变量：显式指定解释器（优先于一切自动发现）。
pub const PYTHON_ENV: &str = "HIGHER_DOCLING_PYTHON";

/// 与 docling 之间唯一的契约。字段名与 `docling_runner.py` 逐字对应。
#[derive(Debug, serde::Deserialize)]
struct RunnerOutput {
    parser_name: String,
    parser_version: Option<String>,
    #[serde(default)]
    sections: Vec<RunnerSection>,
    #[serde(default)]
    chunks: Vec<RunnerChunk>,
}

#[derive(Debug, serde::Deserialize)]
struct RunnerSection {
    title: Option<String>,
    ordinal: i64,
    parent_index: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct RunnerChunk {
    ordinal: i64,
    text: String,
    section_index: Option<usize>,
}

/// 运行时探测结果。**只描述事实**，不含任何解析能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoclingRuntimeState {
    /// 找到了解释器（尚未验证 docling 能否导入）。
    Found(PathBuf),
    /// 没有任何可用的运行时。
    Missing,
}

impl DoclingRuntimeState {
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found(_))
    }

    pub fn interpreter(&self) -> Option<&Path> {
        match self {
            Self::Found(p) => Some(p.as_path()),
            Self::Missing => None,
        }
    }

    /// 人类可读的一句话，用于 IPC 状态回报。
    pub fn describe(&self) -> String {
        match self {
            Self::Found(p) => format!("Docling runtime found at {}", p.display()),
            Self::Missing => format!(
                "Docling runtime not found; install docling=={PINNED_VERSION} into \
                 %LOCALAPPDATA%\\Higher\\runtimes\\{RUNTIME_DIR_NAME}\\ or set {PYTHON_ENV}"
            ),
        }
    }
}

/// 解释器候选路径，按优先级排列。
///
/// 顺序即策略：
/// ```text
/// 1. 显式配置（用户/诊断说了算）
/// 2. 本夜新建的隔离运行时（首选落点）
/// 3. 既有的隔离运行时目录（**只读复用**，绝不改写它）
/// 4. PATH 上的 python（开发机兜底）
/// ```
pub fn interpreter_candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    if let Ok(explicit) = std::env::var(PYTHON_ENV) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let runtimes = PathBuf::from(&local).join("Higher").join("runtimes");
        // 本夜首选落点。
        out.push(
            runtimes
                .join(RUNTIME_DIR_NAME)
                .join("Scripts")
                .join("python.exe"),
        );
        // 既有目录：只读复用。**不**创建、**不**删除、**不**写入。
        out.push(runtimes.join("docling").join("Scripts").join("python.exe"));
        // POSIX 形态（跨平台构建时的兜底；Windows 上不存在即为空）。
        out.push(runtimes.join(RUNTIME_DIR_NAME).join("bin").join("python3"));
        out.push(runtimes.join("docling").join("bin").join("python3"));
    }

    for name in ["python3", "python"] {
        if let Some(p) = which(name) {
            out.push(p);
        }
    }

    out
}

/// 极简 `which`：只查 PATH，不引入依赖。
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string())
            .split(';')
            .map(|s| s.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 探测运行时：第一个**真实存在**的解释器候选即答案。
///
/// 注意这里只判断「解释器在不在」，**不**判断 docling 能否导入 ——
/// 那是一次真实的进程调用，属于解析阶段的事。把两者分开，是为了让
/// 「运行时缺失」和「运行时在但装坏了」得到不同的错误分类。
pub fn discover_runtime() -> DoclingRuntimeState {
    for candidate in interpreter_candidates() {
        if candidate.is_file() {
            return DoclingRuntimeState::Found(candidate);
        }
    }
    DoclingRuntimeState::Missing
}

/// 既有 `DoclingRuntime` 契约的接线：把发现到的解释器交给它。
///
/// 这样「运行时健康」在整个应用里只有一个真相源，而不是本模块自己再判一次。
pub fn runtime_adapter() -> crate::runtime::DoclingRuntime {
    let endpoint = discover_runtime()
        .interpreter()
        .map(|p| p.to_string_lossy().to_string());
    crate::runtime::DoclingRuntime::new(endpoint, false)
}

/// Docling 解析器：实现 [`DocumentParser`]，把解析委托给成熟运行时。
///
/// 它**不**持有数据库连接、**不**写学习事实、**不**访问网络 ——
/// 与 [`DocumentParser`] 的纪律一致。
pub struct DoclingParser {
    interpreter: PathBuf,
    /// 运行时目录（用于把 runner 放在运行时旁边；缺省时落到临时目录）。
    runner_dir: Option<PathBuf>,
}

impl DoclingParser {
    /// 按自动发现的结果构造；运行时缺失时返回 `None`。
    pub fn discover() -> Option<Self> {
        let interpreter = discover_runtime().interpreter()?.to_path_buf();
        let runner_dir = interpreter
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        Some(Self {
            interpreter,
            runner_dir,
        })
    }

    /// 显式指定解释器（测试与诊断用）。
    pub fn with_interpreter(interpreter: impl Into<PathBuf>) -> Self {
        let interpreter = interpreter.into();
        let runner_dir = interpreter
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        Self {
            interpreter,
            runner_dir,
        }
    }

    /// runner 脚本的落地路径。
    ///
    /// 优先放在运行时目录旁边（一次性、可审计）；不可写时退回临时目录。
    /// 无论哪种情况，写的都是 **Higher 自己的** runner，绝不改动 docling 包。
    fn runner_path(&self) -> PathBuf {
        let name = format!("higher_docling_runner_{}.py", std::process::id());
        if let Some(dir) = &self.runner_dir {
            if dir.is_dir() {
                return dir.join(name);
            }
        }
        std::env::temp_dir().join(name)
    }

    /// 把 runner 写盘（内容来自 `include_str!`，不依赖打包后的资源路径）。
    fn materialize_runner(&self) -> Result<PathBuf, ParseFailure> {
        let path = self.runner_path();
        if path.is_file() {
            return Ok(path);
        }
        std::fs::write(&path, include_str!("docling_runner.py"))
            .map_err(|e| ParseFailure::Failed(format!("cannot write docling runner: {e}")))?;
        Ok(path)
    }
}

/// runner 的退出码 → 错误分类（与 `docling_runner.py` 头部逐字对应）。
const EXIT_RUNTIME_UNAVAILABLE: i32 = 3;
const EXIT_UNSUPPORTED: i32 = 4;

impl DocumentParser for DoclingParser {
    fn name(&self) -> String {
        "docling".to_string()
    }

    fn version(&self) -> Option<String> {
        // 版本由解析器自己上报（runner 输出里的 `parser_version`）。
        // 这里不猜 —— 猜出来的版本号写进 `document_revisions` 就是伪造审计信息。
        None
    }

    fn parse(&self, file_name: &str, bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        if !self.interpreter.is_file() {
            return Err(ParseFailure::RuntimeUnavailable(format!(
                "Docling interpreter not found at {}",
                self.interpreter.display()
            )));
        }
        if bytes.is_empty() {
            return Err(ParseFailure::Unsupported("empty input".to_string()));
        }

        let runner = self.materialize_runner()?;

        // 源文件写进临时目录：解析器是纯函数边界，不碰附件目录。
        let work = std::env::temp_dir().join(format!(
            "higher_docling_{}_{}",
            std::process::id(),
            sanitize(file_name)
        ));
        std::fs::write(&work, bytes)
            .map_err(|e| ParseFailure::Failed(format!("cannot stage input file: {e}")))?;

        let output = Command::new(&self.interpreter)
            .arg(&runner)
            .arg(&work)
            .output();

        let _ = std::fs::remove_file(&work);

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                return Err(ParseFailure::RuntimeUnavailable(format!(
                    "cannot launch Docling runtime {}: {e}",
                    self.interpreter.display()
                )))
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                format!("docling runner exited with {:?}", output.status.code())
            } else {
                stderr
            };
            return Err(match output.status.code() {
                Some(EXIT_RUNTIME_UNAVAILABLE) => ParseFailure::RuntimeUnavailable(detail),
                Some(EXIT_UNSUPPORTED) => ParseFailure::Unsupported(detail),
                _ => ParseFailure::Failed(detail),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: RunnerOutput = serde_json::from_str(&stdout).map_err(|e| {
            ParseFailure::Failed(format!("docling runner produced invalid JSON: {e}"))
        })?;

        Ok(ParsedDocument {
            sections: parsed
                .sections
                .into_iter()
                .map(|s| ParsedSection {
                    title: s.title,
                    ordinal: s.ordinal,
                    parent_index: s.parent_index,
                })
                .collect(),
            chunks: parsed
                .chunks
                .into_iter()
                .map(|c| ParsedChunk {
                    ordinal: c.ordinal,
                    text: c.text,
                    section_index: c.section_index,
                })
                .collect(),
            parser_name: if parsed.parser_name.trim().is_empty() {
                "docling".to_string()
            } else {
                parsed.parser_name
            },
            parser_version: parsed
                .parser_version
                .or_else(|| Some(PINNED_VERSION.to_string())),
        })
    }
}

/// 只保留文件名里安全的字符，避免临时文件路径被文件名污染。
fn sanitize(file_name: &str) -> String {
    let base = Path::new(file_name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "input".to_string());
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "input".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // O2-18 的前置：缺失运行时必须是**可恢复**的分类，而不是崩溃或静默。
    #[test]
    fn missing_interpreter_is_recoverable_runtime_unavailable() {
        let parser = DoclingParser::with_interpreter(
            std::env::temp_dir().join("definitely-not-a-python-binary"),
        );
        let err = parser.parse("x.pdf", b"%PDF-1.4 fake").unwrap_err();
        assert_eq!(err.code(), "DOCLING_UNAVAILABLE");
        assert!(err.is_recoverable());
        match err {
            ParseFailure::RuntimeUnavailable(_) => {}
            other => panic!("expected RuntimeUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_is_unsupported_not_unavailable() {
        let parser = DoclingParser::with_interpreter(std::env::current_exe().unwrap());
        let err = parser.parse("x.pdf", b"").unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_INPUT");
        assert!(!err.is_recoverable());
    }

    #[test]
    fn discovery_is_total_and_never_panics() {
        // 无论机器上有没有 Docling，探测都必须给出一个确定的答案。
        let state = discover_runtime();
        let _ = state.describe();
        assert!(state.is_found() == state.interpreter().is_some());
    }

    #[test]
    fn sanitize_strips_path_separators() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("a b/c*d.pdf"), "c_d.pdf");
        assert_eq!(sanitize(""), "input");
    }
}
