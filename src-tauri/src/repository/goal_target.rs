//! GoalTarget 通用目标核心（DEV-0059 §11）。
//!
//! - 通用核心：candidate/draft/active/historical/dismissed 状态机
//! - 考研语义（scenario_type=postgraduate）：role = reach | safety；
//!   v021 partial unique index 保证 active reach / safety 每档案各 ≤1
//! - update behavior（§11.3）：替换时旧 active → historical，新版本 → active（历史可查）
//! - legacy 目标源（§11.4）：target_description/target_date/goal_brief_json/goals.name
//!   只能作为 candidate 呈现，禁止自动猜 active
//! - data_json 为 scenario-specific contract（§11.2），Repository 校验 postgraduate JSON

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GoalTarget {
    pub id: i64,
    pub profile_id: i64,
    pub scenario_type: String,
    pub role: String,
    pub title: String,
    pub target_date: Option<String>,
    pub data_json: String,
    pub provenance_json: String,
    pub status: String,
    pub version: i64,
    pub supersedes_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub activated_at: Option<String>,
}

/// §11.4：legacy 目标源候选（未确认，绝不自动 active）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LegacyTargetCandidate {
    pub source: String,
    pub title: String,
    pub target_date: Option<String>,
    pub detail: String,
}

pub struct GoalTargetRepository<'a> {
    conn: &'a Connection,
}

impl<'a> GoalTargetRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// §11.2：校验 scenario-specific data_json（当前仅 postgraduate 有硬性契约）。
    pub fn validate_data_json(scenario_type: &str, data: &serde_json::Value) -> Result<(), String> {
        if scenario_type != "postgraduate" {
            return Ok(());
        }
        let non_empty = |k: &str| -> bool {
            data.get(k)
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        };
        if non_empty("institution_name") && non_empty("program_name") {
            Ok(())
        } else {
            Err("考研目标必须包含 institution_name（院校）与 program_name（专业）".to_string())
        }
    }

    /// 创建（status 由调用方给出；postgraduate 直接 active 需经 activate 保证唯一）。
    pub fn create(
        &self,
        profile_id: i64,
        scenario_type: &str,
        role: &str,
        title: &str,
        target_date: Option<&str>,
        data_json: &str,
        provenance_json: &str,
        status: &str,
    ) -> Result<GoalTarget, String> {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data_json) {
            Self::validate_data_json(scenario_type, &v).map_err(|e| e.to_string())?;
        }
        self.conn
            .execute(
                "INSERT INTO goal_targets (profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.get(id, profile_id)?.ok_or("创建失败".to_string())
    }

    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<GoalTarget>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status, version, supersedes_id, created_at, updated_at, activated_at
                      FROM goal_targets WHERE id=?1 AND profile_id=?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse_gt).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn list_by_profile(&self, profile_id: i64) -> Result<Vec<GoalTarget>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status, version, supersedes_id, created_at, updated_at, activated_at
                      FROM goal_targets WHERE profile_id=?1 ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![profile_id], parse_gt).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 当前 active（可指定 scenario/role 过滤）。
    pub fn list_active(&self, profile_id: i64, scenario_type: Option<&str>, role: Option<&str>) -> Result<Vec<GoalTarget>, String> {
        let mut sql = String::from(
            "SELECT id, profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status, version, supersedes_id, created_at, updated_at, activated_at
             FROM goal_targets WHERE profile_id=?1 AND status='active'",
        );
        let mut ps: Vec<rusqlite::types::Value> = vec![profile_id.into()];
        if let Some(s) = scenario_type {
            sql.push_str(" AND scenario_type=?");
            ps.push(rusqlite::types::Value::from(s.to_string()));
        }
        if let Some(r) = role {
            sql.push_str(" AND role=?");
            ps.push(rusqlite::types::Value::from(r.to_string()));
        }
        sql.push_str(" ORDER BY id DESC");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ps.iter()), parse_gt).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// §11.3：激活（一个 transaction）——同 scenario+role 的其他 active → historical；
    /// 目标 → active + activated_at。保证 active 唯一性（postgraduate 由 partial unique 兜底）。
    pub fn activate(&self, profile_id: i64, id: i64) -> Result<GoalTarget, String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let _ = activate_in_tx(&tx, profile_id, id).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.get(id, profile_id)?.ok_or("激活失败".to_string())
    }

    /// 编辑 active 目标 = 新版本 target（旧 → historical，新 → active；§11.3）。
    pub fn replace_with(
        &self,
        profile_id: i64,
        id: i64,
        title: &str,
        target_date: Option<&str>,
        data_json: &str,
        provenance_json: &str,
    ) -> Result<GoalTarget, String> {
        let old = self.get(id, profile_id)?.ok_or("目标不存在或不属于当前档案")?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data_json) {
            Self::validate_data_json(&old.scenario_type, &v)?;
        }
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE goal_targets SET status='historical', updated_at=datetime('now') WHERE id=?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        let next_ver = old.version + 1;
        tx.execute(
            "INSERT INTO goal_targets (profile_id, scenario_type, role, title, target_date, data_json, provenance_json, status, version, supersedes_id, activated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8,?9,datetime('now'))",
            params![profile_id, old.scenario_type, old.role, title, target_date, data_json, provenance_json, next_ver, old.id],
        )
        .map_err(|e| e.to_string())?;
        let new_id = tx.last_insert_rowid();
        tx.commit().map_err(|e| e.to_string())?;
        self.get(new_id, profile_id)?.ok_or("替换失败".to_string())
    }

    /// 放弃候选（dismissed；历史保留）。
    pub fn dismiss(&self, profile_id: i64, id: i64) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE goal_targets SET status='dismissed', updated_at=datetime('now') WHERE id=?1 AND profile_id=?2 AND status!='active'",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("目标不存在、不属于当前档案或处于 active".to_string());
        }
        Ok(())
    }

    /// §11.4：Legacy 目标源候选（只读聚合；绝不自动激活）。
    /// 来源：study_profiles.target_description/target_date、goals.goal_brief_json、goals.name。
    pub fn list_legacy_candidates(&self, profile_id: i64) -> Result<Vec<LegacyTargetCandidate>, String> {
        let mut out: Vec<LegacyTargetCandidate> = Vec::new();
        // study_profiles.target_description
        if let Ok(row) = self.conn.query_row(
            "SELECT COALESCE(target_description,''), target_date FROM study_profiles WHERE id=?1",
            params![profile_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        ) {
            let (desc, date) = row;
            if !desc.trim().is_empty() {
                out.push(LegacyTargetCandidate {
                    source: "study_profiles.target_description".into(),
                    title: desc.chars().take(60).collect(),
                    target_date: date,
                    detail: desc,
                });
            }
        }
        // goals.goal_brief_json（final 行）
        if let Ok(v) = self.conn.query_row(
            "SELECT goal_brief_json FROM goals WHERE profile_id=?1 AND goal_level='final' AND archived_at IS NULL AND goal_brief_json IS NOT NULL AND goal_brief_json != ''",
            params![profile_id],
            |r| r.get::<_, String>(0),
        ) {
            if let Ok(b) = serde_json::from_str::<serde_json::Value>(&v) {
                let title = b.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
                let outcome = b.get("outcome").and_then(|t| t.as_str()).unwrap_or("").to_string();
                let deadline = b.get("deadline").and_then(|t| t.as_str()).unwrap_or("").to_string();
                if !title.trim().is_empty() {
                    out.push(LegacyTargetCandidate {
                        source: "goals.goal_brief_json".into(),
                        title: title.clone(),
                        target_date: if deadline.trim().is_empty() { None } else { Some(deadline) },
                        detail: if outcome.trim().is_empty() { title } else { format!("{title}｜{outcome}") },
                    });
                }
            }
        }
        Ok(out)
    }
}

