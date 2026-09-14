// Foundation 2.0 §6: data-domain commands (cleanup / export / distributions).
use crate::ai;
use crate::db;
use crate::platform;
use crate::repository;
use crate::repository::cleanup::CleanupRepository;
use crate::repository::learning_data::LearningDataRepository;
use crate::repository::mastery::MasteryRepository;
use crate::commands::agent::primary_client;

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
pub fn backup_database(app: &tauri::AppHandle, db_path: &std::path::Path) -> Result<std::path::PathBuf, String> {
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
    let scope = repository::cleanup::CleanupScope::from_str(&scope)
        .ok_or("未知的清理范围")?;
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
    repository::learning_data::LearningDataRepository::new(&conn)
        .stats(profile_id, &period_start, &period_end)
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
                let label = format!("{}~{}", &mon[5..].replace('-', "/"), &sun[5..].replace('-', "/"));
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
    Ok(MasteryView { assessment: a, stale })
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
    let base = format!("{}\n\n{}", context, ai::prompts::user_instruction(ai::AiAction::MasteryAssessment));
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
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| format!("评估结果解析失败：{e}"))?;
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
        v.get(key).and_then(|d| d.get("score")).and_then(|s| s.as_i64())
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
        understanding_score: if status == "scored" { dim("understanding") } else { None },
        coverage_score: if status == "scored" { dim("coverage") } else { None },
        verification_score: if status == "scored" { dim("verification") } else { None },
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

