//! v028 · Local Sync Foundation（DEV-SYNC-001）。
//!
//! Windows ↔ Android 局域网本地同步的数据层基础，全部为**旁路基础设施**：
//! 不修改 v001-v027 的任何业务表 / FK / local integer id。
//!
//! 新增表：
//! - sync_local_device  本机设备单例（UUID v4，migration 时生成）
//! - sync_entity_map    local id ↔ global sync_id 映射（Sync Identity Layer）
//! - sync_outbox        待同步变更队列（由 DB Trigger 捕获）
//! - sync_peers         已配对设备 + shared_token + 双向游标
//! - sync_runtime_guard Remote Apply 守卫（applying_remote=1 时 Trigger 不写 outbox，防回声）
//! - sync_conflicts     冲突记录（MVP：不自动覆盖，仅记录）
//!
//! 存量数据：为已有 study_profiles / goals / learning_items / tasks
//! 每行分配 UUID v4（SQL 侧生成，等价 128-bit global id），不产生 outbox。
//!
//! Trigger：4 张业务表 × AFTER INSERT / AFTER UPDATE / BEFORE DELETE，
//! 不论修改来自 UI / Repository / AI ChangeSet / Planner，只要落表即被捕获；
//! guard=1 时仅跳过（Remote Apply 由 apply.rs 显式维护 entity_map）。

use rusqlite::Connection;

#[cfg(target_os = "android")]
const DEVICE_PLATFORM: &str = "android";
#[cfg(not(target_os = "android"))]
const DEVICE_PLATFORM: &str = "windows";

#[cfg(target_os = "android")]
const DEVICE_NAME: &str = "Higher Android";
#[cfg(not(target_os = "android"))]
const DEVICE_NAME: &str = "Higher Windows";

/// SQLite 内联 UUID v4 表达式（randomblob 128-bit，供 Trigger 与存量回填使用）。
const UUID_V4_SQL: &str = "lower(hex(randomblob(4))) || '-' || \
     lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-' || \
     substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || \
     lower(hex(randomblob(6)))";

/// 单表三触发器（AFTER INSERT / AFTER UPDATE / BEFORE DELETE）。
/// `{T}` = 业务表名，`{E}` = entity_type 字面量。
const TRIGGER_SQL: &str = r#"
CREATE TRIGGER sync_{T}_after_insert AFTER INSERT ON {T}
BEGIN
    INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
    SELECT '{E}', NEW.id, {UUID}, datetime('now'), datetime('now')
    WHERE (SELECT applying_remote FROM sync_runtime_guard WHERE id = 1) = 0;

    INSERT INTO sync_outbox (entity_type, sync_id, operation, changed_at)
    SELECT '{E}', m.sync_id, 'upsert', datetime('now')
    FROM sync_entity_map m
    WHERE m.entity_type = '{E}' AND m.local_id = NEW.id
      AND m.deleted_at IS NULL
      AND (SELECT applying_remote FROM sync_runtime_guard WHERE id = 1) = 0;
END;

CREATE TRIGGER sync_{T}_after_update AFTER UPDATE ON {T}
BEGIN
    INSERT INTO sync_outbox (entity_type, sync_id, operation, changed_at)
    SELECT '{E}', m.sync_id, 'upsert', datetime('now')
    FROM sync_entity_map m
    WHERE m.entity_type = '{E}' AND m.local_id = NEW.id
      AND m.deleted_at IS NULL
      AND (SELECT applying_remote FROM sync_runtime_guard WHERE id = 1) = 0;
END;

CREATE TRIGGER sync_{T}_before_delete BEFORE DELETE ON {T}
BEGIN
    INSERT INTO sync_outbox (entity_type, sync_id, operation, changed_at)
    SELECT '{E}', m.sync_id, 'delete', datetime('now')
    FROM sync_entity_map m
    WHERE m.entity_type = '{E}' AND m.local_id = OLD.id
      AND (SELECT applying_remote FROM sync_runtime_guard WHERE id = 1) = 0;

    UPDATE sync_entity_map
    SET deleted_at = datetime('now'), updated_at = datetime('now')
    WHERE entity_type = '{E}' AND local_id = OLD.id
      AND (SELECT applying_remote FROM sync_runtime_guard WHERE id = 1) = 0;
