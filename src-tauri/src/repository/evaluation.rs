use rusqlite::{params, Connection};

/// 学习验证记录：一次练习 / 测试 / 回忆 / 应用 / 其他的验证结果。
///
/// 只记录事实（发生了什么验证 + 结果是什么），不解释原因（Feedback System），不做自动调整（Adjustment System）。
///
/// 关键可空设计：
/// - learning_item_id 可空（全科模拟 / 阶段综合 / 学习方法验证等不绑定到特定节点）
/// - total_items / correct_items / incorrect_items 全部可空（回忆/应用等验证不一定有题数）
/// - score / max_score 全部可空（非打分型验证可不填）
///
/// evaluation_type 枚举：practice / test / recall / application / other
/// outcome 枚举：unrated / passed / partial / failed
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Evaluation {
    pub id: i64,
    /// v013 起 Profile 直挂
    #[serde(default)]
    pub profile_id: i64,
    /// v013 起可空（Goal = 可选规划上下文）
    #[serde(default)]
    pub goal_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub title: String,
    pub evaluation_type: String,
    pub source: Option<String>,
    pub occurred_at: String,
    pub total_items: Option<i64>,
    pub correct_items: Option<i64>,
    pub incorrect_items: Option<i64>,
    pub score: Option<f64>,
    pub max_score: Option<f64>,
    pub outcome: String,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct EvaluationRepository<'a> {
    conn: &'a Connection,
}

/// 档案内验证统计（按类型 / 按结果的真实计数）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EvaluationStats {
    pub by_type: Vec<super::CountPair>,
    pub by_outcome: Vec<super::CountPair>,
}

