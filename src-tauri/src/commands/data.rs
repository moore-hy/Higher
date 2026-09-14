// Foundation 2.0 §6: data-domain commands (cleanup / export / distributions).
use crate::ai;
use crate::commands::agent::chrono_now;
use crate::commands::agent::primary_client;
use crate::db;
use crate::platform;
use crate::repository;
use crate::repository::setting::SettingRepository;
use crate::sandbox;
use crate::AttachmentDir;
use tauri::Manager;

// =============== Profile Data Cleanup（DEV-0030） ===============

/// 备份目录（Windows：dev = 项目 .higher/backups；prod = AppLocalData/backups，DEV-0065.2R §15；
/// Android：AppLocalData/backups，DEV-MOBILE-001 §33）。
pub fn backups_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = platform::storage::backups_root(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{}", e))?;
    Ok(dir)
}

/// DEV-0057 §163-164：真实运行 DB 路径（dev = 项目 .data；prod = AppLocalData/higher.db，DEV-0065.2R §14）。
/// 修复：vault 快照/备份源路径不再硬编码 CARGO_MANIFEST_DIR（prod 恒 size=0 的 Bug）。
/// DEV-0066 §13：pub(crate)——ai::commands 共享 Apply 按 AppHandle 取真实路径做快照。
/// DEV-MOBILE-001 §33：平台路径逻辑收敛至 platform::storage（Android = App Sandbox）。
pub(crate) fn runtime_db_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    platform::storage::runtime_db_path(app)
}

/// 备份数据库 → higher-YYYYMMDD-HHmmss.db；保留最近 10 个（只操作 Higher 自己的 backups 目录）。
pub fn backup_database(
    app: &tauri::AppHandle,
    db_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let dir = backups_dir(app)?;
    // 本地时间命名（无 chrono：用系统命令获取不可行；用 UTC 近似——由 Rust 标准库 SystemTime 转换）
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    // UTC+8 偏移（项目时区 Asia/Shanghai；命名用途，无需精确时区库）
    let local = now + 8 * 3600;
    let days = local / 86400;
    let rem = local % 86400;
    // civil from days
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let name = format!(
        "higher-{:04}{:02}{:02}-{:02}{:02}{:02}.db",
        y, m, d, h, mi, s
    );
    let target = dir.join(&name);
    std::fs::copy(db_path, &target).map_err(|e| format!("备份失败：{}", e))?;

    // Retention：最多 10 个（只删 higher-*.db）
    let mut backups: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("higher-") && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();
    if backups.len() > 10 {
        backups.sort(); // 文件名字典序 = 时间序
        for old in &backups[..backups.len() - 10] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(target)
}

/// 预览清理（只读）。
#[tauri::command]
pub fn preview_profile_cleanup(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope).ok_or("未知的清理范围")?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::cleanup::CleanupRepository::new(&conn).preview(profile_id, scope, &today)
}

// =============== Learning Data + AI Mastery（DEV-0050 PHASE D；Section 6 increment 10） ===============
// =============== Learning Data（DEV-0050 / PHASE D §42-45,58-60） ===============

/// 单周期三指标中的两个实数据（学习时间 + 任务完成；掌握度走 mastery 接口）。
#[tauri::command]
pub fn get_learning_stats(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_start: String,
    period_end: String,
) -> Result<repository::learning_data::LearningStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::learning_data::LearningDataRepository::new(&conn).stats(
        profile_id,
        &period_start,
        &period_end,
    )
}

/// 趋势序列（§58：day=14 / week=8(周一起) / month=12 / year=5；以当前 UTC+8 周期收尾）。
#[tauri::command]
pub fn get_learning_trend_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    bucket: String, // day | week | month | year
) -> Result<Vec<repository::learning_data::TrendPoint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let buckets = period_buckets(&bucket)?;
    let starts: Vec<String> = buckets.iter().map(|b| b.1.clone()).collect();
    let ends: Vec<String> = buckets.iter().map(|b| b.2.clone()).collect();
    let period_type = match bucket.as_str() {
        "day" => "day",
        "week" => "week",
        "month" => "month",
        _ => "year",
    };
    let mastery = repository::mastery::MasteryRepository::new(&conn)
        .trend(profile_id, period_type, &starts, &ends)
        .map_err(|e| e.to_string())?;
    repository::learning_data::LearningDataRepository::new(&conn)
        .trend(profile_id, &buckets, &mastery)
}

/// 生成趋势 buckets：(label, start, end)，旧→新，最后一个 = 当前周期。
pub fn period_buckets(bucket: &str) -> Result<Vec<(String, String, String)>, String> {
    // 当前 UTC+8 日期（学习日不变量）
    let today = now_utc8_date();
    let mut out: Vec<(String, String, String)> = Vec::new();
    match bucket {
        "day" => {
            for i in (0..14).rev() {
                let d = shift_date(&today, -(i as i64));
                out.push((d[5..].replace('-', "/"), d.clone(), d));
            }
        }
        "week" => {
            // 周一~周日；当前周为最后一段
            let dow = weekday_of(&today);
            let this_mon = shift_date(&today, -(dow as i64 - 1));
            for i in (0..8).rev() {
                let mon = shift_date(&this_mon, -(i as i64) * 7);
                let sun = shift_date(&mon, 6);
                let label = format!(
                    "{}~{}",
                    &mon[5..].replace('-', "/"),
                    &sun[5..].replace('-', "/")
                );
                out.push((label, mon, sun));
            }
        }
        "month" => {
            let (mut y, mut m) = (
                today[0..4].parse::<i64>().unwrap_or(2026),
                today[5..7].parse::<i64>().unwrap_or(1),
            );
            for _ in 0..12 {
                let label = format!("{}-{:02}", y, m);
                let dim = days_in_month(y, m);
                out.push((
                    label,
                    format!("{}-{:02}-01", y, m),
                    format!("{}-{:02}-{:02}", y, m, dim),
                ));
                m -= 1;
                if m == 0 {
                    m = 12;
                    y -= 1;
                }
            }
            out.reverse();
        }
        "year" => {
            let mut y = today[0..4].parse::<i64>().unwrap_or(2026);
            for _ in 0..5 {
                out.push((
                    y.to_string(),
                    format!("{}-01-01", y),
                    format!("{}-12-31", y),
                ));
                y -= 1;
            }
            out.reverse();
        }
        other => return Err(format!("未知的趋势周期：{other}")),
    }
    Ok(out)
}

/// 当前学习日（UTC+8）"YYYY-MM-DD"。
pub fn now_utc8_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        + 8 * 3600;
    unix_to_date(secs)
}

pub fn unix_to_date(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// "YYYY-MM-DD" ± n 天。
pub fn shift_date(d: &str, delta: i64) -> String {
    let y = d[0..4].parse::<i64>().unwrap_or(2026);
    let m = d[5..7].parse::<i64>().unwrap_or(1);
    let day = d[8..10].parse::<i64>().unwrap_or(1);
    // civil → days（Howard Hinnant）
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468 + delta;
    unix_to_date(days * 86400)
}

/// "YYYY-MM-DD" → 周一=1…周日=7。
pub fn weekday_of(d: &str) -> i64 {
    let y = d[0..4].parse::<i64>().unwrap_or(2026);
    let m = d[5..7].parse::<i64>().unwrap_or(1);
    let day = d[8..10].parse::<i64>().unwrap_or(1);
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + day) % 7;
    if w == 0 {
        7
    } else {
        w
    }
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

// =============== AI Mastery（DEV-0050 / PHASE D §46-57） ===============

/// 最新评估（含 stale 标记；None = 未评估）。
#[derive(Debug, serde::Serialize)]
pub struct MasteryView {
    assessment: Option<repository::mastery::MasteryAssessment>,
    stale: bool,
}

#[tauri::command]
pub fn get_latest_mastery(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<MasteryView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::mastery::MasteryRepository::new(&conn);
    let a = repo
        .latest(profile_id, &period_type, &period_start, &period_end)
        .map_err(|e| e.to_string())?;
    let stale = match &a {
        Some(a) => repo
            .stale_since(profile_id, &period_start, &period_end, &a.created_at)
            .map_err(|e| e.to_string())?,
        None => false,
    };
    Ok(MasteryView {
        assessment: a,
        stale,
    })
}

/// 评估历史（详情用）。
#[tauri::command]
pub fn list_mastery_history(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<Vec<repository::mastery::MasteryAssessment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::mastery::MasteryRepository::new(&conn)
        .list_history(profile_id, &period_type, &period_start, &period_end)
        .map_err(|e| e.to_string())
}

/// §47/§56：用户主动点击「AI评估」→ 构建专用 Context → 单次 AI 调用（无工具循环）
/// → 校验 JSON（score 0-100 / 40-30-30 / 证据不足不带分）→ Higher 普通 Command 落库。
/// AI Write Tools 仍为 0；AI 不能改 Goal/Task/Session/Knowledge。
#[tauri::command]
pub async fn assess_mastery(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<repository::mastery::MasteryAssessment, String> {
    if !["day", "week", "month", "year"].contains(&period_type.as_str()) {
        return Err(format!("未知的周期类型：{period_type}"));
    }
    // 1) 构建专用 Context（锁内短临界区）
    let (context, primary_caps) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
                profile_id,
                action: ai::AiAction::MasteryAssessment,
                session_id: None,
                learning_item_id: None,
                user_instruction: None,
                date: Some(format!("{period_start}..{period_end}")),
            },
        )?;
        // §18 Capability：Mastery 需 PRIMARY structured_json
        let caps = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        (ctx, caps)
    };
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }

    // 2) 单次调用（不进工具循环；一次结构修复重试）
    let client = primary_client(&state)?;
    let base = format!(
        "{}\n\n{}",
        context,
        ai::prompts::user_instruction(ai::AiAction::MasteryAssessment)
    );
    let mut messages = vec![
        ai::client::ChatMessage::system(ai::prompts::SYSTEM_PROMPT),
        ai::client::ChatMessage::user(base),
    ];
    let mut raw = String::new();
    for attempt in 0..2 {
        let c = client
            .chat(messages.clone(), true, None, Some(4096))
            .await?;
        let content = c.content.ok_or("模型没有返回内容")?;
        let trimmed = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        if serde_json::from_str::<serde_json::Value>(&trimmed).is_ok() || attempt == 1 {
            raw = trimmed;
            break;
        }
        messages.push(ai::client::ChatMessage::assistant(content));
        messages.push(ai::client::ChatMessage::user(
            "上面的输出不是合法 JSON。请严格只输出一个合法 JSON 对象（不要 markdown 代码块）。",
        ));
    }
    if raw.is_empty() {
        return Err("模型返回无法解析为 JSON".to_string());
    }

    // 3) 映射 + 服务端校验（§50/§53）
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("评估结果解析失败：{e}"))?;
    let str_list = |key: &str| -> Vec<String> {
        v.get(key)
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let dim = |key: &str| -> Option<i64> {
        v.get(key)
            .and_then(|d| d.get("score"))
            .and_then(|s| s.as_i64())
    };
    let status = v
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("insufficient_evidence")
        .to_string();
    let model = ai_setting_model(&state)?;
    let a = repository::mastery::MasteryAssessment {
        id: 0,
        profile_id,
        goal_id: None,
        period_type,
        period_start,
        period_end,
        confidence: v
            .get("confidence")
            .and_then(|s| s.as_str())
            .unwrap_or("low")
            .to_string(),
        summary: v
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        score: if status == "scored" {
            v.get("score").and_then(|s| s.as_i64())
        } else {
            None
        },
        understanding_score: if status == "scored" {
            dim("understanding")
        } else {
            None
        },
        coverage_score: if status == "scored" {
            dim("coverage")
        } else {
            None
        },
        verification_score: if status == "scored" {
            dim("verification")
        } else {
            None
        },
        strengths: str_list("strengths"),
        gaps: str_list("gaps"),
        evidence: str_list("evidence"),
        suggestions: str_list("suggestions"),
        model,
        created_at: String::new(),
        status,
    };

    // 4) 落库（append-only）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = repository::mastery::MasteryRepository::new(&conn).insert(&a)?;
    repository::mastery::MasteryRepository::new(&conn)
        .latest(a.profile_id, &a.period_type, &a.period_start, &a.period_end)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "评估已保存但读取失败".to_string())
        .map(|mut m| {
            let _ = id;
            m.id = id;
            m
        })
}

pub fn ai_setting_model(state: &tauri::State<'_, db::DbState>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
    Ok(s.model)
}

// =============== DEV-0059 Versioning / GoalTarget / Planning / Review（Section 6 increment 13） ===============
// =============== DEV-0059 · PersonalProfile Versioning / GoalTarget / Planning / Review ===============

/// §8：PersonalProfile 版本历史（含 superseded；历史可查）。
#[tauri::command]
pub fn list_personalization_profile_versions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .list_profile_versions(profile_id)
}

/// DEV-0059.1 §6：某 PersonalProfile 版本使用的 Personal Source snapshot（只读）。
#[tauri::command]
pub fn list_sources_for_personal_profile_version(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    version_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .list_sources_for_version(version_id, profile_id)
}

// ---- GoalTarget（§11） ----

#[tauri::command]
pub fn create_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: String,
    role: String,
    title: String,
    target_date: Option<String>,
    data_json: String,
    provenance_json: String,
    status: String,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).create(
        profile_id,
        &scenario_type,
        &role,
        &title,
        target_date.as_deref(),
        &data_json,
        &provenance_json,
        &status,
    )
}

#[tauri::command]
pub fn list_goal_targets(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::goal_target::GoalTarget>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
pub fn list_active_goal_targets(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: Option<String>,
    role: Option<String>,
) -> Result<Vec<repository::goal_target::GoalTarget>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).list_active(
        profile_id,
        scenario_type.as_deref(),
        role.as_deref(),
    )
}

/// §11.3：激活（同 scenario+role 其他 active → historical）。
#[tauri::command]
pub fn activate_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).activate(profile_id, id)
}

/// §11.3：替换 active 目标（旧 → historical，新版本 → active）。
#[tauri::command]
pub fn replace_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    target_date: Option<String>,
    data_json: String,
    provenance_json: String,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).replace_with(
        profile_id,
        id,
        &title,
        target_date.as_deref(),
        &data_json,
        &provenance_json,
    )
}

#[tauri::command]
pub fn dismiss_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).dismiss(profile_id, id)
}

/// §11.4：Legacy 目标源候选（只读；不自动激活）。
#[tauri::command]
pub fn list_legacy_goal_candidates(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::goal_target::LegacyTargetCandidate>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).list_legacy_candidates(profile_id)
}

