//! AI Mastery Assessments（DEV-0050 / PHASE D §46-60）。
//!
//! - append-only：同一 period 多次评估全部保留；UI 只读最新
//! - AI Write Tools 保持 0：评估由用户点击 → ai_analyze(专用 prompt) → 校验 JSON →
//!   Higher 普通 Command 落库（AI 永不直接写库）
//! - stale：assessment 之后该周期新增 ended Session / Evaluation → 提示重新评估（不自动）

use rusqlite::{params, Connection};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MasteryAssessment {
    pub id: i64,
    pub profile_id: i64,
    pub goal_id: Option<i64>,
    pub period_type: String, // day | week | month | year
    pub period_start: String,
    pub period_end: String,
    pub status: String, // scored | insufficient_evidence
    pub score: Option<i64>,
    pub confidence: String, // low | medium | high
    pub summary: String,
    pub understanding_score: Option<i64>,
    pub coverage_score: Option<i64>,
    pub verification_score: Option<i64>,
    pub strengths: Vec<String>,
    pub gaps: Vec<String>,
    pub evidence: Vec<String>,
    pub suggestions: Vec<String>,
    pub model: String,
    pub created_at: String,
}

const COLS: &str = "id, profile_id, goal_id, period_type, period_start, period_end, status, score, confidence, summary, understanding_score, coverage_score, verification_score, strengths_json, gaps_json, evidence_json, suggestions_json, model, created_at";

pub struct MasteryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MasteryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 落库前服务端校验（§50/§53：证据不足不得硬打分；分数范围/三维上限）。
    pub fn validate(a: &MasteryAssessment) -> Result<(), String> {
        match a.status.as_str() {
            "scored" => {
                let s = a.score.ok_or("scored 评估缺少 score")?;
                if !(0..=100).contains(&s) {
                    return Err(format!("score 超出 0-100：{s}"));
                }
                let u = a.understanding_score.ok_or("scored 评估缺少 understanding")?;
                let c = a.coverage_score.ok_or("scored 评估缺少 coverage")?;
                let v = a.verification_score.ok_or("scored 评估缺少 verification")?;
                if u > 40 || c > 30 || v > 30 || u < 0 || c < 0 || v < 0 {
                    return Err(format!("维度分数超限（40/30/30）：{u}/{c}/{v}"));
                }
            }
            "insufficient_evidence" => {
                if a.score.is_some() {
                    return Err("insufficient_evidence 不得携带 score".to_string());
                }
            }
            other => return Err(format!("非法 status：{other}")),
        }
        if !["low", "medium", "high"].contains(&a.confidence.as_str()) {
            return Err(format!("非法 confidence：{}", a.confidence));
        }
        Ok(())
    }

    /// 插入（append-only，不更新旧行）。
    pub fn insert(&self, a: &MasteryAssessment) -> Result<i64, String> {
        Self::validate(a)?;
        let n = self
            .conn
            .execute(
                "INSERT INTO mastery_assessments
                 (profile_id, goal_id, period_type, period_start, period_end, status, score, confidence, summary,
                  understanding_score, coverage_score, verification_score, strengths_json, gaps_json, evidence_json, suggestions_json, model)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    a.profile_id, a.goal_id, a.period_type, a.period_start, a.period_end,
                    a.status, a.score, a.confidence, a.summary,
                    a.understanding_score, a.coverage_score, a.verification_score,
                    serde_json::to_string(&a.strengths).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&a.gaps).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&a.evidence).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&a.suggestions).unwrap_or_else(|_| "[]".into()),
                    a.model,
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("保存评估失败".to_string());
        }
        Ok(self.conn.last_insert_rowid())
    }

    /// 某周期最新一条评估（UI 展示用；学习日 UTC+8 归属）。
    pub fn latest(
        &self,
        profile_id: i64,
        period_type: &str,
        period_start: &str,
        period_end: &str,
    ) -> rusqlite::Result<Option<MasteryAssessment>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM mastery_assessments
             WHERE profile_id = ?1 AND period_type = ?2 AND period_start = ?3 AND period_end = ?4
             ORDER BY id DESC LIMIT 1",
            COLS
        ))?;
        let mut rows = stmt.query_map(
            params![profile_id, period_type, period_start, period_end],
            parse_row,
        )?;
        rows.next().transpose()
    }

    /// 全部历史（详情/调试）。
    pub fn list_history(
        &self,
        profile_id: i64,
        period_type: &str,
        period_start: &str,
        period_end: &str,
    ) -> rusqlite::Result<Vec<MasteryAssessment>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM mastery_assessments
             WHERE profile_id = ?1 AND period_type = ?2 AND period_start = ?3 AND period_end = ?4
             ORDER BY id DESC",
            COLS
        ))?;
        let rows = stmt.query_map(
            params![profile_id, period_type, period_start, period_end],
            parse_row,
        )?;
        rows.collect()
    }

    /// 趋势序列（§60）：只有真正评估过的周期返回条目（未评估 ≠ 0）。
    pub fn trend(
        &self,
        profile_id: i64,
        period_type: &str,
        starts: &[String],
        ends: &[String],
    ) -> rusqlite::Result<Vec<(usize, i64)>> {
        let mut out = Vec::new();
        for (i, (s, e)) in starts.iter().zip(ends.iter()).enumerate() {
            if let Some(a) = self.latest(profile_id, period_type, s, e)? {
                if let Some(score) = a.score {
                    out.push((i, score));
                }
            }
        }
        Ok(out)
    }

    /// §57 stale：assessment 创建后该周期（学习日 UTC+8）是否新增 ended Session / Evaluation。
    pub fn stale_since(
        &self,
        profile_id: i64,
        period_start: &str,
        period_end: &str,
        assessment_created_at: &str,
    ) -> rusqlite::Result<bool> {
        let sess: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_sessions
             WHERE profile_id = ?1 AND status='completed'
               AND date(started_at, '+8 hours') BETWEEN date(?2) AND date(?3)
               AND ended_at > datetime(?4)",
            params![profile_id, period_start, period_end, assessment_created_at],
            |r| r.get(0),
        )?;
        let evals: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM evaluations
             WHERE profile_id = ?1
               AND date(occurred_at, '+8 hours') BETWEEN date(?2) AND date(?3)
               AND occurred_at > datetime(?4)",
            params![profile_id, period_start, period_end, assessment_created_at],
            |r| r.get(0),
        )?;
        Ok(sess + evals > 0)
    }
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MasteryAssessment> {
    let parse_list = |raw: String| -> Vec<String> {
        serde_json::from_str(&raw).unwrap_or_default()
    };
    Ok(MasteryAssessment {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        period_type: row.get(3)?,
        period_start: row.get(4)?,
        period_end: row.get(5)?,
        status: row.get(6)?,
        score: row.get(7)?,
        confidence: row.get(8)?,
        summary: row.get(9)?,
        understanding_score: row.get(10)?,
        coverage_score: row.get(11)?,
        verification_score: row.get(12)?,
        strengths: parse_list(row.get(13)?),
        gaps: parse_list(row.get(14)?),
        evidence: parse_list(row.get(15)?),
        suggestions: parse_list(row.get(16)?),
        model: row.get(17)?,
        created_at: row.get(18)?,
    })
}
