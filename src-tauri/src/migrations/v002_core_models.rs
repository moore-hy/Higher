use rusqlite::Connection;

/// V2 核心业务模型：goals / learning_items / tasks / study_sessions。
///
/// 保持通用学习管理结构，不含任何特定学科（考研/数学/英语等）固定字段。
/// 学科内容只能由用户作为数据创建。
///
/// 外键级联策略：
/// - 删除 Goal → 级联删除其 Learning Item → 级联删除其 Task → Session 的 task_id 置空
/// - 删除 Learning Item → 级联删除其 Task 与 Study Session
/// - 删除 Task → 其 Session 的 task_id 置空（保留学习记录本身）
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS goals (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            description TEXT,
            status      TEXT NOT NULL DEFAULT 'active',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS learning_items (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id        INTEGER NOT NULL,
            parent_id      INTEGER,
            name           TEXT NOT NULL,
            description    TEXT,
            mastery_status TEXT NOT NULL DEFAULT 'not_started',
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)   REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (parent_id) REFERENCES learning_items(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            learning_item_id INTEGER NOT NULL,
            title            TEXT NOT NULL,
            planned_date     TEXT,
            status           TEXT NOT NULL DEFAULT 'pending',
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS study_sessions (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id          INTEGER,
            learning_item_id INTEGER NOT NULL,
            started_at       TEXT NOT NULL,
            ended_at         TEXT,
            duration_seconds INTEGER,
            status           TEXT NOT NULL DEFAULT 'active',
            note             TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (task_id)          REFERENCES tasks(id)          ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE
        );",
    )
}