// ---- PlanningBlueprint / Phase / Milestone（§14-16/§25.1） ----

#[tauri::command]
pub fn create_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: String,
    title: String,
    content_md: String,
    structured_json: Option<String>,
    source_snapshot_json: String,
    provenance_json: String,
    review_interval_days: i64,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).create_blueprint(
        profile_id,
        &scenario_type,
        &title,
        &content_md,
        structured_json.as_deref(),
        &source_snapshot_json,
        &provenance_json,
        review_interval_days,
    )
}

#[tauri::command]
pub fn list_planning_blueprints(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
pub fn get_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<Option<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).get_blueprint(id, profile_id)
}

#[tauri::command]
pub fn get_active_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).get_active(profile_id)
}

/// §25.1：激活（事务：supersede + active + 安全投影；投影 14 天）。
#[tauri::command]
pub fn activate_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let today = chrono_today();
    repository::planning::PlanningRepository::new(&conn).activate(profile_id, id, &today, 14)
}

#[tauri::command]
pub fn add_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_key: String,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    objective_md: String,
    sort_order: i64,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).add_phase(
        blueprint_id,
        &phase_key,
        &title,
        start_date.as_deref(),
        end_date.as_deref(),
        &objective_md,
        sort_order,
    )
}

#[tauri::command]
pub fn list_planning_phases(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
) -> Result<Vec<repository::planning::PlanningPhase>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_phases(blueprint_id)
}

#[tauri::command]
pub fn add_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: Option<i64>,
    milestone_key: String,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    date_precision: String,
    date_status: String,
    provenance_json: String,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).add_milestone(
        blueprint_id,
        phase_id,
        &milestone_key,
        &title,
        start_date.as_deref(),
        end_date.as_deref(),
        &date_precision,
        &date_status,
        &provenance_json,
    )
}

#[tauri::command]
pub fn list_planning_milestones(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
) -> Result<Vec<repository::planning::PlanningMilestone>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_milestones(blueprint_id)
}

// ---- DEV-0059.1 §10/§11：Manual Planning + Review Cadence ----

/// §10：手工编辑 Blueprint 基础信息（title + content_md）。
#[tauri::command]
pub fn update_planning_blueprint_meta(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    content_md: String,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).update_blueprint_meta(
        profile_id,
        id,
        &title,
        &content_md,
    )
}

/// §11：Review Cadence——只改 review_enabled / review_interval_days / next_review_at（不调 AI）。
#[tauri::command]
pub fn update_planning_review_cadence(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    review_enabled: bool,
    review_interval_days: Option<i64>,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).update_review_cadence(
        profile_id,
        id,
        review_enabled,
        review_interval_days,
    )
}

/// §10：Phase 更新。
#[tauri::command]
pub fn update_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: i64,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    objective_md: String,
    sort_order: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).update_phase(
        blueprint_id,
        phase_id,
        &title,
        start_date.as_deref(),
        end_date.as_deref(),
        &objective_md,
        sort_order,
    )
}

/// §10：Phase 删除。
#[tauri::command]
pub fn delete_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).delete_phase(blueprint_id, phase_id)
}

/// §10：Milestone 更新。
#[tauri::command]
pub fn update_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    milestone_id: i64,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    date_precision: String,
    date_status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).update_milestone(
        blueprint_id,
        milestone_id,
        &title,
        start_date.as_deref(),
        end_date.as_deref(),
        &date_precision,
        &date_status,
    )
}

/// §10：Milestone 删除。
#[tauri::command]
pub fn delete_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    milestone_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .delete_milestone(blueprint_id, milestone_id)
}

// ---- PlanningReview（§17-18/§39） ----

#[tauri::command]
pub fn create_planning_review_due(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    blueprint_id: Option<i64>,
    period_start: String,
    period_end: String,
    trigger_type: String,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).create_due(
        profile_id,
        blueprint_id,
        &period_start,
        &period_end,
        &trigger_type,
    )
}

#[tauri::command]
pub fn list_planning_reviews(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning_review::PlanningReview>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
pub fn set_planning_review_status(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .set_status(id, profile_id, &status)
}

/// §18：是否该进行阶段复盘了（只读，不调 AI）。
#[tauri::command]
pub fn is_planning_review_due(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let today = chrono_today();
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .is_review_due(profile_id, &today)
}

/// §30：最新已确认 Review 的 risk_state（Today 风险 Banner；启动只读）。
#[tauri::command]
pub fn get_planning_review_risk(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).latest_risk_state(profile_id)
}

// ---- DEV-0059.1 §3：Planning Review AI 全链（用户确认后才调用 Provider） ----

/// DEV-0059.2 §2：当前周期复盘（cadence 周期 + open review dedupe）。
/// 周期由后端按 active Blueprint 的 review_interval_days 计算（前端禁止硬编码 14）；
/// 同 profile/blueprint 已存在 open review（due/running/waiting_approval）时复用，不重复创建。
#[tauri::command]
pub fn prepare_current_planning_review(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    trigger_type: String,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (rid, status, cs_id, snapshot) =
        repository::planning_review::PlanningReviewRepository::new(&conn)
            .prepare_current(profile_id, &trigger_type)?;
    let snapshot_json: serde_json::Value = if snapshot.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&snapshot).unwrap_or(serde_json::Value::Null)
    };
    Ok(serde_json::json!({
        "review_id": rid,
        "status": status,
        "change_set_id": cs_id,
        "snapshot": snapshot_json,
    }))
}

/// §3 step 1：准备复盘——置 running + 构建 evidence snapshot（不调 Provider）。
/// 返回 snapshot JSON 供前端展示摘要（蓝图/周期任务/可信学习/可信验证/档案/目标）。
#[tauri::command]
pub fn prepare_planning_review_ai(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    review_id: i64,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::planning_review::PlanningReviewRepository::new(&conn);
    let rev = repo
        .get(review_id, profile_id)?
        .ok_or("复盘记录不存在或不属于当前档案")?;
    let snapshot = repository::planning_review::PlanningReviewRepository::build_snapshot(
        &conn,
        profile_id,
        rev.blueprint_id,
        &rev.period_start,
        &rev.period_end,
    )?;
    repo.prepare_running(review_id, profile_id, &snapshot)?;
    serde_json::from_str(&snapshot).map_err(|e| e.to_string())
}

/// §3 step 3：用户确认后启动 AI 评估（真实 Provider；本命令内一次调用）。
///
/// - AI 输出 NO_CHANGE → review completed + cadence 刷新（无 ChangeSet）
/// - AI 输出 ADJUSTMENT_PROPOSAL → Blueprint vN+1 draft 编译为 ChangeSet waiting_approval
///   （用户后续 Review → Apply → vN superseded / vN+1 active / review 自动 completed）
/// - Provider 失败 / 输出不可用 → review failed，正式数据不变，不后台 retry
#[tauri::command]
pub async fn run_planning_review_ai(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    review_id: i64,
) -> Result<String, String> {
    use ai::client::ChatMessage;
    // 1) 读 review + snapshot（必须 running）
    let snapshot_json = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let rev = repository::planning_review::PlanningReviewRepository::new(&conn)
            .get(review_id, profile_id)?
            .ok_or("复盘记录不存在或不属于当前档案")?;
        if rev.status != "running" {
            return Err(format!(
                "复盘当前状态为 {}，请先准备后再启动 AI 评估",
                rev.status
            ));
        }
        if rev.evidence_snapshot_json.trim().is_empty() {
            return Err("复盘缺少证据快照，请先准备".to_string());
        }
        rev.evidence_snapshot_json
    };
    // 2) AI 配置 + client（§18：Planning Review 需 PRIMARY structured_json）
    let primary_caps = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }
    let client = primary_client(&state)?;
    // 3) 组装消息（json_mode；只读评估，不给工具）
    let system = "你是学习规划阶段复盘评估助手。基于提供的真实证据快照评估该周期学习执行情况。\
        只输出一个 JSON 对象（不要 markdown 代码块、不要解释文字），结构：\
        {\"decision\":\"NO_CHANGE\"|\"ADJUSTMENT_PROPOSAL\",\"assessment_md\":\"对本周期的评估与建议（可读文本）\",\
        \"risk_state\":\"normal|attention|off_reach|near_safety|below_safety\",\
        \"recommendation\":\"调整建议要点（数组或字符串）\",\
        \"blueprint\":{标题/阶段/里程碑/未来任务…}或null}。\
        规则：decision=NO_CHANGE 时 blueprint 必须为 null；decision=ADJUSTMENT_PROPOSAL 时必须给出调整后的完整蓝图。\
        蓝图格式：{\"title\":\"…\",\"summary\":\"…\",\"review_interval_days\":14,\
        \"phases\":[{\"phase_key\":\"P1\",\"title\":\"…\",\"start_date\":\"YYYY-MM-DD 或 null\",\"end_date\":\"YYYY-MM-DD 或 null\",\"objective_md\":\"…\",\"sort_order\":1}],\
        \"milestones\":[{\"milestone_key\":\"M1\",\"title\":\"…\",\"start_date\":\"…\",\"end_date\":\"…\",\"date_precision\":\"day|range|month|unknown\",\"date_status\":\"estimated|official|user_confirmed|outdated|needs_review\"}],\
        \"future_tasks\":[{\"title\":\"具体任务（科目：内容+量）\",\"planned_date\":\"YYYY-MM-DD\",\"estimated_minutes\":60}],\
        \"assumptions\":[],\"unresolved\":[],\"external_facts\":[],\
        \"source_review\":[{\"source_id\":12,\"source_name\":\"老师规划.docx\",\"decision\":\"keep|modify|conflict|missing\",\"original\":\"原规划内容摘要\",\"suggested\":\"建议内容\",\"reason\":\"为什么\",\"evidence\":\"依据\"}],\
        \"suggested_target_changes\":[]}。\
        任务名必须具体可执行（如「高数：极限计算基础题 15 题」），禁止占位词。\
        日期不得晚于快照周期结束 +14 天。无法确定的信息写入 unresolved，禁止编造。\
        若快照中提供了规划资料且你对资料有修改/冲突/缺失判断，必须填写 source_review（modify 必须给 reason 与 suggested）；无资料或无需审查时可留空。";
    let user = format!(
        "【当前日期】{}\n【周期复盘证据快照】\n{}",
        crate::repository::planning::today_utc8(),
        snapshot_json
    );
    let messages = vec![
        ChatMessage::system(system.to_string()),
        ChatMessage::user(user),
    ];
    let completion = client
        .chat(messages, true, None, Some(4096))
        .await
        .map_err(|e| {
            // Provider 失败 → review failed，正式数据不变
            if let Ok(conn) = state.0.lock() {
                let _ = repository::planning_review::PlanningReviewRepository::new(&conn)
                    .set_status(review_id, profile_id, "failed");
            }
            format!("AI 评估失败：{}", e)
        })?;
    let raw = completion.content.unwrap_or_default();
    // 4) 应用 AI 评估输出（NO_CHANGE → completed；ADJUSTMENT_PROPOSAL → ChangeSet waiting_approval；
    //    输出不可用 → review failed；本函数 Provider 无关，测试可直测）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::planner::apply_review_assessment(&conn, profile_id, review_id, &raw).map_err(|e| {
        let _ = repository::planning_review::PlanningReviewRepository::new(&conn)
            .set_status(review_id, profile_id, "failed");
        e
    })
}

// ---- Planning Source（§13） ----

#[tauri::command]
pub fn import_planning_source(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    path: String,
    source_kind: String,
) -> Result<serde_json::Value, String> {
    use sha2::{Digest, Sha256};
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let src = sandbox::resolve_import_source(&path)?;
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source")
        .to_string();
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let ftype = match ext.as_str() {
        "txt" | "md" | "docx" | "pdf" | "xlsx" => ext,
        _ => {
            return Err(format!(
                "不支持的规划资料格式：{ext}（支持 txt/md/docx/pdf/xlsx）"
            ))
        }
    };
    // 复制原件到附件沙箱
    let root = adir.0.join("planning_sources").join(profile_id.to_string());
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stored = root.join(&name);
    std::fs::copy(&src, &stored).map_err(|e| format!("复制规划资料失败：{e}"))?;
    let bytes = std::fs::read(&stored).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = format!("{:x}", hasher.finalize());
    // 提取文本（扫描 PDF 明确报错）
    let text = match ftype.as_str() {
        "txt" | "md" => repository::personalization::decode_text(bytes)?,
        "docx" => repository::personalization::extract_docx(&stored)?,
        "pdf" => repository::personalization::extract_pdf(&stored)?,
        "xlsx" => repository::source_ingest::extract_xlsx_text(&stored)?,
        _ => return Err("不支持的格式".to_string()),
    };
    if text.trim().is_empty() {
        return Err("无法从该文件中提取文字（扫描版 PDF 请先 OCR 后另存为文本）".to_string());
    }
    let repo = repository::planning_source::PlanningSourceRepository::new(&conn);
    let sid = repo.insert(
        profile_id,
        &source_kind,
        &name,
        &ftype,
        &stored.to_string_lossy(),
        &sha,
    )?;
    repo.store_chunks(sid, profile_id, &text)?;
    repo.set_status(sid, "ready")?;
    Ok(
        serde_json::json!({ "id": sid, "name": name, "file_type": ftype, "sha256": sha, "chars": text.chars().count() }),
    )
}

#[tauri::command]
pub fn list_planning_sources(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning_source::PlanningSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_source::PlanningSourceRepository::new(&conn).list(profile_id)
}

#[tauri::command]
pub fn get_planning_source_text(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    source_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_source::PlanningSourceRepository::new(&conn)
        .joined_text(profile_id, source_id)
}

// ---- Import/Export（§31.3：用户明确 save path；只写所选路径） ----

#[tauri::command]
pub fn write_export_file(path: String, content_base64: String) -> Result<(), String> {
    use base64::Engine as _;
    let p = std::path::PathBuf::from(&path);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Err("未指定有效导出路径".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content_base64)
        .map_err(|e| format!("导出数据编码错误：{e}"))?;
    std::fs::write(&p, bytes).map_err(|e| format!("写入导出文件失败：{e}"))?;
    Ok(())
}

// ---------- Web（PHASE K） ----------

#[tauri::command]
pub fn get_web_search_settings(
    state: tauri::State<'_, db::DbState>,
) -> Result<(bool, bool), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let enabled = SettingRepository::new(&conn)
        .get("websearch.enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let has_key = SettingRepository::new(&conn)
        .get("websearch.brave_key")
        .ok()
        .flatten()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    Ok((enabled, has_key))
}

