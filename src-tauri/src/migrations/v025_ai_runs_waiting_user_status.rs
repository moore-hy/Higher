//! v025 · ai_runs.status 增加 'waiting_user'（DEV-0066 Phase E §7）。
//!
//! Global Agent 调用 request_user_input 后本轮 Run 挂起等待用户回答：
//! - workflow_state = 'waiting_user'（v021 列，无 CHECK，本就可用）
//! - ai_runs.status 不得是普通 completed（§7-6），必须是可区分的挂起终态
//!
//! v017 的 CHECK 不含 'waiting_user'，SQLite 无法就地修改 CHECK，
//! 按框架既定的表重建流程（run_migrations 已在迁移事务外关闭 FK，
//! 末尾 PRAGMA foreign_key_check 兜底）重建 ai_runs：
//! 列集合与顺序 = v017 建表 + v021 三列 + v024 八列（ALTER 追加序），
//! 仅扩展 status CHECK；两个索引原样重建。
//! 引用方（ai_run_events CASCADE / ai_pending_actions SET NULL）按表名引用，
//! 重建后自动指向新表，数据完整复制，不丢失任何行。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE ai_runs_v025 (
            id                TEXT PRIMARY KEY,
            profile_id        INTEGER NOT NULL,
            conversation_id   INTEGER,
            mode              TEXT NOT NULL DEFAULT 'readonly',
            action            TEXT NOT NULL DEFAULT 'assistant_chat',
            status            TEXT NOT NULL DEFAULT 'queued'
                              CHECK (status IN ('queued','running','waiting_approval',
                                               'waiting_user','completed','cancelled','failed')),
            error             TEXT NOT NULL DEFAULT '',
            prompt_tokens     INTEGER,
            completion_tokens INTEGER,
            total_tokens      INTEGER,
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
            workflow_type     TEXT,
            workflow_state    TEXT,
            workflow_json     TEXT,
            primary_ai_profile_id   INTEGER NULL,
            primary_profile_name    TEXT NULL,
            primary_adapter_kind    TEXT NULL,
            primary_model           TEXT NULL,
            control_ai_profile_id   INTEGER NULL,
            control_profile_name    TEXT NULL,
            control_adapter_kind    TEXT NULL,
            control_model           TEXT NULL
        );

        INSERT INTO ai_runs_v025 (
            id, profile_id, conversation_id, mode, action, status, error,
            prompt_tokens, completion_tokens, total_tokens, created_at, updated_at,
            workflow_type, workflow_state, workflow_json,
            primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
            control_ai_profile_id, control_profile_name, control_adapter_kind, control_model
        )
        SELECT
            id, profile_id, conversation_id, mode, action, status, error,
            prompt_tokens, completion_tokens, total_tokens, created_at, updated_at,
            workflow_type, workflow_state, workflow_json,
            primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
            control_ai_profile_id, control_profile_name, control_adapter_kind, control_model
        FROM ai_runs;

        DROP TABLE ai_runs;
        ALTER TABLE ai_runs_v025 RENAME TO ai_runs;

        CREATE INDEX IF NOT EXISTS idx_airun_profile
            ON ai_runs(profile_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_airun_workflow
            ON ai_runs(profile_id, conversation_id, workflow_type, id DESC);

        PRAGMA foreign_key_check;"#,
    )
}
