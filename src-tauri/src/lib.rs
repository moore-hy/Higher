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

// =============== DEV-0053 · Daily & Dual-Tree Loop ===============

/// §90：统一日报查询（Today=今天；Calendar=选中日期）。
#[tauri::command]
fn get_daily_learning_report(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<repository::daily_report::DailyReport, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::daily_report::DailyReportRepository::new(&conn).get(profile_id, &date)
}

/// §52：未归类学习列表。
#[tauri::command]
fn list_unassigned_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .list_unassigned(profile_id, limit.unwrap_or(50))
}

/// §52：整理进知识（只改 learning_item_id 关联）。
#[tauri::command]
fn organize_session_into_knowledge(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    learning_item_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .set_learning_item(session_id, profile_id, Some(learning_item_id))?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "session",
        session_id,
        profile_id,
        "session",
        "",
        None,
    );
    // 刷新索引标题
    if let Ok(s) = repository::study_session::StudySessionRepository::new(&conn).get(session_id) {
        if let Some(sess) = s {
            let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
                "session",
                session_id,
                profile_id,
                &sess.title,
                sess.note.as_deref().unwrap_or(""),
                Some(&sess.started_at),
            );
        }
    }
    Ok(())
}

/// §35：修改活动分类。
#[tauri::command]
fn set_session_activity_kind(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    activity_kind: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .set_activity_kind(session_id, profile_id, &activity_kind)
}

/// §35：修改 Session 目标关联。
#[tauri::command]
fn set_session_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    goal_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let n = conn
        .execute(
            "UPDATE study_sessions SET goal_id = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND profile_id = ?3",
            rusqlite::params![goal_id, session_id, profile_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("学习记录不存在或不属于当前档案".to_string());
    }
    Ok(())
}

/// §36：从 Activity 生成后续任务（新建 Task；原 Activity 保留）。
#[tauri::command]
fn create_followup_task_from_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    planned_date: Option<String>,
    estimated_minutes: Option<i64>,
) -> Result<repository::task::Task, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let sess = repository::study_session::StudySessionRepository::new(&conn)
        .get(session_id)
        .map_err(|e| e.to_string())?
        .filter(|s| s.profile_id == profile_id)
        .ok_or("学习记录不存在或不属于当前档案")?;
    let title = if sess.title.trim().is_empty() {
        format!("学习记录 #{}", sess.id)
    } else {
        format!("继续：{}", sess.title)
    };
    repository::task::TaskRepository::new(&conn).create_v2(
        profile_id,
        sess.goal_id,
        &title,
        planned_date.as_deref(),
        None,
        sess.learning_item_id,
        estimated_minutes,
        "structured",
        "normal",
    )
}

/// §45：Goal Detail 学习记录（Day 直查；Month/Annual/Final 经 descendant）。
#[tauri::command]
fn list_sessions_by_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .list_by_goal(profile_id, goal_id, limit.unwrap_or(50))
}

/// §23：Task V2 全字段创建。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn create_task_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<repository::task::Task, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let t = repository::task::TaskRepository::new(&conn).create_v2(
        profile_id,
        goal_id,
        &title,
        planned_date.as_deref(),
        planned_time.as_deref(),
        learning_item_id,
        estimated_minutes,
        task_kind.as_deref().unwrap_or("structured"),
        priority.as_deref().unwrap_or("normal"),
    )?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "task",
        t.id,
        profile_id,
        &t.title,
        &t.title,
        None,
    );
    Ok(t)
}

/// §23：Task V2 全字段编辑。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn update_task_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::task::TaskRepository::new(&conn).update_v2(
        id,
        &title,
        planned_date.as_deref(),
        planned_time.as_deref(),
        learning_item_id,
        goal_id,
        estimated_minutes,
        task_kind.as_deref().unwrap_or("structured"),
        priority.as_deref().unwrap_or("normal"),
    )?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "task",
        id,
        profile_id,
        &title,
        &title,
        None,
    );
    Ok(())
}

