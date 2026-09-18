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
//!
//! # 子进程必须有预算（否则会造出一条不可恢复的路）
//!
//! docling 首次解析 PDF / DOCX / PPTX 要从 Hub 下载版面模型（实测约 164 MB）。
//! 没有上限时子进程可以长时间阻塞，而作业会一直停在 `Parsing`；
//! `retry_ingestion` 只接受 `Failed`，于是「解析卡住」= 作业永久停在 `Parsing`。
//! 所以这里用 [`DEFAULT_PARSE_TIMEOUT_SECS`] 给子进程一个硬上限，
//! 超时即 kill 并归类为 `Failed` + `PARSER_FAILED`（**可恢复**）。
//!
//! # 模型缓存必须住在隔离运行时内部
//!
//! 子进程默认继承父进程环境，于是模型会落到用户 home 的
//! `~/.cache/huggingface`，让「隔离运行时」变成两处真相。
//! [`DoclingParser::model_cache_env`] 在用户**没有**自己设 `HF_HOME`、
//! 且解释器确实来自 `%LOCALAPPDATA%\Higher\runtimes\` 时，把缓存指向
//! 运行时内部的 `hf-cache\`。用户的选择永远优先。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::parser::{DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection};

/// 本夜锁定的运行时目录名（O2 §6）。
pub const RUNTIME_DIR_NAME: &str = "docling-2.73.0-o2";

/// 本夜锁定的候选版本（写入 `document_revisions.parser_version` 兜底用）。
pub const PINNED_VERSION: &str = "2.73.0";

/// 环境变量：显式指定解释器（优先于一切自动发现）。
pub const PYTHON_ENV: &str = "HIGHER_DOCLING_PYTHON";

/// 解析子进程的硬上限，单位秒。超过即 kill 并归类为 `Failed`。
///
/// 为什么必须有这个上限：docling 首次解析 PDF / DOCX / PPTX 时要从 Hub 拉
/// 版面模型（实测约 164 MB）。网络不可达时子进程会**长时间阻塞**，而
/// `ingest_source` 此时把作业留在 `Parsing`。`retry_ingestion` 只接受 `Failed`，
/// 于是「解析卡死」= 作业永久停在 `Parsing`，这是一条**不可恢复**的路。
/// 有了上限，卡死会变成 `Failed` + `PARSER_FAILED`（可恢复），用户重试即可。
///
/// 900 秒是刻意的宽松值：它要容纳一次真实的模型下载，只拦「真的卡住了」。
pub const DEFAULT_PARSE_TIMEOUT_SECS: u64 = 900;

/// 覆盖解析超时的环境变量（秒）。
pub const TIMEOUT_ENV: &str = "HIGHER_DOCLING_TIMEOUT_SECS";

/// 模型缓存环境变量。Docling 通过 huggingface_hub 下载版面 / 表格模型，
/// 缓存位置由 `HF_HOME` 决定。
pub const HF_HOME_ENV: &str = "HF_HOME";

/// 隔离运行时内部的模型缓存目录名（相对运行时根目录）。
///
/// **只有一个定义处**：适配器与诊断/测试都读它。此前这个名字曾在两处
/// 各写一遍（`hf-cache` / `.hf-cache`），结果是模型会被下到两个不同的目录、
/// 白白重复下载 164MB。点号开头与 huggingface_hub 自己的隐藏缓存惯例一致，
/// 也与 venv 根目录里既有的 `.lock` 同类。
pub const MODEL_CACHE_DIR_NAME: &str = ".hf-cache";

/// 并发解析时的临时文件唯一化计数器。
static STAGE_SEQ: AtomicU64 = AtomicU64::new(0);

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

