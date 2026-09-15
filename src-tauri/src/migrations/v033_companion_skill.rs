//! v033 · Companion Skill V1（M4）+ Companion World / Expedition / Return（M5）。
//!
//! ## 为什么是新表（而不是复用既有表）
//!
//! Companion 是**独立产品子系统**（任务书 §M4 开头），它拥有自己的真相：
//!
//! ```text
//! identity / personality seed / current behavior state
//! world & expedition state / companion memories / bounded dialogue summary
//! interaction cooldowns
//! ```
//!
//! 而它**不拥有**（必须从 canonical learning 系统读取，绝不复制）：
//!
//! ```text
//! task truth / learning mastery truth / today learning minutes truth
//! evaluation truth / next action ranking truth
//! ```
//!
//! 既有表中没有任何一张能承载上面这组语义（`study_profiles` 是学习档案，
//! `ai_*` 是 AI 运行记录，`evaluations` / `study_sessions` 是学习真相）。
//! 因此单次迁移建立 V1 的五张表。
//!
//! ## 硬约束（逐条对应任务书）
//!
//! - **不建模「能量钱包 / 可花费学习余额」**（§M4-C / §M5-C）。因此本迁移
//!   **没有** balance / coins / energy / fuel 之类的列：readiness 是**派生状态**
//!   （`expedition_readiness`），不是余额；`companion_expeditions` 只记录
//!   「机会已被兑现成哪一次远征」，不记录「花了多少」。
//! - **不在 SQLite 存大二进制资产**（§M4-C 结尾）：五张表全部只有文本/整数列，
//!   没有任何 BLOB；memory 只存 `kind/title/body` 文本。
//! - **无后台 tick**（§M5-B）：`finished_at` 在**开始**时一次算定
//!   （`started_at + duration_seconds`），因此「now >= finished_at → 完成」是纯函数，
//!   不需要任何后台 CPU 定时器。
//! - `profile_id` 一律外键级联：删除学习档案 → 该档案全部 companion 数据一并清除
//!   （§12 Profile Isolation 的物理基础；逻辑隔离仍由每个 repository 查询强制）。
//!
//! Ledger：施工前实测 `latest_version() == 32`，故本次为**下一个连续版本** v033
//! （§M4-C「never rewrite history」）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        // ---- companion_profiles：身份 + 性格种子（持久，不随每次访问重建）----
        "CREATE TABLE IF NOT EXISTS companion_profiles (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            -- 一个学习档案恒对应一个 companion 身份
            profile_id       INTEGER NOT NULL UNIQUE REFERENCES study_profiles(id) ON DELETE CASCADE,
            -- 稳定身份 key（不是显示名；显示名走 nickname）
            companion_id     TEXT NOT NULL,
            -- 性格原型 key（有限枚举，见 companion::types::ARCHETYPES）
            archetype        TEXT NOT NULL,
            -- 用户起的名字（可空 = 未起名）
            nickname         TEXT,
            -- 确定性种子：由 profile_id 派生，保证同一档案恒得同一性格表现
            personality_seed INTEGER NOT NULL,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- ---- companion_world_state：世界 + 行为状态（每档案一行）----
        CREATE TABLE IF NOT EXISTS companion_world_state (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id            INTEGER NOT NULL UNIQUE REFERENCES study_profiles(id) ON DELETE CASCADE,
            -- 派生的远征就绪度（**不是**余额；见 companion::readiness 的锁定策略）
            expedition_readiness  TEXT NOT NULL DEFAULT 'NOT_READY'
                                  CHECK (expedition_readiness IN
                                         ('NOT_READY','READY_SHORT','READY_MEDIUM','READY_LONG')),
            readiness_updated_at  TEXT,
            -- 当前场景（自由文本：V1 只有 home / wilds；不设 CHECK 以便后续扩展）
            current_scene         TEXT NOT NULL DEFAULT 'home',
            -- 当前行为状态（§M4-B 的七个状态）
            current_behavior      TEXT NOT NULL DEFAULT 'idle'
                                  CHECK (current_behavior IN
                                         ('idle','curious','resting','expedition','returning','celebrating','recovery')),
            last_interaction_at   TEXT,
            -- §M4-G：每次「来访/返回」最多一次主动学习邀请
            last_nudge_at         TEXT,
            updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- ---- companion_events：companion 侧事件（含未决的 return 事件）----
        CREATE TABLE IF NOT EXISTS companion_events (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id   INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            event_type   TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at  TEXT
        );

        -- ---- companion_expeditions：远征事实（含确定性结算所需的全部字段）----
        CREATE TABLE IF NOT EXISTS companion_expeditions (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id              INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            -- running → ready（now >= finished_at）→ collected
            status                  TEXT NOT NULL DEFAULT 'running'
                                    CHECK (status IN ('running','ready','collected')),
            started_at              TEXT NOT NULL DEFAULT (datetime('now')),
            duration_seconds        INTEGER NOT NULL CHECK (duration_seconds > 0),
            -- §M5-B：开始时一次算定 = started_at + duration_seconds（无需后台 tick）
            finished_at             TEXT,
            -- 起程时的就绪档位（只可能是三档「可远征」之一）
            readiness_tier_at_start TEXT NOT NULL
                                    CHECK (readiness_tier_at_start IN
                                           ('READY_SHORT','READY_MEDIUM','READY_LONG')),
            -- §M5-B/§M5-E：确定性结果的全部输入（同一 seed + 同一 tier → 同一故事）
            seed                    INTEGER NOT NULL,
            theme                   TEXT NOT NULL,
            collected_at            TEXT
        );

        -- ---- companion_memories：返回时留下的记忆/收藏（纯文本，无二进制）----
        CREATE TABLE IF NOT EXISTS companion_memories (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id  INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            kind        TEXT NOT NULL,
            title       TEXT NOT NULL,
            body        TEXT NOT NULL,
            -- 可选来源弱引用（无 FK：与 micro_learning_events.source_id 同规矩）
            source_type TEXT,
            source_id   INTEGER,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 当前未收口远征（state machine 的输入）：一次索引命中
        CREATE INDEX IF NOT EXISTS idx_companion_expeditions_open
            ON companion_expeditions(profile_id, status, finished_at);

        CREATE INDEX IF NOT EXISTS idx_companion_events_profile_time
            ON companion_events(profile_id, created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_companion_memories_profile_time
            ON companion_memories(profile_id, created_at DESC);

        PRAGMA foreign_key_check;",
    )
}
