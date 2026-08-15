use rusqlite::Connection;

/// V5 StudyProfile System：study_profiles 表 + goals.profile_id 列 + 旧数据迁移。
///
/// 建立 Higher 最顶层本地学习容器 StudyProfile：
/// - 一个学习档案 = 一个独立的"学习世界"（如 2027 考研 / Linux 内核学习）
/// - 不同档案之间数据完全隔离
/// - 仅在 goals 表增加 profile_id，其余实体通过 Goal 链式追踪所属档案
///   （LearningItem → Goal → Profile，Task → LearningItem → Goal → Profile，etc.）
///
/// 旧数据兼容策略（DEV-0009 §二十三/§二十四）：
/// - 如果数据库已存在旧 Goal，则自动创建一个默认档案"已有数据"，
///   并把所有旧 Goal.profile_id 统一关联到该档案。
/// - 如果数据库没有任何旧 Goal，则不创建默认档案，让用户首次看到创建档案引导。
///
/// active_profile_id 复用已有 settings 表（key-value），不新建 preferences 表。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // 1. 创建 study_profiles 表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS study_profiles (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            name               TEXT NOT NULL,
            profile_type       TEXT,
            target_description TEXT,
            target_date        TEXT,
            current_situation  TEXT,
            notes              TEXT,
            status             TEXT NOT NULL DEFAULT 'active',
            last_opened_at     TEXT,
            metadata_json      TEXT,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_study_profiles_status ON study_profiles(status);",
    )?;

    // 2. 给 goals 表增加 profile_id 列（可空，用于旧数据兼容期）
    //    SQLite 的 ALTER TABLE ADD COLUMN 不支持 IF NOT EXISTS，需要先检测列是否存在。
    let has_profile_id: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(goals)")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name)
        })?;
        let mut found = false;
        for r in rows {
            if r? == "profile_id" {
                found = true;
                break;
            }
        }
        found
    };

    if !has_profile_id {
        conn.execute_batch(
            "ALTER TABLE goals ADD COLUMN profile_id INTEGER REFERENCES study_profiles(id) ON DELETE SET NULL;",
        )?;
    }

    // 3. 旧数据迁移：如果存在 profile_id 为 NULL 的 Goal，创建默认档案并关联
    let old_goal_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL",
        [],
        |row| row.get(0),
    )?;

    if old_goal_count > 0 {
        // 创建默认档案"已有数据"
        conn.execute(
            "INSERT INTO study_profiles (name, profile_type, target_description, current_situation, notes)
             VALUES ('已有数据', 'migrated', '由旧版本数据自动迁移', '迁移前的学习数据', 'DEV-0005 自动创建的默认档案，用于承载 v004 之前的旧 Goal')",
            [],
        )?;
        let default_profile_id = conn.last_insert_rowid();

        // 把所有旧 Goal 关联到默认档案
        conn.execute(
            "UPDATE goals SET profile_id = ?1 WHERE profile_id IS NULL",
            rusqlite::params![default_profile_id],
        )?;

        // 同时把该默认档案设为 active，保证旧用户重启后直接看到原有数据
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('active_profile_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
            rusqlite::params![default_profile_id.to_string()],
        )?;

        println!(
            "[migration] v005: migrated {} old goal(s) into default profile id={}",
            old_goal_count, default_profile_id
        );
    } else {
        println!("[migration] v005: no old goals to migrate, skipping default profile creation");
    }

    Ok(())
}
