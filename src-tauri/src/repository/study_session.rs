use rusqlite::{params, Connection};

/// 学习会话（BATCH-04 / v013 起 Profile First · Study First）。
///
/// - **只要求 Profile**（profile_id NOT NULL 直挂）；goal_id / task_id / learning_item_id 全可空
/// - title：会话标题（Quick="快速学习"；Task=task.title；Knowledge=item.name；可改）
/// - 开始即落库（active）；结束写 ended_at + duration
/// - Session 结束 = 永久学习历史（先保存后归档；关闭归档层不丢记录）
/// - 历史可重开编辑（title/note/媒体/关联）；可手动修正时间（重算 duration）；
///   可删除（连同仅属于该 Session 的附件；不动 Knowledge 正文）
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StudySession {
    pub id: i64,
    pub profile_id: i64,
    #[serde(default)]
    pub goal_id: Option<i64>,
    #[serde(default)]
    pub task_id: Option<i64>,
    #[serde(default)]
    pub learning_item_id: Option<i64>,
    #[serde(default)]
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub status: String,
    pub note: Option<String>,
    /// v014 富文本文档（Tiptap JSON）；NULL = 历史纯文本 Session
    #[serde(default)]
    pub note_document_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 手动修正过 started_at/ended_at 的标记（§69：明确标记，不在主界面突出）
    #[serde(default)]
    pub time_corrected: i64,
    /// v018（DEV-0053 §27-29）：core | regular | accumulation | unplanned
    #[serde(default = "default_activity_kind")]
    pub activity_kind: String,
    /// v020（DEV-0057 §99）：normal | needs_review | confirmed | corrected
    #[serde(default = "default_review_state")]
    pub duration_review_state: String,
}

fn default_activity_kind() -> String {
    "unplanned".into()
}

fn default_review_state() -> String {
    "normal".into()
}

/// 学习日归属不变量（DEV-0049 §11.4，全项目统一）：
/// - started_at 以 **UTC** 存储（datetime('now')）；
/// - Higher 学习日 = **UTC+8** 日历日；
/// - 因此任何"某学习日 D 的 Session"过滤必须使用
///   `date(started_at, '+8 hours') = D`（或 BETWEEN），禁止裸 `date(started_at)`。
const SESSION_COLUMNS: &str = "id, profile_id, goal_id, task_id, learning_item_id, title, started_at, ended_at, duration_seconds, status, note, note_document_json, created_at, updated_at, time_corrected, activity_kind, duration_review_state";

fn cols(alias: &str) -> String {
    SESSION_COLUMNS
        .split(", ")
        .map(|c| format!("{}.{}", alias, c))
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct StudySessionRepository<'a> {
    conn: &'a Connection,
}