/// 隔离运行时内部的模型缓存目录 —— 与 [`DoclingParser::model_cache_env`] 指向同一处。
///
/// 存在的意义是让调用方（诊断、测试）能**如实**判断「ML 模型是否已经就位」，
/// 而不必自己再拼一遍路径，也不必真的先解析一份 PDF 才知道。
pub fn managed_model_cache_dir() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        PathBuf::from(local)
            .join("Higher")
            .join("runtimes")
            .join(RUNTIME_DIR_NAME)
            .join(MODEL_CACHE_DIR_NAME),
    )
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
    /// 优先放在运行时目录旁边（可审计）；不可写时退回临时目录。
    /// 无论哪种情况，写的都是 **Higher 自己的** runner，绝不改动 docling 包。
    ///
    /// 名字是**稳定**的，不带 pid：内容来自 `include_str!`，对同一个二进制永远
    /// 相同，所以复用同一个文件即可。早先用 `_<pid>` 命名，结果是每启动一次
    /// 应用就往运行时目录里多堆一个 7KB 的脚本 —— 无上界的目录污染。
    fn runner_path(&self) -> PathBuf {
        const NAME: &str = "higher_docling_runner.py";
        if let Some(dir) = &self.runner_dir {
            if dir.is_dir() {
                return dir.join(NAME);
            }
        }
        std::env::temp_dir().join(NAME)
    }

    /// 把 runner 写盘（内容来自 `include_str!`，不依赖打包后的资源路径）。
    ///
    /// 内容一致就直接复用，不重写。需要写时先落临时文件再改名 ——
    /// 这样并发进程永远读不到写了一半的脚本。
    fn materialize_runner(&self) -> Result<PathBuf, ParseFailure> {
        let path = self.runner_path();
        let body = include_str!("docling_runner.py");
        if std::fs::read_to_string(&path).is_ok_and(|existing| existing == body) {
            return Ok(path);
        }
        let staged = path.with_extension("py.tmp");
        std::fs::write(&staged, body)
            .map_err(|e| ParseFailure::Failed(format!("cannot write docling runner: {e}")))?;
        std::fs::rename(&staged, &path)
            .map_err(|e| ParseFailure::Failed(format!("cannot install docling runner: {e}")))?;
        Ok(path)
    }

    /// 本次解析的超时上限。
    ///
    /// 环境变量给了合法值就用它，否则用 [`DEFAULT_PARSE_TIMEOUT_SECS`]。
    fn parse_timeout(&self) -> Duration {
        resolve_timeout(std::env::var(TIMEOUT_ENV).ok().as_deref())
    }

    /// 模型缓存应当落在哪里（`HF_HOME`）。
    ///
    /// 只在**两个条件同时成立**时返回 `Some`：
    ///
    /// 1. 用户自己没有设 `HF_HOME` —— 用户的选择永远优先，绝不覆盖；
    /// 2. 解释器来自 Higher 自己管理的隔离运行时目录。
    ///
    /// 第 2 条是为了不劫持 PATH 上的系统 Python：那种情况下把缓存塞进
    /// Python 安装目录是越界的。对隔离运行时则相反 —— 让模型缓存住在
    /// 运行时内部，整个运行时才是**自洽可搬迁**的一个目录，而不是
    /// 「venv 在这里、200MB 模型在用户 home」的两处真相。
    fn model_cache_env(&self) -> Option<(String, String)> {
        if std::env::var_os(HF_HOME_ENV).is_some() {
            return None;
        }
        let dir = self.runner_dir.as_ref()?;
        if !is_managed_runtime(&self.interpreter) {
            return None;
        }
        Some((
            HF_HOME_ENV.to_string(),
            dir.join(MODEL_CACHE_DIR_NAME).to_string_lossy().to_string(),
        ))
    }
}

/// 解释器是否来自 Higher 管理的隔离运行时目录
/// （`%LOCALAPPDATA%\Higher\runtimes\...`）。
///
/// 判定基于**路径事实**，不做任何猜测：要么在 runtimes 目录下，要么不是。
fn is_managed_runtime(interpreter: &Path) -> bool {
    match std::env::var("LOCALAPPDATA") {
        Ok(local) => is_under_runtimes(interpreter, Path::new(&local)),
        Err(_) => false,
    }
}

