use rusqlite::{params, Connection};

/// 学习对象：用户具体正在学习的对象（如"高等数学"、"极限"、"Linux 进程调度"）。
///
/// 通过 `parent_id` 形成任意层级树：
/// - 根节点 `parent_id = NULL`
/// - 子节点 `parent_id` 指向同一 Goal 下的某个父节点
///
/// mastery_status 为简单可解释状态，不使用百分比掌握度。
/// content 为用户自己的知识正文（自由文本，DEV-0010 知识体系工作区）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LearningItem {
    pub id: i64,
    /// v013 起 Profile 直挂
    #[serde(default)]
    pub profile_id: i64,
    /// v013 起可空（Goal = 可选规划上下文，不是权限门）
    #[serde(default)]
    pub goal_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub mastery_status: String,
    /// 手动排序（v012 / DEV-0305）：同一父级下兄弟顺序；默认 0 回退 id 排序
    #[serde(default)]
    pub sort_order: i64,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 知识节点学习数据概览（自动从 StudySession / Evaluation 聚合，用户不能填写）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeNodeStats {
    pub study_seconds: i64,
    pub session_count: i64,
    pub evaluation_count: i64,
    pub last_studied_at: Option<String>,
}

pub struct LearningItemRepository<'a> {
    conn: &'a Connection,
}

