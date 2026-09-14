// Foundation 2.0 §6: knowledge-domain commands (KnowledgeDocument / workspace / move).
use crate::db;
use crate::sandbox;
use crate::AttachmentDir;
use crate::repository;
use crate::repository::attachment::AttachmentRepository;
use crate::repository::learning_item::LearningItemRepository;
use crate::repository::study_session::StudySessionRepository;
use rusqlite::Connection;

// =============== Knowledge Move（DEV-0028） ===============

/// 移动知识节点（拒绝：自己/后代/跨 Goal/非法 parent）。
#[tauri::command]
pub fn move_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    new_parent_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn).move_item(id, new_parent_id)
}


// =============== Session Note / 学习附件（Section 6 increment 8，verbatim from lib.rs） ===============
// =============== Session Note / 学习记录（DEV-0017） ===============

/// 更新本次学习笔记（Learning Workspace 自动保存；不影响 Knowledge content）。
#[tauri::command]
pub fn update_session_note(
    state: tauri::State<'_, db::DbState>,
    session_id: i64,
    note: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .update_note(session_id, &note)
        .map_err(|e| e.to_string())
}

/// 某知识节点的学习记录（Knowledge"学习记录"区域）。
#[tauri::command]
pub fn list_sessions_by_learning_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_by_learning_item(learning_item_id, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

/// 按 id 读取 Session（Learning Workspace 加载）。
#[tauri::command]
pub fn get_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn).get(id).map_err(|e| e.to_string())
}

// =============== 学习附件（DEV-0018，文件本体在 app data / attachments） ===============

/// 计算附件存储相对目录：<profile>/<goal>/<item>/，并生成唯一文件名。
pub fn attachment_target(
    conn: &Connection,
    dir: &std::path::Path,
    profile_id: i64,
    item_id: Option<i64>,
    original_name: &str,
) -> Result<(std::path::PathBuf, String), String> {
    let repo = AttachmentRepository::new(conn);
    // v013 Profile First：item 可空；目录只按 profile 组织（不再要求 Goal 存在）
    if let Some(id) = item_id {
        repo.validate_public(profile_id, id)
            .map_err(|e| e.to_string())?;
    }
    let ext = std::path::Path::new(original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "bin".to_string());
    let uuid = {
        let mut b = [0u8; 8];
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        b[..8].copy_from_slice(&(t.as_nanos() as u64).to_le_bytes());
        b.iter().map(|x| format!("{:02x}", x)).collect::<String>()
    };
    let file_name = format!("{}.{}", uuid, ext);
    let rel = match item_id {
        Some(id) => format!("{}/item/{}/{}", profile_id, id, file_name),
        None => format!("{}/session/{}", profile_id, file_name),
    };
    let full = dir.join(&rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建附件目录失败：{}", e))?;
    }
    Ok((full, rel))
}

