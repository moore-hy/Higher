//! NIGHT SHIFT O2 · M5 —— 文档导入的**生产 IPC 入口**。
//!
//! # 为什么命令层这么薄
//!
//! 与 `commands/mod.rs` 的既有约定一致：命令层只做**参数解析 + DB 锁 + 文件字节搬运**。
//! 全部业务逻辑都在下层：
//!
//! ```text
//! 状态机 / 事务边界        -> crate::document_intelligence::ingestion
//! 归属校验 / profile 隔离  -> crate::repository::document_ingestion
//! 解析                     -> crate::document_intelligence::docling_parser
//! 检索索引维护             -> crate::repository::search（既有）
//! ```
//!
//! 命令里**没有**第二个状态机、**没有**解析代码、**没有**检索写入。
//! 一旦把它们搬到这里，就会多出一个真相源 —— 那正是任务书要避免的。
//!
//! # 前端**不能**提交 section / chunk 真相
//!
//! 没有任何命令接受「章节」或「chunk」作为入参。结构只能由解析器产出、
//! 由服务层在一个事务里落库。前端能说的只有「把哪个附件导入进来」。
//!
//! # 导入 ≠ 学会
//!
//! 这里的每一个命令都不产生 `LearningMoment` / `Evidence` / `MemoryReview` /
//! FSRS 推进。导入只把材料变成可检索的上下文（O2 §17 永久不变量）。

use crate::db;
use crate::document_intelligence::docling_parser::{discover_runtime, DoclingRuntimeState};
use crate::document_intelligence::ingestion::{self, IngestionOutcome};
use crate::document_intelligence::retrieval;
use crate::document_intelligence::types::ContextPack;
use crate::repository::document_ingestion::{
    DocumentChunkRow, DocumentIngestionRepository, DocumentSectionRow, DocumentSourceRow,
    IngestionJobRow,
};
use crate::sandbox;
use crate::AttachmentDir;

/// 一个来源的完整可读视图：来源 + 最新作业 + 已就绪的结构规模。
///
/// 一次 IPC 返回列表页需要的全部信息，前端**不得**为每一行再开一个命令。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct DocumentSourceView {
    pub source: DocumentSourceRow,
    pub latest_job: Option<IngestionJobRow>,
    /// 最新 `Ready` revision 的 id（没有就是 `None`）。
    pub ready_revision_id: Option<i64>,
    pub section_count: i64,
    pub chunk_count: i64,
}

/// 一个 revision 的完整结构投影。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct DocumentStructureView {
    pub profile_id: i64,
    pub revision_id: i64,
    pub sections: Vec<DocumentSectionRow>,
    pub chunks: Vec<DocumentChunkRow>,
}

/// Docling 运行时状态（只读诊断，供 UI 决定是否提示「解析运行时未安装」）。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct DocumentRuntimeStatus {
    pub available: bool,
    pub detail: String,
    /// 运行时缺失时，这条消息就是用户唯一需要看到的东西。
    pub remedy: Option<String>,
}

// ============================ 运行时状态 ============================

/// 报告 Docling 运行时是否在位。
///
/// 这个命令**不**触发安装、**不**下载任何东西、**不**访问网络。
/// 它只回答一个问题：现在能不能解析。缺运行时是一条**正常**状态。
#[tauri::command]
pub fn get_document_runtime_status() -> Result<DocumentRuntimeStatus, String> {
    Ok(runtime_status())
}

fn runtime_status() -> DocumentRuntimeStatus {
    match discover_runtime() {
        DoclingRuntimeState::Found(_) => DocumentRuntimeStatus {
            available: true,
            detail: discover_runtime().describe(),
            remedy: None,
        },
        DoclingRuntimeState::Missing => DocumentRuntimeStatus {
            available: false,
            detail: discover_runtime().describe(),
            remedy: Some(format!(
                "安装 docling=={} 到隔离运行时目录，或设置 HIGHER_DOCLING_PYTHON 指向已有解释器",
                crate::document_intelligence::docling_parser::PINNED_VERSION
            )),
        },
    }
}

