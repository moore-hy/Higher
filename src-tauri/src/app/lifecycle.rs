// Foundation 2.0 §6: startup lifecycle — the Tauri `.setup()` closure body,
// moved verbatim from lib.rs::run(). Behaviour and ordering are unchanged;
// `t0` (Startup Trace T0) is threaded in from builder::build().
use crate::ai;
use crate::db;
use crate::platform;
use crate::repository;
use crate::sync;
use crate::AttachmentDir;
use tauri::Manager;

pub fn setup(
    app: &mut tauri::App,
    t0: std::time::Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    // 创建主窗口（DEV-MOBILE-001 §40-42：平台差异收敛至 platform::window）
    // DEV-MOBILE-001 F1 §七：Windows 顺序不变（窗口先行）；
    // Android 在 setup 末尾 Runtime Ready 后才创建 WebView（见下方 cfg(mobile) 块）。
    #[cfg(desktop)]
    platform::window::build_main_window(app)?;

    #[cfg(mobile)]
    println!("[ANDROID-BOOT] DB_OPEN_START");

    // 初始化本地 SQLite 数据库
    // Windows：dev = src-tauri/.data（零回归）；prod = AppLocalData（DEV-0065.2R §9）
    // Android：dev/prod 一律 AppLocalData App Sandbox（DEV-MOBILE-001 §36）
    let db_dir = platform::storage::runtime_data_root(app.handle())?;
    std::fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("higher.db");
    // T1：窗口创建完成 → DB open 前
    let t1 = t0.elapsed().as_millis();
    // open 内部会自动执行待处理的 Migration
    let db_state = db::DbState::open(&db_path)?;
    #[cfg(mobile)]
    println!("[ANDROID-BOOT] DB_READY");
    // T2：DB ready + migration complete（open 内含迁移）
    let t2 = t0.elapsed().as_millis();
    println!("[HigherStartup] t1_window_built_ms={t1} t2_db_migration_ready_ms={t2}");

    // DEV-0057 §71-72：Search Index 版本门——版本缺失/变化才一次性 rebuild（不默认每次全重建）。
    {
        if let Ok(mut guard) = db_state.0.lock() {
            if let Ok(active) = guard.query_row(
                "SELECT value FROM settings WHERE key='active_profile_id'",
                [],
                |r| r.get::<_, String>(0),
            ) {
                if let Ok(pid) = active.parse::<i64>() {
                    let _ = repository::search::ensure_index_version(&mut guard, pid);
                }
            }
        }
    }

    app.manage(db_state);
    // POST-M7 §S3 / FINAL IMPLEMENTATION SAFETY PATCH §3：
    // SecretMigrationService best-effort 预迁移——后台线程执行，**不阻塞启动**；
    // SecretStore 失败 → 不清空 legacy plaintext、不伪造 secret_ref、仅 sanitized
    // 警告；可在未来启动或进入 AI Settings 时安全重试（幂等 / 可恢复）。
    {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            let state = handle.state::<db::DbState>();
            let store = ai::secret_store::production_secret_store();
            let (migrated, failed) = ai::secret_migration::run_best_effort(&state, &*store);
            if migrated > 0 || failed > 0 {
                println!(
                    "[secret-migration] migrated={migrated} pending_failed={failed} (failed rows keep legacy plaintext; will retry)"
                );
            }
        });
    }
    #[cfg(mobile)]
    println!("[ANDROID-BOOT] STATE_MANAGED");

    // 附件根目录（Windows：dev = src-tauri/.data/attachments 零回归；
    // prod = %LOCALAPPDATA%\com.higher.desktop\attachments，DEV-0065.2R §15；
    // Android：App Sandbox attachments/，DEV-MOBILE-001 §36）
    let att_root = platform::storage::attachments_root(app.handle())?;
    std::fs::create_dir_all(&att_root)?;
    app.manage(AttachmentDir(att_root));

    // DEV-0052：AI Run Manager（Active Run Registry）+ Vault
    app.manage(ai::run::RunManager::new());
    let vault_dir = platform::storage::vault_root(app.handle())?;
    std::fs::create_dir_all(&vault_dir)?;
    app.manage(ai::vault::VaultState::new(vault_dir));

    // DEV-SYNC-001-F1：设备同步服务器句柄（默认关闭，用户主动启动）。
    // 必须在 WebView 就绪前 manage，否则 sync_server_status/start/stop
    // 的 State<SyncServerHandle> 解析失败 → IPC reject → 前端永久加载中。
    app.manage(sync::server::SyncServerHandle::new());

    // 学习提醒（DEV-0042）：启动调度线程 + 按 DB 重建全部 profile 的排定通知
    // （DEV-MOBILE-001 §44-49：平台差异收敛至 platform::notification）
    platform::notification::start(app.handle());

    // DEV-MOBILE-001 F1 §七：Android 启动顺序——
    // 初始化目录 → DB/Migration → manage(DbState) → AttachmentDir →
    // RunManager → Vault → Runtime Ready 之后才创建 WebView
    // （前端首帧即有完整后端状态，避免冷启动白屏/加载中卡死）。
    #[cfg(mobile)]
    {
        platform::window::build_main_window(app)?;
        println!("[ANDROID-BOOT] WEBVIEW_CREATED");
    }

    Ok(())
}
