//! v036 · AI Provider Secret Reference（POST-M7 AI FOUNDATION §S3-D）。
//!
//! ```text
//! secret_ref TEXT   -- 稳定凭据引用（UUID）；真实 API Key 存 OS SecretStore
//! ```
//!
//! 纪律（FINAL CORRECTION PATCH §C / §C1 / §4.5）：
//! - schema migration = **DB structure only**。本 `up()` 只做 ALTER TABLE，
//!   **禁止**出现任何 keyring::Entry::{set,get,delete}_password 副作用；
//! - SQLite transaction ≠ Windows Credential Manager transaction，两者无法原子
//!   提交——legacy plaintext → SecretStore 的搬移由应用层可恢复的
//!   `SecretMigrationService`（ai/secret_migration.rs）负责，不由 schema 迁移执行；
//! - `api_key` 列暂时保留：仅作为 SecretMigration 兼容期内的 legacy 明文回退读取
//!   路径；正常新路径（create / update）不再把 secret 写进该列。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        // 只增列：存量 profile 原样保留；secret_ref 默认 NULL（尚未迁移）。
        "ALTER TABLE ai_provider_profiles
            ADD COLUMN secret_ref TEXT;

         PRAGMA foreign_key_check;",
    )
}
