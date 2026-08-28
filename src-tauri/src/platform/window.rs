//! 主窗口创建（DEV-MOBILE-001 §40-42）。
//!
//! Windows（零回归契约，§41）：标题 "Higher"、1024×720、可缩放、
//! 无原生装饰（自绘标题栏）、debug 使用 `src-tauri/.webview-data`。
//!
//! Android（§42）：仅执行 Tauri Android 支持的主 WebView 创建；
//! 禁止 desktop 尺寸 / 装饰 / maximize / minimize / drag region / 自定义 data_directory。

/// 创建主窗口（label = "main"）。
pub fn build_main_window(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(desktop)]
    {
        use tauri::{WebviewUrl, WebviewWindowBuilder};

        let mut builder = WebviewWindowBuilder::new(
            app,
            "main",
            WebviewUrl::App("index.html".into()),
        )
        .title("Higher")
        .inner_size(1024.0, 720.0)
        .resizable(true)
        // DEV-0065.1 §13：移除 Windows 原生标题栏（白条根因）；
        // 前端 .titlebar（34px 自绘）接管 拖拽/双击最大化/最小化/关闭。
        // 禁止 transparent/fullscreen 等（§14）——壁纸是 WebView 背景，非 OS 透明。
        .decorations(false);

        #[cfg(debug_assertions)]
        {
            // 开发模式：webview 数据目录放项目本地 .webview-data/，
            // 避免污染系统 AppData 并支持在受限环境（如沙箱）中调试
            let webview_data_dir =
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".webview-data");
            std::fs::create_dir_all(&webview_data_dir)?;
            builder = builder.data_directory(webview_data_dir);
        }

        builder.build()?;
    }

    #[cfg(mobile)]
    {
        use tauri::{WebviewUrl, WebviewWindowBuilder};

        // Android：仅创建主 WebView（无桌面专属配置）
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into())).build()?;
    }

    Ok(())
}