// ============================ 来源 ============================

/// 列出该档案的全部文档来源（含最新作业状态与结构规模）。
#[tauri::command]
pub fn list_document_sources(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<DocumentSourceView>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = DocumentIngestionRepository::new(&conn);
    let sources = repo.list_sources(profile_id).map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        let latest_job = repo
            .latest_job_for_source(profile_id, source.id)
            .map_err(|e| e.to_string())?;
        let ready_revision_id = latest_job.as_ref().and_then(|j| {
            if j.state == "Ready" {
                j.revision_id
            } else {
                None
            }
        });
        let (section_count, chunk_count) = match ready_revision_id {
            Some(rev) => (
                repo.list_sections(profile_id, rev)
                    .map_err(|e| e.to_string())?
                    .len() as i64,
                repo.list_chunks(profile_id, rev)
                    .map_err(|e| e.to_string())?
                    .len() as i64,
            ),
            None => (0, 0),
        };
        out.push(DocumentSourceView {
            source,
            latest_job,
            ready_revision_id,
            section_count,
            chunk_count,
        });
    }
    Ok(out)
}

/// 从**既有学习附件**创建一个文档来源。
///
/// 不接受路径、不接受文件字节、不接受文件名 —— 附件是文件本体的唯一真相源，
/// 这里只是把一个已存在的附件**登记**为可导入的文档来源。
/// 跨档案附件在仓储层被拒绝（`ATTACHMENT_NOT_IN_PROFILE`），且不留任何行。
#[tauri::command]
pub fn import_document_source(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    attachment_id: i64,
    display_name: Option<String>,
    origin: Option<String>,
    domain: Option<String>,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = DocumentIngestionRepository::new(&conn);

    // 展示名缺省取附件的真实文件名 —— 不编造一个「文档 1」。
    let display_name = match display_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            let file_name: Option<String> = conn
                .query_row(
                    "SELECT file_name FROM learning_attachments WHERE id = ?1 AND profile_id = ?2",
                    rusqlite::params![attachment_id, profile_id],
                    |r| r.get(0),
                )
                .ok();
            file_name.ok_or_else(|| "附件不存在或不属于当前档案".to_string())?
        }
    };

    repo.create_source(
        profile_id,
        attachment_id,
        &display_name,
        origin.as_deref(),
        domain.as_deref(),
        "attachment",
    )
    .map_err(|e| e.to_string())
}

// ============================ 导入生命周期 ============================

/// 启动一次导入。**幂等地推进状态机**，不做解析以外的事。
///
/// 流程（§13）：
/// ```text
/// Pending -> Parsing -> （解析，不持有写事务）-> 结构写入 + 索引（同一事务）-> Ready
///                                                              ↘ Failed（无半成品）
/// ```
/// 运行时缺失时返回 `state = "Failed"` + `error_code = "DOCLING_UNAVAILABLE"` +
/// `recoverable = true`：这是一条**可恢复**的路，不是崩溃。
#[tauri::command]
pub fn start_document_ingestion(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    source_id: i64,
) -> Result<IngestionOutcome, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    run_ingestion(&mut conn, &adir.0, profile_id, source_id, false)
}

/// 安全重试：只有**已终结为 `Failed`** 的最新作业才允许重试。
///
/// 正在 `Parsing` / `Indexing` 的来源会被拒绝（`INVALID_JOB_STATE`）——
/// 并发写同一份结构只会得到第二个作业撞唯一索引，那是一个本不该出现的错误。
#[tauri::command]
pub fn retry_document_ingestion(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    source_id: i64,
) -> Result<IngestionOutcome, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    run_ingestion(&mut conn, &adir.0, profile_id, source_id, true)
}

/// 读取某个来源的最新导入状态。
#[tauri::command]
pub fn get_document_ingestion_status(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    source_id: i64,
) -> Result<Option<IngestionJobRow>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    DocumentIngestionRepository::new(&conn)
        .latest_job_for_source(profile_id, source_id)
        .map_err(|e| e.to_string())
}

