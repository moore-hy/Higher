//! M4/M5 — Companion 仓储：v033 五张表的**唯一**读写入口。
//!
//! 边界纪律：
//!
//! - 本仓储**只**触碰 `companion_*` 表 —— 它**不**写 `tasks` / `study_sessions` /
//!   `evaluations` / `micro_learning_events` / `learning_items`
//!   （§M4-A「Companion does NOT own learning truth」的物理保证）；
//! - 唯一例外是**只读**地取学习项名称用于 §M5-D 主题推断（`list_learning_item_names`）；
//! - 全部方法按 `profile_id` 过滤（§12 Profile Isolation）；
//! - 时间戳一律走 SQLite UTC 口径（`datetime('now')` / `datetime(?1, ?2)`），
//!   避免 M2 那类「chrono `T`/`Z` 与 SQLite 空格」的字符串比较陷阱。

use crate::companion::deterministic::stable_hash;
use crate::companion::types::{
    BehaviorState, CompanionExpedition, CompanionMemory, CompanionProfile, CompanionWorldState,
    ExpeditionReadiness, ExpeditionStatus, ARCHETYPES, COMPANION_ID, SCENE_HOME,
};
use rusqlite::{params, Connection};

/// M5-D 主题推断使用的学习项上限（RAM-light）。
pub const THEME_NAME_LIMIT: i64 = 200;

pub struct CompanionRepository<'a> {
    conn: &'a Connection,
}

