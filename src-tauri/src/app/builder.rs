// Foundation 2.0 §6: Tauri Builder assembly — plugin registration and
// command registration. Moved verbatim from lib.rs::run(). Plugin order,
// setup hook and the generate_handler! command list are unchanged.
use crate::commands::agent::*;
use crate::commands::data::*;
use crate::commands::document::*;
use crate::commands::knowledge::*;
use crate::commands::learning_state::*;
use crate::commands::planning::*;
use crate::commands::profile::*;
use crate::commands::recurrence::*;
use crate::commands::settings::*;
use crate::commands::sync::*;
use crate::commands::system::*;
use crate::commands::training::*;

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

/// 装配 Tauri Builder，供 lib.rs::run() 启动。
pub fn build() -> tauri::Builder<tauri::Wry> {
    // Startup Trace T0：与原 lib.rs::run 语义一致（Builder::default() 之前）。
    let t0 = std::time::Instant::now();
    tauri::Builder::default()
        .setup(move |app| crate::app::lifecycle::setup(app, t0))
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
            delete_study_profile,
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
            // PRODUCT-2.0 §24 Planning Intake（Draft only，不写正式表）
            crate::commands::intake::get_planning_intake_draft,
            crate::commands::intake::save_planning_intake_draft,
            crate::commands::intake::set_planning_intake_status,
            crate::commands::intake::discard_planning_intake_draft,
            // PRODUCT-2.0 §34-§38 Knowledge Canvas
            crate::commands::knowledge::get_knowledge_canvas,
            crate::commands::knowledge::save_knowledge_canvas,
            crate::commands::knowledge::list_canvas_embeds,
            crate::commands::knowledge::add_canvas_embed,
            crate::commands::knowledge::update_canvas_embed_geometry,
            crate::commands::knowledge::delete_canvas_embed,
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
            // HIGHER CLOSED LOOP V1 §PHASE 1 / §PHASE 2
            get_learning_state,
            get_next_learning_action,
            // HIGHER COGNITIVE CORE V1.2 §19 — Today Coach 单一后端视图
            get_today_coach_snapshot,
            // HIGHER COGNITIVE CORE V1.2 §25 — Memory 页单一后端视图（一次 IPC）
            get_memory_dashboard,
            // HIGHER COGNITIVE CORE V1.2 §26 — Progress 页四轴视图（无跨轴聚合分）
            get_cognitive_progress,
            // A2-3 —— Person State V1 / Know Me（只读投影，单一入口）
            get_person_state,
            // REAL LEARNING ENGINE V1 · W4 — TrainingExperience（§19 / §13 / §15 / §20）
            create_training_run_for_item,
            get_training_session,
            record_training_interaction,
            // A2-2 —— 受控后端验证通路（唯一能产出权威判定方式的命令）
            verify_training_interaction,
            precheck_training_verification,
            transition_training_run,
            complete_training_run,
            // PACK A 收口 —— 块推进（Owner 补充决定 D11–D21）
            start_training_block,
            advance_training_block,
            try_complete_training_block,
            // GROUNDED LEARNING BRIDGE V1 · W5 —— 块接地材料读取（§10）
            get_block_grounded_material,
            // PACK A POST-PUSH AUDIT HOTFIX-01 —— 启动与提前结束（FIX D / FIX F2）
            start_training_run,
            abandon_training_run,
            // PACK A POST-PUSH AUDIT HOTFIX-01 —— 命令栏主动意图捕获（FIX H）
            crate::commands::learning_intent::capture_learning_intent_from_text,
            // HIGHER DAILY EXPERIENCE V1 §PHASE 3 / §PHASE 4
            record_micro_action,
            // HIGHER 1.0 §M1-A 有限 Learning Pack
            get_learning_pack,
            // HIGHER 1.0 §M4 / §M5 Companion Skill + World / Expedition / Return
            crate::commands::companion::get_companion_state,
            crate::commands::companion::interact_companion,
            crate::commands::companion::start_companion_expedition,
            crate::commands::companion::settle_companion_expeditions,
            crate::commands::companion::collect_companion_return,
            crate::commands::companion::get_companion_memories,
            crate::commands::companion::get_companion_learning_nudge,
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
            // NIGHT SHIFT O2 · M5 — 文档导入生产路径（v042 + 既有 SearchRepository）
            get_document_runtime_status,
            list_document_sources,
            list_document_sources_for_item,
            import_document_source,
            start_document_ingestion,
            retry_document_ingestion,
            get_document_ingestion_status,
            get_document_structure,
            cancel_document_ingestion,
            // NIGHT SHIFT O2 · M6 — 文档 chunk 进入既有 Context Compiler
            search_document_context,
        ])
}
