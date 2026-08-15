/// V11 Task Lifecycle（BATCH-03.1 / DEV-0031）。
///
/// 背景（实测）：v002 的 tasks.learning_item_id 为 NOT NULL，SQLite 无法安全
/// 对既有表 DROP NOT NULL；且 Task 归档需要显式状态。本 Migration 是任务书
/// §六授权的唯一 v011，仅解决两件事：
/// 1. tasks.learning_item_id → 可空（title-only Task；永久产品规则 §110）
/// 2. tasks.archived_at TEXT NULL（归档时间；NULL=活跃）
///
/// 实现方式（零数据破坏）：新建 tasks_new → INSERT SELECT 复制全部现有行
/// （含 id，保持外键引用不变）→ DROP 旧表 → 改名 → 重建索引。
/// 全程在 Migration 事务内；study_sessions.task_id 引用按 id 保持有效。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE tasks_new (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id           INTEGER NOT NULL,
            learning_item_id  INTEGER,
            title             TEXT NOT NULL,
            planned_date      TEXT,
            planned_time      TEXT,
            status            TEXT NOT NULL DEFAULT 'pending',
            archived_at       TEXT,
            plan_id           INTEGER,
            recurring_rule_id INTEGER,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)           REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id)  REFERENCES learning_items(id) ON DELETE CASCADE,
            FOREIGN KEY (plan_id)           REFERENCES plans(id)          ON DELETE SET NULL
        );

        INSERT INTO tasks_new
            (id, goal_id, learning_item_id, title, planned_date, planned_time, status,
             plan_id, recurring_rule_id, created_at, updated_at)
        SELECT t.id,
               li.goal_id,
               t.learning_item_id,
               t.title,
               t.planned_date,
               t.planned_time,
               t.status,
               t.plan_id,
               t.recurring_rule_id,
               t.created_at,
               t.updated_at
        FROM tasks t
        JOIN learning_items li ON t.learning_item_id = li.id;

        DROP TABLE tasks;
        ALTER TABLE tasks_new RENAME TO tasks;

        CREATE INDEX IF NOT EXISTS idx_tasks_goal ON tasks(goal_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_item ON tasks(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_rule_date ON tasks(recurring_rule_id, planned_date);
        CREATE INDEX IF NOT EXISTS idx_tasks_date ON tasks(planned_date);",
    )
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // down 仅测试场景；不可逆时保持现状（up 已事务保护）
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
