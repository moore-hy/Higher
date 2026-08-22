//! v024 · Multi-Provider AI Profiles & Action Continuation（DEV-0062）。
//!
//! 1. `ai_provider_profiles`：多 AI Connection（adapter_kind = deepseek | openai_compatible；
//!    thinking_mode = off | deepseek_model_suffix；compatibility_status 四态）。
//! 2. `ai_pending_actions`：Action Clarification 持久化 Control State
//!    （每个 (profile_id, conversation_id) 最多 1 active，partial unique index）。
//! 3. `ai_runs` 增加 8 列 provider snapshot（Run 开始时真实使用的 Primary/Control；旧 Run NULL）。
//! 4. Legacy settings KV（ai.base_url / ai.api_key / ai.model / ai.thinking_enabled）→
//!    迁移出一个 DeepSeek Connection 并设为 active primary（原值保留，Key 不删除）。
//!
//! 禁止修改 v001-v023；不删除任何 legacy 数据。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_provider_profiles (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            display_name         TEXT NOT NULL COLLATE NOCASE UNIQUE,
            adapter_kind         TEXT NOT NULL
                                 CHECK (adapter_kind IN ('deepseek','openai_compatible')),
            base_url             TEXT NOT NULL,
            api_key              TEXT NOT NULL DEFAULT '',
            model                TEXT NOT NULL,
            thinking_mode        TEXT NOT NULL DEFAULT 'off'
                                 CHECK (thinking_mode IN ('off','deepseek_model_suffix')),
            enabled              INTEGER NOT NULL DEFAULT 1
                                 CHECK (enabled IN (0,1)),
            capabilities_json    TEXT NOT NULL DEFAULT '{}',
            compatibility_status TEXT NOT NULL DEFAULT 'untested'
                                 CHECK (compatibility_status IN
                                 ('untested','full','limited','incompatible')),
            last_test_message    TEXT NOT NULL DEFAULT '',
            last_tested_at       TEXT,
            created_at           TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at           TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_provider_profiles_enabled
            ON ai_provider_profiles(enabled);

        CREATE TABLE IF NOT EXISTS ai_pending_actions (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id            INTEGER NOT NULL
                                  REFERENCES study_profiles(id) ON DELETE CASCADE,
            conversation_id       INTEGER NOT NULL
                                  REFERENCES ai_conversations(id) ON DELETE CASCADE,
            source_run_id         TEXT
                                  REFERENCES ai_runs(id) ON DELETE SET NULL,
            status                TEXT NOT NULL DEFAULT 'active'
                                  CHECK (status IN
                                  ('active','resolved','cancelled','expired','stale')),
            semantic_action_json  TEXT NOT NULL,
            candidates_json       TEXT NOT NULL,
            clarification_message TEXT NOT NULL DEFAULT '',
            attempt_count         INTEGER NOT NULL DEFAULT 0,
            expires_at            TEXT NOT NULL,
            created_at            TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at            TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at           TEXT
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_pending_actions_active
            ON ai_pending_actions(profile_id, conversation_id)
            WHERE status = 'active';

        ALTER TABLE ai_runs ADD COLUMN primary_ai_profile_id INTEGER NULL;
        ALTER TABLE ai_runs ADD COLUMN primary_profile_name TEXT NULL;
        ALTER TABLE ai_runs ADD COLUMN primary_adapter_kind TEXT NULL;
        ALTER TABLE ai_runs ADD COLUMN primary_model TEXT NULL;
        ALTER TABLE ai_runs ADD COLUMN control_ai_profile_id INTEGER NULL;
        ALTER TABLE ai_runs ADD COLUMN control_profile_name TEXT NULL;
        ALTER TABLE ai_runs ADD COLUMN control_adapter_kind TEXT NULL;
        ALTER TABLE ai_runs ADD COLUMN control_model TEXT NULL;",
    )?;

    // ---- Legacy DeepSeek Settings → Connection（§11：原值完整保留，Key 不删除） ----
    let legacy: (String, String, String, bool) = {
        let get = |k: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![k],
                |r| r.get::<_, String>(0),
            )
            .ok()
        };
        let base = get("ai.base_url")
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "https://api.deepseek.com".to_string());
        let key = get("ai.api_key").unwrap_or_default();
        let model = get("ai.model")
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "deepseek-v4-flash".to_string());
        let thinking = get("ai.thinking_enabled")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        (base, key, model, thinking)
    };
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_provider_profiles",
        [],
        |r| r.get(0),
    )?;
    if existing == 0 {
        conn.execute(
            "INSERT INTO ai_provider_profiles
             (display_name, adapter_kind, base_url, api_key, model, thinking_mode)
             VALUES ('DeepSeek', 'deepseek', ?1, ?2, ?3, ?4)",
            rusqlite::params![
                legacy.0.trim(),
                legacy.1,
                legacy.2.trim(),
                if legacy.3 { "deepseek_model_suffix" } else { "off" }
            ],
        )?;
    }
    // Active primary 默认 = 迁移出的 DeepSeek Connection（§12；Control 缺省 = Follow Primary）
    let active: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'ai.active_primary_profile_id'",
            [],
            |r| r.get(0),
        )
        .ok();
    if active.map(|v| v.trim().is_empty()).unwrap_or(true) {
        let pid: Option<i64> = conn
            .query_row(
                "SELECT id FROM ai_provider_profiles WHERE display_name = 'DeepSeek'",
                [],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = pid {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('ai.active_primary_profile_id', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![id.to_string()],
            )?;
        }
    }
    Ok(())
}
