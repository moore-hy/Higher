/// V12 UX Convergence（BATCH-03.2 / DEV-0304+0305）。
///
/// 背景任务书授权（§六.2 数据迁移）：为支持「先学，再归档」与「知识树手动排序」：
/// 1. study_sessions：learning_item_id → 可空（快速学习不预选知识；结束时再归档），
///    新增 goal_id NOT NULL（Profile 直属，title-only 学习也能隔离）
/// 2. learning_items：新增 sort_order INTEGER NOT NULL DEFAULT 0（拖拽手动排序；
///    未设置时回退 id 排序保证旧行为不变）
/// 3. learning_attachments：learning_item_id → 可空（快速学习中粘贴/上传媒体的
///    附件可先挂在 Session 上，归档时随 Session 关联知识）
///
/// 实现（零破坏，事务内）：重建表 + INSERT SELECT 保留全部行（id 不变）；
/// learning_items 直接 ADD COLUMN。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE study_sessions_new (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id           INTEGER NOT NULL,
            learning_item_id  INTEGER,
            task_id           INTEGER,
            started_at        TEXT NOT NULL DEFAULT (datetime('now')),
            ended_at          TEXT,
            duration_seconds  INTEGER,
            status            TEXT NOT NULL DEFAULT 'active',
            note              TEXT,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE,
            FOREIGN KEY (task_id)          REFERENCES tasks(id)          ON DELETE SET NULL
        );

        INSERT INTO study_sessions_new
            (id, goal_id, learning_item_id, task_id, started_at, ended_at,
             duration_seconds, status, note, created_at, updated_at)
        SELECT ss.id,
               li.goal_id,
               ss.learning_item_id,
               ss.task_id,
               ss.started_at,
               ss.ended_at,
               ss.duration_seconds,
               ss.status,
               ss.note,
               ss.created_at,
               ss.updated_at
        FROM study_sessions ss
        JOIN learning_items li ON ss.learning_item_id = li.id;

        DROP TABLE study_sessions;
        ALTER TABLE study_sessions_new RENAME TO study_sessions;

        CREATE INDEX IF NOT EXISTS idx_sessions_item ON study_sessions(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_task ON study_sessions(task_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_started ON study_sessions(started_at);

        -- learning_attachments：learning_item_id → 可空（快速学习 Session 附件）
        -- 注：v009 原表无 goal_id / updated_at，此处保持同构仅放开 item 可空。
        CREATE TABLE learning_attachments_new (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            learning_item_id INTEGER,
            session_id       INTEGER,
            attachment_type  TEXT NOT NULL CHECK (attachment_type IN ('image','video','drawing','file')),
            file_name        TEXT NOT NULL,
            relative_path    TEXT NOT NULL,
            mime_type        TEXT,
            caption          TEXT NOT NULL DEFAULT '',
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE,
            FOREIGN KEY (session_id)       REFERENCES study_sessions(id) ON DELETE SET NULL
        );

        INSERT INTO learning_attachments_new
            (id, learning_item_id, session_id, attachment_type,
             file_name, relative_path, mime_type, caption, created_at)
        SELECT la.id, la.learning_item_id, la.session_id,
               la.attachment_type, la.file_name, la.relative_path,
               la.mime_type, la.caption, la.created_at
        FROM learning_attachments la;

        DROP TABLE learning_attachments;
        ALTER TABLE learning_attachments_new RENAME TO learning_attachments;

        CREATE INDEX IF NOT EXISTS idx_attachments_item ON learning_attachments(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_attachments_session ON learning_attachments(session_id);

        ALTER TABLE learning_items
            ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;",
    )
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
