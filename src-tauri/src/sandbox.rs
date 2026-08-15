//! Higher Runtime Sandbox（DEV-0022）：路径安全 Guard。
//!
//! 原则：除「用户主动选择的导入源文件」与「用户配置的 AI Base URL」外，
//! Higher 所有持久读写必须限制在自己的数据目录（dev = src-tauri/.data，prod = app_data_dir）。
//!
//! - 相对路径静态检查：拒绝 `..`、绝对路径（含盘符 `/` `\`）、UNC、空路径
//! - 解析后 canonicalize：目标必须仍以 sandbox_root 为前缀（防编码/分隔符逃逸）
//! - 仅 `resolve_import_source` 允许 Sandbox 外路径（用户在系统 Dialog 主动选择的单个文件）

use std::path::{Component, Path, PathBuf};

/// 静态校验 relative_path 合法性（在拼接前执行）。
pub fn validate_relative(rel: &str) -> Result<(), String> {
    let rel = rel.trim();
    if rel.is_empty() {
        return Err("附件路径为空".to_string());
    }
    // 统一检查两种分隔符下的 ..
    let norm = rel.replace('\\', "/");
    if norm.contains("..") {
        return Err("附件路径不得包含 ..".to_string());
    }
    // 绝对路径（Unix 风格）
    if norm.starts_with('/') {
        return Err("附件路径必须是相对路径".to_string());
    }
    // Windows 盘符（C: D: …）或任意 "X:" 前缀
    let bytes = norm.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return Err("附件路径不得包含盘符（必须为相对路径）".to_string());
    }
    // UNC（\\server\share）与反斜杠开头
    if rel.starts_with("\\\\") || rel.starts_with('\\') {
        return Err("附件路径不得为网络路径或绝对路径".to_string());
    }
    Ok(())
}

/// sandbox_root + relative_path → 规范化后的目标路径。
/// 先静态校验，再规范化并验证仍在 sandbox_root 内（双重防线）。
pub fn resolve_in_sandbox(sandbox_root: &Path, rel: &str) -> Result<PathBuf, String> {
    validate_relative(rel)?;
    let root = sandbox_root.canonicalize().map_err(|_| "附件存储目录不可用".to_string())?;
    let joined = root.join(rel.trim());

    // 逐组件规范化（不要求文件已存在）
    let mut normalized = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::ParentDir => return Err("路径越界（..）已拒绝".to_string()),
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.starts_with(&root) {
        return Err("路径越界：目标不在 Higher 数据目录内".to_string());
    }

    // 文件已存在时再做 canonicalize 双重确认（解析符号链接等）
    if normalized.exists() {
        let canon = normalized
            .canonicalize()
            .map_err(|_| "附件路径解析失败".to_string())?;
        if !canon.starts_with(&root) {
            return Err("路径越界：目标不在 Higher 数据目录内".to_string());
        }
        return Ok(canon);
    }
    Ok(normalized)
}

/// 唯一允许的 Sandbox 外路径：用户在系统 Dialog 主动选择的具体文件（导入源）。
/// 必须是已存在的文件（不允许目录；不扫描其所在目录）。
pub fn resolve_import_source(source: &str) -> Result<PathBuf, String> {
    let p = Path::new(source);
    if source.trim().is_empty() {
        return Err("未选择文件".to_string());
    }
    let meta = std::fs::metadata(p).map_err(|_| "所选文件不可访问".to_string())?;
    if meta.is_dir() {
        return Err("请选择具体文件（不支持选择文件夹）".to_string());
    }
    Ok(p.to_path_buf())
}
