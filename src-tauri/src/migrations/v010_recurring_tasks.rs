/// V10 Recurring Tasks + Task 时间字段（BATCH-03 / DEV-0026）。
///
/// - recurring_task_rules：重复任务规则（daily / weekly；Profile 经 Goal 链追踪）
/// - tasks.planned_time：任务计划时间（Higher 内语义；无系统通知）
/// - tasks.recurring_rule_id：生成该 Task 的规则（可空；无 FK，避免重建 tasks 表——
///   关系由 Repository Guard + index 保证）
///
/// 安全：不删除/不清空任何既有数据；旧 Task 两新列均为 NULL。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS recurring_task_rules (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id          INTEGER NOT NULL,
            learning_item_id INTEGER NOT NULL,
            title            TEXT NOT NULL,
            repeat_type      TEXT NOT NULL,
            weekdays_json    TEXT NOT NULL DEFAULT '[]',
            time_of_day      TEXT,
            start_date       TEXT NOT NULL,
            end_date         TEXT,
            enabled          INTEGER NOT NULL DEFAULT 1,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_recurring_rules_goal ON recurring_task_rules(goal_id);

        ALTER TABLE tasks ADD COLUMN planned_time TEXT;
        ALTER TABLE tasks ADD COLUMN recurring_rule_id INTEGER;
        CREATE INDEX IF NOT EXISTS idx_tasks_rule_date ON tasks(recurring_rule_id, planned_date);",
    )
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // SQLite 不支持 DROP COLUMN（旧版本）；down 仅在测试场景使用
    let _ = conn.execute_batch("DROP TABLE IF EXISTS recurring_task_rules;");
    Ok(())
}
