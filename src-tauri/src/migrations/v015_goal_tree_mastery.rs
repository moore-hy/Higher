/// V15 Goal Tree + AI Mastery（DEV-0050 / PHASE B+D）。
///
/// Goal Tree（§16-32）：goals 自关联树 final→year→month→day（同一数据模型四种层级）。
/// - 新列：parent_goal_id / goal_level(final|year|month|day|legacy) / period_start / period_end / sort_order
/// - 旧数据升级（§30）：每 Profile——0 Goal→建 final「未设置最终目标」；1 Goal→标 final；
///   多 Goal→最小 id 标 final，其余 legacy（不猜层级，数据完整保留）
/// - Final 每 Profile 唯一：partial unique index（§31）
/// - 同父同层级同 period 唯一（year/month/day 防重复，§22-24）
///
/// AI Mastery（§55）：mastery_assessments append-only 历史表。
/// 不删除/不改写任何既有数据；v014→v015 直接升级。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // ---------- goals 加列（幂等：列存在跳过） ----------
    for (col, ddl) in [
        ("parent_goal_id", "ALTER TABLE goals ADD COLUMN parent_goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL;"),
        ("goal_level", "ALTER TABLE goals ADD COLUMN goal_level TEXT NOT NULL DEFAULT 'legacy';"),
        ("period_start", "ALTER TABLE goals ADD COLUMN period_start TEXT;"),
        ("period_end", "ALTER TABLE goals ADD COLUMN period_end TEXT;"),
        ("sort_order", "ALTER TABLE goals ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;"),
    ] {
        if !stmt_exists_col(conn, col) {
            conn.execute_batch(ddl)?;
        }
    }

    // ---------- 索引 ----------
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_goals_parent ON goals(parent_goal_id);
         CREATE INDEX IF NOT EXISTS idx_goals_profile_level ON goals(profile_id, goal_level);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_goals_final_unique
           ON goals(profile_id) WHERE goal_level = 'final';
         CREATE UNIQUE INDEX IF NOT EXISTS idx_goals_sibling_period_unique
           ON goals(parent_goal_id, goal_level, period_start)
           WHERE goal_level IN ('year','month','day');",
    )?;

    // ---------- 旧数据升级（§30，逐 Profile） ----------
    let profile_ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM study_profiles")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for pid in profile_ids {
        let (cnt, min_id): (i64, Option<i64>) = conn.query_row(
            "SELECT COUNT(*), MIN(id) FROM goals WHERE profile_id = ?1",
            rusqlite::params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        match (cnt, min_id) {
            (0, _) => {
                // 无 Goal：创建占位 Final（§20）
                conn.execute(
                    "INSERT INTO goals (profile_id, name, goal_level, parent_goal_id)
                     VALUES (?1, '未设置最终目标', 'final', NULL)",
                    rusqlite::params![pid],
                )?;
            }
            (1, Some(gid)) => {
                conn.execute(
                    "UPDATE goals SET goal_level='final', parent_goal_id=NULL WHERE id=?1",
                    rusqlite::params![gid],
                )?;
            }
            (_, Some(gid)) => {
                // 多 Goal：最小 id → final；其余 legacy（不猜）
                conn.execute(
                    "UPDATE goals SET goal_level='final', parent_goal_id=NULL WHERE id=?1",
                    rusqlite::params![gid],
                )?;
                conn.execute(
                    "UPDATE goals SET goal_level='legacy' WHERE profile_id=?1 AND id != ?2",
                    rusqlite::params![pid, gid],
                )?;
            }
            _ => {}
        }
    }

    // ---------- mastery_assessments（§55，append-only） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mastery_assessments (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL,
            goal_id             INTEGER,
            period_type         TEXT NOT NULL CHECK (period_type IN ('day','week','month','year')),
            period_start        TEXT NOT NULL,
            period_end          TEXT NOT NULL,
            status              TEXT NOT NULL CHECK (status IN ('scored','insufficient_evidence')),
            score               INTEGER,
            confidence          TEXT NOT NULL CHECK (confidence IN ('low','medium','high')),
            summary             TEXT NOT NULL DEFAULT '',
            understanding_score INTEGER,
            coverage_score      INTEGER,
            verification_score  INTEGER,
            strengths_json      TEXT NOT NULL DEFAULT '[]',
            gaps_json           TEXT NOT NULL DEFAULT '[]',
            evidence_json       TEXT NOT NULL DEFAULT '[]',
            suggestions_json    TEXT NOT NULL DEFAULT '[]',
            model               TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id) REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_id)    REFERENCES goals(id)          ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mastery_profile_period
          ON mastery_assessments(profile_id, period_type, period_start, period_end, created_at);",
    )?;

    Ok(())
}

fn stmt_exists_col(conn: &rusqlite::Connection, col: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('goals') WHERE name = ?1",
        rusqlite::params![col],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
