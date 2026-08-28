//! 通知调度启动适配（DEV-MOBILE-001 §44-49）。
//!
//! Windows（零回归契约，§45）：保持现有 `notifications.rs`
//! in-process 20s 调度线程 + 启动 resync，不改变任何语义。
//!
//! Android（§46-49）：不依赖常驻线程调度（进程随时可能被回收）；
//! Alpha 阶段暂不启用后台提醒调度（记录 P2），即时通知仍经
//! `tauri-plugin-notification`（权限拒绝不影响 App 运行，§47）。
//! 禁止为 Android 修改 Windows scheduler（§49）。

/// 启动通知调度（setup 中调用一次）。
pub fn start(app: &tauri::AppHandle) {
    #[cfg(desktop)]
    {
        crate::notifications::start_scheduler(app.clone());
        crate::notifications::resync(app);
    }

    #[cfg(mobile)]
    {
        // Android Alpha：不启动常驻调度线程（DEV-MOBILE-001 §49，P2 记录）
        let _ = app;
    }
}
