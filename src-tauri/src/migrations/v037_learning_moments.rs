//! v037 · Learning Moments（HIGHER COGNITIVE CORE V1.2 §8）。
//!
//! ## 为什么必须是一张新表（THIRD-PARTY FIRST / REUSE-BEFORE-ADD 的定向检查结论）
//!
//! 施工前按任务书 §4/§5.1 的顺序检查既有表能否承载 Learning Moment 语义：
//!
//! 1. **`evaluations`** —— 承载不了。它的 `evaluation_type` 是 canonical 六值集合，
//!    `outcome` 是成功/失败语义；而 Learning Moment 的核心是**「发生了一次什么学习行为」
//!    的 append-only 事实**（20 种 moment_type，含 `hint_requested`、`confusion_detected`、
//!    `interest_signal` 这类**无成败**事件）。把无成败事件塞进 `evaluations` 会污染
//!    canonical 学习证据（`ai::learning_load` 的输入），与 §4「不替换既有 canonical truth」
//!    正相反。
//! 2. **`micro_learning_events`** —— 承载不了。它是既有 Micro Action primitive
//!    （4 种 action_type + 3 种 result），语义窄于 Learning Moment，且其 `action_type`
//!    带 CHECK 约束。Learning Moment 必须能表达 recall/practice/transfer/error 的
//!    完整生命周期与 `hint_level` / `confidence` / `evidence_quality` 三要素。
//! 3. **`evidence_quality` 与 `metadata_json` 无处安放**，而二者是 §10 证据阶梯
//!    与 §9 provenance 要求的硬字段。
//!
//! 结论：**语义不足**，新表被任务书 §8 直接锁定为 `learning_moments`。
//!
//! ## 设计约束（任务书 §8 锁定）
//!
//! - `profile_id` 级联删除（档案删除 → 其 moments 一并清除）；
//! - `session_id` / `learning_item_id` / `goal_id` 删除后 **SET NULL**：
//!   **证据是历史事实**，来源消失不抹除「当时发生过一次学习」这件事；
//! - 三个索引名称与列序**精确**照抄任务书 §8，不增不减；
//! - **无触发器**（§8 明确 "Do not add triggers"）；
//! - `up()` 幂等（`IF NOT EXISTS`）；返回前执行 `PRAGMA foreign_key_check`；
//! - **不从旧 session 回填合成 moment**：历史缺失 = `unknown`，**不是 failure**。
//!
//! Ledger：本次为 §7 锁定的 v037（施工前仓库最高为 v036_ai_provider_secret_ref）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS learning_moments (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL,
            session_id          INTEGER NULL,
            learning_item_id    INTEGER NULL,
            goal_id             INTEGER NULL,
            moment_type         TEXT NOT NULL,
            occurred_at         TEXT NOT NULL,
            source_type         TEXT NOT NULL,
            source_id           TEXT NULL,
            result              TEXT NULL,
            hint_level          INTEGER NULL,
            confidence          TEXT NULL,
            evidence_quality    TEXT NOT NULL,
            metadata_json       TEXT NOT NULL DEFAULT '{}',
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY(profile_id) REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY(session_id) REFERENCES study_sessions(id) ON DELETE SET NULL,
            FOREIGN KEY(learning_item_id) REFERENCES learning_items(id) ON DELETE SET NULL,
            FOREIGN KEY(goal_id) REFERENCES goals(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_learning_moments_profile_time
        ON learning_moments(profile_id, occurred_at DESC, id DESC);

        CREATE INDEX IF NOT EXISTS idx_learning_moments_item_time
        ON learning_moments(profile_id, learning_item_id, occurred_at DESC, id DESC);

        CREATE INDEX IF NOT EXISTS idx_learning_moments_session
        ON learning_moments(profile_id, session_id, id);

        PRAGMA foreign_key_check;",
    )
}
