//! SecretMigrationService（POST-M7 AI FOUNDATION §S3-E / FINAL CORRECTION PATCH
//! §C2..§C6）。
//!
//! 唯一职责：legacy SQLite plaintext credential → OS SecretStore 的**应用层**
//! 可恢复搬移。它不是第二个 migration framework，也绝不进 schema migration 的
//! `up()`（§4.5：SQLite transaction ≠ Windows Credential Manager transaction，
//! 无法原子提交）。
//!
//! 三段式编排（S3-D1：OS 凭据 I/O 永不在持有 DB 锁时执行）：
//!
//! ```text
//! 1. collect_pending(conn)   -- 短 DB 读：snapshot 待迁移 (profile_id, plaintext)
//! 2. store_pending(store, ..) -- 【无 DB 锁】SecretStore.set + read-back verify
//! 3. commit_migrated(conn, ..) -- 重新拿锁：短事务写 secret_ref + 清空 plaintext
//! ```
//!
//! 失败契约（§C4）：
//! - SecretStore.set 失败 / read-back 失败 → 该行**不提交**，DB plaintext 原样保留，
//!   secret_ref 不伪造 → runtime 仍可回退（数据可恢复 > 零 orphan）；
//! - commit 失败 → best-effort SecretStore.delete（允许暂时 orphan，不丢数据）。
//!
//! 幂等 / 可恢复（§C6）：每次启动或进入 AI Settings 可安全重跑；
//! 仅处理 `auth_mode='bearer' AND api_key != '' AND secret_ref IS NULL`；
//! 已迁移成功的不再重写；crash halfway → 下次启动安全重试。

use rusqlite::{params, Connection};

use super::secret_store::{generate_secret_ref, SecretStore};

/// 待迁移凭据（短 DB 读的内存快照；不落日志）。
pub struct PendingSecretMigration {
    pub profile_id: i64,
    pub plaintext: String,
}

/// 已写入 SecretStore 并通过 read-back verify 的凭据。
pub struct StoredSecret {
    pub profile_id: i64,
    pub secret_ref: String,
}

/// Step 1（短 DB 读）：收集待迁移行。调用方拿锁→调用→立刻释放。
pub fn collect_pending(conn: &Connection) -> Result<Vec<PendingSecretMigration>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, api_key FROM ai_provider_profiles
             WHERE auth_mode = 'bearer' AND api_key != '' AND secret_ref IS NULL",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(PendingSecretMigration {
                profile_id: r.get(0)?,
                plaintext: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Step 2（**无 DB 锁**）：逐条写入 SecretStore 并 read-back verify。
/// 返回 (成功列表, 失败列表(profile_id, sanitized reason))。
pub fn store_pending(
    store: &dyn SecretStore,
    pending: Vec<PendingSecretMigration>,
) -> (Vec<StoredSecret>, Vec<(i64, String)>) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for p in pending {
        let secret_ref = generate_secret_ref();
        // 1. write
        if let Err(_e) = store.set(&secret_ref, &p.plaintext) {
            // set 失败：不生成 secret_ref 记录、不清空 plaintext（宁可保留旧数据也不丢 Key）
            failed.push((p.profile_id, "credential storage unavailable".to_string()));
            continue;
        }
        // 2. read-back verify（值必须逐字节一致）
        match store.get(&secret_ref) {
            Ok(Some(v)) if v == p.plaintext => {}
            Ok(_) => {
                // read-back 不一致：删掉刚写入的坏条目，保留 legacy plaintext
                let _ = store.delete(&secret_ref);
                failed.push((p.profile_id, "secret read-back verify failed".to_string()));
                continue;
            }
            Err(_) => {
                let _ = store.delete(&secret_ref);
                failed.push((p.profile_id, "secret read-back verify failed".to_string()));
                continue;
            }
        }
        ok.push(StoredSecret {
            profile_id: p.profile_id,
            secret_ref,
        });
    }
    (ok, failed)
}

