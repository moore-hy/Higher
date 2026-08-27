//! v026 · personalization_profiles 增加 user_context_json（DEV-0070 Phase F v2.0 §8）。
//!
//! UserContext（AI 对用户的长期理解模型）属于「用户长期信息」，不是一次 AI Run：
//! - md_content：原始资料
//! - structured_json：已有结构化资料（DEV-0059 compile 产物）
//! - user_context_json：AI 理解模型（本列）
//!
//! 单列 ALTER 追加，不动既有数据；nullable，旧行为 NULL（读取端回退 md 解析）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "ALTER TABLE personalization_profiles ADD COLUMN user_context_json TEXT;",
    )
}
