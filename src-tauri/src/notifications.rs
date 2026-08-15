//! 学习提醒同步器（DEV-0042，纯 Rust 无 UI）。
//!
//! 职责：把未来 30 天内带 planned_time 的未完成任务注册为"到点提醒"，
//! 并在任务/规则变化后重新对齐（先清理旧项，再按最新数据注册）。
//!
//! 实现说明（desktop 平台约束）：
//! `tauri-plugin-notification` 的 OS 级 schedule/cancel 仅在 mobile 端生效，
//! desktop 端 `builder().show()` 只支持立即弹出（id/schedule 字段被忽略，
//! 参见插件 desktop.rs）。因此本模块用进程内定时器实现"带时间调度"：
//! - 调度项存活于本进程内存（应用启动时由 DB 重建，天然幂等）；
//! - settings KV `notifications.v1` 记录 Higher 自己注册过的 `{key: id}`
//!   （cancel 只针对这些记录，严禁 cancelAll）；
//! - 稳定 ID：`nid = task_id * 100000 + (YYYYMMDD % 100000)`，
//!   同一 task+date 恒同 ID；改期后 key/ID 变化 → 旧项不再出现在期望集合，
//!   同步时被清理（等效 cancel）。
//!
//! 时间约定：planned_date/planned_time 为本地（Asia/Shanghai）墙上时间；
//! 转 epoch 时按 UTC+8 处理，与 lib.rs backup_database 的既有约定一致。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::repository::setting::SettingRepository;
use crate::repository::study_profile::StudyProfileRepository;

/// settings KV：学习提醒总开关（"1"/"0"，默认 "1"）。
const ENABLED_KEY: &str = "notifications.enabled";
/// settings KV：已注册通知 ID 记录 `{"ids":{"<task_id>_<date>": <id>}}`。
const STORE_KEY: &str = "notifications.v1";
/// 项目固定时区偏移（Asia/Shanghai）。
const TZ_OFFSET_SECS: i64 = 8 * 3600;

#[derive(Debug, Serialize, Deserialize, Default)]
struct NotificationStore {
    #[serde(default)]
    ids: HashMap<String, u32>,
}

/// 进程内待触发项（desktop 无 OS 级调度，见模块注释）。
#[derive(Debug)]
struct ScheduledItem {
    /// 稳定通知 ID（与 settings KV 记录一致；调试/对账用）
    #[allow(dead_code)]
    id: u32,
    profile_id: i64,
    title: String,
    body: String,
    fire_at: SystemTime,
}

static SCHEDULED: OnceLock<Mutex<Vec<ScheduledItem>>> = OnceLock::new();

fn scheduled_items() -> &'static Mutex<Vec<ScheduledItem>> {
    SCHEDULED.get_or_init(|| Mutex::new(Vec::new()))
}

/// 稳定通知 ID：task_id * 100000 + planned_date 的 YYYYMMDD 后 5 位。
/// 同一 task+date 恒同 ID（确定性）；改期后自然得到新 ID。
fn stable_notification_id(task_id: i64, planned_date: &str) -> u32 {
    let digits: String = planned_date
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    let yyyymmdd: u64 = if digits.len() >= 8 {
        digits[..8].parse().unwrap_or(0)
    } else {
        digits.parse().unwrap_or(0)
    };
    (task_id as u32)
        .wrapping_mul(100_000)
        .wrapping_add((yyyymmdd % 100_000) as u32)
}