#[tauri::command]
pub fn set_web_search_settings(
    state: tauri::State<'_, db::DbState>,
    enabled: bool,
    brave_key: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = SettingRepository::new(&conn);
    repo.set("websearch.enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    if let Some(k) = brave_key {
        repo.set("websearch.brave_key", k.trim())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- Vault（PHASE U） ----------

#[tauri::command]
pub fn vault_status(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<serde_json::Value, String> {
    let locked = vault.is_locked();
    let stats = if locked { None } else { vault.stats().ok() };
    Ok(serde_json::json!({
        "locked": locked,
        "hint": "测试版密码为 root",
        "stats": stats,
    }))
}

#[tauri::command]
pub fn vault_unlock(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    password: String,
) -> Result<(), String> {
    vault.unlock(&password)
}

#[tauri::command]
pub fn vault_lock(vault: tauri::State<'_, crate::ai::vault::VaultState>) -> Result<(), String> {
    vault.lock();
    Ok(())
}

#[tauri::command]
pub fn vault_list_events(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    limit: Option<i64>,
) -> Result<Vec<crate::ai::vault::VaultEvent>, String> {
    vault.list_events(limit.unwrap_or(100))
}

#[tauri::command]
pub fn vault_list_snapshots(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<Vec<(i64, String, i64, String)>, String> {
    vault.list_snapshots()
}

#[tauri::command]
pub fn vault_create_snapshot(
    app: tauri::AppHandle,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<i64, String> {
    // DEV-0057 §164：真实运行 DB 路径（prod 不再恒 size=0）
    let db_path = runtime_db_path(&app);
    let real = if db_path.exists() {
        Some(db_path.as_path())
    } else {
        None
    };
    vault.snapshot("manual", real)
}

#[tauri::command]
pub fn vault_export_events(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<String, String> {
    vault.export_events_json()
}

// ---------- AI Run（PHASE B：start / cancel） ----------

/// §17：立即返回 run_id，后台执行。事件：ai://delta / ai://step / ai://source /
/// ai://changeset / ai://run-status / ai://error。前端 listen 后更新 UI。
#[tauri::command]
pub async fn ai_start_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    runs: tauri::State<'_, ai::run::RunManager>,
    _vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    conversation_id: i64,
    user_message: String,
    page_label: String,
    knowledge_path: Option<String>,
    session_title: Option<String>,
    date: Option<String>,
    // DEV-0060.1 PART A（§6.1）：Runtime Time Truth 由前端每次 send 传入（WebView 本地时间）
    local_date: Option<String>,
    local_datetime: Option<String>,
    timezone_offset_minutes: Option<i64>,
    // DEV-0077.3 §十四-§十六：前端 invoke 前生成的 Runtime Correlation ID。
    // Option 保持旧前端兼容；未传时以 run_id 兜底（事件仍可按 run_id 匹配）。
    client_turn_id: Option<String>,
) -> Result<String, String> {
    // mode（§13：conversation 临时 mode 优先于 profile 偏好）+ DEV-0062 §26/§30：
    // Run 开始时一次性 resolve immutable Primary / Control config（此后整个 Run 固定使用）
    let (profiles, mode, web_enabled, brave_key) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let resolved = ai::provider::resolve_active_ai_profiles(&conn)?;
        let conv_mode = repository::conversation::ConversationRepository::new(&conn)
            .get(conversation_id, profile_id)
            .ok()
            .flatten()
            .map(|c| c.mode);
        let m = conv_mode.unwrap_or_else(|| {
            SettingRepository::new(&conn)
                .get(&format!("ai.mode.{}", profile_id))
                .ok()
                .flatten()
                .unwrap_or_else(|| "readonly".to_string())
        });
        let we = SettingRepository::new(&conn)
            .get("websearch.enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        let bk = SettingRepository::new(&conn)
            .get("websearch.brave_key")
            .ok()
            .flatten()
            .unwrap_or_default();
        (resolved, m, we, bk)
    };
    // DEV-0061R §34：Unified Higher AI——mode 仅 legacy 读取（不再参与 run_chat_turn 判定）
    let _legacy_mode = mode;

    // 记录用户消息（DEV-0060 §5.3：保存后拿到 AiMessage.id，run_chat_turn 按 ID 排除当前消息）
    let current_message_id = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", &user_message, None)?
            .id
    };

    let (run_id, token) = runs.register();
    let run_id_clone = run_id.clone();
    let app_handle = app.clone();

    // 后台执行（tauri async spawn；State 生命周期从 AppHandle 重新获取以满足 'static）
    // DEV-0066 PHASE A：主入口切换为 Global Agent（run_agent_turn）——
    // 不再先经 Turn Interpreter 路由（§8.1）；旧 run_chat_turn 保留为 legacy（§34）。
    // DEV-0077.3 §三十/§三十三/§五十二：消息持久化与终态/错误事件的全部
    // 收口已由 agent_turn_core + AiRuntimeEmitter 按唯一顺序完成——本层
    // 只负责 runs.finish，禁止再补发 ai://run-status / ai://error 或重复
    // 写「[出错]」消息（§九十二 One Message Truth / TC015）。
    let turn_client_id = client_turn_id.unwrap_or_else(|| run_id.clone());
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<db::DbState>();
        let runs = app_handle.state::<ai::run::RunManager>();
        let vault = app_handle.state::<crate::ai::vault::VaultState>();
        let result = ai::agent::run_agent_turn(
            &app_handle,
            &state,
            &vault,
            profile_id,
            conversation_id,
            &run_id_clone,
            &token,
            current_message_id,
            &user_message,
            profiles.primary.clone(),
            &page_label,
            knowledge_path.as_deref(),
            session_title.as_deref(),
            date.as_deref(),
            web_enabled,
            &brave_key,
            local_date.as_deref().map(String::from).unwrap_or_default(),
            local_datetime
                .as_deref()
                .map(String::from)
                .unwrap_or_default(),
            timezone_offset_minutes.unwrap_or(480),
            &turn_client_id,
        )
        .await;
        runs.finish(&run_id_clone);
        if let Err(e) = result {
            eprintln!("[AI-RUNTIME] run_failed_converged run_id={run_id_clone} err={e}");
        }
    });
    Ok(run_id)
}

/// 单轮对话执行（streaming + 工具循环 + 引用校验/修复 + Memory Extract + ChangeSet 落库）。
/// DEV-0060 PART A：current_message_id = 本轮用户消息的 ai_messages.id（按 ID 排除，禁止 content equality）。
#[allow(clippy::too_many_arguments)]
pub async fn run_chat_turn(
    app: &tauri::AppHandle,
    state: &db::DbState,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    token: &tokio_util::sync::CancellationToken,
    current_message_id: i64,
    user_message: &str,
    // DEV-0062 §26/§30：Run 开始时 resolve 的 immutable 双角色 config（整 Run 固定）
    primary: ai::provider::AiRuntimeConfig,
    control: ai::provider::AiRuntimeConfig,
    page_label: &str,
    knowledge_path: Option<&str>,
    session_title: Option<&str>,
    date: Option<&str>,
    web_enabled: bool,
    brave_key: &str,
    // DEV-0060.1 PART A：Runtime Time Truth（前端传入；Backend 校验）
    local_date: String,
    local_datetime: String,
    timezone_offset_minutes: i64,
) -> Result<&'static str, String> {
    use ai::client::{AiClient, ChatMessage};
    // §27 Provider Role Mapping：PRIMARY（FastChat/HigherRead/Planner/…）｜CONTROL（Interpreter/Repair/Selection）
    let client = AiClient::new(primary.clone());
    let control_client = AiClient::new(control.clone());
    vault.record_ai("run_started", run_id, page_label);
    let mut trace = ai::trace::Trace::new(run_id);
    // DEV-0061R §42.1：run 一开始就 INSERT ai_runs(status='running')——
    // 早期 trace event（route/context/provider/grounding…）的 FK 由此满足，不再丢失。
    // DEV-0062 §29：同 INSERT 写 provider snapshot（本 Run 真实 Primary/Control；历史不漂移）。
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error,
                primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
                control_ai_profile_id, control_profile_name, control_adapter_kind, control_model)
             VALUES (?1,?2,?3,'assistant','turn','running','',
                ?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                run_id,
                profile_id,
                conversation_id,
                primary.profile_id,
                primary.display_name,
                primary.adapter_kind.as_str(),
                primary.model,
                control.profile_id,
                control.display_name,
                control.adapter_kind.as_str(),
                control.model,
            ],
        );
        trace.turn_started(&conn, page_label);
    }
    // DEV-0061R §4/§34：Unified Higher AI——不再有 readonly/assistant 用户模式；
    // 旧 conversation.mode 只是 legacy 兼容值，不再阻止 Proposal。写入恒走 Approval Boundary。
    let is_assistant = true;
    // 兼容兜底：前端未传（旧调用）→ Backend UTC+8 学习日（仍不交给模型猜）
    let local_date = if local_date.trim().is_empty() {
        crate::repository::planning::today_utc8()
    } else {
        local_date
    };
    let local_datetime = if local_datetime.trim().is_empty() {
        format!("{local_date} 00:00")
    } else {
        local_datetime
    };
    // Envelope 校验失败 → 明确错误（不让模型在没有 Time Truth 的情况下猜日期）
    let envelope = ai::runtime::AiRuntimeEnvelope::validated(
        &local_date,
        &local_datetime,
        timezone_offset_minutes,
        page_label,
        date,
        profile_id,
        conversation_id,
        "assistant",
    )?;

    // ---- DEV-0062 §44 · Pending Action Continuation Gate（Turn Priority #4：
    // 先于 Planner gate 与 Turn Interpreter；「第一个/8月24日那个」不得先进入 Interpreter） ----
    {
        let gate = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let repo = repository::ai_pending_action::AiPendingActionRepository::new(&conn);
            let pending = repo
                .find_active(profile_id, conversation_id)
                .map_err(|e| e.to_string())?;
            pending.map(|p| {
                let candidates = repo.candidates(&p);
                // §54 Stale Candidate Protection：候选身份变化（删/日期/状态/标题/enabled）→ stale
                if ai::action_continuation::candidates_stale(&conn, profile_id, &candidates) {
                    let _ = repo.set_status(p.id, "stale");
                    Some((p.id, "stale", "刚才候选中的任务已经发生变化。为避免修改错对象，请重新告诉我现在要修改哪个任务。\n正式数据没有变化。".to_string(), None))
                } else {
                    match ai::action_continuation::resolve_pending_selection(
                        user_message, &candidates, &envelope,
                    ) {
                        // §51 取消：本地 0 Provider / 0 ChangeSet
                        ai::action_continuation::PendingSelection::Cancel => {
                            let _ = repo.set_status(p.id, "cancelled");
                            Some((p.id, "cancelled", "已取消刚才这次修改，正式数据没有变化。".to_string(), None))
                        }
                        // §53 NoMatch：明显在尝试选择但无命中；pending 保持 active，attempt+1
                        ai::action_continuation::PendingSelection::NoMatch(refed) => {
                            let _ = repo.bump_attempt(p.id);
                            Some((p.id, "active", ai::action_continuation::no_match_text(&refed, &candidates), None))
                        }
                        // §50 StillAmbiguous：重新展示候选；不得选第一个
                        ai::action_continuation::PendingSelection::StillAmbiguous => {
                            let _ = repo.bump_attempt(p.id);
                            Some((p.id, "active", format!("还不能确定是哪一个，请从以下候选中选择：{}\n（正式数据没有变化。）", ai::action_continuation::candidates_text(&candidates)), None))
                        }
                        // §52 New Intent：旧 pending cancelled，本轮走正常 Runtime（不劫持）
                        ai::action_continuation::PendingSelection::NotSelection => {
                            let _ = repo.set_status(p.id, "cancelled");
                            None
                        }
                        // §47/§64 Selected：复用原 SemanticAction + 原 Patch → Domain Compiler
                        // → 真实 ChangeSet（Turn Interpreter 0 Call / Candidate Selection 0 Call）
                        ai::action_continuation::PendingSelection::Selected(real_id, _) => {
                            match serde_json::from_str::<ai::action::SemanticAction>(
                                &p.semantic_action_json,
                            ) {
                                Err(_) => {
                                    let _ = repo.set_status(p.id, "cancelled");
                                    None
                                }
                                Ok(act) => {
                                    trace.route_decided(&conn, "pending_continuation", "local", &[]);
                                    let mut input = ai::action::PlanInput {
                                        user_message,
                                        conversation_id,
                                        ..Default::default()
                                    };
                                    if let Some((etype, _)) = act.primary_reference() {
                                        let resolved =
                                            ai::grounding::GroundingOutcome::Resolved(real_id);
                                        if etype == "task" {
                                            input.pre_task = Some(resolved);
                                        } else {
                                            input.pre_rule = Some(resolved);
                                        }
                                    }
                                    match ai::action::plan_action(&conn, profile_id, &envelope, &input, &act) {
                                        Ok(ai::action::ActionOutcome::ProposalReady { ops, title, summary, .. })
                                            if !ops.is_empty()
                                                && ai::action::validate_ops(&envelope, &act, &ops).is_ok() =>
                                        {
                                            match repository::changeset::ChangeSetRepository::new(&conn)
                                                .create(profile_id, Some(conversation_id), Some(run_id), &title, &summary, &ops)
                                            {
                                                Ok(cs_id) => {
                                                    let _ = repo.set_status(p.id, "resolved");
                                                    let text = format!(
                                                        "已经准备好修改提案：{title}（{summary}）。共 {} 项操作。\n点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。",
                                                        ops.len()
                                                    );
                                                    Some((p.id, "resolved", text, Some(cs_id)))
                                                }
                                                Err(e) => {
                                                    let _ = repo.set_status(p.id, "cancelled");
                                                    Some((p.id, "cancelled", format!("提案生成失败：{e}\n\n（正式数据没有变化。）"), None))
                                                }
                                            }
                                        }
                                        Ok(ai::action::ActionOutcome::NothingToChange(m)) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", m, None))
                                        }
                                        Ok(ai::action::ActionOutcome::NotFound(m))
                                        | Ok(ai::action::ActionOutcome::Unsupported(m))
                                        | Ok(ai::action::ActionOutcome::ContractFailure(m)) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", m, None))
                                        }
                                        Ok(ai::action::ActionOutcome::Clarification { message, .. }) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", message, None))
                                        }
                                        Ok(ai::action::ActionOutcome::ProposalReady { .. }) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", "没有产生可执行的修改。正式数据没有变化。".to_string(), None))
                                        }
                                        Err(e) => {
                                            let _ = repo.set_status(p.id, "cancelled");
                                            Some((p.id, "cancelled", format!("{e}\n\n（正式数据没有变化。）"), None))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
        };
        // None = NotSelection / 反序列化失败 → 旧 pending 已 cancelled，正常 Runtime 继续
        if let Some((_pid, pending_status, text, cs_id)) = gate.flatten() {
            let made_changeset = cs_id.is_some();
            if let Some(cs_id) = cs_id {
                ai::run::emit(
                    Some(app),
                    "ai://changeset",
                    run_id,
                    serde_json::json!({ "change_set_id": cs_id }),
                );
            }
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                    conversation_id,
                    profile_id,
                    "assistant",
                    &text,
                    Some(run_id),
                );
                let _ = conn.execute(
                    "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                     VALUES (?1,?2,?3,'assistant','pending_action',?4,?5)
                     ON CONFLICT(id) DO UPDATE SET status=excluded.status, error=excluded.error, updated_at=datetime('now')",
                    rusqlite::params![run_id, profile_id, conversation_id,
                        if made_changeset { "waiting_approval" } else { "completed" },
                        format!("pending:{pending_status}")],
                );
                trace.run_finished(
                    &conn,
                    if made_changeset {
                        "waiting_approval"
                    } else {
                        "completed"
                    },
                );
            }
            vault.record_ai("run_completed", run_id, "pending_action");
            return Ok(if made_changeset {
                "waiting_approval"
            } else {
                "completed"
            });
        }
    }

    // ---- DEV-0060 PART F：先做 workflow/gate 决策（决定 purpose 后再按需构建 Context） ----
    let (last_assistant, workflow_state, workflow_payload) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let last = repository::conversation::ConversationRepository::new(&conn)
            .list_messages(conversation_id, profile_id, 1, 0)
            .unwrap_or_default()
            .into_iter()
            .find(|m| {
                m.role == "assistant" && !m.content.trim().is_empty() && m.id != current_message_id
            })
            .map(|m| m.content)
            .unwrap_or_default();
        match ai::planner::read_workflow_payload(&conn, profile_id, conversation_id) {
            Some((s, p)) => (last, Some(s), p),
            None => (last, None, ai::planner::PlanningWorkflowPayload::default()),
        }
    };
    let gate = ai::planner::planning_gate(user_message, is_assistant);
    // §10.1：active workflow 不再无条件劫持——确定性分流（取消/继续/新意图）
    let continuing_decision = if workflow_state
        .as_deref()
        .map(ai::planner::workflow_active)
        .unwrap_or(false)
    {
        ai::planner::planning_continuation_decision(user_message, workflow_state.as_deref())
    } else {
        ai::planner::PlanningContinuation::NewIntent
    };
    // DEV-0060.1 §11（Active Planner 收口）：is_new_intent_message 关键词表不再作为
    // active Planner 下的唯一判断——除 Explicit Cancel（本地确定性）与显式新规划请求外，
    // 续跑 vs 新意图由 Semantic Router 判定（见下方 Turn Router 块）。
    let workflow_is_active = workflow_state
        .as_deref()
        .map(ai::planner::workflow_active)
        .unwrap_or(false);
    // 兼容兜底：旧会话（无 workflow 记录）且上一条是澄清提问
    let legacy_clarification = is_assistant
        && workflow_state.is_none()
        && ai::planner::is_clarification_reply(&last_assistant)
        && !ai::planner::is_new_intent_message(user_message)
        && !ai::planner::is_workflow_exit_intent(user_message);
    // Context Purpose 预判（Router 之后才最终定 route；planning 语境先按 planning 装载，
    // SemanticAction 路径不消费该 context，FastChat 只用 bounded history）
    let maybe_planning =
        legacy_clarification || workflow_is_active || gate == ai::planner::PlanningGate::Planning;

    // ---- DEV-0060 PART I：用户明确取消规划（不调 AI；workflow→cancelled；无 ChangeSet） ----
    if matches!(
        continuing_decision,
        ai::planner::PlanningContinuation::Cancel
    ) {
        let msg = "已退出这次规划流程。你可以继续问其他问题。";
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                msg,
                Some(run_id),
            );
            let mut payload = workflow_payload.clone();
            payload.updated_by_user_turn = user_message.to_string();
            ai::planner::set_workflow_payload(
                &conn,
                run_id,
                profile_id,
                conversation_id,
                ai::planner::WORKFLOW_STATE_CANCELLED,
                &payload,
            );
        }
        vault.record_ai("run_completed", run_id, "planner_cancelled");
        return Ok("planner_cancelled");
    }

    // ---- Context Builder（PART C：按 purpose 按需装载；Generic 只注入页面/模式） ----
    let (context_pack, recent_msgs, context_purpose) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let page = ai::context_builder::PageContext {
            page_label: page_label.to_string(),
            knowledge_path: knowledge_path.map(String::from),
            session_title: session_title.map(String::from),
            date: date.map(String::from),
            conversation_id: Some(conversation_id),
        };
        let purpose =
            ai::context_builder::detect_context_purpose(user_message, &page, maybe_planning);
        let report = ai::context_builder::build(
            &conn,
            profile_id,
            user_message,
            &page,
            "assistant",
            purpose,
        )?;
        let recent: Vec<(i64, String, String)> =
            repository::conversation::ConversationRepository::new(&conn)
                .list_messages(conversation_id, profile_id, 20, 0)
                .unwrap_or_default()
                .into_iter()
                .map(|m| (m.id, m.role, m.content))
                .collect();
        (report, recent, purpose)
    };
    let context_text = context_pack
        .layers
        .iter()
        .map(|l| format!("{}\n{}", l.name, l.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    // DEV-0061R §34：readonly「需要助手模式」gate 整体删除（Unified Higher AI）。

    // ---- DEV-0061R §9 · Turn Interpreter（唯一控制入口；ONE request 同时出 route+action） ----
    // §11 收口顺序：Explicit Cancel（上方已本地处理）→ 旧会话澄清兜底 → 显式规划 gate →
    // FastChat local shortcut → Turn Interpreter（一次控制调用，temp=0）。
    // DEV-0061R §34：NeedsAssistant 分支删除（Unified Higher AI；写入走 Approval Boundary）。
    let mut router_skills: Vec<String> = Vec::new();
    let mut turn: ai::runtime::TurnDecision = if legacy_clarification {
        // 旧会话无 workflow 记录：上一条是澄清提问 → 本地确定性续跑（不调 Interpreter）
        ai::runtime::TurnDecision::PlannerContinuation
    } else if gate == ai::planner::PlanningGate::Planning {
        // 显式规划蓝图（§12.2 收窄后的词表）→ Dedicated Planner
        ai::runtime::TurnDecision::Planning
    } else {
        let use_fast_local = !workflow_is_active && ai::runtime::fast_chat_shortcut(user_message);
        if use_fast_local {
            ai::runtime::TurnDecision::FastChat
        } else {
            // §18/§23 Capability Honesty：CONTROL（Interpreter/Repair/Selection）需要
            // basic_chat + structured_json + temperature_zero（DEV-0062R §13.2 加入
            // basic_chat）。任一 Known False（Some(false)）→ Provider 调用前安全拒绝
            // （0 ChangeSet；untested/unknown 的 legacy 迁移连接保持可运行，§13.3）。
            // §13.4：Limited ≠ Action 禁用——只按能力项判断，不看 overall status。
            if ai::provider::control_known_false(&control.capabilities) {
                let msg = ai::provider::control_capability_error(&control.display_name);
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','turn','completed','control_capability_guard')
                         ON CONFLICT(id) DO UPDATE SET status='completed', error='control_capability_guard', updated_at=datetime('now')",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    trace.run_finished(&conn, "completed");
                }
                vault.record_ai("run_completed", run_id, "control_capability_guard");
                return Ok("completed");
            }
            // §10：输入只有 当前消息 + Envelope + Planner 摘要 + Skill 摘要
            // + 最多 3 条 recent user messages（仅指代型辅助）+ Semantic Contract。
            // DEV-0062 §60：完整请求不带历史——needs_reference_history 门控（Current User Intent）
            let pending_q: Vec<String> = workflow_payload
                .pending_questions
                .iter()
                .map(|q| q.question.clone())
                .collect();
            let recent_user: Vec<String> = if ai::runtime::needs_reference_history(user_message) {
                recent_msgs
                    .iter()
                    .filter(|(_, r, _)| r == "user")
                    .map(|(_, _, c)| c.clone())
                    .collect()
            } else {
                Vec::new()
            };
            let prompt = ai::runtime::turn_interpreter_prompt(
                user_message,
                &envelope,
                workflow_is_active,
                &pending_q,
                &recent_user,
            );
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                // §27/§28：Interpreter = CONTROL AI（trace 带 ai_role + provider snapshot）
                trace.provider_request_started_role(
                    &conn,
                    1,
                    "secondary",
                    0,
                    "control",
                    Some(&control),
                );
            }
            let raw = control_client
                .chat_with_temperature(
                    vec![ChatMessage::system(prompt)],
                    true,
                    None,
                    Some(1400),
                    0.0, // §11：控制层 deterministic
                )
                .await
                .ok()
                .and_then(|c| c.content)
                .unwrap_or_default();
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                trace.provider_request_finished(&conn, 1, "secondary");
            }
            let mut decision = ai::runtime::parse_turn_decision(&raw);
            // §19 Repair Once：Interpreter 输出结构不合法 → 一次修复（temp=0；只含
            // Contract + invalid JSON + parser error，不带对话/资料）
            if decision.is_none() && !raw.trim().is_empty() {
                let repair = ai::semantic_contract::repair_instruction(
                    &raw,
                    "TurnInterpreter JSON 不合法或缺少必需字段",
                );
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    trace.provider_request_started_role(
                        &conn,
                        2,
                        "secondary",
                        0,
                        "control",
                        Some(&control),
                    );
                }
                let raw2 = control_client
                    .chat_with_temperature(
                        vec![ChatMessage::system(repair)],
                        true,
                        None,
                        Some(1400),
                        0.0,
                    )
                    .await
                    .ok()
                    .and_then(|c| c.content)
                    .unwrap_or_default();
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    trace.provider_request_finished(&conn, 2, "secondary");
                    trace.semantic_action_repaired(
                        &conn,
                        if raw2.trim().is_empty() {
                            "failed"
                        } else {
                            "repaired"
                        },
                    );
                }
                decision = ai::runtime::parse_turn_decision(&raw2);
            }
            decision.unwrap_or(ai::runtime::TurnDecision::HigherRead {
                // Conservative default：Interpreter 失败（含 Repair 后）→ 读路径兜底
                // （绝不把动作请求当 FastChat；active Planner 续跑交由上方 legacy 判定）
                skills: vec![],
            })
        }
    };
    // planner_continuation 仅在 active Planner 时成立；否则读路径
    if matches!(turn, ai::runtime::TurnDecision::PlannerContinuation) && !workflow_is_active {
        turn = ai::runtime::TurnDecision::HigherRead { skills: vec![] };
    }
    if let ai::runtime::TurnDecision::HigherRead { skills } = &turn {
        router_skills = skills.clone();
    }
    let route: String = match &turn {
        ai::runtime::TurnDecision::FastChat => "fast_chat".into(),
        ai::runtime::TurnDecision::HigherRead { .. } => "higher_read".into(),
        ai::runtime::TurnDecision::Action { .. } => "action".into(),
        ai::runtime::TurnDecision::Planning => "planning_gate".into(),
        ai::runtime::TurnDecision::PlannerContinuation => "planner_continuation".into(),
        ai::runtime::TurnDecision::Clarification { .. } => "clarification".into(),
    };
    // §11：active Planner 被新意图接管 → paused（旧规划不再劫持后续轮次）
    if workflow_is_active && route != "planner_continuation" && route != "planning_gate" {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::planner::set_workflow_payload(
            &conn,
            run_id,
            profile_id,
            conversation_id,
            ai::planner::WORKFLOW_STATE_PAUSED,
            &workflow_payload,
        );
    }
    // route → planning 管线变量（payload 记账 / instruction 构建）
    let continuing_planning = route == "planner_continuation";
    let is_planning_request = route == "planner_continuation" || route == "planning_gate";
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        trace.route_decided(
            &conn,
            &route,
            if route == "fast_chat" || is_planning_request {
                "local"
            } else {
                "semantic"
            },
            &router_skills,
        );
        trace.turn_decided(
            &conn,
            &route,
            if route == "fast_chat" || is_planning_request {
                "local"
            } else {
                "interpreter"
            },
        );
    }

    // ---- PART D · FastChat：真流式（tools=0 / Memory Extract=0 / 私有 Context=0） ----
    if route == "fast_chat" {
        // DEV-0062R §14 Primary Basic Capability Honesty：basic_chat 已知 false →
        // 不发送已知必失败请求（也不偷偷换 Connection）
        if primary.capabilities.basic_chat == Some(false) {
            let msg = ai::provider::primary_basic_error(&primary.display_name);
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                    conversation_id,
                    profile_id,
                    "assistant",
                    &msg,
                    Some(run_id),
                );
                let _ = conn.execute(
                    "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                     VALUES (?1,?2,?3,'assistant','fast_chat','completed','primary_basic_guard')
                     ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_basic_guard', updated_at=datetime('now')",
                    rusqlite::params![run_id, profile_id, conversation_id],
                );
                trace.run_finished(&conn, "completed");
            }
            vault.record_ai("run_completed", run_id, "primary_basic_guard");
            return Ok("completed");
        }
        let hist = ai::runtime::bound_history(&recent_msgs, current_message_id, 8, 14_000);
        let mut msgs: Vec<ChatMessage> = vec![ChatMessage::system(format!(
            "{}\n\n{}",
            ai::prompts::SYSTEM_PROMPT,
            envelope.prompt_block()
        ))];
        for (_id, r, c) in hist {
            msgs.push(ChatMessage {
                role: r,
                content: c,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        msgs.push(ChatMessage::user(user_message.to_string()));
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.context_built(
                &conn,
                msgs.iter().map(|m| m.content.chars().count()).sum(),
                &["fast_chat".into()],
            );
            trace.provider_request_started_role(&conn, 1, "main", 0, "primary", Some(&primary));
        }
        // DEV-0062 §24 Streaming Degradation：streaming=false（已知）→ 直接单次 non-stream
        // （完整回答晚一点出现；绝不重复生成两遍答案）。unknown → 先 stream，失败时仅
        // 未产生任何 delta 才允许一次 non-stream fallback（chat_stream 有部分内容即返回 Ok）。
        let mut first_delta = false;
        let streamed = if primary.capabilities.streaming == Some(false) {
            Err("streaming_disabled".to_string())
        } else {
            client
                .chat_stream(
                    msgs.clone(),
                    Some(2048),
                    0.3,
                    |d| {
                        first_delta = true;
                        ai::run::emit(
                            Some(app),
                            "ai://delta",
                            run_id,
                            serde_json::json!({ "delta": d }),
                        );
                    },
                    token.clone(),
                )
                .await
        };
        let (final_text, usage) = match streamed {
            Ok((t, u)) => (t, u),
            Err(_) => {
                // stream 失败 → 单次非流式 fallback（仍只有 1 次主请求语义）
                let c = client.chat(msgs, false, None, Some(2048)).await?;
                (c.content.unwrap_or_default(), c.usage)
            }
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            if first_delta {
                trace.provider_first_delta(&conn, 1);
            }
            trace.provider_request_finished(&conn, 1, "main");
        }
        let _ = usage;
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &final_text,
                Some(run_id),
            )?;
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,?4,'fast_chat','completed','')
                 ON CONFLICT(id) DO UPDATE SET status='completed', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, "assistant"],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "fast_chat");
        return Ok("completed");
    }

    // ---- DEV-0061R §9/§45.2 · Action：TurnDecision::Action 直接携带 SemanticAction ----
    // （一次控制请求同时决定 route 与 action；不再有第二次"到底是什么 action"调用）
    if let ai::runtime::TurnDecision::Action { action: act } = &turn {
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.semantic_action_parsed(&conn, act.type_name());
        }
        let mut made_changeset = false;
        let final_text: String = {
            // ---- DEV-0060.2 · Pre-Grounding（§9 优先级：Recent → Retrieval →
            // 唯一候选直接 Ground（0 call）→ 2..8 候选一次轻量 Selection（≤1 call））----
            let mut input = ai::action::PlanInput {
                user_message,
                conversation_id,
                ..Default::default()
            };
            if let Some((etype, hint)) = act.primary_reference() {
                let retrieved: Option<ai::grounding::GroundingOutcome> = (|| {
                    let conn = state.0.lock().ok()?;
                    trace.grounding_started(&conn, etype);
                    if hint.recency_hint.is_some() {
                        return ai::grounding::resolve_recent(
                            &conn,
                            profile_id,
                            conversation_id,
                            hint,
                        )
                        .ok();
                    }
                    let cands = if etype == "task" {
                        ai::grounding::retrieve_task_candidates(&conn, profile_id, hint, &envelope)
                            .ok()?
                    } else {
                        ai::grounding::retrieve_rule_candidates(&conn, profile_id, hint).ok()?
                    };
                    trace.candidates_retrieved(&conn, etype, cands.len());
                    if cands.is_empty() {
                        trace.grounding_not_found(&conn, etype);
                        return Some(ai::grounding::GroundingOutcome::NotFound(String::new()));
                    }
                    if cands.len() == 1 {
                        // AI-GND-006：候选唯一直接 Ground，0 额外 Provider Call
                        trace.grounding_resolved(&conn, etype, false);
                        return Some(ai::grounding::GroundingOutcome::Resolved(cands[0].real_id));
                    }
                    trace.grounding_ambiguous(&conn, cands.len());
                    Some(ai::grounding::GroundingOutcome::Ambiguous(cands))
                })();
                let grounded = match retrieved {
                    Some(ai::grounding::GroundingOutcome::Ambiguous(cands))
                        if (2..=ai::grounding::MAX_CANDIDATES).contains(&cands.len()) =>
                    {
                        // AI-GND-007/008：一次 Candidate Selection；只允许从 candidate_id 中选
                        let prompt = ai::grounding::selection_prompt(user_message, hint, &cands);
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            trace.candidate_selection_started(&conn, cands.len());
                            trace.provider_request_started_role(
                                &conn,
                                2,
                                "secondary",
                                0,
                                "control",
                                Some(&control),
                            );
                        }
                        let raw = control_client
                            .chat_with_temperature(
                                vec![ChatMessage::system(prompt)],
                                true,
                                None,
                                Some(300),
                                0.0, // §11：Candidate Selection deterministic
                            )
                            .await
                            .ok()
                            .and_then(|c| c.content)
                            .unwrap_or_default();
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            trace.provider_request_finished(&conn, 2, "secondary");
                        }
                        let sel = ai::grounding::parse_selection(&raw, &cands);
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            trace.candidate_selection_finished(
                                &conn,
                                match &sel {
                                    ai::grounding::SelectionOutcome::Selected(_) => "selected",
                                    ai::grounding::SelectionOutcome::Ambiguous(_) => "ambiguous",
                                    ai::grounding::SelectionOutcome::NoneFound => "none",
                                    ai::grounding::SelectionOutcome::Invalid => "invalid",
                                },
                            );
                        }
                        input.selection = Some(sel.clone());
                        input.selection_called = true;
                        let desc = hint.title_hint.trim().to_string();
                        let out = ai::grounding::ground_single(&desc, cands, Some(&sel));
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            match &out {
                                ai::grounding::GroundingOutcome::Resolved(_) => {
                                    trace.grounding_resolved(&conn, etype, true)
                                }
                                ai::grounding::GroundingOutcome::Ambiguous(c) => {
                                    trace.grounding_ambiguous(&conn, c.len())
                                }
                                _ => trace.grounding_not_found(&conn, etype),
                            }
                        }
                        Some(out)
                    }
                    other => other,
                };
                if etype == "task" {
                    input.pre_task = grounded;
                } else {
                    input.pre_rule = grounded;
                }
            }
            // ---- Grounded Action Plan（plan_action：多 op 仍 ONE ChangeSet）----
            let planned = {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                ai::action::plan_action(&conn, profile_id, &envelope, &input, &act)
            };
            match planned {
                Err(e) => format!("{e}\n\n（正式数据没有变化。）"),
                Ok(ai::action::ActionOutcome::ProposalReady {
                    ops,
                    title,
                    summary,
                    ..
                }) => {
                    // Empty Plan Guard（AI-GND-010/011）：0 op 绝不调 ChangeSetRepository::create，
                    // 用户绝不见内部错误文案
                    if ops.is_empty() {
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            trace.empty_plan_guarded(&conn, "zero_operations");
                        }
                        "没有产生可执行的修改。正式数据没有变化。".to_string()
                    } else if let Err(e) = ai::action::validate_ops(&envelope, &act, &ops) {
                        format!("{e}\n\n（正式数据没有变化。）")
                    } else {
                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                        trace.action_plan_compiled(&conn, ops.len());
                        match repository::changeset::ChangeSetRepository::new(&conn).create(
                            profile_id,
                            Some(conversation_id),
                            Some(run_id),
                            &title,
                            &summary,
                            &ops,
                        ) {
                            Ok(cs_id) => {
                                made_changeset = true;
                                ai::run::emit(
                                    Some(app),
                                    "ai://changeset",
                                    run_id,
                                    serde_json::json!({ "change_set_id": cs_id, "title": title, "count": ops.len() }),
                                );
                                // §25.2：backend deterministic 总结（禁止再调模型写漂亮总结）
                                format!(
                                            "已经准备好修改提案：{title}（{summary}）。共 {} 项操作。\n点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。",
                                            ops.len()
                                        )
                            }
                            Err(e) => format!("提案生成失败：{e}\n\n（正式数据没有变化。）"),
                        }
                    }
                }
                Ok(ai::action::ActionOutcome::NothingToChange(msg)) => {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    trace.empty_plan_guarded(&conn, "nothing_to_change");
                    msg
                }
                Ok(ai::action::ActionOutcome::Clarification {
                    message,
                    candidates,
                }) => {
                    // DEV-0062 §43：Ambiguous 澄清 → 持久化 Control State（ai_pending_actions
                    // active；同会话旧 pending 先 cancelled；restart 可续；0 ChangeSet）
                    if !candidates.is_empty() {
                        let pending_cands: Vec<repository::ai_pending_action::PendingCandidate> =
                            candidates
                                .iter()
                                .map(
                                    repository::ai_pending_action::PendingCandidate::from_grounding,
                                )
                                .collect();
                        let action_json = serde_json::to_string(&act).unwrap_or_default();
                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                        let _ =
                            repository::ai_pending_action::AiPendingActionRepository::new(&conn)
                                .create_or_replace(
                                    profile_id,
                                    conversation_id,
                                    Some(run_id),
                                    &action_json,
                                    &pending_cands,
                                    &message,
                                );
                    }
                    message
                }
                Ok(ai::action::ActionOutcome::NotFound(msg))
                | Ok(ai::action::ActionOutcome::Unsupported(msg))
                | Ok(ai::action::ActionOutcome::ContractFailure(msg)) => msg,
            }
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &final_text,
                Some(run_id),
            )?;
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant','semantic_action',?4,'')
                 ON CONFLICT(id) DO UPDATE SET status=?4, updated_at=datetime('now')",
                rusqlite::params![
                    run_id,
                    profile_id,
                    conversation_id,
                    if made_changeset {
                        "waiting_approval"
                    } else {
                        "completed"
                    }
                ],
            );
            trace.run_finished(
                &conn,
                if made_changeset {
                    "waiting_approval"
                } else {
                    "completed"
                },
            );
        }
        vault.record_ai("run_completed", run_id, "semantic_action");
        return Ok(if made_changeset {
            "waiting_approval"
        } else {
            "completed"
        });
    }

    // ---- DEV-0061R §9 · Clarification（陈述 vs 执行；Interpreter 直接给出确认问题） ----
    if let ai::runtime::TurnDecision::Clarification { question } = &turn {
        let question = if question.trim().is_empty() {
            "你的意思是希望我把它加入 Higher 吗？（例如设成每日任务/创建任务）如果想执行，请直接说「帮我创建…」；正式数据目前没有变化。".to_string()
        } else {
            format!(
                "{}\n（正式数据目前没有变化；如需执行请直接确认。）",
                question.trim()
            )
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.semantic_action_parsed(&conn, "router_clarification");
            let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &question,
                Some(run_id),
            );
            trace.run_finished(&conn, "clarification");
        }
        vault.record_ai("run_completed", run_id, "clarification");
        return Ok("clarification");
    }

    // DEV-0060 PART H/J/L：Planner 指令 = PLAN_DRAFT_INSTRUCTION + PLANNER_TURN_PROTOCOL +
    // workflow Q&A + Planning Truth。本地"旧 GoalBrief 缺项固定三问"gate 移除（§12）：
    // 缺什么信息由 Provider 按 Protocol 问（≤5、不重复已回答字段）；
    // 已有 active GoalTarget 时旧 Brief 永不阻塞（PART L）。
    // DEV-0060.1：is_planning_request 已由 Turn Router 决定（planner_continuation / planning_gate）。

    // ---- DEV-0062 §18/§23 · PRIMARY Capability Guard（HigherRead / Planner） ----
    // 已知缺失（Some(false)）→ 用户友好能力错误（0 raw 400 / missing field）；
    // untested/unknown（含 v024 迁移 legacy DeepSeek）保持可运行。
    // DEV-0062R §14：basic_chat 已知 false → 不发送已知必失败请求。
    if primary.capabilities.basic_chat == Some(false) {
        let msg = ai::provider::primary_basic_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &msg,
                Some(run_id),
            );
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_basic_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_basic_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_basic_guard");
        return Ok("completed");
    }
    if primary.capabilities.tool_calls == Some(false) {
        // HigherRead（读工具循环）与 Dedicated Planner（planning 工具）都依赖 Tool Calling
        let msg = ai::provider::primary_tools_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &msg,
                Some(run_id),
            );
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_capability_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_capability_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_capability_guard");
        return Ok("completed");
    }
    if is_planning_request && primary.capabilities.structured_json == Some(false) {
        let msg = ai::provider::primary_json_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
                conversation_id,
                profile_id,
                "assistant",
                &msg,
                Some(run_id),
            );
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_capability_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_capability_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_capability_guard");
        return Ok("completed");
    }
    let mut planning_payload = workflow_payload.clone();
    let instruction = if is_planning_request {
        let truth = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            ai::planner::build_planning_truth_context(&conn, profile_id)
        };
        // payload 记账（§10.3）：续跑 → 吸收本轮回答；新开 → 记录原始请求与目标来源
        if continuing_planning {
            planning_payload.record_user_reply(user_message);
        } else {
            planning_payload.original_request = user_message.chars().take(2000).collect();
            planning_payload.started_from_run_id = run_id.to_string();
            planning_payload.goal_source = if truth.has_active_goal_target {
                "goal_target".to_string()
            } else {
                "none".to_string()
            };
            planning_payload.updated_by_user_turn = user_message.to_string();
        }
        ai::planner::build_planning_instruction(&truth.instruction, &planning_payload)
    } else if is_assistant {
        ai::prompts::ASSISTANT_CHAT_INSTRUCTION.to_string()
    } else {
        format!("{}\n\n{}", ai::prompts::READONLY_INTENT, "以上为只读协议。若用户消息并不涉及修改数据（纯咨询/分析），忽略该协议，正常回答（但不得调用任何修改类工具）。")
    };
    // DEV-0060 PART A §5.1-5.2：消息组装——Context/Instruction 走 system（背景），
    // 当前用户消息永远是最后一个真实 User Turn（禁止 Context 冒充 User Message）。
    // DEV-0060.1 §6：读路径注入 Runtime Time Truth（Planner 路径 truth context 已含日期）
    let context_text = if is_planning_request {
        context_text
    } else {
        format!("{}\n\n{}", envelope.prompt_block(), context_text)
    };
    let mut messages: Vec<ChatMessage> = ai::planner::build_chat_messages(
        ai::prompts::SYSTEM_PROMPT,
        &context_text,
        &instruction,
        &recent_msgs,
        current_message_id,
        user_message,
    );

    // ---- Source Registry（§108） ----
    let mut sources: Vec<ai::web::WebSource> = Vec::new();
    let mut tool_trace: Vec<ai::tools::ToolTraceEntry> = Vec::new();
    let mut changeset_ids: Vec<i64> = Vec::new();
    let mut used_web = false;
    let mut usage_total = ai::client::Usage::default();

    // ---- 工具循环（最多 6 轮） ----
    const MAX_ROUNDS: usize = 6;
    // DEV-0060.1 PART J（§21.2）：按 route 动态裁剪工具——禁止每轮全量 21 tools。
    // planning（含续跑）→ planning+web；读路径 → personal/task/knowledge/read。
    let route_for_tools = if is_planning_request {
        "planning"
    } else {
        "higher_read"
    };
    let tools =
        ai::tools::tool_definitions_for_scopes(&ai::tools::scopes_for_route(route_for_tools));
    let mut final_text = String::new();
    let mut cancelled = false;
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        trace.context_built(
            &conn,
            context_text.chars().count(),
            &context_pack.chips.clone(),
        );
    }
    'outer: for _round in 0..MAX_ROUNDS {
        if token.is_cancelled() {
            cancelled = true;
            break;
        }
        // 工具循环轮用非流式（需要 tool_calls）；最终轮流式
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_started_role(
                &conn,
                _round as i64 + 1,
                "main",
                tools.as_array().map(|a| a.len()).unwrap_or(0),
                "primary",
                Some(&primary),
            );
        }
        let completion = client
            .chat(messages.clone(), false, Some(tools.clone()), Some(4096))
            .await?;
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_finished(&conn, _round as i64 + 1, "main");
        }
        usage_total.prompt_tokens += completion.usage.prompt_tokens;
        usage_total.completion_tokens += completion.usage.completion_tokens;
        usage_total.total_tokens += completion.usage.total_tokens;
        let tool_calls = match ai::planner::classify_tool_round(
            completion.tool_calls.as_ref(),
            completion.content.as_deref(),
        ) {
            ai::planner::ToolRoundOutcome::FinalAnswer(text) => {
                // DEV-0060 §6.1（PART B）：无 tool_calls → completion.content 即本轮最终回答。
                // 直接采用并通过 ai://delta 发送完整文本；**不得再次请求 Provider**
                // （旧的 assistant-only 二次 chat_stream 已删除：避免回复漂移/指令丢失/双倍 token）。
                final_text = text;
                ai::run::emit(
                    Some(app),
                    "ai://delta",
                    run_id,
                    serde_json::json!({ "delta": final_text }),
                );
                break 'outer;
            }
            ai::planner::ToolRoundOutcome::ExecuteTools(tc) => tc,
        };
        // 处理 tool calls
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: completion.content.clone().unwrap_or_default(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });
        for tc in tool_calls.as_array().cloned().unwrap_or_default() {
            if token.is_cancelled() {
                cancelled = true;
                break 'outer;
            }
            let fname = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let fid = tc
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let args: serde_json::Value =
                serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            if !ai::tools::TOOL_ALLOWLIST.contains(&fname) {
                messages.push(ChatMessage {
                    role: "tool".into(),
                    content: format!("未知工具 {}（拒绝）", fname),
                    tool_calls: None,
                    tool_call_id: Some(fid),
                    name: Some(fname.to_string()),
                });
                continue;
            }
            // DEV-0061R §34：readonly 助手门已删除（Unified AI；propose_change_set
            // 只产生 ChangeSet Draft，正式写入仍走用户 Approval）
            // web 门（未启用 → 明确提示）
            if (fname == "web_search" || fname == "web_open") && !web_enabled {
                tool_trace.push(ai::tools::ToolTraceEntry {
                    tool: fname.into(),
                    label: ai::tools::tool_label(fname).into(),
                    status: "error".into(),
                });
                messages.push(ChatMessage {
                    role: "tool".into(),
                    content: "联网搜索未启用（设置 → 联网搜索）".into(),
                    tool_calls: None,
                    tool_call_id: Some(fid),
                    name: Some(fname.to_string()),
                });
                continue;
            }
            let result: Result<String, String> = match fname {
                "web_search" => {
                    used_web = true;
                    let q = args
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let count = args.get("count").and_then(|v| v.as_i64()).unwrap_or(5) as u32;
                    let fresh = args
                        .get("freshness")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let res = ai::web::brave_search(brave_key, &q, count, fresh.as_deref()).await;
                    match res {
                        Ok(items) => {
                            let mut out_items = Vec::new();
                            for (title, url, snippet, published) in items {
                                let sid = format!("S{}", sources.len() + 1);
                                let ws = ai::web::WebSource {
                                    sid: sid.clone(),
                                    title: title.clone(),
                                    url: url.clone(),
                                    snippet: snippet.clone(),
                                    published_at: published,
                                    source_type: "web".into(),
                                    retrieved_at: chrono_now(),
                                };
                                ai::run::emit(
                                    Some(app),
                                    "ai://source",
                                    run_id,
                                    serde_json::to_value(&ws).unwrap_or_default(),
                                );
                                out_items.push(serde_json::json!({ "sid": sid, "title": title, "url": url, "snippet": snippet }));
                                sources.push(ws);
                            }
                            Ok(serde_json::json!({ "results": out_items, "note": "引用时用 [[S1]] 格式" }).to_string())
                        }
                        Err(e) => Err(e),
                    }
                }
                "web_open" => {
                    used_web = true;
                    let url = if let Some(sid) = args.get("sid").and_then(|v| v.as_str()) {
                        sources
                            .iter()
                            .find(|s| s.sid == sid)
                            .map(|s| s.url.clone())
                            .ok_or_else(|| {
                                format!("来源 {} 不存在（只能打开 web_search 返回过的来源）", sid)
                            })?
                    } else if let Some(u) = args.get("url").and_then(|v| v.as_str()) {
                        u.to_string()
                    } else {
                        String::new()
                    };
                    ai::web::web_open(&url).await
                }
                "propose_change_set" => {
                    let title = args
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("修改提案")
                        .to_string();
                    let summary = args
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ops_json = args
                        .get("operations")
                        .cloned()
                        .unwrap_or(serde_json::json!([]));
                    let ops: Vec<repository::changeset::ProposedOp> =
                        serde_json::from_value(ops_json)
                            .map_err(|e| format!("提案格式错误：{e}"))?;
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let cs_id = repository::changeset::ChangeSetRepository::new(&conn).create(
                        profile_id,
                        Some(conversation_id),
                        Some(run_id),
                        &title,
                        &summary,
                        &ops,
                    )?;
                    changeset_ids.push(cs_id);
                    vault.record_ai("changeset_proposed", run_id, &title);
                    ai::run::emit(
                        Some(app),
                        "ai://changeset",
                        run_id,
                        serde_json::json!({ "change_set_id": cs_id, "title": title, "count": ops.len() }),
                    );
                    Ok(serde_json::json!({ "ok": true, "change_set_id": cs_id, "note": "提案已生成，等待用户审查" }).to_string())
                }
                _ => {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    ai::tools::execute_read_tool(&conn, profile_id, fname, &args)
                }
            };
            match result {
                Ok(out) => {
                    tool_trace.push(ai::tools::ToolTraceEntry {
                        tool: fname.into(),
                        label: ai::tools::tool_label(fname).into(),
                        status: "success".into(),
                    });
                    messages.push(ChatMessage {
                        role: "tool".into(),
                        content: out.chars().take(20_000).collect(),
                        tool_calls: None,
                        tool_call_id: Some(fid),
                        name: Some(fname.to_string()),
                    });
                }
                Err(e) => {
                    tool_trace.push(ai::tools::ToolTraceEntry {
                        tool: fname.into(),
                        label: ai::tools::tool_label(fname).into(),
                        status: "error".into(),
                    });
                    messages.push(ChatMessage {
                        role: "tool".into(),
                        content: format!("[错误] {}", e),
                        tool_calls: None,
                        tool_call_id: Some(fid),
                        name: Some(fname.to_string()),
                    });
                }
            }
        }
    }
    if cancelled {
        // §19-20：保留已产出；数据 0 修改（ChangeSet 未 apply 本就不动数据）
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = repository::conversation::ConversationRepository::new(&conn).add_message(
            conversation_id,
            profile_id,
            "assistant",
            &format!(
                "（已停止。已生成内容：{}）",
                if final_text.is_empty() {
                    "无"
                } else {
                    &final_text
                }
            ),
            Some(run_id),
        );
        vault.record_ai("run_cancelled", run_id, "");
        return Ok("cancelled");
    }

    // ---- DEV-0055 PART 12/15/16 + DEV-0060 PART H：Planning Pipeline 收尾（Deterministic Compile） ----
    // 规划请求：模型输出 PlannerTurnResult JSON（clarification / plan_draft / handoff_chat）
    // → Backend 确定性处理（不依赖模型调 propose_change_set —— §33/§58）。
    if is_planning_request && changeset_ids.is_empty() {
        let trimmed = final_text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        // DEV-0060 §12：先按 PlannerTurnResult 协议解析；失败回退直接 PlanDraft（兼容旧输出）
        let turn: Option<(String, serde_json::Value)> =
            serde_json::from_str::<serde_json::Value>(trimmed)
                .ok()
                .and_then(|v| {
                    let t = v.get("type").and_then(|t| t.as_str()).map(String::from);
                    t.map(|t| (t, v))
                });
        if let Some((t, v)) = &turn {
            if t == "clarification" {
                // TYPE A：解析 questions（≤5）→ 过滤已回答字段（T10）→ workflow=clarifying
                let qs: Vec<ai::planner::PlannerQuestion> = v
                    .get("questions")
                    .and_then(|q| q.as_array())
                    .map(|arr| {
                        arr.iter()
                            .take(ai::planner::MAX_BLOCKING_QUESTIONS)
                            .filter_map(|q| serde_json::from_value(q.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                let remaining =
                    ai::planner::filter_pending_questions(qs, &planning_payload.answered);
                let reply = ai::planner::format_clarification_reply(&remaining);
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(
                            conversation_id,
                            profile_id,
                            "assistant",
                            &reply,
                            Some(run_id),
                        );
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','planning','completed','clarification')
                         ON CONFLICT(id) DO UPDATE SET status='completed'",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    let mut payload = planning_payload.clone();
                    payload.pending_questions = remaining;
                    ai::planner::set_workflow_payload(
                        &conn,
                        run_id,
                        profile_id,
                        conversation_id,
                        ai::planner::WORKFLOW_STATE_CLARIFYING,
                        &payload,
                    );
                }
                vault.record_ai("run_completed", run_id, "clarification");
                return Ok("clarification");
            }
            if t == "handoff_chat" {
                // TYPE C §12：用户当前消息不是继续本规划 → workflow=paused（inactive），
                // 以 Provider 给出的正常回复完成本轮（不被旧 Planner 劫持）
                let msg = v
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                if !msg.is_empty() {
                    final_text = msg;
                }
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    ai::planner::set_workflow_payload(
                        &conn,
                        run_id,
                        profile_id,
                        conversation_id,
                        ai::planner::WORKFLOW_STATE_PAUSED,
                        &planning_payload,
                    );
                }
                // 落库 + 返回（跳过 plan_draft 管线）
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    repository::conversation::ConversationRepository::new(&conn).add_message(
                        conversation_id,
                        profile_id,
                        "assistant",
                        &final_text,
                        Some(run_id),
                    )?;
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,?4,'planning','completed','handoff_chat')
                         ON CONFLICT(id) DO UPDATE SET status='completed'",
                        rusqlite::params![run_id, profile_id, conversation_id,
                            "assistant"],
                    );
                }
                vault.record_ai("run_completed", run_id, "handoff_chat");
                return Ok("handoff_chat");
            }
        }
        let draft_value: Option<serde_json::Value> = match &turn {
            Some((t, v)) if t == "plan_draft" => v.get("draft").cloned(),
            _ => serde_json::from_str::<serde_json::Value>(trimmed).ok(),
        };
        let draft_parsed: Option<ai::planner::PlanDraft> = draft_value
            .and_then(|d| serde_json::from_value(d).ok())
            .or_else(|| serde_json::from_str::<ai::planner::PlanDraft>(trimmed).ok());
        match draft_parsed {
            Some(mut draft) => {
                // DEV-0057 §88-90：Validation 失败 → 模型自动重试**一次**（错误回喂）；
                // 第二次仍失败 → 显示具体错误（不循环）。
                let mut validation = {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    // DEV-0059.2 §7：Blueprint 场景继承 active GoalTarget 主场景（compile 前 resolve）
                    if let Some(bp) = draft.blueprint.as_mut() {
                        bp.scenario_type =
                            ai::planner::resolve_blueprint_scenario(&conn, profile_id, bp, false);
                    }
                    ai::planner::validate_plan_draft(&conn, profile_id, &draft)
                };
                if !validation.errors.is_empty() && !cancelled {
                    let err_list = validation.errors.join("；");
                    let retry_prompt = format!(
                        "你上一版计划草稿未通过系统校验：{}\n\n请修正以上全部问题后，重新输出完整 JSON（同一 schema，不要解释文字）。",
                        err_list
                    );
                    messages.push(ChatMessage::assistant(trimmed.to_string()));
                    messages.push(ChatMessage::user(retry_prompt));
                    if token.is_cancelled() {
                        cancelled = true;
                    }
                    if !cancelled {
                        if let Ok(retry) =
                            client.chat(messages.clone(), false, None, Some(4096)).await
                        {
                            usage_total.prompt_tokens += retry.usage.prompt_tokens;
                            usage_total.completion_tokens += retry.usage.completion_tokens;
                            usage_total.total_tokens += retry.usage.total_tokens;
                            let rtext = retry
                                .content
                                .unwrap_or_default()
                                .trim()
                                .trim_start_matches("```json")
                                .trim_start_matches("```")
                                .trim_end_matches("```")
                                .trim()
                                .to_string();
                            if let Ok(d2) = serde_json::from_str::<ai::planner::PlanDraft>(&rtext) {
                                draft = d2;
                                validation = {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    if let Some(bp) = draft.blueprint.as_mut() {
                                        bp.scenario_type = ai::planner::resolve_blueprint_scenario(
                                            &conn, profile_id, bp, false,
                                        );
                                    }
                                    ai::planner::validate_plan_draft(&conn, profile_id, &draft)
                                };
                            }
                        }
                    }
                }
                let (mut validation, ops, _final_id) = {
                    let mut v = validation;
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let fid: Option<i64> = conn
                        .query_row(
                            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
                            rusqlite::params![profile_id],
                            |r| r.get(0),
                        )
                        .ok();
                    // DEV-0060 PART K §15.2：无 active GoalTarget 时 target_proposal 编入同一 ChangeSet
                    let has_gt = !repository::goal_target::GoalTargetRepository::new(&conn)
                        .list_active(profile_id, None, None)
                        .unwrap_or_default()
                        .is_empty();
                    // DEV-0077.4-A.1 F1：Production 唯一编译入口（禁 fallback）；
                    // Grounding 缺失 → 计入校验错误，走既有「错误回喂重试一次」失败分支
                    let ops = match ai::planner::compile_production_plan(
                        &conn, profile_id, fid, has_gt, &draft,
                    ) {
                        Ok((o, _)) => o,
                        Err(e) => {
                            v.errors.push(e);
                            Vec::new()
                        }
                    };
                    (v, ops, fid)
                };
                if !validation.errors.is_empty() {
                    // §55 验证失败 → 拒绝入库；提示重新生成（一次内联修复机会：把错误回喂重试一轮）
                    let err_list = validation.errors.join("；");
                    final_text = format!(
                        "计划草稿未通过校验，暂未生成可应用方案：{}\n\n请回复「重新生成」，我会修正后重新提交。",
                        err_list
                    );
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(
                            conversation_id,
                            profile_id,
                            "assistant",
                            &final_text,
                            Some(run_id),
                        );
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','planning','failed','plan_validation')
                         ON CONFLICT(id) DO UPDATE SET status='failed'",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    // §6.8：校验失败 → workflow failed（用户回复"重新生成"将重开规划）
                    ai::planner::set_workflow_state(
                        &conn,
                        run_id,
                        profile_id,
                        conversation_id,
                        ai::planner::WORKFLOW_STATE_FAILED,
                        None,
                    );
                    vault.record_ai("run_completed", run_id, "plan_validation_failed");
                    return Ok("plan_validation_failed");
                }
                if !validation.overloaded_days.is_empty() {
                    // §57 OVERLOADED：标记提示（本轮接受一次降载重试不可行——直接告知）
                    let od = validation.overloaded_days.join("；");
                    final_text.push_str(&format!(
                        "\n\n（部分日期计划量超出可用时间：{}。可在审查中取消超载任务。）",
                        od
                    ));
                }
                if !ai::planner::ops_within_limit(&ops) {
                    final_text = "生成的计划规模过大（超过单次修改上限 120 项）。长期计划会随着学习进度变化，建议按月或 14 天滚动生成。".to_string();
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(
                            conversation_id,
                            profile_id,
                            "assistant",
                            &final_text,
                            Some(run_id),
                        );
                    // §6.8：超限 → workflow failed
                    ai::planner::set_workflow_state(
                        &conn,
                        run_id,
                        profile_id,
                        conversation_id,
                        ai::planner::WORKFLOW_STATE_FAILED,
                        None,
                    );
                    vault.record_ai("run_completed", run_id, "plan_too_large");
                    return Ok("plan_too_large");
                }
                // §58-59：Compiler → ChangeSet（ForwardRef 由 create 期 Guard 兜底）
                let cs_title = format!("学习计划（{} 项）", ops.len());
                // DEV-0058 §103-104：summary 只显示非零项（零项不展示）；§121 休息日计数
                let goal_count =
                    draft.year_goals.len() + draft.month_goals.len() + draft.day_goals.len();
                let rest_count = draft.day_goals.iter().filter(|d| d.rest_day).count();
                let mut summary_parts: Vec<String> = Vec::new();
                if goal_count > 0 {
                    summary_parts.push(format!("阶段目标 +{}", goal_count));
                }
                if draft.knowledge_nodes.len() > 0 {
                    summary_parts.push(format!("知识节点 +{}", draft.knowledge_nodes.len()));
                }
                if draft.tasks.len() > 0 {
                    summary_parts.push(format!("学习任务 +{}", draft.tasks.len()));
                }
                if rest_count > 0 {
                    summary_parts.push(format!("休息日 {}", rest_count));
                }
                let summary = summary_parts.join(" · ");
                // 日期范围（§103/§113）
                let mut plan_dates: Vec<&str> = draft
                    .day_goals
                    .iter()
                    .map(|d| d.period.as_str())
                    .chain(draft.tasks.iter().map(|t| t.date.as_str()))
                    .collect();
                plan_dates.sort_unstable();
                plan_dates.dedup();
                let range_line = match (plan_dates.first(), plan_dates.last()) {
                    (Some(a), Some(b)) if a != b => {
                        format!("计划范围：{} → {}", fmt_md(a), fmt_md(b))
                    }
                    (Some(a), _) => format!("计划范围：{}", fmt_md(a)),
                    _ => String::new(),
                };
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                match repository::changeset::ChangeSetRepository::new(&conn).create(
                    profile_id,
                    Some(conversation_id),
                    Some(run_id),
                    &cs_title,
                    &summary,
                    &ops,
                ) {
                    Ok(cs_id) => {
                        changeset_ids.push(cs_id);
                        // §66/§106：AI 只能说"已准备好计划"，不说"已加入"；零项行不展示（§104）
                        let mut lines: Vec<String> = vec!["已经准备好一份可执行计划。".into()];
                        if !range_line.is_empty() {
                            lines.push(range_line);
                        }
                        lines.push("本次将：".into());
                        if goal_count > 0 {
                            lines.push(format!("新增 {} 个阶段目标", goal_count));
                        }
                        if draft.knowledge_nodes.len() > 0 {
                            lines.push(format!("新增 {} 个知识节点", draft.knowledge_nodes.len()));
                        }
                        if draft.tasks.len() > 0 {
                            lines.push(format!("安排 {} 个学习任务", draft.tasks.len()));
                        }
                        if rest_count > 0 {
                            lines.push(format!("包含 {} 个休息日", rest_count));
                        }
                        lines.push(
                            "点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。".into(),
                        );
                        final_text = lines.join("\n");
                        ai::run::emit(
                            Some(app),
                            "ai://changeset",
                            run_id,
                            serde_json::json!({
                                "change_set_id": cs_id, "title": cs_title, "count": ops.len()
                            }),
                        );
                    }
                    Err(e) => {
                        final_text = format!("计划转换失败：{e}\n\n请回复「重新生成」。");
                    }
                }
            }
            None => {
                // 模型未按格式输出 → 引导重试（不假装成功 §66-68）
                final_text = format!(
                    "{}\n\n（系统提示：本次未生成结构化计划草稿，正式数据没有变化。请回复「重新生成计划」。）",
                    if final_text.is_empty() { "（无内容）" } else { &final_text }
                );
            }
        }
    }

    // ---- Citation 校验（§115-116） ----
    let mut citation_warning = None;
    if used_web {
        let valid_ids: Vec<String> = sources.iter().map(|s| s.sid.clone()).collect();
        let mut bad: Vec<String> = Vec::new();
        for cap in citation_re(&final_text).find_iter(&final_text) {
            let id = cap.1.to_string();
            if !valid_ids.contains(&id) {
                bad.push(id);
            }
        }
        let has_any = valid_ids
            .iter()
            .any(|id| final_text.contains(&format!("[[{}]]", id)));
        if (!bad.is_empty() || !has_any) && !final_text.is_empty() {
            // §116 一次 Citation Repair（只加引用不加事实）
            let listed = valid_ids
                .iter()
                .map(|s| format!("[[{}]]", s))
                .collect::<Vec<_>>()
                .join(" ");
            let repair_prompt = format!(
                "你刚才的回答{}。请只在原回答基础上为依赖网络信息的句子添加已有来源引用（{}），不得新增任何事实或删改内容；原样输出修改后的完整回答。",
                if bad.is_empty() { "没有任何来源引用" } else { "包含不存在的来源引用" },
                listed
            );
            messages.push(ChatMessage::assistant(final_text.clone()));
            messages.push(ChatMessage::user(repair_prompt));
            if let Ok(c) = client.chat(messages.clone(), false, None, Some(4096)).await {
                if let Some(t) = c.content {
                    let valid_now = valid_ids
                        .iter()
                        .any(|id| t.contains(&format!("[[{}]]", id)));
                    if valid_now
                        && citation_re(&t)
                            .find_iter(&t)
                            .iter()
                            .all(|m| valid_ids.contains(&m.1.to_string()))
                    {
                        final_text = t;
                    } else {
                        citation_warning = Some("本次联网回答的来源关联不完整，请谨慎参考。");
                    }
                }
            }
        }
    }

    // DEV-0061R §34：旧只读协议解析已删除（Unified Higher AI）。

    // ---- DEV-0062 §62 · Truth Guard（禁止假 Proposal） ----
    // route=HigherRead 且用户是明确 Write Intent 且本轮 0 ChangeSet →
    // 禁止保留模型「已创建/已修改/提案已准备」文本，最终可见内容改为确定性真话
    // （error / trace = write_route_miss；主修复是 Pending Action Continuation）。
    let requires_change_set = is_assistant && ai::prompts::detect_write_intent(user_message);
    let mut guard_appended = false;
    if requires_change_set && changeset_ids.is_empty() && !final_text.is_empty() {
        final_text = "本轮没有生成可审批的修改方案，正式数据没有变化。\n\n如果你是在修改某个任务，请明确任务对象后重试；\n若刚才 Higher 正在让你选择候选，请直接从候选中选择。".to_string();
        guard_appended = true;
    }

    // ---- 保存 assistant 消息 + 来源 ----
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let mut save = final_text.clone();
        if let Some(w) = &citation_warning {
            save.push_str(&format!("\n\n（{}）", w));
        }
        repository::conversation::ConversationRepository::new(&conn).add_message(
            conversation_id,
            profile_id,
            "assistant",
            &save,
            Some(run_id),
        )?;
        // ai_sources 落库（run 结束释放 RAM，历史进 DB §180）
        for s in &sources {
            let _ = conn.execute(
                "INSERT INTO ai_sources (profile_id, run_id, source_type, title, url, snippet, published_at)
                 VALUES (?1,?2,'web',?3,?4,?5,?6)",
                rusqlite::params![profile_id, run_id, s.title, s.url, s.snippet, s.published_at],
            );
        }
        // ai_runs 终态（§8：记录 requires_change_set）
        let _ = conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error, prompt_tokens, completion_tokens, total_tokens)
             VALUES (?1,?2,?3,?4,'assistant_chat',?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET status='completed', error=excluded.error, updated_at=datetime('now')",
            rusqlite::params![run_id, profile_id, conversation_id, "assistant",
                "completed",
                if guard_appended { "write_route_miss" } else { "" },
                usage_total.prompt_tokens, usage_total.completion_tokens, usage_total.total_tokens],
        );
        // §6.8：规划成功生成 ChangeSet → workflow waiting_approval（用户应用后 → applied）
        if is_planning_request && !changeset_ids.is_empty() {
            ai::planner::set_workflow_payload(
                &conn,
                run_id,
                profile_id,
                conversation_id,
                ai::planner::WORKFLOW_STATE_WAITING_APPROVAL,
                &planning_payload,
            );
        }
    }
    vault.record_ai(
        "run_completed",
        run_id,
        &format!("tokens={}", usage_total.total_tokens),
    );

    // ---- §9：guard → 通知前端显示 [重新生成修改方案] ----
    if guard_appended {
        ai::run::emit(
            Some(app),
            "ai://run-status",
            run_id,
            serde_json::json!({
                "status": "no_changeset",
                "message": "Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。",
            }),
        );
    }

    // DEV-0061R §34：旧「需要助手模式」前端通知已删除（Unified Higher AI）。

    // ---- Memory Extract（§36-38：run 完成后轻量二次调用） ----
    // DEV-0060 §6.4：Generic Chat（如「1+1」「你好」「解释概念」）无长期用户事实 →
    // 跳过 Memory Extract（secondary operation 也按需；Personal/Planning 保持原逻辑）。
    if !user_message.trim().is_empty()
        && !final_text.is_empty()
        && context_purpose != ai::context_builder::ContextPurpose::Generic
    {
        let extract = client
            .chat(
                vec![ai::client::ChatMessage::user(format!(
                    "{}\n\n用户消息：{}\n\nAI 回复：{}",
                    ai::prompts::MEMORY_EXTRACT_INSTRUCTION,
                    user_message.chars().take(4000).collect::<String>(),
                    final_text.chars().take(4000).collect::<String>()
                ))],
                true,
                None,
                Some(1000),
            )
            .await;
        if let Ok(c) = extract {
            let raw = c.content.unwrap_or_default();
            let t2 = raw
                .trim()
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(t2) {
                if let Some(arr) = v.get("memories").and_then(|m| m.as_array()) {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let repo = repository::memory::MemoryRepository::new(&conn);
                    let before_count = repo.count_since(profile_id, "2000-01-01").unwrap_or(0);
                    for m in arr.iter().take(5) {
                        // DEV-0057 §209/§214：新 Extractor 只生成实际支持类型
                        //（system_observation / goal_context 无 writer → 不入库，§42-43）
                        let mtype_raw = m
                            .get("memory_type")
                            .and_then(|x| x.as_str())
                            .unwrap_or("user_fact");
                        let mtype = match mtype_raw {
                            "user_fact" | "user_opinion" | "user_preference"
                            | "user_constraint" | "ai_inference" => mtype_raw,
                            _ => "user_fact",
                        };
                        // DEV-0057 §214-215：key 不再由模型自由决定——
                        // category + normalized subject 稳定生成（去空白/标点/小写截断），
                        // 同一事实重复 → supersede 而非无限重复。
                        let category = m.get("category").and_then(|x| x.as_str()).unwrap_or("chat");
                        let subject =
                            m.get("memory_key")
                                .and_then(|x| x.as_str())
                                .unwrap_or_else(|| {
                                    m.get("memory_value").and_then(|x| x.as_str()).unwrap_or("")
                                });
                        let normalized_key = normalize_memory_key(category, subject);
                        let rec = repository::memory::MemoryRecord {
                            id: 0,
                            profile_id,
                            memory_type: mtype.to_string(),
                            category: category.to_string(),
                            memory_key: normalized_key,
                            memory_value: m
                                .get("memory_value")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            source_kind: if mtype == "ai_inference" {
                                "ai_inference"
                            } else {
                                "user_message"
                            }
                            .to_string(),
                            source_ref: format!("conversation:{}", conversation_id),
                            source_excerpt: m
                                .get("source_excerpt")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            importance: m
                                .get("importance")
                                .and_then(|x| x.as_i64())
                                .unwrap_or(3)
                                .clamp(1, 5),
                            confidence: m
                                .get("confidence")
                                .and_then(|x| x.as_str())
                                .unwrap_or("medium")
                                .to_string(),
                            status: "active".into(),
                            valid_from: None,
                            valid_to: None,
                            supersedes_id: None,
                            created_at: String::new(),
                            updated_at: String::new(),
                            last_used_at: None,
                        };
                        if !rec.memory_value.is_empty() {
                            // DEV-0076 §七：AI 生成的记忆候选必须 pending_confirmation
                            //（确认门；v027 CHECK 已无 'active'，旧 insert 会违约）
                            let _ = repo.create_pending_memory(&rec);
                        }
                    }
                    // §87-88：新长期信息 → dirty
                    let after_count = repo.count_since(profile_id, "2000-01-01").unwrap_or(0);
                    if after_count > before_count {
                        let _ = repository::personalization::PersonalizationRepository::new(&conn)
                            .mark_dirty(profile_id);
                    }
                }
            }
        }
    }
    Ok("completed")
}

