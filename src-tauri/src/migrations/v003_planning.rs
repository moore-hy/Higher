use rusqlite::Connection;

/// V3 Planning System：study_stages / plans 表 + tasks.plan_id。
///
/// 建立 Planning System 最小数据模型：
/// - StudyStage：Goal 下的学习阶段（基础/强化/真题/冲刺，名称由用户自定义）
/// - Plan：阶段内的学习计划，可关联 Goal / Stage / Learning Item
/// - tasks.plan_id：Task 可选关联 Plan（保持轻量，plan_id 可空）
///
/// 外键级联策略：
/// - 删除 Goal → 级联删除其 Stage → 级联删除其 Plan
/// - 删除 Stage → 其 Plan 的 stage_id 置空（保留 Plan，允许无阶段计划）
/// - 删除 Learning Item → 其 Plan 的 learning_item_id 置空（保留 Plan）
/// - 删除 Plan → 其 Task 的 plan_id 置空（保留 Task，不破坏历史执行）
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS study_stages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id     INTEGER NOT NULL,
            name        TEXT NOT NULL,
            description TEXT,
            start_date  TEXT,
            end_date    TEXT,
            status      TEXT NOT NULL DEFAULT 'active',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS plans (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id          INTEGER NOT NULL,
            stage_id         INTEGER,
            learning_item_id INTEGER,
            title            TEXT NOT NULL,
            description      TEXT,
            start_date       TEXT,
            end_date         TEXT,
            status           TEXT NOT NULL DEFAULT 'active',
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (stage_id)         REFERENCES study_stages(id)   ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE SET NULL
        );

        ALTER TABLE tasks ADD COLUMN plan_id INTEGER REFERENCES plans(id) ON DELETE SET NULL;",
    )
}
