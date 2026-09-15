//! v031 · Knowledge Canvas（PRODUCT-2.0 §35）。
//!
//! 一个 Knowledge Node 对应一块 Excalidraw 画布（spatial base），
//! 媒体/链接以 Higher Embed Layer（`knowledge_canvas_embeds`）叠加，
//! **禁止**把大图片 base64 长期塞进 elements_json、禁止把视频 base64 塞进 SQLite JSON（§35.1）。
//! 二进制一律走既有 attachment storage，Excalidraw image element 通过
//! `customData.higherAttachmentId` 与 attachment 记录关联。
//!
//! Ledger（§0C.3）：原预留 KnowledgeCanvas = v034；因 migration 必须与仓库现有最高版本连续，
//! 实际执行为 **v031**，其余预留槽位继续顺延（见 .higher/HIGHER_PRODUCT_2_PROGRESS.md）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS knowledge_canvases (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            learning_item_id INTEGER NOT NULL REFERENCES learning_items(id) ON DELETE CASCADE,
            -- Excalidraw elements 数组（只存矢量/引用，不存二进制）
            elements_json    TEXT NOT NULL DEFAULT '[]',
            -- Excalidraw appState 的持久化子集（zoom/scroll 等）
            app_state_json   TEXT,
            -- 单调递增，用于「保存冲突/旧响应覆盖新内容」防护（§38）
            revision         INTEGER NOT NULL DEFAULT 0,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(learning_item_id)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_canvases_profile
            ON knowledge_canvases(profile_id, learning_item_id);

        -- §37：视频 / 链接 / 文件 card 等媒体叠加层（Excalidraw = spatial base）
        CREATE TABLE IF NOT EXISTS knowledge_canvas_embeds (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            learning_item_id INTEGER NOT NULL REFERENCES learning_items(id) ON DELETE CASCADE,
            -- image | video | file | link
            kind             TEXT NOT NULL,
            -- 二进制一律走 attachment（§35.1）
            attachment_id    INTEGER REFERENCES learning_attachments(id) ON DELETE SET NULL,
            -- link 专用；禁止默认任意 iframe（§37：只做 title / domain / open）
            url              TEXT,
            title            TEXT,
            x                REAL NOT NULL DEFAULT 0,
            y                REAL NOT NULL DEFAULT 0,
            width            REAL NOT NULL DEFAULT 0,
            height           REAL NOT NULL DEFAULT 0,
            z_index          INTEGER NOT NULL DEFAULT 0,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_canvas_embeds_item
            ON knowledge_canvas_embeds(profile_id, learning_item_id, z_index);

        PRAGMA foreign_key_check;",
    )
}