/// §110 citation 正则替代（手工扫描 [[Sx]]）。
pub struct CitationIter;
pub fn citation_re(_s: &str) -> CitationIter {
    CitationIter
}

/// DEV-0057 §214：memory_key 归一——`{category}::{subject 规范化}`。
/// 规范化：小写 + 仅保留字母数字（标点/空白直接删除）+ 截断 60 字符。
/// 稳定可重现：同事实的任意书写差异（空格/标点/大小写）→ 同 key（supersede 生效前提）。
pub fn normalize_memory_key(category: &str, subject: &str) -> String {
    let mut norm = String::new();
    for ch in subject.chars() {
        if ch.is_alphanumeric() {
            norm.extend(ch.to_lowercase());
        }
    }
    let norm: String = norm.chars().take(60).collect();
    format!("{}::{}", category.to_lowercase(), norm)
}

/// DEV-0055：UTC+8 学习日 YYYY-MM-DD（Planning Pipeline 注入当前日期）。
pub fn chrono_today() -> String {
    // 学习日 = UTC+8（与 StudySession 学习日不变量一致）
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs() as i64 + 8 * 3600;
            days_to_iso(secs / 86400)
        })
        .unwrap_or_else(|_| "1970-01-01".to_string())
}

/// DEV-0058 §103/§120：`YYYY-MM-DD` → `M月D日`（用户可读；非法输入原样返回）。
pub fn fmt_md(d: &str) -> String {
    if d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-' {
        let m: i64 = d[5..7].parse().unwrap_or(0);
        let day: i64 = d[8..10].parse().unwrap_or(0);
        format!("{}月{}日", m, day)
    } else {
        d.to_string()
    }
}

