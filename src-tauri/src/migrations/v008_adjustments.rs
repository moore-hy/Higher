use rusqlite::Connection;

/// V8 Adjustment System：adjustments 表。
///
/// Adjustment = 发现问题后"我准备怎么改变下一步学习"的调整决策。
/// 架构原则：Adjustment 绝不成为第二套 Task 系统——
/// - 真正执行仍用 Task / Plan / StudySession
/// - Adjustment 只记录"为什么这次计划/任务发生改变"及与 Feedback / Task 的关系
/// - 创建 relearn/practice 类调整时同时创建正式 Task（一事务两记录，见 command 层）
/// - 不自动 resolve Feedback（安排重新学习 ≠ 问题已解决）
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS adjustments (
            id                INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            feedback_id       INTEGER NOT NULL,
            goal_id           INTEGER NOT NULL,
            learning_item_id  INTEGER,
            adjustment_type   TEXT NOT NULL,
            title             TEXT NOT NULL,
            note              TEXT NOT NULL DEFAULT '',
            status            TEXT NOT NULL DEFAULT 'planned',
            target_date       TEXT,
            task_id           INTEGER,
            plan_id           INTEGER,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at      TEXT,
            FOREIGN KEY (feedback_id) REFERENCES feedbacks(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE SET NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE SET NULL,
            FOREIGN KEY (plan_id) REFERENCES plans(id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_adjustments_feedback_id ON adjustments(feedback_id);
        CREATE INDEX IF NOT EXISTS idx_adjustments_goal_id ON adjustments(goal_id);
        CREATE INDEX IF NOT EXISTS idx_adjustments_status ON adjustments(status);",
    )?;
    Ok(())
}
