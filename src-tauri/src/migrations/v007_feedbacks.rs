use rusqlite::Connection;

/// V7 Feedback System：feedbacks 表。
///
/// Feedback = 从真实学习 Evidence 中暴露出来、被用户确认值得后续处理的问题。
/// - 严禁 failed Evaluation 自动创建 Feedback（必须用户确认）
/// - goal_id NOT NULL（Profile 归属链：Feedback → Goal → Profile）
/// - learning_item_id / evaluation_id 可选关联（同 Goal 校验在 Repository 层）
/// - status: open / resolved / dismissed（不物理删除历史）
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feedbacks (
            id                INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            goal_id           INTEGER NOT NULL,
            learning_item_id  INTEGER,
            evaluation_id     INTEGER,
            feedback_type     TEXT NOT NULL,
            title             TEXT NOT NULL,
            description       TEXT NOT NULL DEFAULT '',
            status            TEXT NOT NULL DEFAULT 'open',
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at       TEXT,
            FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE SET NULL,
            FOREIGN KEY (evaluation_id) REFERENCES evaluations(id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedbacks_goal_id ON feedbacks(goal_id);
        CREATE INDEX IF NOT EXISTS idx_feedbacks_learning_item_id ON feedbacks(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_feedbacks_evaluation_id ON feedbacks(evaluation_id);
        CREATE INDEX IF NOT EXISTS idx_feedbacks_status ON feedbacks(status);",
    )?;
    Ok(())
}