impl<'a> EvaluationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建 Evaluation。
    ///
    /// 校验：
    /// - 若 learning_item_id 存在，必须属于指定 goal_id（跨 Goal 拒绝）
    /// - 题数校验（空跳过）：total >= 0 / correct >= 0 / incorrect >= 0 / correct + incorrect <= total
    /// - 分数校验（空跳过）：score >= 0；若 max_score 存在则 max_score > 0 且 score <= max_score
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        evaluation_type: &str,
        source: Option<&str>,
        occurred_at: Option<&str>,
        total_items: Option<i64>,
        correct_items: Option<i64>,
        incorrect_items: Option<i64>,
        score: Option<f64>,
        max_score: Option<f64>,
        outcome: Option<&str>,
        note: Option<&str>,
    ) -> rusqlite::Result<Evaluation> {
        // 跨档案防护：learning_item 若存在，其 profile 必须一致
        if let Some(item_id) = learning_item_id {
            let item_profile: i64 = self.conn.query_row(
                "SELECT profile_id FROM learning_items WHERE id = ?1",
                params![item_id],
                |row| row.get(0),
            ).map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => rusqlite::Error::InvalidParameterName(
                    format!("learning_item_id {} 不存在", item_id)
                ),
                other => other,
            })?;
            if item_profile != profile_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "跨档案验证被拒绝：所选知识不属于当前学习档案".to_string(),
                ));
            }
        }

        Self::validate_counts_and_scores(
            total_items, correct_items, incorrect_items, score, max_score,
        )?;

        let outcome = outcome.unwrap_or("unrated");

        if occurred_at.is_some() {
            self.conn.execute(
                "INSERT INTO evaluations (
                    profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
                )",
                params![
                    profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note
                ],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO evaluations (
                    profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7, ?8, ?9, ?10, ?11, ?12, ?13
                )",
                params![
                    profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note
                ],
            )?;
        }

        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 按 ID 获取 Evaluation。
    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note, created_at, updated_at
             FROM evaluations WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| parse_evaluation(row))?;
        rows.next().transpose()
    }

    /// 最近 N 条 Evaluation（跨 Goal，按 occurred_at DESC）。
    pub fn list_recent(&self, limit: i64) -> rusqlite::Result<Vec<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note, created_at, updated_at
             FROM evaluations ORDER BY occurred_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| parse_evaluation(row))?;
        rows.collect()
    }

    /// 最近 N 条 Evaluation（Profile Scope，通过 JOIN goals 过滤）。
    pub fn list_recent_by_profile(
        &self,
        profile_id: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.profile_id, e.goal_id, e.learning_item_id, e.title, e.evaluation_type, e.source,
                    e.occurred_at, e.total_items, e.correct_items, e.incorrect_items,
                    e.score, e.max_score, e.outcome, e.note, e.created_at, e.updated_at
             FROM evaluations e WHERE e.profile_id = ?1
             ORDER BY e.occurred_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, limit], |row| parse_evaluation(row))?;
        rows.collect()
    }

    /// 指定档案在某一天（YYYY-MM-DD，按 occurred_at 归日）的全部 Evaluation。
    /// 用于学习复盘按天聚合（Profile Scope）。
    pub fn list_by_date_by_profile(
        &self,
        profile_id: i64,
        date: &str,
    ) -> rusqlite::Result<Vec<Evaluation>> {
        self.list_by_range_by_profile(profile_id, date, date)
    }

    /// 指定档案在日期范围 [start, end]（按 occurred_at 归日）的全部 Evaluation。
    pub fn list_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.profile_id, e.goal_id, e.learning_item_id, e.title, e.evaluation_type, e.source,
                    e.occurred_at, e.total_items, e.correct_items, e.incorrect_items,
                    e.score, e.max_score, e.outcome, e.note, e.created_at, e.updated_at
             FROM evaluations e WHERE e.profile_id = ?1 AND date(e.occurred_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY e.occurred_at",
        )?;
        let rows = stmt.query_map(params![profile_id, start, end], |row| parse_evaluation(row))?;
        rows.collect()
    }

    /// 档案内验证统计（按类型 / 按结果的真实计数，整体进度页用）。
    pub fn stats_by_profile(&self, profile_id: i64) -> rusqlite::Result<EvaluationStats> {
        let mut by_type = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT e.evaluation_type, COUNT(*)
                 FROM evaluations e WHERE e.profile_id = ?1
                 GROUP BY e.evaluation_type",
            )?;
            let rows = stmt.query_map(params![profile_id], |row| {
                Ok(super::CountPair {
                    label: row.get(0)?,
                    count: row.get(1)?,
                })
            })?;
            for r in rows {
                by_type.push(r?);
            }
        }
        let mut by_outcome = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT e.outcome, COUNT(*)
                 FROM evaluations e WHERE e.profile_id = ?1
                 GROUP BY e.outcome",
            )?;
            let rows = stmt.query_map(params![profile_id], |row| {
                Ok(super::CountPair {
                    label: row.get(0)?,
                    count: row.get(1)?,
                })
            })?;
            for r in rows {
                by_outcome.push(r?);
            }
        }
        Ok(EvaluationStats { by_type, by_outcome })
    }

    /// 按 Goal 列出 Evaluation（occurred_at DESC）。
    pub fn list_by_goal(&self, goal_id: i64) -> rusqlite::Result<Vec<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note, created_at, updated_at
             FROM evaluations WHERE goal_id = ?1 ORDER BY occurred_at DESC",
        )?;
        let rows = stmt.query_map(params![goal_id], |row| parse_evaluation(row))?;
        rows.collect()
    }

    /// 按 Learning Item 列出 Evaluation（occurred_at DESC）。
    pub fn list_by_learning_item(&self, learning_item_id: i64) -> rusqlite::Result<Vec<Evaluation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, goal_id, learning_item_id, title, evaluation_type, source,
                    occurred_at, total_items, correct_items, incorrect_items,
                    score, max_score, outcome, note, created_at, updated_at
             FROM evaluations WHERE learning_item_id = ?1 ORDER BY occurred_at DESC",
        )?;
        let rows = stmt.query_map(params![learning_item_id], |row| parse_evaluation(row))?;
        rows.collect()
    }

    /// 更新 Evaluation 内容字段。
    ///
    /// V1 不允许修改 goal_id / learning_item_id 关联（§32：修改关联需重新做跨 Goal 校验，
    /// 为了简单和安全 V1 只改内容字段，关联如误填可删除重录）。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: i64,
        title: &str,
        evaluation_type: &str,
        source: Option<&str>,
        occurred_at: &str,
        total_items: Option<i64>,
        correct_items: Option<i64>,
        incorrect_items: Option<i64>,
        score: Option<f64>,
        max_score: Option<f64>,
        outcome: &str,
        note: Option<&str>,
    ) -> rusqlite::Result<()> {
        Self::validate_counts_and_scores(
            total_items, correct_items, incorrect_items, score, max_score,
        )?;

        self.conn.execute(
            "UPDATE evaluations SET
                title = ?1, evaluation_type = ?2, source = ?3,
                occurred_at = ?4,
                total_items = ?5, correct_items = ?6, incorrect_items = ?7,
                score = ?8, max_score = ?9, outcome = ?10, note = ?11,
                updated_at = datetime('now')
             WHERE id = ?12",
            params![
                title, evaluation_type, source, occurred_at,
                total_items, correct_items, incorrect_items,
                score, max_score, outcome, note, id
            ],
        )?;
        Ok(())
    }

    /// 删除 Evaluation（仅用户明确操作）。
    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM evaluations WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 校验题数与分数指标（create / update 共用）。
    ///
    /// 为空的字段跳过校验。
    fn validate_counts_and_scores(
        total_items: Option<i64>,
        correct_items: Option<i64>,
        incorrect_items: Option<i64>,
        score: Option<f64>,
        max_score: Option<f64>,
    ) -> rusqlite::Result<()> {
        // 题数校验
        if let Some(t) = total_items {
            if t < 0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "total_items 不能为负数（当前 {}）", t
                )));
            }
            let c = correct_items.unwrap_or(0);
            let i = incorrect_items.unwrap_or(0);
            if c < 0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "correct_items 不能为负数（当前 {}）", c
                )));
            }
            if i < 0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "incorrect_items 不能为负数（当前 {}）", i
                )));
            }
            if c + i > t {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "题数校验失败：correct({}) + incorrect({}) > total({})",
                    c, i, t
                )));
            }
        } else {
            // 没有 total_items 时，单独填写 correct/incorrect 也不能为负
            if let Some(c) = correct_items {
                if c < 0 {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "correct_items 不能为负数（当前 {}）", c
                    )));
                }
            }
            if let Some(i) = incorrect_items {
                if i < 0 {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "incorrect_items 不能为负数（当前 {}）", i
                    )));
                }
            }
        }

        // 分数校验
        if let Some(s) = score {
            if s < 0.0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "score 不能为负数（当前 {}）", s
                )));
            }
            if let Some(m) = max_score {
                if m <= 0.0 {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "max_score 必须大于 0（当前 {}）", m
                    )));
                }
                if s > m {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "分数校验失败：score({}) > max_score({})", s, m
                    )));
                }
            }
        } else if let Some(m) = max_score {
            if m <= 0.0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "max_score 必须大于 0（当前 {}）", m
                )));
            }
        }

        Ok(())
    }
}

fn parse_evaluation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Evaluation> {
    Ok(Evaluation {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        title: row.get(4)?,
        evaluation_type: row.get(5)?,
        source: row.get(6)?,
        occurred_at: row.get(7)?,
        total_items: row.get(8)?,
        correct_items: row.get(9)?,
        incorrect_items: row.get(10)?,
        score: row.get(11)?,
        max_score: row.get(12)?,
        outcome: row.get(13)?,
        note: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}