fn parse_gt(r: &rusqlite::Row<'_>) -> rusqlite::Result<GoalTarget> {
    Ok(GoalTarget {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        scenario_type: r.get(2)?,
        role: r.get(3)?,
        title: r.get(4)?,
        target_date: r.get(5)?,
        data_json: r.get(6)?,
        provenance_json: r.get(7)?,
        status: r.get(8)?,
        version: r.get(9)?,
        supersedes_id: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        activated_at: r.get(13)?,
    })
}

/// §11.3 激活核心（供 ChangeSet apply 外层事务内复用；不嵌套新事务）。
pub fn activate_in_tx(
    tx: &rusqlite::Transaction<'_>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let cur: Option<(i64, String, String)> = tx
        .query_row(
            "SELECT id, scenario_type, role FROM goal_targets WHERE id=?1 AND profile_id=?2 AND status IN ('candidate','draft','historical','dismissed')",
            params![id, profile_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((tid, scenario, role)) = cur else {
        return Err("目标不存在或已处于 active（如需替换请先编辑再激活）".to_string());
    };
    tx.execute(
        "UPDATE goal_targets SET status='historical', updated_at=datetime('now')
         WHERE profile_id=?1 AND status='active' AND scenario_type=?2 AND role=?3",
        params![profile_id, scenario, role],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE goal_targets SET status='active', activated_at=datetime('now'), updated_at=datetime('now') WHERE id=?1",
        params![tid],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
