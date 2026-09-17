//! v038 · Memory Engine（HIGHER COGNITIVE CORE V1.2 §12）。
//!
//! ## 两张表的分工
//!
//! - `memory_units`：**当前**排程状态缓存（stability / difficulty / retrievability /
//!   next_review_at / 计数）。它**不是**排程真相 —— 真相是 FSRS 算法的确定性状态，
//!   缓存在这里只是为了让「到期查询」不必逐条重算。
//! - `memory_reviews`：每次复习的**不可变账本**（`rating` /
//!   `state_before_json` / `state_after_json` / `elapsed_days` / `scheduled_days`），
//!   使任何一次排程变化都可被审计与重建。
//!
//! ## 为什么不能复用既有 `memory` 相关表
//!
//! 仓库已有 `repository/memory.rs`（AI 记忆 / 用户上下文记忆）。其语义是
//! **AI 对用户的理解**（`ai_memories` / `memory_records`），与「一个学习项的记忆强度」
//! 完全不同的领域，且不含 FSRS 状态字段。按 §5.1 的 4 问检查：
//! ①列不足（无 stability/difficulty/retrievability/next_review_at）；
//! ②枚举无法表达（无 rating / fsrs state）；
//! ③是另一 canonical 消费者的输入（污染 AI 记忆会直接改变 agent 上下文）；
//! ④无法表达「一个学习项 × 多个 memory_key」的多态。→ 四条全部不满足，新表成立。
//!
//! ## 设计约束（任务书 §12 锁定）
//!
//! - `profile_id` 与 `linked_learning_item_id` 均级联删除；
//! - 唯一键 `(profile_id, linked_learning_item_id, memory_key)`；
//! - `desired_retention` 默认 `0.90`；
//! - 三个索引名称与列序精确照抄；
//! - **不做全量 MemoryUnit 回填**（§12 / §41）；
//! - `PRAGMA foreign_key_check` 在返回前执行。
//!
//! Ledger：本次为 §7 锁定的 v038。**本次不创建 v039。**

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_units (
            id                       INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id               INTEGER NOT NULL,
            linked_learning_item_id  INTEGER NOT NULL,
            memory_key               TEXT NOT NULL,
            memory_kind              TEXT NOT NULL,
            stability                REAL NULL,
            difficulty               REAL NULL,
            retrievability           REAL NULL,
            last_review_at           TEXT NULL,
            next_review_at           TEXT NULL,
            desired_retention        REAL NOT NULL DEFAULT 0.90,
            review_count             INTEGER NOT NULL DEFAULT 0,
            lapse_count              INTEGER NOT NULL DEFAULT 0,
            fsrs_state_json          TEXT NOT NULL DEFAULT '{}',
            created_at               TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at               TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY(profile_id) REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY(linked_learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE,
            UNIQUE(profile_id, linked_learning_item_id, memory_key)
        );

        CREATE TABLE IF NOT EXISTS memory_reviews (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL,
            memory_unit_id      INTEGER NOT NULL,
            learning_moment_id  INTEGER NULL,
            rating              TEXT NOT NULL,
            reviewed_at         TEXT NOT NULL,
            elapsed_days        INTEGER NOT NULL,
            scheduled_days      INTEGER NOT NULL,
            state_before_json   TEXT NOT NULL,
            state_after_json    TEXT NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY(profile_id) REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY(memory_unit_id) REFERENCES memory_units(id) ON DELETE CASCADE,
            FOREIGN KEY(learning_moment_id) REFERENCES learning_moments(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_memory_units_due
        ON memory_units(profile_id, next_review_at, id);

        CREATE INDEX IF NOT EXISTS idx_memory_units_item
        ON memory_units(profile_id, linked_learning_item_id, id);

        CREATE INDEX IF NOT EXISTS idx_memory_reviews_unit_time
        ON memory_reviews(profile_id, memory_unit_id, reviewed_at DESC, id DESC);

        PRAGMA foreign_key_check;",
    )
}
