pub mod ai;
pub mod db;
pub mod migrations;
pub mod notifications;
pub mod platform;
pub mod repository;
pub mod sandbox;
pub mod sync;
pub mod commands;

// Foundation 2.0 §6: re-export extracted command handlers at crate root so the
// invoke_handler! list below stays unchanged (command *names* are preserved).
use commands::system::*;
use commands::profile::*;
use commands::planning::*;
use commands::recurrence::*;
use commands::knowledge::*;
use commands::agent::*;
use commands::data::*;
use commands::settings::*;
use commands::sync::*;

use repository::{
    adjustment::AdjustmentRepository, attachment::AttachmentRepository,
    evaluation::EvaluationRepository, feedback::FeedbackRepository, goal::GoalRepository,
    insight::InsightRepository, learning_item::LearningItemRepository, plan::PlanRepository,
    setting::SettingRepository, study_profile::StudyProfileRepository,
    study_session::StudySessionRepository, study_stage::StudyStageRepository,
    task::TaskRepository,
};
use rusqlite::Connection;
use tauri::Manager;

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

/// DEV-SYNC-003：barcode-scanner crate 为 mobile-only（#![cfg(mobile)]），
/// 桌面端注入 no-op 插件占位以保持 Builder 链一致。
#[cfg(mobile)]
fn mobile_barcode_scanner_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_barcode_scanner::init()
}

