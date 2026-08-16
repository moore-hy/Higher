//! Vault 保险箱（DEV-0052 / PHASE U §156-173）。
//!
//! 独立 SQLite（HigherVault.hvault，≠ higher.db）。测试版应用层锁定（非密码学安全容器，不得宣传强加密）。
//! 固定测试密码 root；默认 LOCKED；写入不受锁影响（审计/备份自动落盘）；读取/查看/导出需解锁；
//! 10 分钟无操作自动重锁；重启重锁；AI（只读/助手）都不能自己解锁——只有用户输入密码或明确说"打开保险箱"后经密码 UI。
//! Blob：SHA-256 去重 + 1MB chunk 顺序写（禁止整文件入 RAM）。
//! Snapshot：手动 + ChangeSet 应用后 + 每天首写（≤1/天）；V1 只做写入/查看/审计/导出，完整 Restore 留后续（Schema 已备）。

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const TEST_PASSWORD: &str = "root";
const AUTO_LOCK_SECS: u64 = 600;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VaultEvent {
    pub seq: i64,
    pub actor_type: String,
    pub actor_id: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<i64>,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub timestamp: String,
    pub run_id: Option<String>,
    pub change_set_id: Option<i64>,
}

pub struct VaultState {
    dir: PathBuf,
    locked: AtomicBool,
    last_activity: Mutex<Option<Instant>>,
}