/// Unix epoch day → ISO 日期（无外部依赖；civil-from-days 算法）。
pub fn days_to_iso(z: i64) -> String {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

impl CitationIter {
    fn find_iter<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut out = Vec::new();
        let b = text.as_bytes();
        let mut i = 0usize;
        while i + 4 <= b.len() {
            if b[i] == b'[' && b[i + 1] == b'[' && b[i + 2] == b'S' {
                let mut j = i + 3;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < b.len() && b[j] == b']' && b[j + 1] == b']' && j > i + 3 {
                    out.push((i, &text[i + 2..j]));
                    i = j + 2;
                    continue;
                }
            }
            i += 1;
        }
        out
    }
}

/// §18 取消。
#[tauri::command]
pub fn ai_cancel_run(
    runs: tauri::State<'_, ai::run::RunManager>,
    run_id: String,
) -> Result<bool, String> {
    Ok(runs.cancel(&run_id))
}

/// DEV-0077.3 §五十六（Run Snapshot）：read-only——前端 Watchdog /
/// Reconcile 的 DB Truth 通道。只返回 run 状态摘要，不返回大段消息
///（Messages 仍走 listAiMessages）。
#[tauri::command]
pub fn ai_get_run_snapshot(
    state: tauri::State<'_, db::DbState>,
    run_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, profile_id, conversation_id, status,
                COALESCE(workflow_state, '') AS workflow_state,
                COALESCE(updated_at, '') AS updated_at,
                (SELECT EXISTS(SELECT 1 FROM ai_messages m WHERE m.run_id = ai_runs.id AND m.role='assistant')) AS has_assistant_message
         FROM ai_runs
         WHERE id = ?1",
        rusqlite::params![run_id],
        |row| {
            let status: String = row.get(3)?;
            let wf: String = row.get(4)?;
            let updated: String = row.get(5)?;
            let has_msg: i64 = row.get(6)?;
            Ok(serde_json::json!({
                "run_id": row.get::<_, String>(0)?,
                "profile_id": row.get::<_, i64>(1)?,
                "conversation_id": row.get::<_, i64>(2)?,
                // DB 用 waiting_user；事件语义统一 needs_user_input（§三十四）
                "status": if status == "waiting_user" { "needs_user_input".to_string() } else { status },
                "workflow_state": wf,
                "updated_at": updated,
                "has_assistant_message": has_msg == 1,
            }))
        },
    )
    .map_err(|e| format!("run_not_found: {e}"))
}

