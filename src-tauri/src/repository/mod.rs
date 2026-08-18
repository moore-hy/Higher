pub mod adjustment;
pub mod attachment;
pub mod changeset;
pub mod cleanup;
pub mod conversation;
pub mod daily_report;
pub mod evaluation;
pub mod feedback;
pub mod goal;
pub mod goal_target;
pub mod insight;
pub mod knowledge_document;
pub mod knowledge_workspace;
pub mod learning_data;
pub mod learning_item;
pub mod mastery;
pub mod memory;
pub mod note;
pub mod personalization;
pub mod plan;
pub mod planning;
pub mod planning_review;
pub mod planning_source;
pub mod recurring_rule;
pub mod search;
pub mod setting;
pub mod source_ingest;
pub mod study_profile;
pub mod study_session;
pub mod study_stage;
pub mod task;

/// 通用「标签 → 计数」结果（如知识掌握状态分布、验证类型分布）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CountPair {
    pub label: String,
    pub count: i64,
}

/// delete_task 结果（BATCH-03.1 §18-20）：有历史时 deleted=false，
/// 前端提示"移除并保留学习历史"并改调 archive_task。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeleteTaskOutcome {
    pub deleted: bool,
    pub has_history: bool,
}

/// 备份文件信息（DEV-0036 §108：仅展示日期/大小/路径）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
    pub name: String,
    pub size_bytes: u64,
    pub path: String,
}

// ============ DEV-0301 日期详情聚合（学习规划 → 点击某天） ============

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DayTaskRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub learning_item_id: Option<i64>,
    pub knowledge: Option<String>,
    pub planned_time: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DaySessionRow {
    pub id: i64,
    pub title: String, // 会话标题 / 知识名 / 任务名 / "学习记录"
    pub learning_item_id: Option<i64>,
    pub task_id: Option<i64>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub note_excerpt: String, // 纯文本前 120 字（含 [图片] 等占位）
    pub attachment_count: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DayDetail {
    pub date: String,
    pub tasks: Vec<DayTaskRow>,
    pub sessions: Vec<DaySessionRow>,
    pub evaluations: Vec<(i64, String, String)>, // (id, title, outcome)
    pub total_seconds: i64,
}

/// 聚合某档案某天真实记录（任务 + Session + 验证 + 总时长）。
pub fn build_day_detail(
    conn: &rusqlite::Connection,
    profile_id: i64,
    date: &str,
) -> Result<DayDetail, String> {
    // 任务（该日计划；含已完成/未完成；不含归档）
    let mut tasks = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.title, t.status, t.learning_item_id, li.name, t.planned_time
                 FROM tasks t
                 LEFT JOIN learning_items li ON t.learning_item_id = li.id
                 WHERE t.profile_id = ?1 AND t.planned_date = date(?2)
                   AND t.archived_at IS NULL
                 ORDER BY t.planned_time IS NULL, t.planned_time, t.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, date], |r| {
                Ok(DayTaskRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    learning_item_id: r.get(3)?,
                    knowledge: r.get(4)?,
                    planned_time: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            tasks.push(row.map_err(|e| e.to_string())?);
        }
    }

    // Session（按 started_at 归日；含无知识快速学习）
    let mut sessions = Vec::new();
    let mut total_seconds = 0i64;
    {
        let mut stmt = conn
            .prepare(
                "SELECT ss.id, COALESCE(ss.title, li.name, tk.title, '学习记录'),
                        ss.learning_item_id, ss.task_id, ss.started_at, ss.ended_at,
                        ss.duration_seconds, COALESCE(ss.note, ''),
                        (SELECT COUNT(*) FROM learning_attachments la
                          WHERE la.session_id = ss.id)
                 FROM study_sessions ss
                 LEFT JOIN learning_items li ON ss.learning_item_id = li.id
                 LEFT JOIN tasks tk ON ss.task_id = tk.id
                 WHERE ss.profile_id = ?1 AND date(ss.started_at, '+8 hours') = date(?2)
                 ORDER BY ss.started_at",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, date], |r| {
                let note_raw: String = r.get(7)?;
                Ok(DaySessionRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    learning_item_id: r.get(2)?,
                    task_id: r.get(3)?,
                    started_at: r.get(4)?,
                    ended_at: r.get(5)?,
                    duration_seconds: r.get(6)?,
                    note_excerpt: note::plain_text(Some(&note_raw))
                        .chars()
                        .take(120)
                        .collect::<String>(),
                    attachment_count: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let s = row.map_err(|e| e.to_string())?;
            total_seconds += s.duration_seconds.unwrap_or(0);
            sessions.push(s);
        }
    }

    // 验证（occurred_at 归日）
    let mut evaluations = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.title, COALESCE(e.outcome, 'unrated')
                 FROM evaluations e
                 WHERE e.profile_id = ?1 AND date(e.occurred_at, '+8 hours') = date(?2)
                 ORDER BY e.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, date], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            evaluations.push(row.map_err(|e| e.to_string())?);
        }
    }

    Ok(DayDetail {
        date: date.to_string(),
        tasks,
        sessions,
        evaluations,
        total_seconds,
    })
}