/// Step 3（重新拿锁，短事务）：写 secret_ref + 清空 legacy plaintext。
/// 单行单事务；某行 commit 失败 → best-effort delete 对应 secret（允许暂时
/// orphan，但 DB 原数据可恢复）。返回成功条数。
pub fn commit_migrated(
    conn: &Connection,
    store: &dyn SecretStore,
    migrated: Vec<StoredSecret>,
) -> Result<usize, String> {
    let mut committed = 0usize;
    for m in migrated {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let res = tx
            .execute(
                "UPDATE ai_provider_profiles
                 SET secret_ref = ?2, api_key = '', updated_at = datetime('now')
                 WHERE id = ?1 AND auth_mode = 'bearer' AND secret_ref IS NULL",
                params![m.profile_id, m.secret_ref],
            )
            .map_err(|e| e.to_string());
        match res {
            // 恰好更新 1 行 = 该行在我们读取后未被并发改动
            Ok(updated) if updated == 1 => {
                if tx.commit().is_ok() {
                    committed += 1;
                    continue;
                }
                // COMMIT 失败：DB 仍引用 legacy；best-effort 清理新 secret
                let _ = store.delete(&m.secret_ref);
            }
            // WHERE 未命中（0 行：行已被并发改 auth_mode / 已迁移 / 已删除）
            // → 视为 commit 失败：DB 不再能以 legacy plaintext 之外的路径引用
            //   该 secret，保留只会产生 orphan → best-effort 删除（§C4）。
            Ok(_) => {
                let _ = tx.rollback();
                let _ = store.delete(&m.secret_ref);
            }
            Err(_) => {
                let _ = tx.rollback();
                let _ = store.delete(&m.secret_ref);
            }
        }
    }
    Ok(committed)
}

/// 便捷编排（§C3 / SAFETY §3）：三段式跑完——**每段各自短暂拿/放应用 DB 锁**，
/// Step 2（SecretStore I/O）期间锁一定处于释放状态（S3-D1 由本函数结构保证）。
/// 返回 (成功迁移条数, 失败条数)。永不 Err 掉整体：迁移失败只记 sanitized 统计，
/// legacy plaintext 保持原样，可在未来启动 / AI Settings 访问时安全重试。
pub fn run_best_effort(db: &crate::db::DbState, store: &dyn SecretStore) -> (usize, usize) {
    // Step 1：短锁读（snapshot 待迁移行）
    let pending = {
        let Ok(conn) = db.0.lock() else {
            return (0, 0);
        };
        match collect_pending(&conn) {
            Ok(v) => v,
            Err(_) => return (0, 0),
        }
    }; // ★ 锁释放
    if pending.is_empty() {
        return (0, 0);
    }
    let total = pending.len();
    // Step 2：★ 无锁 —— SecretStore set + read-back verify
    let (migrated, failed) = store_pending(store, pending);
    // Step 3：短锁提交
    let committed = {
        let Ok(conn) = db.0.lock() else {
            return (0, total);
        };
        commit_migrated(&conn, store, migrated).unwrap_or(0)
    }; // ★ 锁释放
    (committed, failed.len() + (total - failed.len() - committed))
}