impl<'a> CompanionRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ---------- 时间（统一 SQLite UTC 口径） ----------

    /// SQLite 口径的当前 UTC 时间（`YYYY-MM-DD HH:MM:SS`）。
    pub fn now(&self) -> rusqlite::Result<String> {
        self.conn
            .query_row("SELECT datetime('now')", [], |r| r.get(0))
    }

    /// `at` 之后 `seconds` 秒（SQLite UTC 口径；无 Rust 侧时间算术）。
    pub fn plus_seconds(&self, at: &str, seconds: i64) -> rusqlite::Result<String> {
        self.conn.query_row(
            "SELECT datetime(?1, ?2)",
            params![at, format!("+{} seconds", seconds)],
            |r| r.get(0),
        )
    }

    /// `at` 与「现在」相差的秒数（用于来访间隔判定）。
    pub fn seconds_since(&self, at: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT CAST(strftime('%s','now') - strftime('%s', ?1) AS INTEGER)",
            params![at],
            |r| r.get(0),
        )
    }

    /// `to` 与 `from` 相差的秒数（**纯函数**：双方都是显式传入的 UTC 文本）。
    ///
    /// 生产路径用 `to = now()`；测试注入固定 `now` 时结果依然可复算。
    pub fn seconds_between(&self, from: &str, to: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT CAST(strftime('%s', ?2) - strftime('%s', ?1) AS INTEGER)",
            params![from, to],
            |r| r.get(0),
        )
    }

    /// `at` 所属的**本地学习日**（UTC+8，YYYY-MM-DD）。
    ///
    /// 与全仓归日口径一致（`date(col, '+8 hours')`），**禁止**在 Rust 侧拿
    /// UTC 字符串的前 10 位当本地日期 —— 那正是 M2 记录过的时区口径缺陷。
    pub fn local_date_of(&self, at: &str) -> rusqlite::Result<String> {
        self.conn
            .query_row("SELECT date(?1, '+8 hours')", params![at], |r| r.get(0))
    }

    // ---------- companion_profiles ----------

    /// 身份是**持久**的：首次调用创建，之后恒返回同一行（§CS-01）。
    ///
    /// `companion_id` / `archetype` / `personality_seed` 全部由 `profile_id`
    /// 确定性派生 —— 同一档案恒得同一身份与性格表现。
    pub fn ensure_profile(&self, profile_id: i64) -> rusqlite::Result<CompanionProfile> {
        if let Some(p) = self.get_profile(profile_id)? {
            return Ok(p);
        }
        let key = format!("profile:{}", profile_id);
        let archetype = ARCHETYPES
            [stable_hash(&[key.as_str(), "archetype"]) as usize % ARCHETYPES.len()];
        let personality_seed = stable_hash(&[key.as_str(), "personality"]);
        self.conn.execute(
            "INSERT OR IGNORE INTO companion_profiles
                (profile_id, companion_id, archetype, personality_seed)
             VALUES (?1, ?2, ?3, ?4)",
            params![profile_id, COMPANION_ID, archetype, personality_seed],
        )?;
        self.get_profile(profile_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_profile(&self, profile_id: i64) -> rusqlite::Result<Option<CompanionProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, companion_id, archetype, nickname, personality_seed,
                    created_at, updated_at
               FROM companion_profiles WHERE profile_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![profile_id], |r| {
            Ok(CompanionProfile {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                companion_id: r.get(2)?,
                archetype: r.get(3)?,
                nickname: r.get(4)?,
                personality_seed: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?;
        rows.next().transpose()
    }

    /// 设置昵称（唯一的身份可变字段；不影响 companion_id 与 seed）。
    pub fn set_nickname(&self, profile_id: i64, nickname: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_profiles
                SET nickname = ?2, updated_at = datetime('now')
              WHERE profile_id = ?1",
            params![profile_id, nickname],
        )?;
        Ok(())
    }

    // ---------- companion_world_state ----------

    pub fn ensure_world_state(&self, profile_id: i64) -> rusqlite::Result<CompanionWorldState> {
        self.conn.execute(
            "INSERT OR IGNORE INTO companion_world_state (profile_id) VALUES (?1)",
            params![profile_id],
        )?;
        self.get_world_state(profile_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_world_state(
        &self,
        profile_id: i64,
    ) -> rusqlite::Result<Option<CompanionWorldState>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, expedition_readiness, readiness_updated_at, current_scene,
                    current_behavior, last_interaction_at, last_nudge_at, updated_at
               FROM companion_world_state WHERE profile_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![profile_id], |r| {
            let readiness_raw: String = r.get(2)?;
            let behavior_raw: String = r.get(5)?;
            Ok(CompanionWorldState {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                expedition_readiness: ExpeditionReadiness::parse(&readiness_raw)
                    .unwrap_or(ExpeditionReadiness::NotReady),
                readiness_updated_at: r.get(3)?,
                current_scene: r.get(4)?,
                current_behavior: BehaviorState::parse(&behavior_raw).unwrap_or(BehaviorState::Idle),
                last_interaction_at: r.get(6)?,
                last_nudge_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        rows.next().transpose()
    }

    /// 落库本次派生的就绪度（§M5-C 快照）+ 场景/行为。
    pub fn set_readiness_and_scene(
        &self,
        profile_id: i64,
        readiness: ExpeditionReadiness,
        scene: &str,
        behavior: BehaviorState,
        at: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_world_state
                SET expedition_readiness = ?2,
                    readiness_updated_at = ?3,
                    current_scene = ?4,
                    current_behavior = ?5,
                    updated_at = datetime('now')
              WHERE profile_id = ?1",
            params![profile_id, readiness.as_str(), at, scene, behavior.as_str()],
        )?;
        Ok(())
    }

    pub fn touch_interaction(&self, profile_id: i64, at: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_world_state
                SET last_interaction_at = ?2, updated_at = datetime('now')
              WHERE profile_id = ?1",
            params![profile_id, at],
        )?;
        Ok(())
    }

    pub fn set_last_nudge(&self, profile_id: i64, at: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_world_state
                SET last_nudge_at = ?2, updated_at = datetime('now')
              WHERE profile_id = ?1",
            params![profile_id, at],
        )?;
        Ok(())
    }

    // ---------- companion_events ----------

    pub fn insert_event(
        &self,
        profile_id: i64,
        event_type: &str,
        payload_json: &str,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO companion_events (profile_id, event_type, payload_json)
             VALUES (?1, ?2, ?3)",
            params![profile_id, event_type, payload_json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// §M4-G：结清该档案所有未决的学习邀请事件（用户已回应）。
    pub fn resolve_open_nudge_events(&self, profile_id: i64, at: &str) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "UPDATE companion_events
                SET resolved_at = ?2
              WHERE profile_id = ?1
                AND event_type = 'learning_nudge'
                AND resolved_at IS NULL",
            params![profile_id, at],
        )?;
        Ok(n)
    }

    /// 是否存在未决的学习邀请（同一来访内不得二次邀请的持久依据之一）。
    pub fn has_open_nudge(&self, profile_id: i64) -> rusqlite::Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM companion_events
              WHERE profile_id = ?1 AND event_type = 'learning_nudge' AND resolved_at IS NULL",
            params![profile_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn count_events(&self, profile_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM companion_events WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )
    }

    // ---------- companion_expeditions ----------

    fn parse_expedition(r: &rusqlite::Row<'_>) -> rusqlite::Result<CompanionExpedition> {
        let status_raw: String = r.get(2)?;
        let tier_raw: String = r.get(6)?;
        Ok(CompanionExpedition {
            id: r.get(0)?,
            profile_id: r.get(1)?,
            status: ExpeditionStatus::parse(&status_raw).unwrap_or(ExpeditionStatus::Running),
            started_at: r.get(3)?,
            duration_seconds: r.get(4)?,
            finished_at: r.get(5)?,
            readiness_tier_at_start: ExpeditionReadiness::parse(&tier_raw)
                .unwrap_or(ExpeditionReadiness::NotReady),
            seed: r.get(7)?,
            theme: r.get(8)?,
            collected_at: r.get(9)?,
        })
    }

    const EXP_COLS: &'static str = "id, profile_id, status, started_at, duration_seconds, \
                                    finished_at, readiness_tier_at_start, seed, theme, collected_at";

    /// 进行中的远征（未到 `finished_at`）。
    pub fn open_expedition(&self, profile_id: i64) -> rusqlite::Result<Option<CompanionExpedition>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM companion_expeditions
              WHERE profile_id = ?1 AND status = 'running'
              ORDER BY id DESC LIMIT 1",
            Self::EXP_COLS
        ))?;
        let mut rows = stmt.query_map(params![profile_id], Self::parse_expedition)?;
        rows.next().transpose()
    }

    /// 已完成、等待收取的远征。
    pub fn ready_expedition(
        &self,
        profile_id: i64,
    ) -> rusqlite::Result<Option<CompanionExpedition>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM companion_expeditions
              WHERE profile_id = ?1 AND status = 'ready'
              ORDER BY finished_at ASC, id ASC LIMIT 1",
            Self::EXP_COLS
        ))?;
        let mut rows = stmt.query_map(params![profile_id], Self::parse_expedition)?;
        rows.next().transpose()
    }

    /// 是否存在**未收口**的远征（running 或 ready）—— 就绪度占位的依据。
    pub fn has_uncollected_expedition(&self, profile_id: i64) -> rusqlite::Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM companion_expeditions
              WHERE profile_id = ?1 AND status IN ('running','ready')",
            params![profile_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_expedition(
        &self,
        profile_id: i64,
        started_at: &str,
        duration_seconds: i64,
        finished_at: &str,
        tier: ExpeditionReadiness,
        seed: i64,
        theme: &str,
    ) -> rusqlite::Result<CompanionExpedition> {
        self.conn.execute(
            "INSERT INTO companion_expeditions
                (profile_id, status, started_at, duration_seconds, finished_at,
                 readiness_tier_at_start, seed, theme)
             VALUES (?1, 'running', ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                profile_id,
                started_at,
                duration_seconds,
                finished_at,
                tier.as_str(),
                seed,
                theme
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_expedition(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_expedition(&self, id: i64) -> rusqlite::Result<Option<CompanionExpedition>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM companion_expeditions WHERE id = ?1",
            Self::EXP_COLS
        ))?;
        let mut rows = stmt.query_map(params![id], Self::parse_expedition)?;
        rows.next().transpose()
    }

    /// §M5-B：`now >= finished_at` → 完成（**纯函数**，无需后台 tick）。
    ///
    /// `at` 由调用方显式传入（生产 = `now()`），因此「关掉 App 再打开」与
    /// 「一直开着」得到**完全相同**的结算结果。
    pub fn list_settleable(&self, profile_id: i64, at: &str) -> rusqlite::Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM companion_expeditions
              WHERE profile_id = ?1
                AND status = 'running'
                AND finished_at IS NOT NULL
                AND finished_at <= ?2
              ORDER BY finished_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![profile_id, at], |r| r.get::<_, i64>(0))?;
        rows.collect()
    }

    pub fn mark_ready(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_expeditions SET status = 'ready' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn mark_collected(&self, id: i64, at: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE companion_expeditions
                SET status = 'collected', collected_at = ?2
              WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    pub fn count_expeditions_by_status(
        &self,
        profile_id: i64,
        status: ExpeditionStatus,
    ) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM companion_expeditions WHERE profile_id = ?1 AND status = ?2",
            params![profile_id, status.as_str()],
            |r| r.get(0),
        )
    }

    // ---------- companion_memories ----------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_memory(
        &self,
        profile_id: i64,
        kind: &str,
        title: &str,
        body: &str,
        source_type: Option<&str>,
        source_id: Option<i64>,
    ) -> rusqlite::Result<CompanionMemory> {
        self.conn.execute(
            "INSERT INTO companion_memories
                (profile_id, kind, title, body, source_type, source_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![profile_id, kind, title, body, source_type, source_id],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_memory(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_memory(&self, id: i64) -> rusqlite::Result<Option<CompanionMemory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, kind, title, body, source_type, source_id, created_at
               FROM companion_memories WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(CompanionMemory {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                source_type: r.get(5)?,
                source_id: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn list_memories(
        &self,
        profile_id: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<CompanionMemory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, kind, title, body, source_type, source_id, created_at
               FROM companion_memories
              WHERE profile_id = ?1
              ORDER BY created_at DESC, id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, limit], |r| {
            Ok(CompanionMemory {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                source_type: r.get(5)?,
                source_id: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn count_memories(&self, profile_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM companion_memories WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )
    }

    // ---------- 只读：学习项名称（§M5-D 主题推断） ----------

    /// **只读**取本档案的学习项名称（用于主题推断）。
    ///
    /// 这是本仓储唯一触及非 `companion_*` 表的地方，且只是读取展示名，
    /// **不写、不改、不复制**任何学习真相。
    pub fn list_learning_item_names(&self, profile_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM learning_items
              WHERE profile_id = ?1
              ORDER BY id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, THEME_NAME_LIMIT], |r| {
            r.get::<_, String>(0)
        })?;
        rows.collect()
    }

    /// 场景默认值（新档案）。
    pub fn default_scene() -> &'static str {
        SCENE_HOME
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::companion::types::THEME_GENERAL;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn mk_profile(conn: &Connection, name: &str) -> i64 {
        crate::repository::study_profile::StudyProfileRepository::new(conn)
            .create(name, None, None, None, None, None)
            .unwrap()
            .id
    }

    #[test]
    fn identity_is_derived_and_persistent() {
        let conn = setup();
        let p = mk_profile(&conn, "R-1");
        let repo = CompanionRepository::new(&conn);
        let a = repo.ensure_profile(p).unwrap();
        let b = repo.ensure_profile(p).unwrap();
        assert_eq!(a.id, b.id, "身份必须持久（不是每次新建）");
        assert_eq!(a.companion_id, COMPANION_ID);
        assert!(ARCHETYPES.contains(&a.archetype.as_str()));
        assert_eq!(a.personality_seed, b.personality_seed);
        assert!(a.personality_seed >= 0);
    }

    #[test]
    fn finished_at_is_set_at_start_so_no_tick_is_needed() {
        let conn = setup();
        let p = mk_profile(&conn, "R-2");
        let repo = CompanionRepository::new(&conn);
        let now = repo.now().unwrap();
        let fin = repo.plus_seconds(&now, 1200).unwrap();
        let e = repo
            .insert_expedition(p, &now, 1200, &fin, ExpeditionReadiness::ReadyShort, 7, THEME_GENERAL)
            .unwrap();
        assert_eq!(e.status, ExpeditionStatus::Running);
        assert!(e.finished_at.is_some());
        // 尚未到点 → 不可结算
        assert!(repo.list_settleable(p, &now).unwrap().is_empty());
        // 把 finished_at 拨到过去 → 立刻可结算（模拟「关掉 App 再打开」）
        conn.execute(
            "UPDATE companion_expeditions SET finished_at = datetime('now','-1 second') WHERE id = ?1",
            params![e.id],
        )
        .unwrap();
        assert_eq!(repo.list_settleable(p, &now).unwrap(), vec![e.id]);
        repo.mark_ready(e.id).unwrap();
        assert_eq!(repo.ready_expedition(p).unwrap().unwrap().id, e.id);
        assert!(repo.has_uncollected_expedition(p).unwrap());
    }

    #[test]
    fn profile_isolation_is_enforced_in_queries() {
        let conn = setup();
        let a = mk_profile(&conn, "R-A");
        let b = mk_profile(&conn, "R-B");
        let repo = CompanionRepository::new(&conn);
        repo.ensure_profile(a).unwrap();
        repo.insert_event(a, "x", "{}").unwrap();
        repo.insert_memory(a, "k", "t", "b", None, None).unwrap();
        assert_eq!(repo.count_events(b).unwrap(), 0);
        assert_eq!(repo.count_memories(b).unwrap(), 0);
        assert!(repo.get_profile(b).unwrap().is_none());
    }
}