/// §11：Apply 成功反馈数据（前端生成 ✓ 已应用 X 项消息，不由模型生成）。
#[tauri::command]
fn get_change_set_apply_summary(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
) -> Result<Vec<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let cs = repository::changeset::ChangeSetRepository::new(&conn)
        .get(change_set_id, profile_id)?
        .ok_or("ChangeSet 不存在或不属于当前档案")?;
    if cs.status != "applied" {
        return Ok(vec![]);
    }
    let ops = repository::changeset::ChangeSetRepository::new(&conn)
        .list_operations(change_set_id, profile_id)?;
    let mut lines = Vec::new();
    for op in ops.iter().filter(|o| o.selected) {
        let entity_label = match op.entity_type.as_str() {
            "goal" => "目标",
            "task" => "任务",
            "knowledge" => "知识节点",
            "document" => "文档",
            "session" => "学习记录",
            "evaluation" => "验证",
            "personalization" => "私人档案",
            _ => "条目",
        };
        let title = op
            .after_json
            .get("title")
            .or_else(|| op.after_json.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        match op.action.as_str() {
            "create" => lines.push(format!("✓ 已创建{}「{}」", entity_label, title)),
            "update" => lines.push(format!("✓ 已修改{}「{}」", entity_label, title)),
            "delete" => lines.push(format!("✓ 已删除{}", entity_label)),
            "status_change" => lines.push(format!("✓ 已调整{}「{}」", entity_label, title)),
            "move" => lines.push(format!("✓ 已移动{}", entity_label)),
            _ => lines.push(format!("✓ 已应用{}", entity_label)),
        }
    }
    if lines.is_empty() {
        lines.push("✓ 已应用修改".to_string());
    }
    Ok(lines)
}

/// 最近备份列表（DEV-0036 §108：仅显示；不做恢复 API）。
#[tauri::command]
fn list_backups(app: tauri::AppHandle) -> Result<Vec<repository::BackupInfo>, String> {
    let dir = backups_dir(&app)?;
    let mut out: Vec<repository::BackupInfo> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with("higher-") && name.ends_with(".db")) {
                return None;
            }
            let size = e.metadata().ok().map(|m| m.len()).unwrap_or(0);
            Some(repository::BackupInfo {
                name: name.clone(),
                size_bytes: size,
                path: e.path().to_string_lossy().to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out.truncate(10);
    Ok(out)
}

/// 执行清理：先备份（失败则取消）→ 单事务删除 → 事务成功后删 Sandbox 附件文件。
#[tauri::command]
fn execute_profile_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope)
        .ok_or("未知的清理范围")?;
    // 1) 备份（失败 → 禁止删除）
    // DEV-MOBILE-001 §38：改经 AppHandle 真实运行路径（Android = App Sandbox）；
    // Windows 与原 db::DbState::database_path() 同指（dev = .data，prod = AppLocalData）。
    let db_path = {
        let _conn = state.0.lock().map_err(|e| e.to_string())?;
        runtime_db_path(&app)
    };
    let _backup = backup_database(&app, &db_path)?;

    // 2) 事务删除 + 收集附件 relative_path
    let (preview, files) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::cleanup::CleanupRepository::new(&conn)
            .execute_collecting(profile_id, scope, &today)?
    };

    // 3) DB 成功后删除 Sandbox 文件（Path Guard 阻止越界；失败不回滚 DB，仅跳过）
    for rel in files {
        if let Ok(path) = sandbox::resolve_in_sandbox(&adir.0, &rel) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(preview)
}

// =============== UI 设置 KV（DEV-0022：界面偏好，如 ui.ai_panel_open） ===============