#[cfg(test)]
mod tests {
    use super::super::secret_store::MemorySecretStore;
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn insert_bearer(conn: &Connection, name: &str, key: &str) -> i64 {
        conn.execute(
            "INSERT INTO ai_provider_profiles (display_name, adapter_kind, base_url, api_key, model, thinking_mode, auth_mode)
             VALUES (?1, 'openai_compatible', 'http://x', ?2, 'm', 'off', 'bearer')",
            params![name, key],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn plaintext_of(conn: &Connection, id: i64) -> String {
        conn.query_row(
            "SELECT api_key FROM ai_provider_profiles WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn secret_ref_of(conn: &Connection, id: i64) -> Option<String> {
        conn.query_row(
            "SELECT secret_ref FROM ai_provider_profiles WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    // ---- SS-05：legacy plaintext 迁移成功 → secret 已拷贝、DB plaintext 清空 ----
    #[test]
    fn ss05_legacy_plaintext_migration_success() {
        let conn = setup_db();
        let store = MemorySecretStore::new();
        let id = insert_bearer(&conn, "legacy", "sk-legacy-key");

        let pending = collect_pending(&conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].plaintext, "sk-legacy-key");

        let (migrated, failed) = store_pending(&store, pending);
        assert!(failed.is_empty());
        assert_eq!(migrated.len(), 1);

        let n = commit_migrated(&conn, &store, migrated).unwrap();
        assert_eq!(n, 1);
        assert_eq!(plaintext_of(&conn, id), "", "迁移成功后 plaintext 必须清空");
        assert!(secret_ref_of(&conn, id).is_some());
        let r = secret_ref_of(&conn, id).unwrap();
        assert_eq!(store.get(&r).unwrap(), Some("sk-legacy-key".to_string()));
    }

    // ---- SS-06：SecretStore 失败 → plaintext NOT cleared ----
    #[test]
    fn ss06_store_failure_keeps_plaintext() {
        let conn = setup_db();
        // FailingStore：set 恒失败
        struct Failing;
        impl SecretStore for Failing {
            fn get(&self, _: &str) -> Result<Option<String>, String> {
                Err("down".into())
            }
            fn set(&self, _: &str, _: &str) -> Result<(), String> {
                Err("down".into())
            }
            fn delete(&self, _: &str) -> Result<(), String> {
                Err("down".into())
            }
        }
        let id = insert_bearer(&conn, "legacy", "sk-keep-me");

        let pending = collect_pending(&conn).unwrap();
        let (migrated, failed) = store_pending(&Failing, pending);
        assert!(migrated.is_empty());
        assert_eq!(failed.len(), 1);
        // 无 migrated → 无 commit → plaintext 原样、secret_ref 不伪造
        assert_eq!(plaintext_of(&conn, id), "sk-keep-me");
        assert!(secret_ref_of(&conn, id).is_none());
    }

    // ---- SS-07（migration 切面）：read-back verify 失败 → 不清空 plaintext ----
    #[test]
    fn ss07_read_back_failure_keeps_plaintext() {
        let conn = setup_db();
        // CorruptingStore：get 返回篡改值
        struct Corrupting;
        impl SecretStore for Corrupting {
            fn get(&self, _: &str) -> Result<Option<String>, String> {
                Ok(Some("tampered".into()))
            }
            fn set(&self, _: &str, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn delete(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
        }
        let id = insert_bearer(&conn, "legacy", "sk-real");

        let pending = collect_pending(&conn).unwrap();
        let (migrated, failed) = store_pending(&Corrupting, pending);
        assert!(migrated.is_empty(), "verify 失败不得进入 commit");
        assert_eq!(failed.len(), 1);
        assert_eq!(plaintext_of(&conn, id), "sk-real");
        assert!(secret_ref_of(&conn, id).is_none());
    }

    // ---- SS（重试语义 §C6）：crash halfway → 下次启动安全重试；已迁移不重写 ----
    #[test]
    fn migration_is_idempotent_and_resumable() {
        let conn = setup_db();
        let store = MemorySecretStore::new();
        let a = insert_bearer(&conn, "A", "sk-a");
        let b = insert_bearer(&conn, "B", "sk-b");

        // 第一轮：只有 A 迁移成功（模拟 crash：B 未处理）
        let pending = collect_pending(&conn).unwrap();
        assert_eq!(pending.len(), 2);
        let a_only: Vec<_> = pending.into_iter().filter(|p| p.profile_id == a).collect();
        let (migrated, failed) = store_pending(&store, a_only);
        assert_eq!(migrated.len(), 1);
        assert_eq!(commit_migrated(&conn, &store, migrated).unwrap(), 1);
        assert!(secret_ref_of(&conn, a).is_some());
        assert!(secret_ref_of(&conn, b).is_none());

        // 第二轮：A 不再出现在 pending（不得重写）；B 被安全重试
        let pending = collect_pending(&conn).unwrap();
        assert_eq!(pending.len(), 1, "已迁移的 A 不得重新迁移");
        assert_eq!(pending[0].profile_id, b);
        let (migrated, failed) = store_pending(&store, pending);
        assert!(failed.is_empty());
        assert_eq!(commit_migrated(&conn, &store, migrated).unwrap(), 1);
        assert_eq!(plaintext_of(&conn, b), "");
        assert_eq!(
            store.get(&secret_ref_of(&conn, b).unwrap()).unwrap(),
            Some("sk-b".to_string())
        );
    }

    // ---- §C4：commit 失败 → best-effort delete，不丢 DB 数据 ----
    #[test]
    fn commit_failure_cleans_up_secret_best_effort() {
        let conn = setup_db();
        let store = MemorySecretStore::new();
        // 用一个会被 WHERE 条件排除的行模拟 commit 失败：先迁移，再把行改成 none
        let id = insert_bearer(&conn, "X", "sk-x");
        let pending = collect_pending(&conn).unwrap();
        let (migrated, failed) = store_pending(&store, pending);
        assert!(failed.is_empty());
        assert_eq!(migrated.len(), 1);
        let orphan_ref = migrated[0].secret_ref.clone();
        assert!(store.contains(&orphan_ref));
        // 模拟并发变更：auth_mode 改为 none → UPDATE WHERE 不命中 → commit 失败路径
        conn.execute(
            "UPDATE ai_provider_profiles SET auth_mode='none' WHERE id=?1",
            params![id],
        )
        .unwrap();
        let n = commit_migrated(&conn, &store, migrated).unwrap();
        assert_eq!(n, 0);
        // best-effort cleanup：SecretStore 中不残留（数据可恢复 > 零 orphan 的例外：
        // DB 行已无法再引用该 secret → 删除是安全的）
        assert!(
            !store.contains(&orphan_ref),
            "commit 失败必须 best-effort 清理"
        );
    }
}