/// 本地墙上时间 "YYYY-MM-DD" + "HH:MM(:SS)" → SystemTime（按 UTC+8 解释）。
fn local_date_time_to_epoch(date: &str, time: &str) -> Option<SystemTime> {
    let d: Vec<i64> = date.split('-').filter_map(|s| s.parse().ok()).collect();
    if d.len() != 3 {
        return None;
    }
    let mut t: Vec<i64> = time
        .split(':')
        .filter_map(|s| s.parse().ok())
        .collect();
    while t.len() < 3 {
        t.push(0);
    }
    let (y, m, day) = (d[0], d[1], d[2]);
    // civil days → epoch days（Howard Hinnant 算法，与 lib.rs 备份命名一致）
    let z = days_from_civil(y, m, day);
    let secs = z * 86400 + t[0] * 3600 + t[1] * 60 + t[2] - TZ_OFFSET_SECS;
    if secs < 0 {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_enabled(conn: &Connection) -> bool {
    SettingRepository::new(conn)
        .get(ENABLED_KEY)
        .ok()
        .flatten()
        .map(|v| v != "0")
        .unwrap_or(true)
}

/// 同步某 profile 的学习提醒：
/// 期望集合（未来 30 天、有 planned_time、未归档、未完成的任务）与
/// 当前已注册项对齐；enabled=0 时只清理（期望集合为空）。
pub fn sync_notifications(conn: &Connection, app: &AppHandle, profile_id: i64) -> Result<(), String> {
    // 1) 期望集合（enabled=0 → 空 = 只清理）
    let mut desired: HashMap<String, (u32, SystemTime, String)> = HashMap::new();
    if is_enabled(conn) {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, planned_date, planned_time FROM tasks
                 WHERE profile_id = ?1
                   AND planned_date BETWEEN date('now') AND date('now', '+30 day')
                   AND planned_time IS NOT NULL
                   AND archived_at IS NULL
                   AND status != 'completed'",
            )
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        for (task_id, title, planned_date, planned_time) in rows {
            // 无 planned_time 的任务不通知；过去的时间点不注册
            let Some(fire_at) = local_date_time_to_epoch(&planned_date, &planned_time) else {
                continue;
            };
            if fire_at <= SystemTime::now() {
                continue;
            }
            let key = format!("{}_{}", task_id, planned_date);
            let nid = stable_notification_id(task_id, &planned_date);
            desired.insert(key, (nid, fire_at, title));
        }
    }

    // 2) 对齐进程内调度列表：清掉本 profile 全部旧项，再写入期望集合。
    //    记录里存在、但不在期望集合中的 ID 由此被取消（等效 cancel；不触碰他人通知）。
    {
        let mut items = scheduled_items()
            .lock()
            .map_err(|_| "通知调度列表已损坏".to_string())?;
        items.retain(|i| i.profile_id != profile_id);
        for (nid, fire_at, title) in desired.values() {
            items.push(ScheduledItem {
                id: *nid,
                profile_id,
                title: "Higher · 到学习时间了".to_string(),
                body: title.clone(),
                fire_at: *fire_at,
            });
        }
    }

    // 3) 持久化注册记录（settings KV；`_ = app` 保留签名与插件注册语义一致）
    let _ = app;
    let store = NotificationStore {
        ids: desired.iter().map(|(k, (nid, _, _))| (k.clone(), *nid)).collect(),
    };
    let json = serde_json::to_string(&store).map_err(|e| e.to_string())?;
    SettingRepository::new(conn)
        .set(STORE_KEY, &json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 变更后重同步（后台线程执行，避免与调用方持有的 DB 锁竞争）：
/// 对全部 profile 执行 sync。
pub fn resync(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<crate::db::DbState>();
        let Ok(conn) = state.0.lock() else { return };
        let profiles = StudyProfileRepository::new(&conn).list().unwrap_or_default();
        for p in profiles {
            if let Err(e) = sync_notifications(&conn, &app, p.id) {
                log::warn!("学习提醒同步失败（profile {}）：{}", p.id, e);
            }
        }
    });
}

/// 启动调度线程（setup 中调用一次；每 20s 检查到点项并弹出系统通知）。
pub fn start_scheduler(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(20));
        let now = SystemTime::now();
        let due: Vec<ScheduledItem> = {
            let Ok(mut items) = scheduled_items().lock() else { return };
            let mut i = 0;
            let mut due = Vec::new();
            while i < items.len() {
                if items[i].fire_at <= now {
                    due.push(items.remove(i));
                } else {
                    i += 1;
                }
            }
            due
        };
        for item in due {
            let _ = app
                .notification()
                .builder()
                .title(item.title)
                .body(item.body)
                .show();
        }
    });
}