/// [`is_managed_runtime`] 的纯判定部分（便于直接验证）。
fn is_under_runtimes(interpreter: &Path, local_appdata: &Path) -> bool {
    interpreter.starts_with(local_appdata.join("Higher").join("runtimes"))
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
        // 序号参与命名，避免同一进程内并发解析互相覆盖。
        let seq = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
        let tag = format!("{}_{}", std::process::id(), seq);
        let temp = std::env::temp_dir();
        let work = temp.join(format!("higher_docling_{tag}_{}", sanitize(file_name)));
        let out_path = temp.join(format!("higher_docling_{tag}.stdout.json"));
        let err_path = temp.join(format!("higher_docling_{tag}.stderr.log"));

        std::fs::write(&work, bytes)
            .map_err(|e| ParseFailure::Failed(format!("cannot stage input file: {e}")))?;

        // stdout / stderr 走**文件**而不是管道：docling 会往 stderr 写大量
        // 进度日志，管道缓冲区写满会让子进程阻塞，而我们同时在轮询等待 ——
        // 那就是教科书式的死锁。文件没有这个容量上限。
        let (stdout_file, stderr_file) = match (File::create(&out_path), File::create(&err_path)) {
            (Ok(o), Ok(e)) => (o, e),
            _ => {
                let _ = std::fs::remove_file(&work);
                return Err(ParseFailure::Failed(
                    "cannot create Docling runner output files".to_string(),
                ));
            }
        };

        let mut command = Command::new(&self.interpreter);
        command
            .arg(&runner)
            .arg(&work)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));
        // 让模型缓存住进隔离运行时内部（用户已设 HF_HOME 时不介入）。
        if let Some((key, value)) = self.model_cache_env() {
            command.env(key, value);
        }

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                cleanup(&[&work, &out_path, &err_path]);
                return Err(ParseFailure::RuntimeUnavailable(format!(
                    "cannot launch Docling runtime {}: {e}",
                    self.interpreter.display()
                )));
            }
        };

        let timeout = self.parse_timeout();
        let status = match wait_with_deadline(&mut child, timeout) {
            Ok(Some(status)) => status,
            Ok(None) => {
                cleanup(&[&work, &out_path, &err_path]);
                // 超时归类为 Failed（**可恢复**）而不是 RuntimeUnavailable：
                // 运行时是在的，只是这一次没在预算内跑完。
                return Err(ParseFailure::Failed(format!(
                    "docling parse timed out after {}s (raise {TIMEOUT_ENV} if this is a \
                     first-time model download)",
                    timeout.as_secs()
                )));
            }
            Err(e) => {
                cleanup(&[&work, &out_path, &err_path]);
                return Err(ParseFailure::Failed(format!(
                    "cannot wait for Docling runtime: {e}"
                )));
            }
        };

        let mut stdout = String::new();
        let _ = File::open(&out_path).and_then(|mut f| f.read_to_string(&mut stdout));
        let mut stderr = String::new();
        let _ = File::open(&err_path).and_then(|mut f| f.read_to_string(&mut stderr));

        cleanup(&[&work, &out_path, &err_path]);

        if !status.success() {
            let detail = tail_of(stderr.trim(), MAX_ERROR_DETAIL_CHARS);
            let detail = if detail.is_empty() {
                format!("docling runner exited with {:?}", status.code())
            } else {
                detail
            };
            return Err(match status.code() {
                Some(EXIT_RUNTIME_UNAVAILABLE) => ParseFailure::RuntimeUnavailable(detail),
                Some(EXIT_UNSUPPORTED) => ParseFailure::Unsupported(detail),
                _ => ParseFailure::Failed(detail),
            });
        }

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

/// 写进 `document_ingestion_jobs.error_detail` 的字符上限。
///
/// docling 会把整轮进度日志写到 stderr；一次失败的原始输出可以有几万字符。
/// 原样入库只会把审计信息变成噪声，所以只保留**尾部** —— runner 自己的失败
/// 说明永远在最后。
const MAX_ERROR_DETAIL_CHARS: usize = 4000;

