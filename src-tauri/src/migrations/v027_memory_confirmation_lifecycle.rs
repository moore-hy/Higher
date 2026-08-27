//! v027 · memory_records 状态域扩展（DEV-0076 §四 Memory 生命周期）。
//!
//! 现有 status CHECK（v017）：active / superseded / dismissed。
//! DEV-0076 确认闭环要求四态：draft → pending_confirmation → confirmed → rejected
//!（superseded/dismissed 历史保留）。
//!
//! 实施约束：
//! - SQLite CHECK 属表定义，扩枚举须重建表（同 v025 先例）；
//! - 存量 `active` 记录 → `confirmed`（DEV-0076 之前语义即「已生效记忆」，
//!   且 explicit 均为用户原话事实，不劣化安全规则 §十二——该规则约束的
//!   是**新增** ai_inference 不得直接 confirmed，迁移不新增任何推断记忆）；
//! - v017 版本行重建不可用（无版本列）→ 复制式重建，列集与索引原样保留。
//!
//! 生命周期映射（§四）：
//! - draft：AI 临时生成（本阶段 Service 内短暂存在，落库即 pending）
//! - pending_confirmation：等待用户确认
//! - confirmed：正式长期记忆（AI 读取口径）
//! - rejected：用户拒绝（不进入 AI 长期读取）

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE memory_records_v027 (
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
            status         TEXT NOT NULL DEFAULT 'pending_confirmation'
                              CHECK (status IN ('draft','pending_confirmation','confirmed',
                                                'rejected','superseded','dismissed')),
            valid_from     TEXT,
            valid_to       TEXT,
            supersedes_id  INTEGER REFERENCES memory_records(id) ON DELETE SET NULL,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at   TEXT
        );
        INSERT INTO memory_records_v027
            (id, profile_id, memory_type, category, memory_key, memory_value, source_kind,
             source_ref, source_excerpt, importance, confidence, status, valid_from, valid_to,
             supersedes_id, created_at, updated_at, last_used_at)
        SELECT id, profile_id, memory_type, category, memory_key, memory_value, source_kind,
               source_ref, source_excerpt, importance, confidence,
               CASE WHEN status = 'active' THEN 'confirmed' ELSE status END,
               valid_from, valid_to, supersedes_id, created_at, updated_at, last_used_at
        FROM memory_records
        ORDER BY id;
        DROP TABLE memory_records;
        ALTER TABLE memory_records_v027 RENAME TO memory_records;
        CREATE INDEX IF NOT EXISTS idx_mem_profile ON memory_records(profile_id, status, memory_type);
        CREATE INDEX IF NOT EXISTS idx_mem_key ON memory_records(profile_id, memory_key);

        PRAGMA foreign_key_check;",
    )
}