/// 从本地文件添加附件（文件选择由前端 dialog 插件完成，Rust 负责复制）。
/// source_path 是唯一允许的 Sandbox 外路径（用户主动选择的单个文件）。
#[tauri::command]
pub fn add_learning_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    attachment_type: String,
    source_path: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 导入源：必须是用户选择的已存在文件（拒绝目录；不扫描所在目录）
    let src = sandbox::resolve_import_source(&source_path)?;
    let original = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, &original)?;
    std::fs::copy(&src, &full).map_err(|e| format!("复制附件失败：{}", e))?;
    let mime = mime_from_ext(&rel);
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            &attachment_type,
            &original,
            &rel,
            mime.as_deref(),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            // DB 失败时清理已复制文件，避免孤儿文件
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 保存画图（Canvas PNG base64）为 drawing 附件。
#[tauri::command]
pub fn save_drawing_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    data_base64: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, "drawing.png")?;
    let bytes = base64_decode(data_base64.trim())?;
    std::fs::write(&full, bytes).map_err(|e| format!("保存画图失败：{}", e))?;
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            "drawing",
            "画图.png",
            &rel,
            Some("image/png"),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 简易 base64 解码（不引入依赖；画图 PNG 与小图读取场景）。
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = match input.trim().find(";base64,") {
        Some(i) => &input.trim()[i + 8..],
        None => input.trim(),
    };
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for ch in input.bytes() {
        if ch == b'=' || ch == b'\n' || ch == b'\r' {
            continue;
        }
        let v = TABLE
            .iter()
            .position(|&t| t == ch)
            .ok_or_else(|| "附件数据格式无效".to_string())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// 从 base64 数据直接创建附件（DEV-0024：编辑器内 Ctrl+V 图片 / 拖入文件）。
/// 数据在 WebView 读取（用户主动粘贴/拖入），复制进 Higher Sandbox。
#[tauri::command]
pub fn add_attachment_from_base64(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    attachment_type: String,
    file_name: String,
    mime_type: Option<String>,
    data_base64: String,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, &file_name)?;
    let bytes = base64_decode(&data_base64)?;
    if bytes.is_empty() {
        return Err("文件内容为空".to_string());
    }
    std::fs::write(&full, &bytes).map_err(|e| format!("保存附件失败：{}", e))?;
    let mime = mime_type.or_else(|| mime_from_ext(&rel));
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            &attachment_type,
            &file_name,
            &rel,
            mime.as_deref(),
            "",
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

pub fn mime_from_ext(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "png" => Some("image/png".into()),
        "jpg" | "jpeg" => Some("image/jpeg".into()),
        "gif" => Some("image/gif".into()),
        "webp" => Some("image/webp".into()),
        "bmp" => Some("image/bmp".into()),
        "svg" => Some("image/svg+xml".into()),
        "mp4" => Some("video/mp4".into()),
        "webm" => Some("video/webm".into()),
        "mov" => Some("video/quicktime".into()),
        "mkv" => Some("video/x-matroska".into()),
        "pdf" => Some("application/pdf".into()),
        _ => None,
    }
}

#[tauri::command]
pub fn list_attachments_by_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
) -> Result<Vec<repository::attachment::LearningAttachment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .list_by_learning_item(learning_item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_attachments_by_session(
    state: tauri::State<'_, db::DbState>,
    session_id: i64,
) -> Result<Vec<repository::attachment::LearningAttachment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .list_by_session(session_id)
        .map_err(|e| e.to_string())
}

/// 读取附件二进制为 base64（图片缩略/原图/视频内嵌播放；仅 Sandbox 内）。
#[tauri::command]
pub fn read_attachment_image(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    id: i64,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let att = AttachmentRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or("附件不存在")?;
    // Sandbox Guard：DB 中的 relative_path 不可信（可能被篡改），必须校验
    let full = sandbox::resolve_in_sandbox(&adir.0, &att.relative_path)
        .map_err(|_| "附件路径非法，已拒绝读取".to_string())?;
    let bytes = std::fs::read(&full).map_err(|_| "附件文件已丢失（请删除该附件记录）".to_string())?;
    let b64 = base64_encode(&bytes);
    let mime = att
        .mime_type
        .clone()
        .or_else(|| mime_from_ext(&att.relative_path))
        .unwrap_or_else(|| "application/octet-stream".into());
    Ok(serde_json::json!({
        "id": att.id,
        "file_name": att.file_name,
        "mime_type": mime,
        "base64": b64,
    }))
}

/// 删除附件（DB 记录 + 仅 Sandbox 内的本地文件；外部文件绝不受影响）。
#[tauri::command]
pub fn delete_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let rel = AttachmentRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())?;
    if let Some(rel) = rel {
        // Sandbox Guard：即使 DB relative_path 被篡改为 ../../xxx，也无法删除外部文件
        match sandbox::resolve_in_sandbox(&adir.0, &rel) {
            Ok(path) => {
                let _ = std::fs::remove_file(path);
            }
            Err(_) => {
                // 路径非法：DB 记录已删除，文件跳过（不触碰任何外部文件）
            }
        }
    }
    Ok(())
}

pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