impl<'a> LearningItemRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Profile First 创建：profile_id 必填；goal/parent 可选。
    /// parent 存在时校验与 profile 一致（跨 Profile 拒绝）。
    pub fn create_for_profile(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        name: &str,
        description: Option<&str>,
        parent_id: Option<i64>,
    ) -> rusqlite::Result<LearningItem> {
        if let Some(pid) = parent_id {
            let parent_profile: i64 = self
                .conn
                .query_row(
                    "SELECT profile_id FROM learning_items WHERE id = ?1",
                    params![pid],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    rusqlite::Error::InvalidParameterName(format!("parent_id {} 不存在", pid))
                })?;
            if parent_profile != profile_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "跨档案 parent 被拒绝：父节点不属于当前学习档案".to_string(),
                ));
            }
        }
        self.conn.execute(
            "INSERT INTO learning_items (profile_id, goal_id, parent_id, name, description)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![profile_id, goal_id, parent_id, name, description],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 兼容旧调用：create_root(goal_id, ...)（内部经 goal 解析 profile）。
    pub fn create_root(
        &self,
        goal_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> rusqlite::Result<LearningItem> {
        let profile_id = self.profile_of_goal(goal_id)?;
        self.create_for_profile(profile_id, Some(goal_id), name, description, None)
    }

    /// 兼容旧调用：create_child(goal_id, parent_id, ...)。
    pub fn create_child(
        &self,
        goal_id: i64,
        parent_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> rusqlite::Result<LearningItem> {
        let profile_id = self.profile_of_goal(goal_id)?;
        self.create_for_profile(profile_id, Some(goal_id), name, description, Some(parent_id))
    }

    fn profile_of_goal(&self, goal_id: i64) -> rusqlite::Result<i64> {
        self.conn
            .query_row(
                "SELECT profile_id FROM goals WHERE id = ?1",
                params![goal_id],
                |r| r.get(0),
            )
            .map_err(|_| rusqlite::Error::InvalidParameterName(format!("goal {} 不存在", goal_id)))
    }

    /// 通用创建（保留旧接口兼容）：若 parent_id 为 None 创建根节点，否则创建子节点并校验。
    pub fn create(
        &self,
        goal_id: i64,
        name: &str,
        description: Option<&str>,
        parent_id: Option<i64>,
    ) -> rusqlite::Result<LearningItem> {
        match parent_id {
            None => self.create_root(goal_id, name, description),
            Some(pid) => self.create_child(goal_id, pid, name, description),
        }
    }

    /// 列出全部 Learning Item（不分 Goal，旧接口保留）。
    pub fn list(&self) -> rusqlite::Result<Vec<LearningItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, parent_id, name, description, mastery_status, sort_order, content, created_at, updated_at
             FROM learning_items ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| parse_learning_item(row))?;
        rows.collect()
    }

    /// 列出指定 Goal 下的全部 Learning Item（不分层级，由前端组装树）。
    pub fn list_by_goal(&self, goal_id: i64) -> rusqlite::Result<Vec<LearningItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, parent_id, name, description, mastery_status, sort_order, content, created_at, updated_at
             FROM learning_items WHERE goal_id = ?1 ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![goal_id], |row| parse_learning_item(row))?;
        rows.collect()
    }

    /// 列出指定档案下的全部 Learning Item（Profile Scope，通过 JOIN goals 过滤）。
    pub fn list_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<LearningItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT li.id, li.profile_id, li.goal_id, li.parent_id, li.name, li.description, li.mastery_status, li.sort_order, li.content, li.created_at, li.updated_at
             FROM learning_items li
             WHERE li.profile_id = ?1
             ORDER BY li.id",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| parse_learning_item(row))?;
        rows.collect()
    }

    /// 获取某节点的直接子节点列表（仅一层，按 id 升序）。
    pub fn get_children(&self, parent_id: i64) -> rusqlite::Result<Vec<LearningItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, parent_id, name, description, mastery_status, sort_order, content, created_at, updated_at
             FROM learning_items WHERE parent_id = ?1 ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![parent_id], |row| parse_learning_item(row))?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<LearningItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, parent_id, name, description, mastery_status, sort_order, content, created_at, updated_at
             FROM learning_items WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| parse_learning_item(row))?;
        rows.next().transpose()
    }

    /// 更新掌握状态（用户主动调整，非自动算法）。
    pub fn update_status(&self, id: i64, mastery_status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE learning_items SET mastery_status = ?1, updated_at = datetime('now')
             WHERE id = ?2",
            params![mastery_status, id],
        )?;
        Ok(())
    }

    /// 更新 Learning Item 名称 / 描述（不改变 id / goal_id / parent_id）。
    pub fn update(
        &self,
        id: i64,
        name: &str,
        description: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE learning_items SET name = ?1, description = ?2, updated_at = datetime('now')
             WHERE id = ?3",
            params![name, description, id],
        )?;
        Ok(())
    }

    /// 更新知识正文（独立接口：自动保存高频调用，不重复提交 name / description / mastery_status）。
    pub fn update_content(&self, id: i64, content: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE learning_items SET content = ?1, updated_at = datetime('now')
             WHERE id = ?2",
            params![content, id],
        )?;
        Ok(())
    }

    /// 知识节点学习数据概览：从 StudySession / Evaluation 自动聚合（用户不能填写）。
    /// study_seconds 只统计已完成 Session 的真实时长；last_studied_at 为最近一次 Session 开始时间。
    pub fn stats(&self, id: i64) -> rusqlite::Result<KnowledgeNodeStats> {
        let (study_seconds, session_count, last_studied_at): (i64, i64, Option<String>) =
            self.conn.query_row(
                "SELECT COALESCE(SUM(duration_seconds), 0), COUNT(*), MAX(started_at)
                 FROM study_sessions WHERE learning_item_id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        let evaluation_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM evaluations WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(KnowledgeNodeStats {
            study_seconds,
            session_count,
            evaluation_count,
            last_studied_at,
        })
    }

    /// 档案内知识掌握状态分布（Profile 直查）。
    pub fn status_counts_by_profile(
        &self,
        profile_id: i64,
    ) -> rusqlite::Result<Vec<super::CountPair>> {
        let mut stmt = self.conn.prepare(
            "SELECT mastery_status, COUNT(*)
             FROM learning_items
             WHERE profile_id = ?1
             GROUP BY mastery_status",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(super::CountPair {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// 安全删除 Learning Item。
    ///
    /// 仅当节点没有子项、没有 Task、没有 Study Session 时才真正删除。
    /// 否则返回 Err，提示用户存在关联数据。
    /// 不做级联危险删除。
    /// content 本身不阻止删除（由前端在 content 非空时二次确认）。
    pub fn safe_delete(&self, id: i64) -> rusqlite::Result<()> {
        // 检查子节点
        let child_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM learning_items WHERE parent_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if child_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该知识节点仍存在 {} 个子项，无法直接删除",
                child_count
            )));
        }

        // 检查 Task
        let task_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if task_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该知识节点仍存在 {} 个任务，无法直接删除",
                task_count
            )));
        }

        // 检查 Study Session
        let session_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if session_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该知识节点仍存在 {} 条学习记录，无法直接删除",
                session_count
            )));
        }

        // 检查 Evaluation（DEV-0007 新增）
        let ev_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM evaluations WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if ev_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该知识节点已有 {} 条验证记录，无法直接删除",
                ev_count
            )));
        }

        // 检查附件（DEV-0018）：明确策略——含附件的节点需先处理附件，避免磁盘文件成孤儿
        let att_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM learning_attachments WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if att_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该知识节点已有 {} 个附件，请先删除附件",
                att_count
            )));
        }

        // 检查 Knowledge Documents（DEV-0051 §51：长期知识文档禁误删）
        let doc_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM knowledge_documents WHERE learning_item_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if doc_count > 0 {
            return Err(rusqlite::Error::InvalidParameterName(
                "该知识节点仍包含文档，请先处理文档后再删除。".to_string(),
            ));
        }

        self.conn.execute(
            "DELETE FROM learning_items WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 移动节点：new_parent_id = None 表示移为根节点。
    ///
    /// 拒绝：移动到自己 / 移动到自己的后代 / 跨 Profile / 目标不存在。
    pub fn move_item(&self, id: i64, new_parent_id: Option<i64>) -> Result<(), String> {
        let item = self
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("知识节点不存在")?;
        if let Some(np) = new_parent_id {
            if np == id {
                return Err("不能移动到自己下面".to_string());
            }
            let parent = self
                .get(np)
                .map_err(|e| e.to_string())?
                .ok_or("目标节点不存在")?;
            if parent.profile_id != item.profile_id {
                return Err("不能移动到其他学习档案的知识下".to_string());
            }
            // 后代检测：沿 parent 链向上若遇到 id → new_parent 是 id 的后代
            let mut cur = Some(np);
            let mut depth = 0u32;
            while let Some(cid) = cur {
                if cid == id {
                    return Err("不能移动到自己的子节点下面".to_string());
                }
                if depth >= 100 {
                    return Err("层级过深，已拒绝".to_string());
                }
                cur = self
                    .get(cid)
                    .map_err(|e| e.to_string())?
                    .and_then(|n| n.parent_id);
                depth += 1;
            }
        }
        self.conn
            .execute(
                "UPDATE learning_items SET parent_id = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![new_parent_id, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 计算 Learning Item 的完整层级路径（如 "数学 > 高等数学 > 极限"）。
    ///
    /// 通过 parent_id 链向上遍历，不冗余存储路径字符串。
    /// 防御性循环检测：最多遍历 100 层，超过则截断。
    pub fn get_full_path(&self, id: i64) -> rusqlite::Result<String> {
        let mut names: Vec<String> = Vec::new();
        let mut current_id = Some(id);
        let mut depth = 0u32;

        while let Some(cid) = current_id {
            if depth >= 100 {
                break; // 防御性：防止循环引用导致死循环
            }
            let row: Option<(String, Option<i64>)> = self
                .conn
                .query_row(
                    "SELECT name, parent_id FROM learning_items WHERE id = ?1",
                    params![cid],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .ok();
            match row {
                Some((name, parent_id)) => {
                    names.push(name);
                    current_id = parent_id;
                }
                None => break,
            }
            depth += 1;
        }

        names.reverse();
        Ok(names.join(" > "))
    }

    /// 手动排序（DEV-0305 §D）：批量写入同一父级下的兄弟顺序。
    /// ordered_ids 必须全部属于同一 Goal；父级一致性由调用方（前端树）保证。
    pub fn reorder_siblings(&self, ordered_ids: &[i64]) -> Result<(), String> {
        for (idx, id) in ordered_ids.iter().enumerate() {
            self.conn
                .execute(
                    "UPDATE learning_items SET sort_order = ?2, updated_at = datetime('now')
                     WHERE id = ?1",
                    params![id, idx as i64],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// 行解析辅助函数（统一 11 列顺序：id, profile_id, goal_id, parent_id, name, description, mastery_status, sort_order, content, created_at, updated_at）。
fn parse_learning_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningItem> {
    Ok(LearningItem {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        parent_id: row.get(3)?,
        name: row.get(4)?,
        description: row.get(5)?,
        mastery_status: row.get(6)?,
        sort_order: row.get(7)?,
        content: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}
