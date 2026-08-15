use rusqlite::Connection;

/// V4 Evaluation System：evaluations 表。
///
/// 建立 Evaluation System V1 通用验证记录模型：
/// - Evaluation = 一次验证的记录（练习/测试/回忆/应用/其他）
/// - goal_id 必须存在；learning_item_id 可选（全科模拟考试等可空）
/// - 题目数量指标（total/correct/incorrect_items）可空；分数指标（score/max_score）可空
/// - outcome（unrated/passed/partial/failed）是用户对本次的简单结论，不是 mastery_status
/// - note 是普通备注，不引入 Markdown/富文本/附件
///
/// 外键策略：
/// - 删除 Goal → 级联删除其 Evaluation（用户主动删 Goal 允许清相关验证）
/// - 删除 Learning Item → RESTRICT（防止验证记录因节点删除失去语义）
///   实际删除路径必须走 LearningItemRepository::safe_delete（先查 + 给用户可读错误），
///   此处 FK RESTRICT 是额外兜底避免绕过 Repository 的误删。
///
/// 索引：goal_id / learning_item_id / occurred_at，避免历史量大后全表扫描。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS evaluations (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id         INTEGER NOT NULL,
            learning_item_id INTEGER,
            title           TEXT NOT NULL,
            evaluation_type TEXT NOT NULL,
            source          TEXT,
            occurred_at     TEXT NOT NULL DEFAULT (datetime('now')),
            total_items     INTEGER,
            correct_items   INTEGER,
            incorrect_items INTEGER,
            score           REAL,
            max_score       REAL,
            outcome         TEXT NOT NULL DEFAULT 'unrated',
            note            TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE RESTRICT
        );

        CREATE INDEX IF NOT EXISTS idx_evaluations_goal_id         ON evaluations(goal_id);
        CREATE INDEX IF NOT EXISTS idx_evaluations_learning_item_id ON evaluations(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_evaluations_occurred_at     ON evaluations(occurred_at);",
    )
}