// =============== /data 聚合 + Reliability（DEV-0055/0057；Section 6 increment 14） ===============
// =============== DEV-0055 · /data 聚合（PART 26-33，Backend aggregate §163） ===============

#[derive(Debug, serde::Serialize)]
pub struct LearningTotals {
    /// §105 有 ended Session 的学习日 distinct 数
    learning_days: i64,
    /// §106 累计秒
    total_seconds: i64,
    /// §107 日均分钟（累计/学习天数）
    daily_avg_minutes: i64,
    /// 今天学习秒（含进行中 elapsed？§74 已结束统计 → 只算 ended）
    today_seconds: i64,
    today_tasks_total: i64,
    today_tasks_completed: i64,
    /// DEV-0057 §102：待确认时长条数（默认统计排除 needs_review；UI 提示"有 N 条待确认"）
    needs_review_count: i64,
}

/// §104-109：累计三数 + 今日两数（单条聚合 SQL；RAM-light）。
/// DEV-0057 §101：可信统计排除 needs_review（confirmed/corrected 计入）。
#[tauri::command]
pub fn get_learning_totals(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<LearningTotals, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (days, total): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(DISTINCT date(started_at,'+8 hours')), COALESCE(SUM(duration_seconds),0)
             FROM study_sessions
             WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_seconds > 0
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let today = chrono_today();
    let today_secs: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions
             WHERE profile_id=?1 AND date(started_at,'+8 hours')=?2 AND ended_at IS NOT NULL
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id, today],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (tt, tc): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),0)
             FROM tasks WHERE profile_id=?1 AND planned_date=?2 AND archived_at IS NULL",
            rusqlite::params![profile_id, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let nrc: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM study_sessions
             WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_review_state='needs_review'",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(LearningTotals {
        learning_days: days,
        total_seconds: total,
        daily_avg_minutes: if days > 0 { total / days / 60 } else { 0 },
        today_seconds: today_secs,
        today_tasks_total: tt,
        today_tasks_completed: tc,
        needs_review_count: nrc,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct KnowledgeTimeSlice {
    name: String,
    seconds: i64,
    item_id: i64,
    child_count: i64,
}

/// §112-117：Knowledge 时间分布（Backend 递归归并到指定层；默认 root children；
/// parent_item_id=Some → 该节点的 children 分布）。未归单独"未归类学习"。
/// DEV-0057 §160：N+1 消除——child×(递归CTE+COUNT) 改为**一条**递归 CTE grouped 归并 +
/// 一条 children COUNT grouped；排除 needs_review（§101）。
#[tauri::command]
pub fn get_knowledge_time_distribution(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    parent_item_id: Option<i64>,
) -> Result<(Vec<KnowledgeTimeSlice>, i64), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 目标层 children（parent=None → root children）
    let children: Vec<(i64, String)> = {
        let sql = match parent_item_id {
            None => "SELECT id, name FROM learning_items WHERE profile_id=?1 AND parent_id IS NULL ORDER BY sort_order, id",
            Some(_) => "SELECT id, name FROM learning_items WHERE profile_id=?1 AND parent_id=?2 ORDER BY sort_order, id",
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<(i64, String)> {
            Ok((r.get(0)?, r.get(1)?))
        };
        let rows = if let Some(p) = parent_item_id {
            stmt.query_map(rusqlite::params![profile_id, p], map)
                .map_err(|e| e.to_string())?
        } else {
            stmt.query_map(rusqlite::params![profile_id], map)
                .map_err(|e| e.to_string())?
        };
        rows.filter_map(|x| x.ok()).collect()
    };
    let want_parent: Option<i64> = parent_item_id;
    // 一条递归 CTE：每个 item 的 (id, 顶层祖先 in 目标层, 直接父) → 按目标层 children 分组 SUM
    let secs_map: std::collections::HashMap<i64, i64> = {
        let sql = "
            WITH RECURSIVE tree(id, root) AS (
                SELECT id, id FROM learning_items
                 WHERE profile_id=?1 AND parent_id IS ?2
                UNION ALL
                SELECT li.id, tree.root FROM learning_items li JOIN tree ON li.parent_id = tree.id
            )
            SELECT tree.root, COALESCE(SUM(ss.duration_seconds),0)
            FROM tree
            JOIN study_sessions ss ON ss.learning_item_id = tree.id
              AND ss.profile_id=?1 AND ss.ended_at IS NOT NULL
              AND ss.duration_review_state != 'needs_review'
            GROUP BY tree.root";
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, want_parent], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let cc_map: std::collections::HashMap<i64, i64> = {
        let mut stmt = conn
            .prepare("SELECT parent_id, COUNT(*) FROM learning_items WHERE profile_id=?1 AND parent_id IS NOT NULL GROUP BY parent_id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let mut out = Vec::new();
    for (id, name) in children {
        let secs = secs_map.get(&id).copied().unwrap_or(0);
        let cc = cc_map.get(&id).copied().unwrap_or(0);
        out.push(KnowledgeTimeSlice {
            name,
            seconds: secs,
            item_id: id,
            child_count: cc,
        });
    }
    // 未归类（同样排除 needs_review）
    let unassigned: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions
             WHERE profile_id=?1 AND learning_item_id IS NULL AND ended_at IS NOT NULL
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok((out, unassigned))
}

/// §118-119：Time-of-Day 分布（核心逻辑在 ai::planner，测试复用）。
#[tauri::command]
pub fn get_time_of_day_distribution(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<(String, i64)>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(ai::planner::time_of_day_distribution(&conn, profile_id))
}

/// §121：计划 vs 实际汇总（range 内每天 planned/actual/completed；不含综合效率 §122）。
#[tauri::command]
pub fn get_plan_vs_actual(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<(String, i64, i64, i64, i64)>, String> {
    // DEV-0057 §160-161：N+1 消除——day×query 改两条 grouped SQL + 内存合并；
    // 同时排除 needs_review（§101 可信统计排除待确认时长）。
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut planned_map: std::collections::HashMap<String, (i64, i64, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT planned_date,
                        COALESCE(SUM(estimated_minutes),0),
                        COUNT(*),
                        COALESCE(SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),0)
                 FROM tasks
                 WHERE profile_id=?1 AND planned_date BETWEEN ?2 AND ?3 AND archived_at IS NULL
                 GROUP BY planned_date",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, start, end], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok())
            .map(|(d, p, t, c)| (d, (p, t, c)))
            .collect()
    };
    let mut actual_map: std::collections::HashMap<String, i64> = {
        let mut stmt = conn
            .prepare(
                "SELECT date(started_at,'+8 hours'), COALESCE(SUM(duration_seconds),0)/60
                 FROM study_sessions
                 WHERE profile_id=?1 AND date(started_at,'+8 hours') BETWEEN ?2 AND ?3
                   AND ended_at IS NOT NULL AND duration_review_state != 'needs_review'
                 GROUP BY date(started_at,'+8 hours')",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, start, end], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let mut out = Vec::new();
    let mut d = start.clone();
    while d <= end {
        let (planned, tt, tc) = planned_map.remove(&d).unwrap_or((0, 0, 0));
        let actual = actual_map.remove(&d).unwrap_or(0);
        out.push((d.clone(), planned, actual, tt, tc));
        d = next_date(&d);
    }
    Ok(out)
}

pub fn next_date(d: &str) -> String {
    let p: Vec<i64> = d.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return d.to_string();
    }
    let epoch =
        ai::planner::sqlite_dt_to_epoch(&format!("{:04}-{:02}-{:02} 00:00:00", p[0], p[1], p[2]))
            .unwrap_or(0);
    days_to_iso(epoch / 86400 + 1)
}

// =============== DEV-0057 · Reliability / Data Trust / Performance ===============

/// §68 手动重建搜索索引（从 Canonical tables 完整重建当前 profile）。
#[tauri::command]
pub fn rebuild_search_index(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<usize, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::search::rebuild_profile(&mut conn, profile_id)
}

/// §153-155 Knowledge 轻量列表（树/导航用；不含 content 正文——正文按需加载）。
#[tauri::command]
pub fn list_learning_items_light(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<crate::ipc::dto::LearningItemLight>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, goal_id, parent_id, name, mastery_status, sort_order, created_at, updated_at
             FROM learning_items WHERE profile_id = ?1 ORDER BY sort_order, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![profile_id], |r| {
            Ok(crate::ipc::dto::LearningItemLight {
                id: r.get::<_, i64>(0)?,
                goal_id: r.get::<_, Option<i64>>(1)?,
                parent_id: r.get::<_, Option<i64>>(2)?,
                name: r.get::<_, String>(3)?,
                mastery_status: r.get::<_, String>(4)?,
                sort_order: r.get::<_, i64>(5)?,
                created_at: r.get::<_, String>(6)?,
                updated_at: r.get::<_, String>(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// §133-136 媒体安全 URL：返回沙箱内附件的绝对路径（前端 convertFileSrc → 按需加载，
/// 主路径不再整文件 base64）。Backend 仍验证附件归属当前 App Attachment Sandbox。
#[tauri::command]
pub fn get_attachment_asset_path(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    attachment_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let rel: String = conn
        .query_row(
            "SELECT relative_path FROM learning_attachments WHERE id=?1 AND profile_id=?2",
            rusqlite::params![attachment_id, profile_id],
            |r| r.get(0),
        )
        .map_err(|_| "附件不存在或不属于当前档案".to_string())?;
    let full = sandbox::resolve_in_sandbox(&adir.0, &rel)?;
    full.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "附件路径非法".to_string())
}

#[tauri::command]
pub fn ai_active_run_count(runs: tauri::State<'_, ai::run::RunManager>) -> Result<usize, String> {
    Ok(runs.active_count())
}

/// §112：来源 URL 用系统浏览器打开（只 http/https；SSRF 校验 + Source Registry 解析）。
#[tauri::command]
pub async fn open_external_url(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    run_id: Option<String>,
    sid_or_url: String,
) -> Result<(), String> {
    // 优先从 Source Registry 按 sid 解析（§109：不信模型自写 URL；用户点击的来自真实列表）
    let url = if sid_or_url.starts_with("S") && !sid_or_url.contains('/') {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let u: Option<String> = conn
            .query_row(
                "SELECT url FROM ai_sources WHERE profile_id=?1 AND run_id=?2 AND url != '' ORDER BY id DESC LIMIT 1",
                rusqlite::params![profile_id, run_id.clone().unwrap_or_default()],
                |r| r.get(0),
            )
            .ok();
        u.ok_or("来源不存在")?
    } else {
        sid_or_url
    };
    ai::web::ssrf_check(&url)?;
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开网页失败：{e}"))
}
