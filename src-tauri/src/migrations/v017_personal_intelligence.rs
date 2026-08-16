/// V17 Personal Intelligence Foundation（DEV-0052 / PHASE Y §197-201）。
///
/// 新表：ai_conversations / ai_messages / ai_runs / ai_run_events / ai_sources /
/// memory_records / personalization_sources / personalization_source_chunks /
/// personalization_profiles / ai_change_sets / ai_change_operations。
/// goals.day_kind（study|rest）。
/// Search：search_index 元数据表 + FTS5 外部内容虚表 + 三触发器 + 既有数据 rebuild。
/// 迁移只增不改；v016 全部数据保留。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // ---------- goals.day_kind ----------
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('goals') WHERE name='day_kind'",
        [],
        |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE goals ADD COLUMN day_kind TEXT NOT NULL DEFAULT 'study'
             CHECK (day_kind IN ('study','rest'));",
        )?;
    }

    // ---------- Conversation（PHASE C） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_conversations (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id   INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            title        TEXT NOT NULL DEFAULT '新对话',
            mode         TEXT NOT NULL DEFAULT 'readonly' CHECK (mode IN ('readonly','assistant')),
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_aiconv_profile ON ai_conversations(profile_id, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ai_messages (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
            profile_id      INTEGER NOT NULL,
            role            TEXT NOT NULL CHECK (role IN ('user','assistant','system_summary')),
            content         TEXT NOT NULL DEFAULT '',
            run_id          TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_aimsg_conv ON ai_messages(conversation_id, id DESC);
        CREATE INDEX IF NOT EXISTS idx_aimsg_profile ON ai_messages(profile_id, id);",
    )?;

    // ---------- AI Run（PHASE B） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_runs (
            id                TEXT PRIMARY KEY,
            profile_id        INTEGER NOT NULL,
            conversation_id   INTEGER,
            mode              TEXT NOT NULL DEFAULT 'readonly',
            action            TEXT NOT NULL DEFAULT 'assistant_chat',
            status            TEXT NOT NULL DEFAULT 'queued'
                              CHECK (status IN ('queued','running','waiting_approval','completed','cancelled','failed')),
            error             TEXT NOT NULL DEFAULT '',
            prompt_tokens     INTEGER,
            completion_tokens INTEGER,
            total_tokens      INTEGER,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_airun_profile ON ai_runs(profile_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS ai_run_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id     TEXT NOT NULL REFERENCES ai_runs(id) ON DELETE CASCADE,
            event_type TEXT NOT NULL,
            data_json  TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_airunev_run ON ai_run_events(run_id, id);

        CREATE TABLE IF NOT EXISTS ai_sources (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id   INTEGER NOT NULL,
            run_id       TEXT,
            source_type  TEXT NOT NULL DEFAULT 'web',
            title        TEXT NOT NULL DEFAULT '',
            url          TEXT NOT NULL DEFAULT '',
            snippet      TEXT NOT NULL DEFAULT '',
            published_at TEXT,
            retrieved_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_aisrc_run ON ai_sources(run_id);",
    )?;

    // ---------- Memory（PHASE D） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_records (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id     INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            memory_type    TEXT NOT NULL CHECK (memory_type IN
                             ('user_fact','user_opinion','user_preference','user_constraint',
                              'system_observation','ai_inference','goal_context')),
            category       TEXT NOT NULL DEFAULT '',
            memory_key     TEXT NOT NULL DEFAULT '',
            memory_value   TEXT NOT NULL DEFAULT '',
            source_kind    TEXT NOT NULL DEFAULT 'user_message'
                              CHECK (source_kind IN ('user_message','higher_db','ai_inference','user_edit')),
            source_ref     TEXT NOT NULL DEFAULT '',
            source_excerpt TEXT NOT NULL DEFAULT '',
            importance     INTEGER NOT NULL DEFAULT 3 CHECK (importance BETWEEN 1 AND 5),
            confidence     TEXT NOT NULL DEFAULT 'medium' CHECK (confidence IN ('low','medium','high')),
            status         TEXT NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active','superseded','dismissed')),
            valid_from     TEXT,
            valid_to       TEXT,
            supersedes_id  INTEGER REFERENCES memory_records(id) ON DELETE SET NULL,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_mem_profile ON memory_records(profile_id, status, memory_type);
        CREATE INDEX IF NOT EXISTS idx_mem_key ON memory_records(profile_id, memory_key);",
    )?;

    // ---------- Personalization（PHASE G/H） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS personalization_sources (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            file_name           TEXT NOT NULL,
            file_type           TEXT NOT NULL CHECK (file_type IN ('txt','md','docx','pdf')),
            relative_path       TEXT NOT NULL,
            sha256              TEXT NOT NULL,
            extracted_text_path TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'extracted'
                                  CHECK (status IN ('imported','extracted','failed')),
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_psrc_profile ON personalization_sources(profile_id);

        CREATE TABLE IF NOT EXISTS personalization_source_chunks (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id   INTEGER NOT NULL REFERENCES personalization_sources(id) ON DELETE CASCADE,
            profile_id  INTEGER NOT NULL,
            chunk_index INTEGER NOT NULL,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_pchunk_source ON personalization_source_chunks(source_id, chunk_index);

        CREATE TABLE IF NOT EXISTS personalization_profiles (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL UNIQUE REFERENCES study_profiles(id) ON DELETE CASCADE,
            md_content       TEXT NOT NULL DEFAULT '',
            structured_json  TEXT,
            status           TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','confirmed')),
            version          INTEGER NOT NULL DEFAULT 1,
            last_compiled_at TEXT,
            last_updated_at  TEXT,
            dirty            INTEGER NOT NULL DEFAULT 0
        );",
    )?;

    // ---------- ChangeSet（PHASE O） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_change_sets (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id      INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            conversation_id INTEGER,
            run_id          TEXT,
            title           TEXT NOT NULL DEFAULT '',
            summary         TEXT NOT NULL DEFAULT '',
            status          TEXT NOT NULL DEFAULT 'draft'
                              CHECK (status IN ('draft','waiting_approval','applied','rejected','cancelled','undone')),
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            applied_at      TEXT,
            rejected_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_cs_profile ON ai_change_sets(profile_id, status, id DESC);

        CREATE TABLE IF NOT EXISTS ai_change_operations (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            change_set_id   INTEGER NOT NULL REFERENCES ai_change_sets(id) ON DELETE CASCADE,
            operation_order INTEGER NOT NULL DEFAULT 0,
            entity_type     TEXT NOT NULL,
            entity_id       INTEGER,
            action          TEXT NOT NULL CHECK (action IN ('create','update','delete','move','status_change')),
            before_json     TEXT,
            after_json      TEXT NOT NULL DEFAULT '{}',
            reason          TEXT NOT NULL DEFAULT '',
            deep_link       TEXT NOT NULL DEFAULT '',
            selected        INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_cop_cs ON ai_change_operations(change_set_id, operation_order);",
    )?;

    // ---------- Search（PHASE E） ----------
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS search_index (
            entity_type TEXT NOT NULL,
            entity_id   INTEGER NOT NULL,
            profile_id  INTEGER NOT NULL,
            title       TEXT NOT NULL DEFAULT '',
            content     TEXT NOT NULL DEFAULT '',
            timestamp   TEXT,
            PRIMARY KEY (entity_type, entity_id)
        );
        CREATE INDEX IF NOT EXISTS idx_si_profile ON search_index(profile_id);
        CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
            title, content, tokenize='unicode61',
            content='search_index', content_rowid='rowid'
        );
        CREATE TRIGGER IF NOT EXISTS search_fts_ai AFTER INSERT ON search_index BEGIN
            INSERT INTO search_fts(rowid, title, content)
            VALUES (new.rowid, new.title, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS search_fts_ad AFTER DELETE ON search_index BEGIN
            INSERT INTO search_fts(search_fts, rowid, title, content)
            VALUES ('delete', old.rowid, old.title, old.content);
        END;
        CREATE TRIGGER IF NOT EXISTS search_fts_au AFTER UPDATE ON search_index BEGIN
            INSERT INTO search_fts(search_fts, rowid, title, content)
            VALUES ('delete', old.rowid, old.title, old.content);
            INSERT INTO search_fts(rowid, title, content)
            VALUES (new.rowid, new.title, new.content);
        END;",
    )?;

    // ---------- 既有数据 rebuild（只读源表，写入 search_index；触发器自动进 FTS） ----------
    conn.execute_batch(
        "INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'goal', id, profile_id, name, COALESCE(name,'') || ' ' || COALESCE(description,''), updated_at
         FROM goals WHERE profile_id IS NOT NULL;

         INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'task', id, profile_id, title, title, updated_at
         FROM tasks;

         INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'session', id, profile_id, title, COALESCE(note,''), started_at
         FROM study_sessions;

         INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'knowledge', id, profile_id, name, COALESCE(content,''), updated_at
         FROM learning_items;

         INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'document', id, profile_id, title, COALESCE(content_text,''), updated_at
         FROM knowledge_documents;

         INSERT OR REPLACE INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
         SELECT 'evaluation', id, profile_id, title, '', occurred_at
         FROM evaluations;
    ",
    )?;

    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
