pub mod ai;
pub mod db;
pub mod migrations;
pub mod notifications;
pub mod platform;
pub mod repository;
pub mod sandbox;
pub mod sync;
pub mod commands;
pub mod app;

/// 附件根目录（app data / attachments；Dev 与 Prod 均使用系统 app data 路径）。
pub struct AttachmentDir(pub std::path::PathBuf);

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
