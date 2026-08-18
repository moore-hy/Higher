//! PlanningReview（DEV-0059 §17-18）。
//!
//! - trigger_type：scheduled / milestone / manual / reality_change / anomaly
//! - status：due / running / waiting_approval / completed / skipped / failed
//! - risk_state：unknown / normal / attention / off_reach / near_safety / below_safety
//! - cadence（§18）：review_enabled + review_interval_days（默认 14）；到期只提醒不自动调 AI
//! - §19.2：postgraduate REACH/SAFETY risk 只在 review 时基于 Evidence 判定；启动只读已存 risk

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningReview {
    pub id: i64,
    pub profile_id: i64,
    pub blueprint_id: Option<i64>,
    pub period_start: String,
    pub period_end: String,
    pub trigger_type: String,
    pub status: String,
    pub evidence_snapshot_json: String,
    pub assessment_md: String,
    pub recommendation_json: String,
    pub risk_state: String,
    pub change_set_id: Option<i64>,
    pub user_decision: String,
    pub resulting_blueprint_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub struct PlanningReviewRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PlanningReviewRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create_due(
        &self,
        profile_id: i64,
        blueprint_id: Option<i64>,
        period_start: &str,
        period_end: &str,
        trigger_type: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO planning_reviews (profile_id, blueprint_id, period_start, period_end, trigger_type, status)
                 VALUES (?1,?2,?3,?4,?5,'due')",
                params![profile_id, blueprint_id, period_start, period_end, trigger_type],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<PlanningReview>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, blueprint_id, period_start, period_end, trigger_type, status,
                             evidence_snapshot_json, assessment_md, recommendation_json, risk_state,
                             change_set_id, user_decision, resulting_blueprint_id, created_at, updated_at, completed_at
                      FROM planning_reviews WHERE id=?1 AND profile_id=?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse_rev).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn list_by_profile(&self, profile_id: i64) -> Result<Vec<PlanningReview>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, blueprint_id, period_start, period_end, trigger_type, status,
                             evidence_snapshot_json, assessment_md, recommendation_json, risk_state,
                             change_set_id, user_decision, resulting_blueprint_id, created_at, updated_at, completed_at
                      FROM planning_reviews WHERE profile_id=?1 ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![profile_id], parse_rev).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn set_status(&self, id: i64, profile_id: i64, status: &str) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE planning_reviews SET status=?1, updated_at=datetime('now'),
                   completed_at=CASE WHEN ?1='completed' THEN datetime('now') ELSE completed_at END
                 WHERE id=?2 AND profile_id=?3",
                params![status, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("复盘记录不存在或不属于当前档案".to_string());
        }
        Ok(())
    }

    /// §39：AI 评估写回（assessment + recommendation + risk；不自动改正式数据）。
    pub fn save_assessment(
        &self,
        id: i64,
        profile_id: i64,
        assessment_md: &str,
        recommendation_json: &str,
        risk_state: &str,
    ) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE planning_reviews SET assessment_md=?1, recommendation_json=?2, risk_state=?3,
                   status='waiting_approval', updated_at=datetime('now')
                 WHERE id=?4 AND profile_id=?5 AND status='running'",
                params![assessment_md, recommendation_json, risk_state, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("复盘记录不存在、不属于当前档案或不在 running 状态".to_string());
        }
        Ok(())
    }

    /// §18：该 profile 是否"该进行阶段复盘了"（只读已存 next_review_at；不调 AI）。
    pub fn is_review_due(&self, profile_id: i64, today: &str) -> Result<bool, String> {
        let due: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM planning_blueprints
                 WHERE profile_id=?1 AND status='active' AND review_enabled=1
                   AND (next_review_at IS NULL OR date(next_review_at) <= date(?2))",
                params![profile_id, today],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(due > 0)
    }

    // =============== DEV-0059.1 §3：Planning Review AI 全链 ===============

    /// DEV-0059.2 §11：PersonalProfile confirmed 版本变化 → 建议复盘（reality_change due）。
    /// 只创建/复用 due 记录；不调 AI；已有 due/running/waiting_approval 不重复创建；
    /// 无 active Blueprint 不提醒。Today/Planning 按现有 due UI 展示。
    pub fn ensure_reality_change_due(&self, profile_id: i64) -> Result<(), String> {
        let Some(bp) = crate::repository::planning::PlanningRepository::new(self.conn)
            .get_active(profile_id)
            .ok()
            .flatten()
        else {
            return Ok(());
        };
        let open: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM planning_reviews
                 WHERE profile_id=?1 AND blueprint_id=?2 AND status IN ('due','running','waiting_approval')",
                params![profile_id, bp.id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if open > 0 {
            return Ok(());
        }
        let today = crate::repository::planning::today_utc8();
        let days = (bp.review_interval_days).max(1);
        let period_start: String = self
            .conn
            .query_row(
                "SELECT date(?1, printf('%+d days', ?2))",
                params![today, -(days - 1)],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        self.create_due(profile_id, Some(bp.id), &period_start, &today, "reality_change")?;
        Ok(())
    }

    /// §3 step 2：置 running + 写入 evidence snapshot（不调 Provider）。
    /// 只允许 due / running / failed 进入 running（已完成/待审批的复盘不允许重跑）。
    pub fn prepare_running(&self, id: i64, profile_id: i64, snapshot_json: &str) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE planning_reviews SET status='running', evidence_snapshot_json=?1, updated_at=datetime('now')
                 WHERE id=?2 AND profile_id=?3 AND status IN ('due','running','failed')",
                params![snapshot_json, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("复盘记录不存在、不属于当前档案或已进入后续阶段".to_string());
        }
        Ok(())
    }

    /// DEV-0059.2 §2：当前周期复盘（cadence 周期 + open review dedupe）。
    ///
    /// - 读取 active Blueprint；`days = max(1, review_interval_days)`（review_enabled=false 手动
    ///   复盘仍可执行，周期用当前间隔；不自动 due）。
    /// - `period_end = today_utc8()`；`period_start = today - (days-1)`，严格覆盖 days 个日历日。
    /// - 同 profile/blueprint 已存在 due/running/waiting_approval → 复用该 open review，不重复创建；
    ///   waiting_approval 原样返回（UI 显示「审阅 AI 调整」，不得再次启动 AI）。
    /// - 返回 (review_id, status, change_set_id, snapshot_json)。
    pub fn prepare_current(
        &self,
        profile_id: i64,
        trigger_type: &str,
    ) -> Result<(i64, String, Option<i64>, String), String> {
        let bp = crate::repository::planning::PlanningRepository::new(self.conn)
            .get_active(profile_id)?
            .ok_or("还没有正式规划蓝图。请先生成或手工新建规划后再复盘。")?;
        let days = (bp.review_interval_days).max(1);
        let today = crate::repository::planning::today_utc8();
        let period_start: String = self
            .conn
            .query_row(
                "SELECT date(?1, printf('%+d days', ?2))",
                params![today, -(days - 1)],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let open: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM planning_reviews
                 WHERE profile_id=?1 AND blueprint_id=?2 AND status IN ('due','running','waiting_approval')
                 ORDER BY id DESC LIMIT 1",
                params![profile_id, bp.id],
                |r| r.get(0),
            )
            .ok();
        match open {
            Some(rid) => {
                let rev = self.get(rid, profile_id)?.ok_or("复盘记录异常")?;
                if rev.status == "waiting_approval" {
                    return Ok((rid, rev.status, rev.change_set_id, String::new()));
                }
                let snapshot = Self::build_snapshot(self.conn, profile_id, Some(bp.id), &period_start, &today)?;
                self.prepare_running(rid, profile_id, &snapshot)?;
                let _ = self
                    .conn
                    .execute(
                        "UPDATE planning_reviews SET period_start=?1, period_end=?2, updated_at=datetime('now')
                         WHERE id=?3",
                        params![period_start, today, rid],
                    )
                    .map_err(|e| e.to_string())?;
                Ok((rid, "running".to_string(), rev.change_set_id, snapshot))
            }
            None => {
                let rid = self.create_due(profile_id, Some(bp.id), &period_start, &today, trigger_type)?;
                let snapshot = Self::build_snapshot(self.conn, profile_id, Some(bp.id), &period_start, &today)?;
                self.prepare_running(rid, profile_id, &snapshot)?;
                Ok((rid, "running".to_string(), None, snapshot))
            }
        }
    }

    /// §3：构建 evidence snapshot（真实数据只读聚合；trust_state='needs_review' 不进 trusted evidence）。
    /// 内容：Active Blueprint + Phase/Milestone + period Tasks + trusted StudySessions +
    /// trusted Evaluations + confirmed PersonalProfile + active GoalTarget。
    pub fn build_snapshot(
        conn: &Connection,
        profile_id: i64,
        blueprint_id: Option<i64>,
        period_start: &str,
        period_end: &str,
    ) -> Result<String, String> {
        // Active Blueprint（未显式指定时取当前 active）
        let bp = match blueprint_id {
            Some(bid) => {
                let mut stmt = conn
                    .prepare("SELECT id, title, version, scenario_type, content_md, structured_json
                              FROM planning_blueprints WHERE id=?1 AND profile_id=?2")
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt.query_map(params![bid, profile_id], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "version": r.get::<_, i64>(2)?,
                        "scenario_type": r.get::<_, String>(3)?,
                        "content_md": r.get::<_, String>(4)?,
                        "structured_json": r.get::<_, Option<String>>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
                rows.next().transpose().map_err(|e| e.to_string())
            }
            None => {
                let mut stmt = conn
                    .prepare("SELECT id, title, version, scenario_type, content_md, structured_json
                              FROM planning_blueprints WHERE profile_id=?1 AND status='active' ORDER BY version DESC LIMIT 1")
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt.query_map(params![profile_id], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "version": r.get::<_, i64>(2)?,
                        "scenario_type": r.get::<_, String>(3)?,
                        "content_md": r.get::<_, String>(4)?,
                        "structured_json": r.get::<_, Option<String>>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
                rows.next().transpose().map_err(|e| e.to_string())
            }
        };
        // Phase / Milestone（属于 Blueprint）
        let mut phases: Vec<serde_json::Value> = Vec::new();
        let mut milestones: Vec<serde_json::Value> = Vec::new();
        if let Ok(Some(ref bpv)) = bp {
            let bp_id = bpv.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let mut ps = conn
                .prepare("SELECT phase_key, title, start_date, end_date, objective_md
                          FROM planning_phases WHERE blueprint_id=?1 ORDER BY sort_order")
                .map_err(|e| e.to_string())?;
            let p_rows = ps
                .query_map(params![bp_id], |r| {
                    Ok(serde_json::json!({
                        "phase_key": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "start_date": r.get::<_, Option<String>>(2)?,
                        "end_date": r.get::<_, Option<String>>(3)?,
                        "objective_md": r.get::<_, String>(4)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            phases = p_rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
            let mut ms = conn
                .prepare("SELECT milestone_key, title, start_date, end_date, date_precision, date_status
                          FROM planning_milestones WHERE blueprint_id=?1 ORDER BY id")
                .map_err(|e| e.to_string())?;
            let m_rows = ms
                .query_map(params![bp_id], |r| {
                    Ok(serde_json::json!({
                        "milestone_key": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "start_date": r.get::<_, Option<String>>(2)?,
                        "end_date": r.get::<_, Option<String>>(3)?,
                        "date_precision": r.get::<_, String>(4)?,
                        "date_status": r.get::<_, String>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            milestones = m_rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        }
        // period Tasks（未归档；含手动与蓝图任务）
        let mut stmt = conn
            .prepare("SELECT title, planned_date, status, origin FROM tasks
                      WHERE profile_id=?1 AND archived_at IS NULL AND date(planned_date) BETWEEN date(?2) AND date(?3)
                      ORDER BY planned_date")
            .map_err(|e| e.to_string())?;
        let tasks = stmt
            .query_map(params![profile_id, period_start, period_end], |r| {
                Ok(serde_json::json!({
                    "title": r.get::<_, String>(0)?,
                    "planned_date": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "origin": r.get::<_, Option<String>>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        let period_tasks = tasks.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        // trusted StudySessions（trusted_study_sessions VIEW 已排除 duration_review_state='needs_review'）
        let mut stmt = conn
            .prepare("SELECT started_at, duration_seconds, COALESCE(note,'') FROM trusted_study_sessions
                      WHERE profile_id=?1 AND date(started_at) BETWEEN date(?2) AND date(?3) ORDER BY started_at")
            .map_err(|e| e.to_string())?;
        let sess = stmt
            .query_map(params![profile_id, period_start, period_end], |r| {
                Ok(serde_json::json!({
                    "started_at": r.get::<_, String>(0)?,
                    "duration_seconds": r.get::<_, Option<i64>>(1)?,
                    "note": r.get::<_, String>(2)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        let trusted_sessions = sess.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        // trusted Evaluations（trust_state != 'needs_review' 才进 Evidence）
        let mut stmt = conn
            .prepare("SELECT title, evaluation_type, outcome, correct_items, total_items, created_at FROM evaluations
                      WHERE profile_id=?1 AND date(created_at) BETWEEN date(?2) AND date(?3)
                        AND (trust_state IS NULL OR trust_state != 'needs_review')
                      ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        let evs = stmt
            .query_map(params![profile_id, period_start, period_end], |r| {
                Ok(serde_json::json!({
                    "title": r.get::<_, String>(0)?,
                    "evaluation_type": r.get::<_, String>(1)?,
                    "outcome": r.get::<_, String>(2)?,
                    "correct_items": r.get::<_, Option<i64>>(3)?,
                    "total_items": r.get::<_, Option<i64>>(4)?,
                    "created_at": r.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        let trusted_evaluations = evs.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        // confirmed PersonalProfile（最新 confirmed version）
        let personal_profile = {
            let mut stmt = conn
                .prepare("SELECT version, md_content, structured_json, confirmed_at
                          FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query_map(params![profile_id], |r| {
                Ok(serde_json::json!({
                    "version": r.get::<_, i64>(0)?,
                    "md_content": r.get::<_, String>(1)?,
                    "structured_json": r.get::<_, Option<String>>(2)?,
                    "confirmed_at": r.get::<_, Option<String>>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?;
            rows.next().transpose().map_err(|e| e.to_string())?
        };
        // active GoalTargets（正式目标主源）
        let mut stmt = conn
            .prepare("SELECT role, title, status, target_date, data_json FROM goal_targets
                      WHERE profile_id=?1 AND status='active' ORDER BY id")
            .map_err(|e| e.to_string())?;
        let gts = stmt
            .query_map(params![profile_id], |r| {
                Ok(serde_json::json!({
                    "role": r.get::<_, String>(0)?,
                    "title": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "target_date": r.get::<_, Option<String>>(3)?,
                    "data_json": r.get::<_, Option<String>>(4)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        let active_goal_targets = gts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        let snapshot = serde_json::json!({
            "built_at": crate::repository::planning::today_utc8(),
            "period": { "start": period_start, "end": period_end },
            "active_blueprint": bp,
            "phases": phases,
            "milestones": milestones,
            "period_tasks": period_tasks,
            "trusted_sessions": trusted_sessions,
            "trusted_evaluations": trusted_evaluations,
            "personal_profile": personal_profile,
            "active_goal_targets": active_goal_targets,
        });
        serde_json::to_string(&snapshot).map_err(|e| e.to_string())
    }

    /// §3：NO_CHANGE → 事务内：review completed + cadence 刷新（last/next_review_at）+ 无 ChangeSet。
    pub fn complete_no_change_with(
        &self,
        id: i64,
        profile_id: i64,
        blueprint_id: Option<i64>,
        assessment_md: &str,
        risk_state: &str,
    ) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let n = tx
            .execute(
                "UPDATE planning_reviews SET status='completed', user_decision='no_change',
                   assessment_md=?1, risk_state=?2, completed_at=datetime('now'), updated_at=datetime('now')
                 WHERE id=?3 AND profile_id=?4 AND status='running'",
                params![assessment_md, risk_state, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("复盘记录不存在、不属于当前档案或不在 running 状态".to_string());
        }
        if let Some(bp) = blueprint_id {
            tx.execute(
                "UPDATE planning_blueprints SET last_review_at=datetime('now'),
                   next_review_at=datetime('now', printf('+%d days', review_interval_days)), updated_at=datetime('now')
                 WHERE id=?1 AND profile_id=?2",
                params![bp, profile_id],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// §3：ADJUSTMENT_PROPOSAL → 保存 assessment + 关联 ChangeSet / resulting Blueprint（waiting_approval）。
    pub fn save_assessment_with_result(
        &self,
        id: i64,
        profile_id: i64,
        assessment_md: &str,
        recommendation_json: &str,
        risk_state: &str,
        change_set_id: Option<i64>,
        resulting_blueprint_id: Option<i64>,
    ) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE planning_reviews SET assessment_md=?1, recommendation_json=?2, risk_state=?3,
                   status='waiting_approval',
                   change_set_id=COALESCE(?4, change_set_id),
                   resulting_blueprint_id=COALESCE(?5, resulting_blueprint_id),
                   updated_at=datetime('now')
                 WHERE id=?6 AND profile_id=?7 AND status='running'",
                params![assessment_md, recommendation_json, risk_state, change_set_id,
                    resulting_blueprint_id, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("复盘记录不存在、不属于当前档案或不在 running 状态".to_string());
        }
        Ok(())
    }


    /// 最新已确认 Review 的 risk_state（§30：Today 风险 Banner 数据源；启动只读）。
    pub fn latest_risk_state(&self, profile_id: i64) -> Result<String, String> {
        Ok(self
            .conn
            .query_row(
                "SELECT risk_state FROM planning_reviews
                 WHERE profile_id=?1 AND status='completed' AND risk_state NOT IN ('unknown','normal')
                 ORDER BY id DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "unknown".to_string()))
    }

    /// §39 step 5/7：无修改 → review completed 并刷新 Blueprint last/next_review_at。
    pub fn complete_no_change(&self, id: i64, profile_id: i64, blueprint_id: i64) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE planning_reviews SET status='completed', user_decision='no_change',
               completed_at=datetime('now'), updated_at=datetime('now')
             WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE planning_blueprints SET last_review_at=datetime('now'),
               next_review_at=datetime('now', printf('+%d days', review_interval_days)), updated_at=datetime('now')
             WHERE id=?1 AND profile_id=?2",
            params![blueprint_id, profile_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
}

fn parse_rev(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanningReview> {
    Ok(PlanningReview {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        blueprint_id: r.get(2)?,
        period_start: r.get(3)?,
        period_end: r.get(4)?,
        trigger_type: r.get(5)?,
        status: r.get(6)?,
        evidence_snapshot_json: r.get(7)?,
        assessment_md: r.get(8)?,
        recommendation_json: r.get(9)?,
        risk_state: r.get(10)?,
        change_set_id: r.get(11)?,
        user_decision: r.get(12)?,
        resulting_blueprint_id: r.get(13)?,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
        completed_at: r.get(16)?,
    })
}