impl<'a> StudySessionRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Quick Study（§39）：只要求 profile_id；task/goal/item 全 NULL；title="快速学习"。
    /// DEV-0053 §29：activity_kind = unplanned（用户可后续重新分类）。
    pub fn start_quick(&self, profile_id: i64, task_id: Option<i64>) -> rusqlite::Result<StudySession> {
        self.start_full(profile_id, None, None, task_id, "快速学习", "unplanned")
    }

    /// 从 Task 开始（§40）：默认 title=task.title。
    /// DEV-0053 §42：历史 Snapshot——复制 Task.goal_id/learning_item_id 并按
    /// task_kind/priority 推导 activity_kind（§28），不随 Task 未来改动漂移（§43）。
    pub fn start_for_task(&self, profile_id: i64, task_id: i64) -> rusqlite::Result<StudySession> {
        let task: Option<(String, Option<i64>, Option<i64>, String, String)> = self
            .conn
            .query_row(
                "SELECT title, goal_id, learning_item_id, task_kind, priority
                 FROM tasks WHERE id = ?1 AND profile_id = ?2",
                params![task_id, profile_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                    ))
                },
            )
            .ok();
        let (title, goal_id, item_id, kind) = match task {
            Some((t, g, i, k, p)) => {
                let activity = if k == "accumulation" {
                    "accumulation"
                } else if p == "core" {
                    "core"
                } else {
                    "regular"
                };
                (t, g, i, activity)
            }
            None => ("任务学习".to_string(), None, None, "regular"),
        };
        self.start_full(profile_id, goal_id, item_id, Some(task_id), &title, kind)
    }

    /// 从 Knowledge 开始（§41）：默认 title=item.name；activity_kind=unplanned
    /// （知识自由学不属于当日计划；可在结束/历史中重新分类）。
    pub fn start_for_item(&self, learning_item_id: i64, task_id: Option<i64>) -> rusqlite::Result<StudySession> {
        let (profile_id, goal_id, name): (i64, Option<i64>, String) = self.conn.query_row(
            "SELECT profile_id, goal_id, name FROM learning_items WHERE id = ?1",
            params![learning_item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        self.start_full(profile_id, goal_id, Some(learning_item_id), task_id, &name, "unplanned")
    }

    /// 兼容旧调用：start(item_id, task_id)。
    pub fn start(&self, learning_item_id: i64, task_id: Option<i64>) -> rusqlite::Result<StudySession> {
        self.start_for_item(learning_item_id, task_id)
    }

    fn start_full(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        learning_item_id: Option<i64>,
        task_id: Option<i64>,
        title: &str,
        activity_kind: &str,
    ) -> rusqlite::Result<StudySession> {
        // DEV-0054 §26-28 Start Guard：一个 Profile 最多一个 active StudySession。
        // 已有 active → 拒绝（人话错误；历史多 active 不在此自动处理 §29）。
        let active_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1 AND status = 'active'",
            params![profile_id],
            |r| r.get(0),
        )?;
        if active_count > 0 {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                Some("你已有一项学习正在进行，请先继续或结束当前学习。".to_string()),
            ));
        }
        let ak = match activity_kind {
            "core" | "regular" | "accumulation" => activity_kind,
            _ => "unplanned",
        };
        self.conn.execute(
            "INSERT INTO study_sessions (profile_id, goal_id, learning_item_id, task_id, title, started_at, status, activity_kind)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), 'active', ?6)",
            params![profile_id, goal_id, learning_item_id, task_id, title, ak],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// DEV-0053 §35/§89：修改活动分类（Activity ⋯ 菜单 / Session 编辑）。
    pub fn set_activity_kind(&self, id: i64, profile_id: i64, kind: &str) -> Result<(), String> {
        let k = match kind {
            "core" | "regular" | "accumulation" | "unplanned" => kind,
            other => return Err(format!("非法活动分类：{other}")),
        };
        let n = self
            .conn
            .execute(
                "UPDATE study_sessions SET activity_kind = ?1, updated_at = datetime('now')
                 WHERE id = ?2 AND profile_id = ?3",
                params![k, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("学习记录不存在或不属于当前档案".to_string());
        }
        Ok(())
    }

    /// DEV-0053 §35/§52：整理进知识（只改 learning_item_id 关联，不复制 Note）。
    /// 目标 item 必须与 Session 同 profile。
    pub fn set_learning_item(&self, id: i64, profile_id: i64, learning_item_id: Option<i64>) -> Result<(), String> {
        if let Some(item) = learning_item_id {
            let item_profile: Option<i64> = self
                .conn
                .query_row(
                    "SELECT profile_id FROM learning_items WHERE id = ?1",
                    params![item],
                    |r| r.get(0),
                )
                .ok();
            if item_profile != Some(profile_id) {
                return Err("所选知识不属于当前学习档案".to_string());
            }
        }
        let n = self
            .conn
            .execute(
                "UPDATE study_sessions SET learning_item_id = ?1, updated_at = datetime('now')
                 WHERE id = ?2 AND profile_id = ?3",
                params![learning_item_id, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("学习记录不存在或不属于当前档案".to_string());
        }
        Ok(())
    }

    /// DEV-0053 §46：Goal 学习记录（Day 直接 goal_id；Month/Annual/Final 经 descendant）。
    pub fn list_by_goal(&self, profile_id: i64, goal_id: i64, limit: i64) -> Result<Vec<StudySession>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "WITH RECURSIVE sub(id) AS (
                     SELECT id FROM goals WHERE id = ?2 AND profile_id = ?1
                     UNION ALL
                     SELECT g.id FROM goals g JOIN sub s ON g.parent_goal_id = s.id
                 )
                 SELECT {} FROM study_sessions s
                 WHERE s.profile_id = ?1 AND s.goal_id IN (SELECT id FROM sub)
                 ORDER BY s.started_at DESC
                 LIMIT ?3",
                cols("s")
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, goal_id, limit.clamp(1, 200)], parse_session)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// DEV-0053 §50-51：未归类学习（learning_item_id IS NULL）。
    pub fn list_unassigned(&self, profile_id: i64, limit: i64) -> Result<Vec<StudySession>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM study_sessions
                 WHERE profile_id = ?1 AND learning_item_id IS NULL
                 ORDER BY started_at DESC
                 LIMIT ?2",
                SESSION_COLUMNS
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, limit.clamp(1, 200)], parse_session)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 结束归档：把本次学习挂到知识 / 任务（均可空=仅保留学习记录）。
    pub fn attach(
        &self,
        id: i64,
        learning_item_id: Option<i64>,
        task_id: Option<i64>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_sessions
             SET learning_item_id = COALESCE(?2, learning_item_id),
                 task_id = COALESCE(?3, task_id),
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![id, learning_item_id, task_id],
        )?;
        Ok(())
    }

    /// 解除知识关联（§132 Unlink）。
    pub fn unlink_item(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_sessions SET learning_item_id = NULL, updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 结束学习（§57 顺序）：flush note 由调用方保证 → ended_at → duration → completed。
    pub fn end(&self, id: i64, note: Option<&str>) -> rusqlite::Result<StudySession> {
        self.conn.execute(
            "UPDATE study_sessions
             SET ended_at = datetime('now'),
                 duration_seconds = CAST(strftime('%s', 'now') - strftime('%s', started_at) AS INTEGER),
                 status = 'completed',
                 note = COALESCE(?2, note),
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![id, note],
        )?;
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 更新笔记（debounce 自动保存 / 历史编辑模式）。
    pub fn update_note(&self, id: i64, note: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_sessions SET note = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, note],
        )?;
        Ok(())
    }

    /// §10 富文本文档保存：note 与 note_document_json **同一条 UPDATE 原子写入**。
    /// 校验：Session 必须存在；不触碰 profile_id/started_at/ended_at/duration/status。
    /// document 为 None = 清除文档（回退纯文本 Session）；JSON 设上限防异常巨量输入。
    pub fn update_document(
        &self,
        id: i64,
        note: &str,
        note_document_json: Option<&str>,
    ) -> rusqlite::Result<()> {
        const MAX_DOC: usize = 5 * 1024 * 1024; // 5MB JSON 上限
        if let Some(doc) = note_document_json {
            if doc.len() > MAX_DOC {
                return Err(rusqlite::Error::InvalidParameterName(
                    "笔记文档过大（超过 5MB 上限），已拒绝保存".to_string(),
                ));
            }
        }
        let n = self.conn.execute(
            "UPDATE study_sessions
             SET note = ?2, note_document_json = ?3, updated_at = datetime('now')
             WHERE id = ?1",
            params![id, note, note_document_json],
        )?;
        if n == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    /// 更新标题（§54：Header 可点击修改；§68 历史编辑）。
    pub fn update_title(&self, id: i64, title: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_sessions SET title = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, title],
        )?;
        Ok(())
    }

    /// 手动修正时间（§69）：改 started_at/ended_at → 重算 duration → 标记 time_corrected。
    pub fn correct_time(
        &self,
        id: i64,
        started_at: &str,
        ended_at: Option<&str>,
    ) -> rusqlite::Result<StudySession> {
        let duration_expr = match ended_at {
            Some(_) => "CAST(strftime('%s', ?3) - strftime('%s', ?2) AS INTEGER)",
            None => "NULL",
        };
        self.conn.execute(
            &format!(
                "UPDATE study_sessions
                 SET started_at = ?2,
                     ended_at = ?3,
                     duration_seconds = {duration_expr},
                     time_corrected = 1,
                     updated_at = datetime('now')
                 WHERE id = ?1"
            ),
            params![id, started_at, ended_at],
        )?;
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 删除 Session（§70）：确认由前端负责；附件物理文件由 command 层按
    /// list_session_attachment_paths 的结果清理；Knowledge 正文不动。
    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM study_sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 删除前取该 Session 附件的 relative_path 列表（仅属于该 Session 的）。
    pub fn list_session_attachment_paths(&self, id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT relative_path FROM learning_attachments WHERE session_id = ?1",
        )?;
        let rows = stmt.query_map(params![id], |r| r.get(0))?;
        rows.collect()
    }

    /// 某知识节点的学习记录（最新在前）。
    pub fn list_by_learning_item(&self, learning_item_id: i64, limit: i64) -> rusqlite::Result<Vec<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions WHERE learning_item_id = ?1
             ORDER BY id DESC LIMIT ?2",
            SESSION_COLUMNS
        ))?;
        let rows = stmt.query_map(params![learning_item_id, limit], parse_session)?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions WHERE id = ?1",
            SESSION_COLUMNS
        ))?;
        let mut rows = stmt.query_map(params![id], parse_session)?;
        rows.next().transpose()
    }

    /// 当前进行中的 Session（全局唯一）。
    pub fn get_active(&self) -> rusqlite::Result<Option<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions WHERE status = 'active' ORDER BY id DESC LIMIT 1",
            SESSION_COLUMNS
        ))?;
        let mut rows = stmt.query_map([], parse_session)?;
        rows.next().transpose()
    }

    /// 最近 N 条（全库；旧测试用）。
    pub fn list_recent(&self, limit: i64) -> rusqlite::Result<Vec<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions ORDER BY id DESC LIMIT ?1",
            SESSION_COLUMNS
        ))?;
        let rows = stmt.query_map(params![limit], parse_session)?;
        rows.collect()
    }

    /// 最近 N 条（Profile 直查）。
    pub fn list_recent_by_profile(
        &self,
        profile_id: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions ss
             WHERE ss.profile_id = ?1
             ORDER BY ss.id DESC LIMIT ?2",
            cols("ss")
        ))?;
        let rows = stmt.query_map(params![profile_id, limit], parse_session)?;
        rows.collect()
    }

    /// 是否存在进行中 Session（切换档案前安全检查；全局唯一）。
    pub fn has_active_session(&self) -> rusqlite::Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// 某档案某天的全部 Session。
    pub fn list_by_date_by_profile(
        &self,
        profile_id: i64,
        date: &str,
    ) -> rusqlite::Result<Vec<StudySession>> {
        self.list_by_range_by_profile(profile_id, date, date)
    }

    /// 某档案日期范围 [start, end] 的全部 Session。
    pub fn list_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<StudySession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM study_sessions ss
             WHERE ss.profile_id = ?1
               AND date(ss.started_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY ss.id",
            cols("ss")
        ))?;
        let rows = stmt.query_map(params![profile_id, start, end], parse_session)?;
        rows.collect()
    }
}

fn parse_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudySession> {
    Ok(StudySession {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        task_id: row.get(3)?,
        learning_item_id: row.get(4)?,
        title: row.get(5)?,
        started_at: row.get(6)?,
        ended_at: row.get(7)?,
        duration_seconds: row.get(8)?,
        status: row.get(9)?,
        note: row.get(10)?,
        note_document_json: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        time_corrected: row.get(14)?,
        activity_kind: row.get(15)?,
        duration_review_state: row.get(16)?,
    })
}