#[cfg(not(mobile))]
fn mobile_barcode_scanner_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("barcode-scanner").build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // DEV-0077.2 Part A §五：Startup Trace T0——进程/应用装配起点（只测不优化，
    // 定位瓶颈后才允许动实现；debug log 一行，无重量级 telemetry）。
    // DEV-MOBILE-001 F1 §九：Android 冷启动定位日志（仅 mobile，Windows stdout 零变化）。
    #[cfg(mobile)]
    println!("[ANDROID-BOOT] PROCESS_START");
    let t0 = std::time::Instant::now();
    tauri::Builder::default()
        .setup(move |app| {
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
            println!(
                "[HigherStartup] t1_window_built_ms={t1} t2_db_migration_ready_ms={t2}"
            );

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
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // DEV-SYNC-003：相机扫码插件仅 mobile 实现（crate 整体 #![cfg(mobile)]）
        .plugin(mobile_barcode_scanner_plugin())
        .invoke_handler(tauri::generate_handler![
            // DB
            ping_db,
            db_status,
            // StudyProfile
            create_study_profile,
            get_study_profile,
            list_study_profiles,
            update_study_profile,
            set_active_study_profile,
            get_active_study_profile,
            clear_active_study_profile,
            get_profile_calendar,
            // V2 查询（复盘 / 进度）
            get_profile_day_sessions,
            get_profile_day_evaluations,
            get_knowledge_status_counts,
            get_evaluation_stats_by_profile,
            // Goal
            create_goal,
            get_goal_tree,
            create_goal_node,
            delete_goal_node,
            get_legacy_planning_counts,
            list_goals,
            list_goals_by_profile,
            update_goal,
            archive_goal,
            restore_goal,
            // LearningItem
            create_learning_item,
            list_learning_items,
            list_learning_items_by_goal,
            list_learning_items_by_profile,
            create_root_learning_item,
            create_child_learning_item,
            update_learning_item_status,
            update_learning_item,
            delete_learning_item,
            get_learning_item_path,
            update_learning_item_content,
            get_learning_item_stats,
            // Task
            create_task,
            list_today_tasks,
            list_today_tasks_by_profile,
            list_all_tasks,
            list_all_tasks_by_profile,
            complete_task,
            uncomplete_task,
            update_task,
            delete_task,
            archive_task,
            unarchive_task,
            list_archived_tasks_by_profile,
            list_tasks_by_range_by_profile,
            create_recurring_rule,
            list_recurring_rules_by_profile,
            update_recurring_rule,
            set_recurring_rule_enabled,
            delete_recurring_rule,
            materialize_recurring_tasks,
            materialize_recurring_tasks_range,
            materialize_recurring_rolling,
            // StudySession
            start_session,
            start_task_session,
            start_quick_session,
            attach_session,
            end_session,
            update_session_title,
            update_session_document,
            correct_session_time,
            unlink_session_item,
            delete_session,
            get_active_session,
            list_recent_sessions,
            list_recent_sessions_by_profile,
            has_active_session,
            // StudyStage
            create_study_stage,
            list_study_stages,
            update_study_stage,
            complete_study_stage,
            archive_study_stage,
            delete_study_stage,
            // Plan
            create_plan,
            list_plans,
            list_plans_by_stage,
            update_plan,
            complete_plan,
            archive_plan,
            delete_plan,
            // Feedback（DEV-0013）
            create_feedback,
            get_feedback,
            update_feedback,
            resolve_feedback,
            dismiss_feedback,
            list_feedbacks_by_profile,
            list_open_feedbacks_by_profile,
            list_feedbacks_by_learning_item,
            list_feedbacks_by_evaluation,
            count_feedbacks_by_status_by_profile,
            // Adjustment（DEV-0014）
            create_adjustment,
            get_adjustment,
            list_adjustments_by_feedback,
            list_adjustments_by_profile,
            list_pending_adjustments_by_profile,
            mark_adjustment_completed,
            cancel_adjustment,
            count_adjustments_by_status_by_profile,
            arrange_relearn_adjustment,
            // Insight / 周期复盘（DEV-0015）
            get_profile_range_sessions,
            get_profile_range_evaluations,
            get_profile_range_tasks,
            get_profile_range_feedbacks_created,
            get_profile_range_feedbacks_resolved,
            get_profile_range_adjustments,
            get_learning_trend,
            get_next_actions,
            // AI 设置（DEV-0016）
            get_ai_settings,
            save_ai_settings,
            test_ai_connection,
            list_ai_provider_profiles,
            get_ai_provider_profile,
            create_ai_provider_profile,
            update_ai_provider_profile,
            delete_ai_provider_profile,
            get_active_ai_profiles,
            set_active_ai_profiles,
            test_ai_provider_connection,
            test_ai_provider_compatibility,
            // Session Note / 学习记录（DEV-0017）
            update_session_note,
            list_sessions_by_learning_item,
            get_session,
            // 学习附件（DEV-0018）
            add_learning_attachment,
            save_drawing_attachment,
            add_attachment_from_base64,
            list_attachments_by_item,
            list_attachments_by_session,
            read_attachment_image,
            delete_attachment,
            // UI 设置 KV（DEV-0022：ui.ai_panel_open 等界面偏好；不新建 Migration）
            get_ui_setting,
            set_ui_setting,
            // 学习提醒（DEV-0042）
            get_notification_enabled,
            set_notification_enabled,
            sync_notifications,
            // Goal Tree + Learning Data + Mastery（DEV-0050）
            get_learning_stats,
            get_learning_trend_v2,
            get_latest_mastery,
            list_mastery_history,
            assess_mastery,
            // Knowledge Documents（DEV-0051）
            create_knowledge_document,
            get_knowledge_document,
            list_knowledge_documents,
            update_knowledge_document,
            rename_knowledge_document,
            delete_knowledge_document,
            get_knowledge_workspace,
            add_document_attachment,
            add_document_attachment_from_base64,
            save_document_drawing,
            list_attachments_by_document,
            // DEV-0052 Personal Intelligence
            get_ai_mode,
            set_ai_mode,
            create_ai_conversation,
            list_ai_conversations,
            list_ai_messages,
            archive_ai_conversation,
            set_ai_conversation_mode,
            search_higher,
            list_memory_records,
            dismiss_memory_record,
            get_ai_change_set,
            list_ai_change_set_operations,
            set_ai_change_op_selected,
            apply_ai_change_set,
            reject_ai_change_set,
            undo_ai_change_set,
            import_personalization_files,
            list_personalization_sources,
            get_user_profile_template,
            list_ai_memories,
            confirm_ai_memory,
            reject_ai_memory,
            update_ai_memory,
            delete_ai_memory,
            get_ai_profile,
            save_ai_profile,
            delete_personalization_source,
            get_personalization_profile,
            compile_personalization,
            confirm_personalization_profile,
            edit_personalization_profile,
            get_requirement_template,
            // DEV-0059 新增命令
            list_personalization_profile_versions,
            list_sources_for_personal_profile_version,
            create_goal_target,
            list_goal_targets,
            list_active_goal_targets,
            activate_goal_target,
            replace_goal_target,
            dismiss_goal_target,
            list_legacy_goal_candidates,
            create_planning_blueprint,
            list_planning_blueprints,
            get_planning_blueprint,
            get_active_planning_blueprint,
            activate_planning_blueprint,
            add_planning_phase,
            list_planning_phases,
            add_planning_milestone,
            list_planning_milestones,
            update_planning_blueprint_meta,
            update_planning_review_cadence,
            update_planning_phase,
            delete_planning_phase,
            update_planning_milestone,
            delete_planning_milestone,
            create_planning_review_due,
            list_planning_reviews,
            set_planning_review_status,
            is_planning_review_due,
            get_planning_review_risk,
            prepare_current_planning_review,
            prepare_planning_review_ai,
            run_planning_review_ai,
            import_planning_source,
            list_planning_sources,
            get_planning_source_text,
            write_export_file,
            get_web_search_settings,
            set_web_search_settings,
            vault_status,
            vault_unlock,
            vault_lock,
            vault_list_events,
            vault_list_snapshots,
            vault_create_snapshot,
            vault_export_events,
            ai_start_run,
            ai_cancel_run,
            ai_get_run_snapshot,
            ai_active_run_count,
            // DEV-0077 Phase U1：Adjustment Proposal 应用 / 暂不调整
            apply_adaptation_proposal,
            dismiss_adaptation_proposal,
            open_external_url,
            // DEV-0053 Daily & Dual-Tree
            get_daily_learning_report,
            list_unassigned_sessions,
            organize_session_into_knowledge,
            set_session_activity_kind,
            set_session_goal,
            create_followup_task_from_session,
            list_sessions_by_goal,
            create_task_v2,
            update_task_v2,
            get_change_set_apply_summary,
            // DEV-0054 Active Session
            list_active_sessions,
            // DEV-0055 Goal Brief / Planning Pipeline / Data
            get_final_goal_state,
            save_final_goal_brief,
            get_learning_totals,
            get_knowledge_time_distribution,
            get_time_of_day_distribution,
            get_plan_vs_actual,
            // DEV-0057 Reliability / Data Trust / Performance
            confirm_session_duration,
            rebuild_search_index,
            list_learning_items_light,
            get_attachment_asset_path,
            // AI 分析统一入口（DEV-0019/0020/0021）
            ai_analyze,
            // Progress 指标 / Knowledge Move（BATCH-03）
            get_progress_metrics,
            move_learning_item,
            reorder_learning_items,
            get_day_detail,
            // Profile Data Cleanup（DEV-0030/0036）
            preview_profile_cleanup,
            execute_profile_cleanup,
            list_backups,
            // Evaluation
            create_evaluation,
            get_evaluation,
            list_recent_evaluations,
            list_recent_evaluations_by_profile,
            list_evaluations_by_goal,
            list_evaluations_by_learning_item,
            update_evaluation,
            delete_evaluation,
            // 设备同步（DEV-SYNC-001 / DEV-SYNC-002）
            sync_server_start,
            sync_server_stop,
            sync_server_status,
            sync_workspace_status,
            sync_pair_with_server,
            sync_client_sync_now,
            sync_client_status,
            sync_conflicts_resolve,
            // DEV-SYNC-003 · QR Pairing
            sync_qr_session_start,
            sync_pair_via_qr,
            sync_unpair,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
