//! v041 · Training Runtime（REAL LEARNING ENGINE V1 · §7–§18）。
//!
//! # 三张表的分工
//!
//! ```text
//! training_runs          一次训练会话的**持久化事实**（模式、状态、计划快照）
//! training_block_runs    该次训练被物化出来的**块列表**（学习块 / 休息块）
//! training_interactions  用户在块内产生的**每一次交互**（幂等键在此）
//! ```
//!
//! # 数据库是历史事实（§7）
//!
//! 训练数据是**学习历史**，不是可随意重算的缓存。因此：
//!
//! - 这三张表**没有**通用 DELETE 命令；
//! - 档案删除可以级联；
//! - 任何「重来一次」都必须是新的一行，而不是覆盖旧行。
//!
//! # 本迁移同时落下的「恰好一次」约束
//!
//! 这些索引不是性能优化，而是**不变量** —— 它们让 §50 的绝对原则在数据库层
//! 变成不可能违反的事实，而不是靠上层自觉：
//!
//! ```text
//! §8  idx_training_runs_one_open          一个档案最多一个未终结的 TrainingRun
//! §8  idx_training_runs_session_unique    一个 StudySession 最多挂一个 TrainingRun
//! §10 idx_training_blocks_one_active      一个 TrainingRun 最多一个 active 块
//! §12 UNIQUE(profile_id, client_action_id) 网络重试不产生第二个学习事实
//! §16 idx_memory_reviews_moment_once      一个 LearningMoment 最多推进一次 FSRS
//! ```
//!
//! # §16 与既有数据的冲突处理
//!
//! `idx_memory_reviews_moment_once` 是**部分唯一索引**。若某个已存在的数据库里
//! 已经存在「同一个 learning_moment_id 有多条 review」的历史行，直接建索引会失败。
//!
//! 本迁移的做法是**先检测、再报错**：发现重复时返回一条可读的诊断错误，
//! 而不是抛一个难懂的 SQLite 约束错误，更**不会**为了建索引而删除任何历史证据。
//! 历史事实只增不减（§7）；如何处置那批历史行属于 Owner 决策，不属于迁移。
//!
//! Ledger：本次为 §2 锁定的 v041。**本次不创建 v042（属于 PACK B）。**

use rusqlite::Connection;

