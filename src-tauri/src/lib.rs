pub mod ai;
pub mod app;
pub mod cognitive;
pub mod commands;
pub mod companion;
pub mod db;
pub mod document_intelligence;
pub mod domain_packs;
pub mod ipc;
pub mod learning_state;
pub mod memory;
pub mod migrations;
pub mod model_router;
pub mod notifications;
pub mod platform;
pub mod repository;
pub mod resource;
pub mod runtime;
pub mod sandbox;
pub mod sync;

/// 附件根目录（app data / attachments；Dev 与 Prod 均使用系统 app data 路径）。
pub struct AttachmentDir(pub std::path::PathBuf);

// §6 moved these helpers to commands/data.rs; re-exported at crate root so the
// legacy integration tests keep addressing them at their original stable paths.
pub use commands::data::{fmt_md, normalize_memory_key};

/// Repository 错误 → 人话（不暴露 FOREIGN KEY constraint failed 等技术词）。
pub fn humanize_repo_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => other.to_string(),
    }
}

// =============== 应用入口 ===============

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // DEV-MOBILE-001 F1 §九：Android 冷启动定位日志（仅 mobile，Windows stdout 零变化）。
    #[cfg(mobile)]
    println!("[ANDROID-BOOT] PROCESS_START");
    // Builder 装配（plugins + command registration）见 app/builder.rs；
    // startup lifecycle 见 app/lifecycle.rs。
    app::builder::build()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
