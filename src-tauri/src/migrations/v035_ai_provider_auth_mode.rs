//! v035 · AI Provider 显式认证模式（POST-M7 AI FOUNDATION §S2-A/§S2-B）。
//!
//! ## 为什么需要这条迁移
//!
//! 此前 `ai_provider_profiles` 隐含「恒 Bearer」：Client 见 api_key 为空一律
//! fail-closed，导致无法连接本地 / 局域网无认证的 OpenAI-compatible server
//! （LM Studio / Ollama / llama.cpp server 等）。
//!
//! 本迁移只增一列、只表达配置事实，**不猜测**：
//!
//! ```text
//! auth_mode TEXT NOT NULL DEFAULT 'bearer'
//! ```
//!
//! - 存量全部默认 `bearer` → 现有 Cloud Provider **零行为变化**（§S2-B / LA-01）；
//! - `none` 由用户在 Settings 显式选择（禁止按 localhost/127.0.0.1 自动推断）；
//! - 这是 DB-structure-only 迁移：不含任何 OS Keyring / SecretStore 副作用
//!   （§4.5 Secret Migration Boundary；secret_ref 属后续 v036，由应用层
//!   SecretMigrationService 负责，schema migration 永远不做凭据 I/O）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        // 只增列，不重建表：存量 profile 原样保留，默认 bearer。
        "ALTER TABLE ai_provider_profiles
            ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'bearer';

         PRAGMA foreign_key_check;",
    )
}