/// 读取一条 UI 偏好（仅允许 ui. 前缀，避免读取 ai.api_key 等敏感值）。
#[tauri::command]
fn get_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
) -> Result<Option<String>, String> {
    if !key.starts_with("ui.") {
        return Err("仅允许读取 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .get(&key)
        .map_err(|e| e.to_string())
}

/// 写入一条 UI 偏好（仅允许 ui. 前缀）。
#[tauri::command]
fn set_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    if !key.starts_with("ui.") {
        return Err("仅允许写入 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .set(&key, &value)
        .map_err(|e| e.to_string())
}

// =============== 学习提醒设置（DEV-0042；settings 键 notifications.enabled） ===============

/// 读取学习提醒开关（默认开启；"0" = 关闭）。
#[tauri::command]
fn get_notification_enabled(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(notifications_setting_enabled(&conn))
}

/// 写入学习提醒开关；写入后立即重同步（关闭 = 只清理已排定项）。
#[tauri::command]
fn set_notification_enabled(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        SettingRepository::new(&conn)
            .set("notifications.enabled", if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 前端触发的学习提醒同步（fire-and-forget；对全部 profile 重建排定）。
#[tauri::command]
fn sync_notifications(app: tauri::AppHandle) -> Result<(), String> {
    notifications::resync(&app);
    Ok(())
}

fn notifications_setting_enabled(conn: &Connection) -> bool {
    SettingRepository::new(conn)
        .get("notifications.enabled")
        .ok()
        .flatten()
        .map(|v| v != "0")
        .unwrap_or(true)
}

// =============== AI 分析（DEV-0019/0020/0021/0022 统一入口） ===============

/// 上下文摘要标签（Panel 显示"已提供上下文"；与 Tool Trace 明确区分，不伪装成工具）。
fn context_labels(act: ai::AiAction) -> Vec<String> {
    // DEV-0046：daily_review 只注入 profile + 当日数据（不注入全库摘要）
    if matches!(act, ai::AiAction::DailyReview) {
        return vec![
            "学习档案".to_string(),
            "当日任务".to_string(),
            "当日学习记录（含笔记摘要）".to_string(),
            "当日验证".to_string(),
            "当日知识关联".to_string(),
        ];
    }
    let mut v = vec![
        "学习档案".to_string(),
        "学习目标".to_string(),
        "当前阶段".to_string(),
        "知识结构".to_string(),
        "最近学习摘要".to_string(),
    ];
    match act {
        ai::AiAction::SessionAnalysis => {
            v.push("本次学习笔记".to_string());
            v.push("附件元数据".to_string());
        }
        ai::AiAction::KnowledgeAnalysis | ai::AiAction::KnowledgeOrganize => {
            v.push("当前知识正文".to_string());
            v.push("子节点内容".to_string());
            v.push("最近学习笔记".to_string());
        }
        ai::AiAction::PlanningAnalysis | ai::AiAction::TodaySuggestion => {
            v.push("学习计划".to_string());
            v.push("今日任务".to_string());
            v.push("最近验证".to_string());
        }
        ai::AiAction::ProfileAnalysis => {
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::AssistantChat => {
            // 按页面附带的默认理解对象（若有）；档案级数据仍全部提供
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::DailyReview => {
            // 已在函数开头提前返回（只注入当日数据）
        }
        ai::AiAction::MasteryAssessment => {
            // assess_mastery 专用（不经 ai_analyze 入口；此分支不可达，防御完备）
            v.push("目标树".to_string());
            v.push("周期任务".to_string());
            v.push("周期学习记录".to_string());
            v.push("周期验证".to_string());
            v.push("关联知识正文".to_string());
        }
    }
    v
}

/// 运行 AI 分析：前端只传 ID，后端构建 Context（Profile Scope）并调用 DeepSeek。
/// 返回 content + usage + 真实 tool_trace + context 标签 + 耗时/轮数；AI 不写库。
/// history：Panel 多轮对话最近消息（由前端按预算截断后传入；业务上下文仍由后端重建）。
#[tauri::command]
async fn ai_analyze(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    action: String,
    session_id: Option<i64>,
    learning_item_id: Option<i64>,
    user_instruction: Option<String>,
    history: Option<Vec<(String, String)>>,
    date: Option<String>,
) -> Result<ai::AiResult, String> {
    let started = std::time::Instant::now();
    let act = ai::AiAction::from_str(&action)
        .ok_or_else(|| format!("未知的 AI 功能：{}", action))?;

    let (context, page_labels, primary_cfg) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let cfg = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
                profile_id,
                action: act,
                session_id,
                learning_item_id,
                user_instruction: user_instruction.clone(),
                date,
            },
        )?;
        // assistant_chat：页面附带的默认对象追加为标签（区分"页面提供"与"档案提供"）
        let mut extra: Vec<String> = Vec::new();
        if act == ai::AiAction::AssistantChat {
            if session_id.is_some() {
                extra.push("当前会话（本次学习）".to_string());
            }
            if learning_item_id.is_some() {
                extra.push("当前知识节点".to_string());
            }
        }
        (ctx, extra, cfg)
    };

    let client = ai::client::AiClient::new(primary_cfg);
    let mut messages = vec![
        ai::client::ChatMessage::system(ai::prompts::SYSTEM_PROMPT),
    ];
    // Panel 对话历史（role, content；仅 user/assistant；后端不信任其他 role）
    if let Some(hist) = &history {
        for (role, content) in hist.iter() {
            if (role == "user" || role == "assistant") && !content.trim().is_empty() {
                messages.push(ai::client::ChatMessage {
                    role: role.clone(),
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }
    messages.push(ai::client::ChatMessage::user(format!(
        "{}\n\n{}",
        context,
        ai::prompts::user_instruction(act)
    )));

    let mut labels = context_labels(act);
    let mut page_idx = labels.len();
    for e in page_labels {
        labels.insert(page_idx, e);
        page_idx += 1;
    }

    // 所有 action 均要求 JSON；一次结构修复重试（最多一次；禁止无限重试）
    for attempt in 0..2 {
        let (content, usage, trace, rounds) = if act.allow_tools() {
            ai::tools::run_with_tools(&state, &client, profile_id, messages.clone(), act.require_json())
                .await?
        } else {
            let c = client
                .chat(messages.clone(), act.require_json(), None, Some(4096))
                .await?;
            let content = c.content.ok_or_else(|| "模型没有返回内容".to_string())?;
            (content, c.usage, Vec::new(), 0)
        };

        // JSON 校验（assistant_chat 额外校验协议类型）
        let trimmed = content.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        let ok = serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
            && (act != ai::AiAction::AssistantChat
                || ai::AssistantChatResponse::parse(trimmed).is_ok());
        if ok || attempt == 1 {
            return Ok(ai::AiResult {
                action: act.as_str().to_string(),
                content: trimmed.to_string(),
                prompt_tokens: nonzero(usage.prompt_tokens),
                completion_tokens: nonzero(usage.completion_tokens),
                total_tokens: nonzero(usage.total_tokens),
                tool_trace: trace,
                context_provided: labels,
                duration_ms: Some(started.elapsed().as_millis() as i64),
                tool_rounds: Some(rounds),
                // §31 Provider Provenance：本次调用真实 snapshot
                provider_profile_name: Some(client.config().display_name.clone()),
                adapter_kind: Some(client.config().adapter_kind.as_str().to_string()),
                provider_model: Some(client.config().model.clone()),
            });
        }

        // 结构修复重试（仅一次）
        messages.push(ai::client::ChatMessage::assistant(content));
        messages.push(ai::client::ChatMessage::user(
            "上面的输出不是合法 JSON。请严格只输出一个合法 JSON 对象（不要 markdown 代码块、不要解释文字）。",
        ));
    }
    unreachable!()
}

fn nonzero(v: i64) -> Option<i64> {
    if v > 0 { Some(v) } else { None }
}

// =============== 设备同步（DEV-SYNC-001/002/003 · QR Pairing） ===============

/// DEV-SYNC-003 §十：生成配对二维码 payload（自动确保监听已启动；刷新 = 新 token）。
#[tauri::command]
fn sync_qr_session_start(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<String, String> {
    if !server.is_running() {
        server
            .start(std::sync::Arc::new(sync::server::DbStateProvider(app)))
            .map_err(|e| e.to_string())?;
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    server.qr_pairing_payload(&conn)
}

/// DEV-SYNC-003 §六：扫码配对——候选 IP 自动连接 + 一次性 token 握手 +
/// 首次双向同步（Android 扫码后调用；不阻塞 UI 线程）。
#[tauri::command]
fn sync_pair_via_qr(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
    payload: String,
) -> Result<sync::client::QrPairResult, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let result = sync::client::pair_via_qr(&conn, &payload, listen.as_deref())?;
    // §九：广播 sync://completed（Bootstrap 导入 + 首次同步）
    use tauri::Emitter;
    if result.pair.outcome.any_change() {
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&result.server_device_id, &result.pair.outcome),
        );
    }
    if let Some(s) = &result.sync {
        if s.applied > 0 {
            let _ = app.emit(
                "sync://completed",
                sync::server::completed_payload(&result.server_device_id, &s.received_detail),
            );
        }
    }
    Ok(result)
}

/// DEV-SYNC-003 §九：解除配对——删除 peer trust/token，业务数据保留。
#[tauri::command]
fn sync_unpair(state: tauri::State<'_, db::DbState>, peer_device_id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::unpair(&conn, &peer_device_id)
}

/// 启动本机同步监听（DEV-SYNC-002 §七：两端平等，对端可主动反向连接）。
#[tauri::command]
fn sync_server_start(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    server
        .start(std::sync::Arc::new(sync::server::DbStateProvider(app)))
        .map_err(|e| e.to_string())?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// 停止本机同步监听（配对码同时失效；已建立的 shared_token 不受影响）。
#[tauri::command]
fn sync_server_stop(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    server.stop();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// 服务器状态（运行中 / 配对码 / 已配对设备 / per-peer 待发送 / 冲突数）。
#[tauri::command]
fn sync_server_status(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// DEV-SYNC-002 §十：同步工作台状态（Windows /sync 页与 Android 详情页共用：
/// peer 卡片 / 最后同步 / per-peer 待发送 / 冲突数 / 本机监听状态）。
#[tauri::command]
fn sync_workspace_status(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::WorkspaceStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::workspace_status(&server, &conn))
}

/// 输入对端 IP + 配对码连接，完成握手 + Bootstrap 全量导入
///（§五：返回导入档案清单，UI 提供「切换到该档案」）。
#[tauri::command]
fn sync_pair_with_server(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
    ip: String,
    port: u16,
    code: String,
) -> Result<sync::client::PairOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // §七：本机若在监听，把地址随配对上报（对端此后可主动反向连接）
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let outcome = sync::client::pair_with_server(&conn, &ip, port, &code, listen.as_deref())?;
    // §九：Bootstrap 导入有变化 → 广播 sync://completed
    if outcome.outcome.any_change() {
        use tauri::Emitter;
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&outcome.server_device_id, &outcome.outcome),
        );
    }
    Ok(outcome)
}

/// DEV-SYNC-002 §十三：冲突批量处理（"local"=保留本机版 / "remote"=保留对端版）。
#[tauri::command]
fn sync_conflicts_resolve(
    state: tauri::State<'_, db::DbState>,
    resolution: String,
) -> Result<u32, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::resolve_conflicts(&conn, &resolution)
}

/// 「立即同步」——双向增量交换（DEV-SYNC-002 §七：两端平等，
/// 任意一端点击均以 client 身份连接对端监听地址完成 Push+Pull+Apply+Ack）。
#[tauri::command]
fn sync_client_sync_now(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::client::SyncSummary, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let summary = sync::client::sync_now(&conn, listen.as_deref())?;
    // §九：本机 Apply 有变化 → 广播 sync://completed（业务页面即时刷新）
    if summary.applied > 0 || summary.conflicts > 0 {
        let peer_id = sync::identity::first_peer(&conn)
            .ok()
            .flatten()
            .map(|p| p.peer_device_id)
            .unwrap_or_default();
        use tauri::Emitter;
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&peer_id, &summary.received_detail),
        );
    }
    Ok(summary)
}

/// 配对摘要状态（peer / 最后同步 / per-peer 待发送 / 冲突数）。
#[tauri::command]
fn sync_client_status(state: tauri::State<'_, db::DbState>) -> Result<sync::client::ClientStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::client_status(&conn).map_err(|e| e.to_string())
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
