//! PlanningBlueprint / Phase / Milestone / PlanningReview + Safe Rolling Horizon Projector
//! （DEV-0059 §14-22）。
//!
//! Blueprint = 应该怎样；Task = 近期准备做什么；Session = 实际做了什么。三者禁止混淆（§14.1）。
//! 激活事务（§25.1）：old active → superseded；new → active；phases/milestones 已随 draft 写入；
//! safe rolling projection（§22）归档可替换旧投影任务 + 生成新 14 天任务；任一失败全回滚。

use rusqlite::{params, Connection};

// =============== Blueprint ===============

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningBlueprint {
    pub id: i64,
    pub profile_id: i64,
    pub scenario_type: String,
    pub version: i64,
    pub status: String,
    pub title: String,
    pub content_md: String,
    pub structured_json: Option<String>,
    pub source_snapshot_json: String,
    pub provenance_json: String,
    pub review_enabled: bool,
    pub review_interval_days: i64,
    pub last_review_at: Option<String>,
    pub next_review_at: Option<String>,
    pub supersedes_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub activated_at: Option<String>,
}

// =============== Phase / Milestone ===============

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningPhase {
    pub id: i64,
    pub blueprint_id: i64,
    pub phase_key: String,
    pub title: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub objective_md: String,
    pub sort_order: i64,
    pub status: String,
    pub data_json: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningMilestone {
    pub id: i64,
    pub blueprint_id: i64,
    pub phase_id: Option<i64>,
    pub milestone_key: String,
    pub title: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub date_precision: String,
    pub date_status: String,
    pub status: String,
    pub provenance_json: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct PlanningRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PlanningRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ---- Blueprint ----

    pub fn create_blueprint(
        &self,
        profile_id: i64,
        scenario_type: &str,
        title: &str,
        content_md: &str,
        structured_json: Option<&str>,
        source_snapshot_json: &str,
        provenance_json: &str,
        review_interval_days: i64,
    ) -> Result<PlanningBlueprint, String> {
        if review_interval_days < 1 {
            return Err("review_interval_days 必须 >= 1".to_string());
        }
        self.conn
            .execute(
                "INSERT INTO planning_blueprints
                 (profile_id, scenario_type, version, status, title, content_md, structured_json,
                  source_snapshot_json, provenance_json, review_enabled, review_interval_days, next_review_at)
                 SELECT ?1, ?2, COALESCE(MAX(version),0)+1, 'draft', ?3, ?4, ?5, ?6, ?7, 1, ?8,
                        datetime('now', printf('+%d days', ?8))
                 FROM planning_blueprints WHERE profile_id=?1",
                params![profile_id, scenario_type, title, content_md, structured_json,
                    source_snapshot_json, provenance_json, review_interval_days],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.get_blueprint(id, profile_id)?.ok_or("创建失败".to_string())
    }

    pub fn get_blueprint(&self, id: i64, profile_id: i64) -> Result<Option<PlanningBlueprint>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, scenario_type, version, status, title, content_md, structured_json,
                             source_snapshot_json, provenance_json, review_enabled, review_interval_days,
                             last_review_at, next_review_at, supersedes_id, created_at, updated_at, activated_at
                      FROM planning_blueprints WHERE id=?1 AND profile_id=?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse_bp).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn list_by_profile(&self, profile_id: i64) -> Result<Vec<PlanningBlueprint>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, scenario_type, version, status, title, content_md, structured_json,
                             source_snapshot_json, provenance_json, review_enabled, review_interval_days,
                             last_review_at, next_review_at, supersedes_id, created_at, updated_at, activated_at
                      FROM planning_blueprints WHERE profile_id=?1 ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![profile_id], parse_bp).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 当前 active Blueprint（每 profile 最多 1，v021 partial unique 兜底）。
    pub fn get_active(&self, profile_id: i64) -> Result<Option<PlanningBlueprint>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, scenario_type, version, status, title, content_md, structured_json,
                             source_snapshot_json, provenance_json, review_enabled, review_interval_days,
                             last_review_at, next_review_at, supersedes_id, created_at, updated_at, activated_at
                      FROM planning_blueprints WHERE profile_id=?1 AND status='active' ORDER BY version DESC LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![profile_id], parse_bp).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// §25.1：激活（一个 transaction，任一失败全回滚）：
    /// old active → superseded → new draft → active + activated_at →
    /// 写 phases/milestones（已随 draft 存在）→ safe rolling projection → 不可出现"active 了 Task 只写一半"。
    pub fn activate(
        &self,
        profile_id: i64,
        id: i64,
        today: &str,
        horizon_days: i64,
    ) -> Result<PlanningBlueprint, String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let draft: Option<(i64, String, String, String)> = tx
            .query_row(
                "SELECT id, structured_json, title, scenario_type FROM planning_blueprints
                 WHERE id=?1 AND profile_id=?2 AND status='draft'",
                params![id, profile_id],
                |r| Ok((r.get(0)?, r.get(1).unwrap_or_default(), r.get(2)?, r.get(3)?)),
            )
            .ok();
        let Some((bid, structured, _title, scenario)) = draft else {
            return Err("蓝图不存在、不属于当前档案或不是 draft 状态".to_string());
        };
        tx.execute(
            "UPDATE planning_blueprints SET status='superseded', updated_at=datetime('now')
             WHERE profile_id=?1 AND status='active'",
            params![profile_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE planning_blueprints SET status='active', activated_at=datetime('now'),
                 next_review_at=datetime('now', printf('+%d days',
                   (SELECT review_interval_days FROM planning_blueprints WHERE id=?1))),
                 updated_at=datetime('now')
             WHERE id=?1",
            params![bid],
        )
        .map_err(|e| e.to_string())?;
        // §22 Safe Rolling Projection（只投影 horizon 内安全任务；不可替换受保护任务）
        let projected = project_tasks_in_tx(
            &tx, profile_id, bid, &scenario, structured.as_str(),
            today, horizon_days,
        )?;
        // 供日志使用
        let _ = projected;
        tx.commit().map_err(|e| e.to_string())?;
        self.get_blueprint(bid, profile_id)?.ok_or("激活失败".to_string())
    }

    // ---- Phase ----

    pub fn add_phase(
        &self,
        blueprint_id: i64,
        phase_key: &str,
        title: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        objective_md: &str,
        sort_order: i64,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO planning_phases (blueprint_id, phase_key, title, start_date, end_date, objective_md, sort_order)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![blueprint_id, phase_key, title, start_date, end_date, objective_md, sort_order],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_phases(&self, blueprint_id: i64) -> Result<Vec<PlanningPhase>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, blueprint_id, phase_key, title, start_date, end_date, objective_md, sort_order, status, data_json
                      FROM planning_phases WHERE blueprint_id=?1 ORDER BY sort_order, id")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![blueprint_id], parse_phase).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // ---- Milestone ----

    pub fn add_milestone(
        &self,
        blueprint_id: i64,
        phase_id: Option<i64>,
        milestone_key: &str,
        title: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        date_precision: &str,
        date_status: &str,
        provenance_json: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO planning_milestones (blueprint_id, phase_id, milestone_key, title, start_date, end_date, date_precision, date_status, provenance_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![blueprint_id, phase_id, milestone_key, title, start_date, end_date,
                    date_precision, date_status, provenance_json],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_milestones(&self, blueprint_id: i64) -> Result<Vec<PlanningMilestone>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, blueprint_id, phase_id, milestone_key, title, start_date, end_date, date_precision, date_status, status, provenance_json, created_at, updated_at
                      FROM planning_milestones WHERE blueprint_id=?1 ORDER BY COALESCE(start_date,'9999'), id")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![blueprint_id], parse_ms).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // =============== DEV-0059.1 §10/§11：Manual Planning + Review Cadence ===============

    /// §10：手工编辑 Blueprint 基础信息（title + content_md；版本/status 不动）。
    pub fn update_blueprint_meta(
        &self,
        profile_id: i64,
        id: i64,
        title: &str,
        content_md: &str,
    ) -> Result<PlanningBlueprint, String> {
        if title.trim().is_empty() {
            return Err("蓝图标题不能为空".to_string());
        }
        let n = self
            .conn
            .execute(
                "UPDATE planning_blueprints SET title=?1, content_md=?2, updated_at=datetime('now')
                 WHERE id=?3 AND profile_id=?4",
                params![title.trim(), content_md, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("蓝图不存在或不属于当前档案".to_string());
        }
        self.get_blueprint(id, profile_id)?.ok_or("蓝图不存在".to_string())
    }

    /// §11：Review Cadence——只改 review_enabled / review_interval_days / next_review_at。
    /// 关闭时 next_review_at=NULL；开启时按新间隔重设 next_review_at（不自动调 AI）。
    pub fn update_review_cadence(
        &self,
        profile_id: i64,
        id: i64,
        review_enabled: bool,
        review_interval_days: Option<i64>,
    ) -> Result<PlanningBlueprint, String> {
        if let Some(days) = review_interval_days {
            if days < 1 {
                return Err("复盘间隔必须 ≥1 天".to_string());
            }
        }
        let interval = match review_interval_days {
            Some(d) => d,
            None => self
                .conn
                .query_row(
                    "SELECT review_interval_days FROM planning_blueprints WHERE id=?1 AND profile_id=?2",
                    params![id, profile_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?,
        };
        let n = self
            .conn
            .execute(
                "UPDATE planning_blueprints SET review_enabled=?1, review_interval_days=?2,
                   next_review_at=CASE WHEN ?1=0 THEN NULL
                                       ELSE datetime('now', printf('+%d days', ?2)) END,
                   updated_at=datetime('now')
                 WHERE id=?3 AND profile_id=?4",
                params![if review_enabled { 1 } else { 0 }, interval, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("蓝图不存在或不属于当前档案".to_string());
        }
        self.get_blueprint(id, profile_id)?.ok_or("蓝图不存在".to_string())
    }

    /// §10：Phase 更新（仅手工维护用；不校验任务投影）。
    pub fn update_phase(
        &self,
        blueprint_id: i64,
        phase_id: i64,
        title: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        objective_md: &str,
        sort_order: i64,
    ) -> Result<(), String> {
        if title.trim().is_empty() {
            return Err("阶段标题不能为空".to_string());
        }
        let n = self
            .conn
            .execute(
                "UPDATE planning_phases SET title=?1, start_date=?2, end_date=?3, objective_md=?4, sort_order=?5
                 WHERE id=?6 AND blueprint_id=?7",
                params![title.trim(), start_date, end_date, objective_md, sort_order, phase_id, blueprint_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("阶段不存在或不属于该蓝图".to_string());
        }
        Ok(())
    }

    pub fn delete_phase(&self, blueprint_id: i64, phase_id: i64) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "DELETE FROM planning_phases WHERE id=?1 AND blueprint_id=?2",
                params![phase_id, blueprint_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("阶段不存在或不属于该蓝图".to_string());
        }
        Ok(())
    }

    /// §10：Milestone 更新。
    pub fn update_milestone(
        &self,
        blueprint_id: i64,
        milestone_id: i64,
        title: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        date_precision: &str,
        date_status: &str,
    ) -> Result<(), String> {
        if title.trim().is_empty() {
            return Err("里程碑标题不能为空".to_string());
        }
        let n = self
            .conn
            .execute(
                "UPDATE planning_milestones SET title=?1, start_date=?2, end_date=?3, date_precision=?4, date_status=?5
                 WHERE id=?6 AND blueprint_id=?7",
                params![title.trim(), start_date, end_date, date_precision, date_status, milestone_id, blueprint_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("里程碑不存在或不属于该蓝图".to_string());
        }
        Ok(())
    }

    pub fn delete_milestone(&self, blueprint_id: i64, milestone_id: i64) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "DELETE FROM planning_milestones WHERE id=?1 AND blueprint_id=?2",
                params![milestone_id, blueprint_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("里程碑不存在或不属于该蓝图".to_string());
        }
        Ok(())
    }
}

// =============== Safe Rolling Horizon Projector（§22） ===============

/// 今天（UTC+8 学习日 YYYY-MM-DD；与后端 chrono_today 同语义，供 ChangeSet 层复用）。
pub fn today_utc8() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = (now + 8 * 3600).div_euclid(86_400); // UTC → UTC+8
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// §22：安全重投影。可替换旧 Task 必须同时满足：
/// origin=blueprint AND status=pending AND planned_date>today AND user_modified_at IS NULL
/// AND 没有 StudySession 关联 → archive（不是 DELETE）。
/// 再按新 Blueprint structured_json.future_tasks 生成 horizon 内任务（projection_key 幂等）。
pub fn project_tasks_in_tx(
    tx: &rusqlite::Transaction<'_>,
    profile_id: i64,
    blueprint_id: i64,
    scenario: &str,
    structured_json: &str,
    today: &str,
    horizon_days: i64,
) -> Result<usize, String> {
    // 1) 归档可替换旧投影任务（含被 supersede 的 / 当前相关 blueprint projection）
    tx.execute(
        "UPDATE tasks SET archived_at=datetime('now'), updated_at=datetime('now')
         WHERE profile_id=?1 AND origin='blueprint' AND status='pending'
           AND planned_date > ?2 AND user_modified_at IS NULL AND archived_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM study_sessions ss WHERE ss.task_id = tasks.id)",
        params![profile_id, today],
    )
    .map_err(|e| e.to_string())?;

    // 2) 从 structured_json 读取 future_tasks
    let mut projected = 0usize;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(structured_json) {
        let tasks = v.get("future_tasks").and_then(|t| t.as_array()).cloned().unwrap_or_default();
        // 只投影 horizon 窗口内（today .. today+horizon_days-1）
        for (i, item) in tasks.iter().enumerate() {
            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
            let date = item.get("planned_date").and_then(|d| d.as_str()).unwrap_or("").trim().to_string();
            if title.is_empty() || date.is_empty() || date.as_str() < today {
                continue;
            }
            if date > horizon_end(today, horizon_days) {
                continue;
            }
            let estimated = item.get("estimated_minutes").and_then(|m| m.as_i64()).unwrap_or(30);
            let learning_item_id: Option<i64> = item.get("learning_item_id").and_then(|v| v.as_i64());
            let phase_id: Option<i64> = item.get("planning_phase_id").and_then(|v| v.as_i64());
            // 幂等：projection_key = "{bp_id}:{idx}"；同蓝图同 key 重复投影不产生第二条
            let projection_key = format!("{blueprint_id}:{i}");
            let exists: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE planning_blueprint_id=?1 AND projection_key=?2",
                    params![blueprint_id, projection_key],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if exists > 0 {
                continue;
            }
            tx.execute(
                "INSERT INTO tasks (profile_id, title, planned_date, estimated_minutes, status, task_kind, priority, origin, planning_blueprint_id, planning_phase_id, projection_key)
                 VALUES (?1,?2,?3,?4,'pending','structured','normal','blueprint',?5,?6,?7)",
                params![profile_id, title, date, estimated, blueprint_id, phase_id, projection_key],
            )
            .map_err(|e| e.to_string())?;
            if let Some(item_id) = learning_item_id {
                let task_id = tx.last_insert_rowid();
                let _ = tx.execute(
                    "UPDATE tasks SET learning_item_id=?1 WHERE id=?2",
                    params![item_id, task_id],
                );
            }
            projected += 1;
        }
    }
    let _ = scenario;
    Ok(projected)
}

/// today + horizon_days-1（YYYY-MM-DD，简单日期加法，不含时区）。
fn horizon_end(today: &str, horizon_days: i64) -> String {
    let parts: Vec<i64> = today
        .split('-')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect();
    if parts.len() < 3 {
        return today.to_string();
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    // 使用儒略日天数计算避免时区
    let days = civil_days(y, m, d) + horizon_days - 1;
    let (yy, mm, dd) = civil_from_days(days);
    format!("{yy:04}-{mm:02}-{dd:02}")
}

/// 公历 → 天数（Howard Hinnant civil_from_days 逆运算）。
fn civil_days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 天数 → 公历。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// =============== 解析 ===============

fn parse_bp(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanningBlueprint> {
    Ok(PlanningBlueprint {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        scenario_type: r.get(2)?,
        version: r.get(3)?,
        status: r.get(4)?,
        title: r.get(5)?,
        content_md: r.get(6)?,
        structured_json: r.get(7)?,
        source_snapshot_json: r.get(8)?,
        provenance_json: r.get(9)?,
        review_enabled: r.get::<_, i64>(10)? == 1,
        review_interval_days: r.get(11)?,
        last_review_at: r.get(12)?,
        next_review_at: r.get(13)?,
        supersedes_id: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
        activated_at: r.get(17)?,
    })
}

fn parse_phase(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanningPhase> {
    Ok(PlanningPhase {
        id: r.get(0)?,
        blueprint_id: r.get(1)?,
        phase_key: r.get(2)?,
        title: r.get(3)?,
        start_date: r.get(4)?,
        end_date: r.get(5)?,
        objective_md: r.get(6)?,
        sort_order: r.get(7)?,
        status: r.get(8)?,
        data_json: r.get(9)?,
    })
}

fn parse_ms(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanningMilestone> {
    Ok(PlanningMilestone {
        id: r.get(0)?,
        blueprint_id: r.get(1)?,
        phase_id: r.get(2)?,
        milestone_key: r.get(3)?,
        title: r.get(4)?,
        start_date: r.get(5)?,
        end_date: r.get(6)?,
        date_precision: r.get(7)?,
        date_status: r.get(8)?,
        status: r.get(9)?,
        provenance_json: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}