/// 由环境变量原值解析出超时；非法值（缺失 / 非数字 / 0）一律回落到默认值。
///
/// 0 必须回落：一个「0 秒超时」会把每一次解析都杀掉。
fn resolve_timeout(raw: Option<&str>) -> Duration {
    let secs = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_PARSE_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// 等到子进程结束，或到 `timeout` 为止。
///
/// 返回 `Ok(Some(status))` = 正常结束；`Ok(None)` = 到点仍未结束
/// （**已经 kill 并回收**，不留僵尸）；`Err` = 等待本身出错。
///
/// 单独抽成函数，是为了能直接验证「到点一定 kill」，
/// 而不必依赖环境变量或一次真实解析。
fn wait_with_deadline(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// 逐个删除临时文件，忽略错误（清理失败不该盖过真正的解析结果）。
fn cleanup(paths: &[&PathBuf]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

/// 取字符串尾部至多 `max` 个字符（按字符而非字节切，避免切碎 UTF-8）。
fn tail_of(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    s.chars().skip(count - max).collect()
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

    // 非法超时值必须回落到默认值 —— 一个「0 秒超时」会杀掉每一次解析。
    #[test]
    fn resolve_timeout_falls_back_on_garbage() {
        let default = Duration::from_secs(DEFAULT_PARSE_TIMEOUT_SECS);
        assert_eq!(resolve_timeout(None), default);
        assert_eq!(resolve_timeout(Some("")), default);
        assert_eq!(resolve_timeout(Some("abc")), default);
        assert_eq!(resolve_timeout(Some("0")), default);
        assert_eq!(resolve_timeout(Some("-5")), default);
        assert_eq!(resolve_timeout(Some(" 120 ")), Duration::from_secs(120));
    }

    // 到点必须**真的 kill**，而不是无限等下去。这条是「解析卡死不会变成
    // 一个永久停在 Parsing 的作业」的直接保证。
    #[test]
    fn wait_with_deadline_kills_a_hanging_child() {
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 20 127.0.0.1 >nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("必须能起一个必然挂住的子进程");
        #[cfg(not(windows))]
        let mut child = Command::new("sleep")
            .arg("20")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("必须能起一个必然挂住的子进程");

        let started = Instant::now();
        let outcome = wait_with_deadline(&mut child, Duration::from_millis(600)).unwrap();
        assert!(outcome.is_none(), "到点必须返回超时，而不是一直等");
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "必须在超时后立刻返回"
        );
        // 已经 kill 并回收，不留僵尸子进程。
        assert!(child.try_wait().unwrap().is_some());
    }

    // 只保留尾部，且必须按字符切 —— 按字节切会切碎 UTF-8。
    #[test]
    fn tail_of_keeps_the_end_and_never_splits_utf8() {
        assert_eq!(tail_of("abcdef", 3), "def");
        assert_eq!(tail_of("abc", 10), "abc");
        let s = "解析失败：网络不可达";
        let t = tail_of(s, 4);
        assert_eq!(t.chars().count(), 4);
        assert!(s.ends_with(&t));
    }

    // 模型缓存只允许落在 Higher 管理的隔离运行时里。
    // PATH 上的系统 Python 绝不能被当成隔离运行时，否则会把 200MB 模型
    // 写进 Python 安装目录 —— 那是越界的。
    #[test]
    fn only_managed_runtime_paths_are_treated_as_isolated() {
        let base = Path::new("C:/Users/x/AppData/Local");
        assert!(is_under_runtimes(
            Path::new(
                "C:/Users/x/AppData/Local/Higher/runtimes/docling-2.73.0-o2/Scripts/python.exe"
            ),
            base
        ));
        assert!(!is_under_runtimes(
            Path::new("C:/Python312/python.exe"),
            base
        ));
        assert!(!is_under_runtimes(
            Path::new("C:/Users/x/AppData/Local/Other/runtimes/py.exe"),
            base
        ));
    }

    // 模型缓存目录名只能有**一个**定义处：适配器与诊断/测试必须指向同一个目录，
    // 否则模型会被下到两处、白白重复下载。
    #[test]
    fn model_cache_dir_is_single_sourced() {
        let Some(dir) = managed_model_cache_dir() else {
            return; // 非 Windows / 无 LOCALAPPDATA：无隔离运行时的概念
        };
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(MODEL_CACHE_DIR_NAME)
        );
        assert_eq!(
            dir.parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str()),
            Some(RUNTIME_DIR_NAME),
            "缓存必须住在隔离运行时内部，而不是别处"
        );
    }

    // runner 的落地名必须是**稳定**的（不带 pid）。早先按 pid 命名，结果是
    // 每启动一次应用就往运行时目录里多堆一个脚本 —— 无上界的目录污染。
    #[test]
    fn runner_path_is_stable_and_carries_no_pid() {
        let parser = DoclingParser::with_interpreter(
            std::env::temp_dir().join("definitely-not-a-python-binary"),
        );
        let path = parser.runner_path();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("higher_docling_runner.py")
        );
        assert!(
            !path
                .to_string_lossy()
                .contains(&std::process::id().to_string()),
            "runner 路径不得包含 pid"
        );
    }

    // 重复物化必须幂等：内容一致就复用，且落盘内容与内嵌 runner 逐字一致。
    #[test]
    fn materialize_runner_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("higher_o2_runner_{}", std::process::id()));
        let scripts = dir.join("Scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        let interpreter = scripts.join("python.exe");
        std::fs::write(&interpreter, b"stub").unwrap();

        let parser = DoclingParser::with_interpreter(&interpreter);
        let first = parser.materialize_runner().unwrap();
        let second = parser.materialize_runner().unwrap();

        assert_eq!(first, second, "两次物化必须得到同一个路径");
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            include_str!("docling_runner.py"),
            "落盘内容必须与内嵌 runner 逐字一致"
        );

        let _ = std::fs::remove_file(&interpreter);
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