/// §16：建索引前检测历史重复行，避免用一条难懂的约束错误掩盖真实原因。
fn assert_no_duplicate_moment_reviews(conn: &Connection) -> rusqlite::Result<()> {
    let duplicate: Option<(i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT learning_moment_id, COUNT(*) AS c
               FROM memory_reviews
              WHERE learning_moment_id IS NOT NULL
              GROUP BY learning_moment_id
             HAVING c > 1
              LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Some((row.get(0)?, row.get(1)?)),
            None => None,
        }
    };

    match duplicate {
        None => Ok(()),
        Some((moment_id, count)) => Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
            Some(format!(
                "v041 无法建立 §16 的「一个 LearningMoment 只推进一次 FSRS」唯一索引：\
                 历史数据中 learning_moment_id={moment_id} 已有 {count} 条 memory_reviews。\
                 迁移不会删除任何历史复习记录（§7：数据库是历史事实）。\
                 需要 Owner 决定如何归并这批历史行后再重跑迁移。"
            )),
        )),
    }
}

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // 注意：下面整段是一个 SQL 字符串字面量。段内注释必须使用 SQL 的 `--`，
    // 写成 Rust 的 `//` 会被 SQLite 当作语法错误（near "/": syntax error）。
    conn.execute_batch(
        "-- ---------------- §8 training_runs ----------------
        CREATE TABLE IF NOT EXISTS training_runs (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,

            profile_id            INTEGER NOT NULL,

            study_session_id      INTEGER NULL,
            learning_item_id      INTEGER NULL,

            mode                  TEXT NOT NULL
                                  CHECK (
                                    mode IN (
                                      'autopilot',
                                      'copilot',
                                      'direct'
                                    )
                                  ),

            status                TEXT NOT NULL
                                  CHECK (
                                    status IN (
                                      'ready',
                                      'active',
                                      'paused',
                                      'completed',
                                      'abandoned'
                                    )
                                  ),

            current_block_ordinal INTEGER NULL,

            plan_snapshot_json    TEXT NOT NULL,

            started_at            TEXT NULL,
            ended_at              TEXT NULL,

            created_at            TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at            TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(study_session_id)
                REFERENCES study_sessions(id)
                ON DELETE SET NULL,

            FOREIGN KEY(learning_item_id)
                REFERENCES learning_items(id)
                ON DELETE SET NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_training_runs_session_unique
        ON training_runs(study_session_id)
        WHERE study_session_id IS NOT NULL;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_training_runs_one_open
        ON training_runs(profile_id)
        WHERE status IN ('ready','active','paused');

        CREATE INDEX IF NOT EXISTS idx_training_runs_profile_time
        ON training_runs(profile_id, created_at DESC, id DESC);

        -- ---------------- §10 training_block_runs ----------------
        CREATE TABLE IF NOT EXISTS training_block_runs (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,

            profile_id         INTEGER NOT NULL,
            training_run_id    INTEGER NOT NULL,

            ordinal            INTEGER NOT NULL,

            protocol_id        TEXT NULL,

            is_break           INTEGER NOT NULL DEFAULT 0
                               CHECK (is_break IN (0,1)),

            goal               TEXT NOT NULL,
            planned_minutes    INTEGER NOT NULL
                               CHECK (planned_minutes >= 0),

            memory_unit_id     INTEGER NULL,

            status             TEXT NOT NULL DEFAULT 'pending'
                               CHECK (
                                 status IN (
                                   'pending',
                                   'active',
                                   'completed',
                                   'skipped'
                                 )
                               ),

            started_at         TEXT NULL,
            ended_at           TEXT NULL,

            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(training_run_id)
                REFERENCES training_runs(id)
                ON DELETE CASCADE,

            FOREIGN KEY(memory_unit_id)
                REFERENCES memory_units(id)
                ON DELETE SET NULL,

            UNIQUE(training_run_id, ordinal)
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_training_blocks_one_active
        ON training_block_runs(training_run_id)
        WHERE status = 'active';

        CREATE INDEX IF NOT EXISTS idx_training_blocks_run
        ON training_block_runs(profile_id, training_run_id, ordinal);

        -- ---------------- §12 training_interactions ----------------
        CREATE TABLE IF NOT EXISTS training_interactions (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,

            profile_id           INTEGER NOT NULL,
            training_run_id      INTEGER NOT NULL,
            block_run_id         INTEGER NOT NULL,

            client_action_id     TEXT NOT NULL,

            interaction_type     TEXT NOT NULL,

            prompt_text          TEXT NULL,
            user_response_text   TEXT NULL,

            hint_level           INTEGER NULL
                                 CHECK (
                                   hint_level IS NULL OR
                                   (hint_level >= 0 AND hint_level <= 3)
                                 ),

            result               TEXT NULL
                                 CHECK (
                                   result IS NULL OR
                                   result IN (
                                     'success',
                                     'partial',
                                     'failure'
                                   )
                                 ),

            effect_summary_json  TEXT NOT NULL DEFAULT '{}',

            created_at           TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(training_run_id)
                REFERENCES training_runs(id)
                ON DELETE CASCADE,

            FOREIGN KEY(block_run_id)
                REFERENCES training_block_runs(id)
                ON DELETE CASCADE,

            UNIQUE(profile_id, client_action_id)
        );

        CREATE INDEX IF NOT EXISTS idx_training_interactions_run
        ON training_interactions(
            profile_id,
            training_run_id,
            id
        );

        CREATE INDEX IF NOT EXISTS idx_training_interactions_block
        ON training_interactions(
            profile_id,
            block_run_id,
            id
        );

        -- ---------------- §18 learning_moments 训练来源索引 ----------------
        CREATE INDEX IF NOT EXISTS idx_learning_moments_training_source
        ON learning_moments(
            profile_id,
            source_id,
            id
        )
        WHERE source_id IS NOT NULL;

        PRAGMA foreign_key_check;",
    )?;

    // ---------------- §16 memory_reviews 恰好一次 ----------------
    // 先检测历史重复（见文件头说明），再建唯一索引。
    assert_no_duplicate_moment_reviews(conn)?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_reviews_moment_once
        ON memory_reviews(learning_moment_id)
        WHERE learning_moment_id IS NOT NULL;",
    )
}
