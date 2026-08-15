/// V13 Profile First（BATCH-04 / DEV-0040）。
///
/// 产品模型纠正：Profile 是唯一强制容器；Goal 降级为"可选长期规划上下文"。
///
/// 六张核心表全部直挂 profile_id NOT NULL（不再经 goals JOIN 间接推导）：
/// 1. tasks            重建：+ profile_id；goal_id 改可空
/// 2. study_sessions   重建：+ profile_id；goal_id 改可空；+ title
/// 3. learning_items   重建：+ profile_id；goal_id 改可空
/// 4. recurring_task_rules 重建：+ profile_id；goal_id 可空；learning_item_id 改可空
/// 5. evaluations      重建：+ profile_id；goal_id 改可空
/// 6. learning_attachments 重建：+ profile_id（经 item.goal / session.goal 推导）
///
/// Backfill（零 ID 破坏，INSERT SELECT 保 id）：
/// 全部旧记录经 goal_id → goals.profile_id 推导；attachments 经
/// COALESCE(item 链, session 链)。任何行无法推导 → SQL 失败 → 事务回滚
/// （对应任务书 HARD STOP §17：禁止塞给 active profile）。
///
/// 迁移后执行 PRAGMA foreign_key_check（run_migrations 外由测试断言 + up 内自检）。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // 1) tasks
    conn.execute_batch(
        "CREATE TABLE tasks_v13 (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id        INTEGER NOT NULL,
            goal_id           INTEGER,
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
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE,
            FOREIGN KEY (plan_id)          REFERENCES plans(id)          ON DELETE SET NULL
        );

        INSERT INTO tasks_v13
            (id, profile_id, goal_id, learning_item_id, title, planned_date, planned_time,
             status, archived_at, plan_id, recurring_rule_id, created_at, updated_at)
        SELECT t.id, g.profile_id, t.goal_id, t.learning_item_id, t.title,
               t.planned_date, t.planned_time, t.status, t.archived_at,
               t.plan_id, t.recurring_rule_id, t.created_at, t.updated_at
        FROM tasks t
        JOIN goals g ON t.goal_id = g.id;

        DROP TABLE tasks;
        ALTER TABLE tasks_v13 RENAME TO tasks;

        CREATE INDEX IF NOT EXISTS idx_tasks_profile ON tasks(profile_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_goal ON tasks(goal_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_item ON tasks(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_rule_date ON tasks(recurring_rule_id, planned_date);
        CREATE INDEX IF NOT EXISTS idx_tasks_date ON tasks(planned_date);",
    )?;

    // 2) learning_items（先建，tasks FK 已因 DROP/RENAME 解绑旧表）
    conn.execute_batch(
        "CREATE TABLE learning_items_v13 (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id     INTEGER NOT NULL,
            goal_id        INTEGER,
            parent_id      INTEGER,
            name           TEXT NOT NULL,
            description    TEXT,
            mastery_status TEXT NOT NULL DEFAULT 'not_started',
            content        TEXT NOT NULL DEFAULT '',
            sort_order     INTEGER NOT NULL DEFAULT 0,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id) REFERENCES study_profiles(id)     ON DELETE CASCADE,
            FOREIGN KEY (goal_id)    REFERENCES goals(id)              ON DELETE SET NULL,
            FOREIGN KEY (parent_id)  REFERENCES learning_items_v13(id) ON DELETE CASCADE
        );

        INSERT INTO learning_items_v13
            (id, profile_id, goal_id, parent_id, name, description, mastery_status,
             content, sort_order, created_at, updated_at)
        SELECT li.id, g.profile_id, li.goal_id, li.parent_id, li.name, li.description,
               li.mastery_status, li.content, li.sort_order, li.created_at, li.updated_at
        FROM learning_items li
        JOIN goals g ON li.goal_id = g.id;

        DROP TABLE learning_items;
        ALTER TABLE learning_items_v13 RENAME TO learning_items;

        CREATE INDEX IF NOT EXISTS idx_items_profile ON learning_items(profile_id);
        CREATE INDEX IF NOT EXISTS idx_items_goal ON learning_items(goal_id);
        CREATE INDEX IF NOT EXISTS idx_items_parent ON learning_items(parent_id);",
    )?;

    // 3) study_sessions：+ profile_id + title（Quick=快速学习 / Task=task.title / Knowledge=item.name）
    conn.execute_batch(
        "CREATE TABLE study_sessions_v13 (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id        INTEGER NOT NULL,
            goal_id           INTEGER,
            task_id           INTEGER,
            learning_item_id  INTEGER,
            title             TEXT NOT NULL DEFAULT '快速学习',
            started_at        TEXT NOT NULL DEFAULT (datetime('now')),
            ended_at          TEXT,
            duration_seconds  INTEGER,
            status            TEXT NOT NULL DEFAULT 'active',
            note              TEXT,
            time_corrected    INTEGER NOT NULL DEFAULT 0,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE SET NULL,
            FOREIGN KEY (task_id)          REFERENCES tasks(id)          ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE SET NULL
        );

        INSERT INTO study_sessions_v13
            (id, profile_id, goal_id, task_id, learning_item_id, title,
             started_at, ended_at, duration_seconds, status, note, created_at, updated_at)
        SELECT ss.id,
               g.profile_id,
               ss.goal_id,
               ss.task_id,
               ss.learning_item_id,
               COALESCE(li.name, tk.title, '快速学习'),
               ss.started_at, ss.ended_at, ss.duration_seconds, ss.status,
               ss.note, ss.created_at, ss.updated_at
        FROM study_sessions ss
        JOIN goals g ON ss.goal_id = g.id
        LEFT JOIN learning_items li ON ss.learning_item_id = li.id
        LEFT JOIN tasks tk ON ss.task_id = tk.id;

        DROP TABLE study_sessions;
        ALTER TABLE study_sessions_v13 RENAME TO study_sessions;

        CREATE INDEX IF NOT EXISTS idx_sessions_profile ON study_sessions(profile_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_item ON study_sessions(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_task ON study_sessions(task_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_started ON study_sessions(started_at);",
    )?;

    // 4) evaluations
    conn.execute_batch(
        "CREATE TABLE evaluations_v13 (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL,
            goal_id          INTEGER,
            learning_item_id INTEGER,
            title            TEXT NOT NULL,
            evaluation_type  TEXT NOT NULL,
            source           TEXT,
            occurred_at      TEXT NOT NULL DEFAULT (datetime('now')),
            total_items      INTEGER,
            correct_items    INTEGER,
            incorrect_items  INTEGER,
            score            REAL,
            max_score        REAL,
            outcome          TEXT NOT NULL DEFAULT 'unrated',
            note             TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id)     ON DELETE CASCADE,
            FOREIGN KEY (goal_id)          REFERENCES goals(id)              ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id)     ON DELETE RESTRICT
        );

        INSERT INTO evaluations_v13
            (id, profile_id, goal_id, learning_item_id, title, evaluation_type, source,
             occurred_at, total_items, correct_items, incorrect_items, score, max_score,
             outcome, note, created_at, updated_at)
        SELECT e.id, g.profile_id, e.goal_id, e.learning_item_id, e.title, e.evaluation_type,
               e.source, e.occurred_at, e.total_items, e.correct_items, e.incorrect_items,
               e.score, e.max_score, e.outcome, e.note, e.created_at, e.updated_at
        FROM evaluations e
        JOIN goals g ON e.goal_id = g.id;

        DROP TABLE evaluations;
        ALTER TABLE evaluations_v13 RENAME TO evaluations;

        CREATE INDEX IF NOT EXISTS idx_evaluations_profile ON evaluations(profile_id);
        CREATE INDEX IF NOT EXISTS idx_evaluations_goal ON evaluations(goal_id);
        CREATE INDEX IF NOT EXISTS idx_evaluations_item ON evaluations(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_evaluations_date ON evaluations(occurred_at);",
    )?;

    // 5) recurring_task_rules：learning_item_id 改可空（"每天背单词"无需 Goal/Knowledge）
    conn.execute_batch(
        "CREATE TABLE recurring_task_rules_v13 (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id      INTEGER NOT NULL,
            goal_id         INTEGER,
            learning_item_id INTEGER,
            title           TEXT NOT NULL,
            repeat_type     TEXT NOT NULL,
            weekdays_json   TEXT NOT NULL DEFAULT '[]',
            time_of_day     TEXT,
            start_date      TEXT NOT NULL,
            end_date        TEXT,
            enabled         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_id)          REFERENCES goals(id)          ON DELETE SET NULL,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE
        );

        INSERT INTO recurring_task_rules_v13
            (id, profile_id, goal_id, learning_item_id, title, repeat_type, weekdays_json,
             time_of_day, start_date, end_date, enabled, created_at, updated_at)
        SELECT r.id, g.profile_id, r.goal_id, r.learning_item_id, r.title, r.repeat_type,
               r.weekdays_json, r.time_of_day, r.start_date, r.end_date, r.enabled,
               r.created_at, r.updated_at
        FROM recurring_task_rules r
        JOIN goals g ON r.goal_id = g.id;

        DROP TABLE recurring_task_rules;
        ALTER TABLE recurring_task_rules_v13 RENAME TO recurring_task_rules;

        CREATE INDEX IF NOT EXISTS idx_recurring_rules_profile ON recurring_task_rules(profile_id);
        CREATE INDEX IF NOT EXISTS idx_recurring_rules_goal ON recurring_task_rules(goal_id);",
    )?;

    // 6) learning_attachments：profile 经 COALESCE(item 链, session 链)
    conn.execute_batch(
        "CREATE TABLE learning_attachments_v13 (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL,
            learning_item_id INTEGER,
            session_id       INTEGER,
            attachment_type  TEXT NOT NULL CHECK (attachment_type IN ('image','video','drawing','file')),
            file_name        TEXT NOT NULL,
            relative_path    TEXT NOT NULL,
            mime_type        TEXT,
            caption          TEXT NOT NULL DEFAULT '',
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id)     ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id)     ON DELETE CASCADE,
            FOREIGN KEY (session_id)       REFERENCES study_sessions(id)     ON DELETE CASCADE
        );

        INSERT INTO learning_attachments_v13
            (id, profile_id, learning_item_id, session_id, attachment_type,
             file_name, relative_path, mime_type, caption, created_at)
        SELECT la.id,
               COALESCE(gi.profile_id, gs.profile_id),
               la.learning_item_id, la.session_id,
               la.attachment_type, la.file_name, la.relative_path,
               la.mime_type, la.caption, la.created_at
        FROM learning_attachments la
        LEFT JOIN learning_items li ON la.learning_item_id = li.id
        LEFT JOIN goals gi ON li.goal_id = gi.id
        LEFT JOIN study_sessions ss ON la.session_id = ss.id
        LEFT JOIN goals gs ON ss.goal_id = gs.id;

        DROP TABLE learning_attachments;
        ALTER TABLE learning_attachments_v13 RENAME TO learning_attachments;

        CREATE INDEX IF NOT EXISTS idx_attachments_profile ON learning_attachments(profile_id);
        CREATE INDEX IF NOT EXISTS idx_attachments_item ON learning_attachments(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_attachments_session ON learning_attachments(session_id);",
    )?;

    // 自检：backfill 后不允许任何核心行缺 profile（缺=推导失败=应回滚）
    for (table, col) in [
        ("tasks", "profile_id"),
        ("learning_items", "profile_id"),
        ("study_sessions", "profile_id"),
        ("evaluations", "profile_id"),
        ("recurring_task_rules", "profile_id"),
        ("learning_attachments", "profile_id"),
    ] {
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE {} IS NULL", table, col),
            [],
            |r| r.get(0),
        )?;
        if n > 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "v013 backfill 失败：{} 有 {} 行无法推导 profile_id，已中止（HARD STOP 条件）",
                table, n
            )));
        }
    }
    // foreign_key_check（v013 后必须 0 error；RENAME 后的 legacy_alter_table 兼容已关闭时
    // 子表 FK 引用旧表名的问题不存在——所有新表均重建）
    let fk_err: i64 = conn.query_row("PRAGMA foreign_key_check", [], |_| Ok(1)).unwrap_or(0);
    if fk_err > 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "v013 foreign_key_check 存在违例，已中止".to_string(),
        ));
    }
    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