END;
"#;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // ---- 1. 基础表 ----
    conn.execute_batch(
        "CREATE TABLE sync_local_device (
            id          INTEGER PRIMARY KEY CHECK (id = 1),
            device_id   TEXT NOT NULL UNIQUE,
            device_name TEXT NOT NULL DEFAULT '',
            platform    TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE sync_entity_map (
            entity_type TEXT NOT NULL
                CHECK (entity_type IN ('study_profile','goal','learning_item','task')),
            local_id    INTEGER NOT NULL,
            sync_id     TEXT NOT NULL UNIQUE,
            deleted_at  TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            PRIMARY KEY (entity_type, local_id)
        );

        CREATE TABLE sync_outbox (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type TEXT NOT NULL,
            sync_id     TEXT NOT NULL,
            operation   TEXT NOT NULL CHECK (operation IN ('upsert','delete')),
            changed_at  TEXT NOT NULL
        );
        CREATE INDEX idx_sync_outbox_entity ON sync_outbox(entity_type, sync_id);

        CREATE TABLE sync_peers (
            peer_device_id               TEXT PRIMARY KEY,
            peer_name                    TEXT,
            peer_platform                TEXT,
            peer_addr                    TEXT,
            shared_token                 TEXT,
            last_acked_local_change_id   INTEGER NOT NULL DEFAULT 0,
            last_received_remote_change_id INTEGER NOT NULL DEFAULT 0,
            paired_at                    TEXT NOT NULL,
            last_sync_at                 TEXT
        );

        CREATE TABLE sync_runtime_guard (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            applying_remote INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO sync_runtime_guard (id, applying_remote) VALUES (1, 0);

        CREATE TABLE sync_conflicts (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type        TEXT NOT NULL,
            sync_id            TEXT NOT NULL,
            local_change_json  TEXT NOT NULL,
            remote_change_json TEXT NOT NULL,
            created_at         TEXT NOT NULL,
            status             TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending','resolved_local','resolved_remote'))
        );",
    )?;

    // ---- 2. 本机设备单例（UUID v4，一次性生成） ----
    conn.execute_batch(&format!(
        "INSERT INTO sync_local_device (id, device_id, device_name, platform, created_at, updated_at)
         SELECT 1, {uuid}, '{name}', '{platform}', datetime('now'), datetime('now')
         WHERE NOT EXISTS (SELECT 1 FROM sync_local_device WHERE id = 1);",
        uuid = UUID_V4_SQL,
        name = DEVICE_NAME,
        platform = DEVICE_PLATFORM,
    ))?;

    // ---- 3. 存量数据 Sync Identity（不产生 outbox，作为同步基线） ----
    conn.execute_batch(&format!(
        "INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
         SELECT 'study_profile', id, {uuid}, datetime('now'), datetime('now') FROM study_profiles;
         INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
         SELECT 'goal', id, {uuid}, datetime('now'), datetime('now') FROM goals;
         INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
         SELECT 'learning_item', id, {uuid}, datetime('now'), datetime('now') FROM learning_items;
         INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
         SELECT 'task', id, {uuid}, datetime('now'), datetime('now') FROM tasks;",
        uuid = UUID_V4_SQL,
    ))?;

    // ---- 4. 变更捕获 Trigger（guard=0 才写 outbox，防 Remote Apply 回声） ----
    let triggers = [
        ("study_profiles", "study_profile"),
        ("goals", "goal"),
        ("learning_items", "learning_item"),
        ("tasks", "task"),
    ];
    for (table, entity) in triggers {
        conn.execute_batch(
            &TRIGGER_SQL
                .replace("{T}", table)
                .replace("{E}", entity)
                .replace("{UUID}", UUID_V4_SQL),
        )?;
    }

    conn.execute_batch("PRAGMA foreign_key_check;")
}
