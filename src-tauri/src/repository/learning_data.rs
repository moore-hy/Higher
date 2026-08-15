//! Learning Data 聚合（DEV-0050 / PHASE D §42-45, §58-60）。
//!
//! 三核心指标（单周期）+ 趋势序列：
//! - 学习时间 = 周期内 ended Session duration_seconds 总和（active 不计入）
//! - 任务完成率 = planned_date 落在周期内的 completed/total（含 archived；历史不因归档消失）
//! - AI 掌握度 = mastery_assessments 最新一条（仅用户主动触发产生）
//!
//! 所有"学习日"统一 UTC+8（DEV-0049 不变量：date(x,'+8 hours')）。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LearningStats {
    pub study_seconds: i64,
    pub tasks_total: i64,
    pub tasks_completed: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TrendPoint {
    pub label: String,
    pub start: String,
    pub end: String,
    pub study_seconds: i64,
    pub tasks_total: i64,
    pub tasks_completed: i64,
    /// None = 未评估（≠0，禁止补 0）
    pub mastery_score: Option<i64>,
}

pub struct LearningDataRepository<'a> {
    conn: &'a Connection,
}

impl<'a> LearningDataRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 单周期统计（start/end 为学习日 YYYY-MM-DD，含两端）。
    pub fn stats(&self, profile_id: i64, start: &str, end: &str) -> Result<LearningStats, String> {
        let study_seconds: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions
                 WHERE profile_id = ?1 AND status = 'completed'
                   AND date(started_at, '+8 hours') BETWEEN date(?2) AND date(?3)",
                params![profile_id, start, end],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let (total, completed): (i64, i64) = self
            .conn
            .query_row(
                "SELECT COUNT(*), SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END)
                 FROM tasks
                 WHERE profile_id = ?1 AND planned_date BETWEEN date(?2) AND date(?3)",
                params![profile_id, start, end],
                |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .map_err(|e| e.to_string())?;
        Ok(LearningStats {
            study_seconds,
            tasks_total: total,
            tasks_completed: completed,
        })
    }

    /// 趋势序列（buckets: (label,start,end) 列表，前端按周期类型生成）。
    /// mastery 由调用方传入（避免 repo 间依赖）。
    pub fn trend(
        &self,
        profile_id: i64,
        buckets: &[(String, String, String)],
        mastery: &[(usize, i64)],
    ) -> Result<Vec<TrendPoint>, String> {
        let mut out = Vec::with_capacity(buckets.len());
        for (i, (label, s, e)) in buckets.iter().enumerate() {
            let st = self.stats(profile_id, s, e)?;
            out.push(TrendPoint {
                label: label.clone(),
                start: s.clone(),
                end: e.clone(),
                study_seconds: st.study_seconds,
                tasks_total: st.tasks_total,
                tasks_completed: st.tasks_completed,
                mastery_score: mastery.iter().find(|(idx, _)| *idx == i).map(|(_, sc)| *sc),
            });
        }
        Ok(out)
    }
}
