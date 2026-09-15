//! v030 · Planning Intake Drafts（PRODUCT-2.0 §24.3）。
//!
//! 规划入口草稿区。这是 Draft，**不是 Formal Truth**。
//!
//! §0A.4 数据 Truth 层级：
//!   Formal DB Truth > Confirmed Memory > **Current Planning Intake** > Retrieved > Conversation
//!
//! 因此本表：
//! - 只存用户/AI 在入口阶段整理的原始与结构化草稿；
//! - 绝不直接投影到 goals / tasks / planning_blueprints；
//! - 只有在用户对 ONE ChangeSet 明确确认（Apply）后，才由 ChangeSet 引擎写入正式表。
//!
//! Ledger 说明（§0C.3）：本文件为 WAVE 3 实际执行时登记的编号。
//! 原 Ledger 预留 PlanningIntake = v034；因 migration 必须与仓库现有最高版本连续，
//! 实际执行为 **v030**，其余预留槽位整体顺延（见 .higher/HIGHER_PRODUCT_2_PROGRESS.md）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS planning_intake_drafts (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id        INTEGER NOT NULL UNIQUE
                              REFERENCES study_profiles(id) ON DELETE CASCADE,
            -- how this draft came in: chat | taskbook | description | import
            source_kind       TEXT NOT NULL DEFAULT 'chat',
            -- 用户原文（对话/导入文本）
            raw_text          TEXT,
            -- 结构化草稿（§24.2 任务书 9 段 → JSON）；解析失败保持 NULL
            structured_json   TEXT,
            -- 完成度（缺哪些段 / 哪些字段为空）→ UI 提示，不阻塞
            completeness_json TEXT,
            -- draft | ready | consumed（consumed = 已生成 ChangeSet 并确认应用）
            status            TEXT NOT NULL DEFAULT 'draft',
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_planning_intake_status
            ON planning_intake_drafts(profile_id, status);

        PRAGMA foreign_key_check;",
    )
}