impl VaultState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            dir: app_data_dir,
            locked: AtomicBool::new(true),
            last_activity: Mutex::new(None),
        }
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join("HigherVault.hvault")
    }

    /// 打开（或复用）连接；建表。写入路径不需要解锁。
    /// rusqlite Connection 非 Clone：每次新开（SQLite 打开轻量；WAL 并发安全）。
    fn open_conn(&self) -> Result<Connection, String> {
        let p = self.vault_path();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建 Vault 目录失败：{e}"))?;
        }
        let conn = Connection::open(&p).map_err(|e| format!("打开 Vault 失败：{e}"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vault_events (
                seq           INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_type    TEXT NOT NULL CHECK (actor_type IN ('USER','AI','SYSTEM')),
                actor_id      TEXT NOT NULL DEFAULT '',
                action        TEXT NOT NULL,
                entity_type   TEXT NOT NULL DEFAULT '',
                entity_id     INTEGER,
                before_json   TEXT,
                after_json    TEXT,
                timestamp     TEXT NOT NULL DEFAULT (datetime('now')),
                run_id        TEXT,
                change_set_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_ve_time ON vault_events(seq DESC);
            CREATE TABLE IF NOT EXISTS vault_blobs (
                sha256 TEXT PRIMARY KEY,
                size   INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS vault_blob_chunks (
                sha256      TEXT NOT NULL REFERENCES vault_blobs(sha256) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                data        BLOB NOT NULL,
                PRIMARY KEY (sha256, chunk_index)
            );
            CREATE TABLE IF NOT EXISTS vault_snapshots (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                kind         TEXT NOT NULL CHECK (kind IN ('manual','changeset','daily')),
                db_size      INTEGER NOT NULL DEFAULT 0,
                manifest_json TEXT NOT NULL DEFAULT '{}',
                created_at   TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS vault_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| format!("初始化 Vault Schema 失败：{e}"))?;
        Ok(conn)
    }

    pub fn is_locked(&self) -> bool {
        // 自动锁检查（§163）
        if let Ok(mut la) = self.last_activity.lock() {
            if let Some(t) = *la {
                if t.elapsed().as_secs() >= AUTO_LOCK_SECS {
                    self.locked.store(true, Ordering::SeqCst);
                    *la = None;
                }
            }
        }
        self.locked.load(Ordering::SeqCst)
    }

    /// §162：只有用户输入密码可解锁（AI 无路径调用此方法——command 层唯一入口由 UI 密码框触发）。
    pub fn unlock(&self, password: &str) -> Result<(), String> {
        if password != TEST_PASSWORD {
            self.record_system("vault_unlock_failed", "vault", None, "密码错误");
            return Err("密码错误（测试版密码为 root）".to_string());
        }
        self.locked.store(false, Ordering::SeqCst);
        if let Ok(mut la) = self.last_activity.lock() {
            *la = Some(Instant::now());
        }
        self.record_system("vault_unlocked", "vault", None, "");
        Ok(())
    }

    pub fn lock(&self) {
        self.locked.store(true, Ordering::SeqCst);
        if let Ok(mut la) = self.last_activity.lock() {
            *la = None;
        }
        self.record_system("vault_locked", "vault", None, "");
    }

    fn touch(&self) {
        if let Ok(mut la) = self.last_activity.lock() {
            *la = Some(Instant::now());
        }
    }

    // ---------- 事件写入（不受锁影响 §161） ----------

    pub fn record(
        &self,
        actor_type: &str,
        actor_id: &str,
        action: &str,
        entity_type: &str,
        entity_id: Option<i64>,
        before_json: Option<&str>,
        after_json: Option<&str>,
        run_id: Option<&str>,
        change_set_id: Option<i64>,
    ) -> Result<(), String> {
        let conn = self.open_conn()?;
        conn.execute(
            "INSERT INTO vault_events (actor_type, actor_id, action, entity_type, entity_id, before_json, after_json, run_id, change_set_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![actor_type, actor_id, action, entity_type, entity_id, before_json, after_json, run_id, change_set_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_user(&self, action: &str, entity_type: &str, entity_id: Option<i64>, detail: &str) {
        let _ = self.record("USER", "", action, entity_type, entity_id, None, Some(detail), None, None);
    }

    pub fn record_ai(&self, action: &str, run_id: &str, detail: &str) {
        let _ = self.record("AI", "", action, "ai", None, None, Some(detail), Some(run_id), None);
    }

    pub fn record_system(&self, action: &str, entity_type: &str, entity_id: Option<i64>, detail: &str) {
        let _ = self.record("SYSTEM", "", action, entity_type, entity_id, None, Some(detail), None, None);
    }

    /// 读取审计（需解锁）。
    pub fn list_events(&self, limit: i64) -> Result<Vec<VaultEvent>, String> {
        if self.is_locked() {
            return Err("保险箱处于锁定状态，请先解锁（测试版密码为 root）".to_string());
        }
        self.touch();
        let conn = self.open_conn()?;
        let mut stmt = conn
            .prepare("SELECT seq, actor_type, actor_id, action, entity_type, entity_id, before_json, after_json, timestamp, run_id, change_set_id
                      FROM vault_events ORDER BY seq DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit.clamp(1, 500)], |r| {
                Ok(VaultEvent {
                    seq: r.get(0)?,
                    actor_type: r.get(1)?,
                    actor_id: r.get(2)?,
                    action: r.get(3)?,
                    entity_type: r.get(4)?,
                    entity_id: r.get(5)?,
                    before_json: r.get(6)?,
                    after_json: r.get(7)?,
                    timestamp: r.get(8)?,
                    run_id: r.get(9)?,
                    change_set_id: r.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // ---------- Blob（§169-170：SHA-256 去重 + 1MB chunk 顺序写） ----------

    pub fn store_blob(&self, path: &Path) -> Result<String, String> {
        let mut f = std::fs::File::open(path).map_err(|e| format!("打开文件失败：{e}"))?;
        use std::io::Read as _;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut buf = vec![0u8; 1024 * 1024];
        // 流式两遍：先 hash（顺序读），再分块写（seek 回）
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total += n as u64;
        }
        let sha = format!("{:x}", hasher.finalize());
        let conn = self.open_conn()?;
        let exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM vault_blobs WHERE sha256=?1", params![sha], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            return Ok(sha); // 去重
        }
        conn.execute("INSERT INTO vault_blobs (sha256, size) VALUES (?1,?2)", params![sha, total as i64])
            .map_err(|e| e.to_string())?;
        use std::io::Seek as _;
        f.rewind().map_err(|e| e.to_string())?;
        let mut idx = 0i64;
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            conn.execute(
                "INSERT INTO vault_blob_chunks (sha256, chunk_index, data) VALUES (?1,?2,?3)",
                params![sha, idx, &buf[..n]],
            )
            .map_err(|e| e.to_string())?;
            idx += 1;
        }
        Ok(sha)
    }

    // ---------- Snapshot（§171-172） ----------

    pub fn snapshot(&self, kind: &str, main_db: Option<&Path>) -> Result<i64, String> {
        let k = match kind {
            "manual" | "changeset" | "daily" => kind,
            _ => "manual",
        };
        let conn = self.open_conn()?;
        // 每日限额（§171：每天首次成功重要写入最多 1 个）
        if k == "daily" {
            let today: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM vault_snapshots WHERE kind='daily' AND date(created_at)=date('now')",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if today > 0 {
                return Ok(0);
            }
        }
        if k == "changeset" {
            let today: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM vault_snapshots WHERE kind='changeset' AND date(created_at)=date('now')",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if today > 0 {
                return Ok(0);
            }
        }
        // 复制 higher.db 到 vault 快照目录（§172）
        let mut size = 0i64;
        let mut manifest = serde_json::json!({});
        if let Some(db) = main_db {
            if db.exists() {
                let snap_dir = self.dir.join("snapshots");
                std::fs::create_dir_all(&snap_dir).map_err(|e| e.to_string())?;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let target = snap_dir.join(format!("higher-{}-{}.db", k, stamp));
                std::fs::copy(db, &target).map_err(|e| format!("备份 higher.db 失败：{e}"))?;
                size = std::fs::metadata(&target).map(|m| m.len() as i64).unwrap_or(0);
                manifest = serde_json::json!({ "db_file": target.file_name().and_then(|n| n.to_str()), "db_size": size });
            }
        }
        conn.execute(
            "INSERT INTO vault_snapshots (kind, db_size, manifest_json) VALUES (?1,?2,?3)",
            params![k, size, manifest.to_string()],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    /// 快照列表（查看需解锁）。
    pub fn list_snapshots(&self) -> Result<Vec<(i64, String, i64, String)>, String> {
        if self.is_locked() {
            return Err("保险箱处于锁定状态".to_string());
        }
        self.touch();
        let conn = self.open_conn()?;
        let mut stmt = conn
            .prepare("SELECT id, kind, db_size, created_at FROM vault_snapshots ORDER BY id DESC LIMIT 100")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn stats(&self) -> Result<(i64, i64, i64), String> {
        let conn = self.open_conn()?;
        let events: i64 = conn
            .query_row("SELECT COUNT(*) FROM vault_events", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let blobs: i64 = conn
            .query_row("SELECT COUNT(*) FROM vault_blobs", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let snaps: i64 = conn
            .query_row("SELECT COUNT(*) FROM vault_snapshots", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok((events, blobs, snaps))
    }

    /// §111 导出（解锁后）：导出审计为 JSON 文本。
    pub fn export_events_json(&self) -> Result<String, String> {
        let events = self.list_events(500)?;
        serde_json::to_string_pretty(&events).map_err(|e| e.to_string())
    }
}
