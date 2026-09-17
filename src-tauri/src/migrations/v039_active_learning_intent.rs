//! v039 · Active Learning Intent（REAL LEARNING ENGINE V1 · §3 锁定 schema）。
//!
//! # 这张表回答什么问题
//!
//! 「此刻这个人想学什么？」——不是长期目标（`goals`），不是学习项（`learning_items`），
//! 而是**当前这一刻的意图**：以什么模式（autopilot / copilot / direct）、
//! 指向什么领域、指向哪个学习项或目标，以及一句自由文本。
//!
//! # 为什么 `profile_id` 就是主键（§3 显式禁止另一种写法）
//!
//! 意图是**瞬时状态**，不是账本：一个档案在同一时刻只能有**一个**当前意图。
//! 因此 `profile_id = PRIMARY KEY`，天然保证「每个档案恰好一行」。
//! 显式禁止 `id INTEGER PRIMARY KEY + 非唯一 profile_id` —— 那会允许同一档案
//! 出现多行「当前意图」，随后每个消费者都要自己决定「哪一行才算数」，
//! 这正是 §50「Expired intent ≠ current intent」想要消灭的二义性。
//! 更新语义由 §4 的 `INSERT ... ON CONFLICT(profile_id) DO UPDATE` 承担：
//! 意图是**整体替换**，绝不与陈旧意图做字段级合并。
//!
//! # 过期不是失败
//!
//! `expires_at` 由 §5 锁定为最长 12 小时。过期只意味着「不再是当前意图」，
//! 它**不产生** failure / skip / 兴趣衰减 / LearningMoment —— 见 §5 与 §50。
//!
//! Ledger：本次为 §2 锁定的 v039。**本次不创建 v040。**

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS active_learning_intent (
            profile_id       INTEGER PRIMARY KEY,

            mode             TEXT NOT NULL
                             CHECK (mode IN ('autopilot','copilot','direct')),

            domain           TEXT NULL
                             CHECK (
                               domain IS NULL OR
                               domain IN (
                                 'generic',
                                 'english',
                                 'mathematics',
                                 'computer_science_408',
                                 'programming'
                               )
                             ),

            learning_item_id INTEGER NULL,
            goal_id          INTEGER NULL,

            free_text        TEXT NULL,

            source           TEXT NOT NULL
                             CHECK (
                               source IN (
                                 'command_bar',
                                 'today_choice',
                                 'journey',
                                 'material'
                               )
                             ),

            created_at       TEXT NOT NULL,
            updated_at       TEXT NOT NULL,
            expires_at       TEXT NOT NULL,

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(learning_item_id)
                REFERENCES learning_items(id)
                ON DELETE SET NULL,

            FOREIGN KEY(goal_id)
                REFERENCES goals(id)
                ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_active_learning_intent_expiry
        ON active_learning_intent(expires_at, profile_id);

        PRAGMA foreign_key_check;",
    )
}