/// 读取一个已就绪 revision 的完整结构（章节 + chunk）。
#[tauri::command]
pub fn get_document_structure(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    revision_id: i64,
) -> Result<DocumentStructureView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = DocumentIngestionRepository::new(&conn);
    Ok(DocumentStructureView {
        profile_id,
        revision_id,
        sections: repo
            .list_sections(profile_id, revision_id)
            .map_err(|e| e.to_string())?,
        chunks: repo
            .list_chunks(profile_id, revision_id)
            .map_err(|e| e.to_string())?,
    })
}

/// 取消一个尚未终结的导入。
#[tauri::command]
pub fn cancel_document_ingestion(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    job_id: i64,
) -> Result<IngestionOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ingestion::cancel_ingestion(&conn, profile_id, job_id).map_err(|e| e.to_string())
}

/// 命令层唯一的一段「基础设施」代码：把附件字节取出来交给服务层。
///
/// 刻意不放进 ingestion 模块：读取沙箱内的文件是**平台/IO** 关注点，
/// 而生命周期与状态机是**领域**关注点。混在一起会让生命周期无法在
/// 没有文件系统的测试里被验证。
fn run_ingestion(
    conn: &mut rusqlite::Connection,
    attachment_root: &std::path::Path,
    profile_id: i64,
    source_id: i64,
    retry: bool,
) -> Result<IngestionOutcome, String> {
    let repo = DocumentIngestionRepository::new(conn);
    let source = repo
        .get_source(profile_id, source_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("来源 {source_id} 不存在或不属于档案 {profile_id}"))?;
    drop(repo);

    // 附件相对路径：**带 profile 过滤**，跨档案的附件在这里拿不到。
    let (file_name, relative_path): (String, String) = conn
        .query_row(
            "SELECT file_name, relative_path FROM learning_attachments
              WHERE id = ?1 AND profile_id = ?2",
            rusqlite::params![source.attachment_id, profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "来源指向的附件不存在或不属于当前档案".to_string())?;

    let full = sandbox::resolve_in_sandbox(attachment_root, &relative_path)?;
    let bytes = std::fs::read(&full).map_err(|e| format!("读取附件失败：{e}"))?;

    // 解析器按**自动发现**构造；运行时缺失时它自己会给出可恢复的
    // DOCLING_UNAVAILABLE —— 命令层不需要（也不应该）在这里提前分支。
    let parser = crate::document_intelligence::docling_parser::DoclingParser::discover();

    match parser {
        Some(parser) => {
            let outcome = if retry {
                ingestion::retry_ingestion(conn, &parser, profile_id, source_id, &file_name, &bytes)
            } else {
                ingestion::ingest_source(conn, &parser, profile_id, source_id, &file_name, &bytes)
            };
            outcome.map_err(|e| e.to_string())
        }
        None => {
            // 运行时完全不在：仍然要走一遍状态机，让作业以**可恢复的 Failed**
            // 收尾，而不是让 UI 拿到一个没有任何记录的裸错误。
            let missing = crate::document_intelligence::parser::UnavailableParser::new();
            let outcome = if retry {
                ingestion::retry_ingestion(
                    conn, &missing, profile_id, source_id, &file_name, &bytes,
                )
            } else {
                ingestion::ingest_source(conn, &missing, profile_id, source_id, &file_name, &bytes)
            };
            outcome.map_err(|e| e.to_string())
        }
    }
}

// ============================ 检索（M6 可达性） ============================

/// 用既有词法检索 + 既有 Context Compiler 编译一份有界文档上下文。
///
/// 这是 M6 的**可达性入口**：证明文档 chunk 真的参与了既有检索链路，
/// 而不是停在「写进索引但没人读」的状态。
/// `semantic_enabled = false` 时词法路径完整可用。
#[tauri::command]
pub fn search_document_context(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    query: String,
    source_ids: Option<Vec<String>>,
    semantic_enabled: Option<bool>,
) -> Result<ContextPack, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    retrieval::compile_document_context(
        &conn,
        profile_id,
        &query,
        source_ids.as_deref().unwrap_or(&[]),
        semantic_enabled.unwrap_or(false),
    )
}
