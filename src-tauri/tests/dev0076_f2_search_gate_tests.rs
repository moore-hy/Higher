//! DEV-0076 Phase F.2 · Search Index Confirmation Gate 专项测试（§七 F2-TC001~005）。
//!
//! FINAL AUDIT P0 修复验证：Memory 的 pending_confirmation / rejected /
//! dismissed / superseded / draft 不得通过通用 Search Index（search_higher
//! → SearchRepository::search）进入 AI 上下文。
//!
//! 两道防线：
//! 1. rebuild_profile 只索引 confirmed（§二）；
//! 2. search() 对 memory 命中二次授权——Search Index = 候选，
//!    memory_records.status='confirmed' = 最终事实源（§三，防历史脏 FTS）。

use app_lib::ai::intelligence;
use app_lib::ai::intelligence::memory as pi_memory;
use app_lib::db::DbState;
use app_lib::repository::memory::MemoryRepository;
use app_lib::repository::search::SearchRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

// =============== fixture ===============

fn setup(name: &str) -> DbState {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let _ = name;
    DbState(std::sync::Mutex::new(conn))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("F2", None, None, None, None, None)
        .unwrap()
        .id
}

/// 唯一可检索词，保证 FTS/LIKE 均能命中（规避 CJK 分词噪音）。
fn item(key: &str, value: &str) -> pi_memory::ExtractedMemory {
    pi_memory::ExtractedMemory {
        kind: "explicit".into(),
        memory_type: "user_fact".into(),
        category: "学习".into(),
        key: key.into(),
        value: value.into(),
        excerpt: value.into(),
        importance: 4,
        confidence: "high".into(),
    }
}

/// 建候选 → （可选）确认。返回 memory id。
fn seed(conn: &Connection, pid: i64, key: &str, value: &str, confirm: bool) -> i64 {
    let id = intelligence::memory_confirmation::create_memory_proposal(conn, pid, &item(key, value)).unwrap();
    if confirm {
        intelligence::memory_confirmation::confirm_memory(conn, pid, id).unwrap();
    }
    id
}

fn search_memory_hits(conn: &Connection, pid: i64, q: &str) -> Vec<(String, i64)> {
    SearchRepository::new(conn)
        .search(pid, q, Some(&["memory".to_string()]), 10)
        .unwrap()
        .into_iter()
        .map(|h| (h.entity_type, h.entity_id))
        .collect()
}

// =============== F2-TC001 · pending + rebuild 不可搜索 ===============

#[test]
fn f2_tc001_pending_not_searchable_after_rebuild() {
    let state = setup("tc001");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let mut conn = state.0.lock().unwrap();

    let a = seed(&conn, pid, "QWERTY候选", "UNIQUE-PENDING-ALPHA-XYZ", false);
    assert_eq!(
        MemoryRepository::new(&conn).get(a, pid).unwrap().unwrap().status,
        "pending_confirmation"
    );

    // rebuild（手动重建命令同款路径）→ pending 不得进入通用索引
    app_lib::repository::search::rebuild_profile(&mut conn, pid).unwrap();

    let hits = search_memory_hits(&conn, pid, "UNIQUE-PENDING-ALPHA");
    assert!(hits.is_empty(), "pending 经 rebuild 后不得被通用搜索返回：{hits:?}");
    // search_index 物理不含该行（只收 confirmed）
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index WHERE profile_id=?1 AND entity_type='memory'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "rebuild 只索引 confirmed（pending 不入索引）");
}

// =============== F2-TC002 · confirmed + rebuild 可搜索 ===============

#[test]
fn f2_tc002_confirmed_searchable_after_rebuild() {
    let state = setup("tc002");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let mut conn = state.0.lock().unwrap();

    let b = seed(&conn, pid, "QWERTY记忆", "UNIQUE-CONFIRMED-BRAVO-XYZ", true);
    app_lib::repository::search::rebuild_profile(&mut conn, pid).unwrap();

    let hits = search_memory_hits(&conn, pid, "UNIQUE-CONFIRMED-BRAVO");
    assert!(
        hits.iter().any(|(t, id)| t == "memory" && *id == b),
        "confirmed 经 rebuild 后应可被通用搜索返回：{hits:?}"
    );
    // 中文 CJK fallback 路径（LIKE）同样可达
    let _ = hits;
}

// =============== F2-TC003 · rejected 不可搜索 ===============

#[test]
fn f2_tc003_rejected_not_searchable_after_rebuild() {
    let state = setup("tc003");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let mut conn = state.0.lock().unwrap();

    // 合法 rejected 场景：候选 → 用户拒绝（§五 FTS 契约：rejected 不可搜索）
    let c = seed(&conn, pid, "QWERTY拒绝", "UNIQUE-REJECTED-CHARLIE-XYZ", false);
    intelligence::memory_confirmation::reject_memory(&conn, pid, c).unwrap();
    app_lib::repository::search::rebuild_profile(&mut conn, pid).unwrap();

    let hits = search_memory_hits(&conn, pid, "UNIQUE-REJECTED-CHARLIE");
    assert!(hits.is_empty(), "rejected 经 rebuild 后不得被通用搜索返回：{hits:?}");
}

// =============== F2-TC004 · 历史脏 FTS 防御（核心） ===============

/// 人工模拟历史脏索引：pending Memory 直接写 search_index + search_fts，
/// 验证 search() 的 DB 二次授权仍将其拦截——数据库 status 是最终安全边界。
#[test]
fn f2_tc004_stale_fts_entry_defended_by_db_status() {
    let state = setup("tc004");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    let d = seed(&conn, pid, "QWERTY脏索", "UNIQUE-STALE-DELTA-XYZ", false); // pending

    // 模拟历史脏 FTS：直接按旧 rebuild 口径写入索引（不经过 confirm）
    conn.execute(
        "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content)
         VALUES ('memory', ?1, ?2, '脏索引标题', ?3)",
        params![d, pid, "UNIQUE-STALE-DELTA-XYZ"],
    )
    .unwrap();
    let rowid: i64 = conn
        .query_row(
            "SELECT rowid FROM search_index WHERE entity_type='memory' AND entity_id=?1 AND profile_id=?2",
            params![d, pid],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO search_fts (rowid, title, content) VALUES (?1, '脏索引标题', ?2)",
        params![rowid, "UNIQUE-STALE-DELTA-XYZ"],
    )
    .unwrap();

    // FTS 确实存在脏行（前置证明：脏数据已建立）
    let fts_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index WHERE profile_id=?1 AND entity_type='memory'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts_n, 1, "前置：脏 FTS 行已存在（模拟历史数据）");

    // 主路（FTS5）：不得返回 pending
    let hits = search_memory_hits(&conn, pid, "UNIQUE-STALE-DELTA");
    assert!(
        hits.iter().all(|(_, id)| *id != d),
        "§三防御：即使 FTS 存在脏 pending 行，search 也不得返回（DB status=事实源）：{hits:?}"
    );
    // CJK fallback 路（LIKE）：同样不得返回
    let like_hits = SearchRepository::new(&conn)
        .search(pid, "脏索引标题", Some(&["memory".to_string()]), 10)
        .unwrap();
    assert!(
        like_hits.iter().all(|h| h.entity_id != d),
        "fallback 路同样过 memory 授权门：{like_hits:?}"
    );
}

// =============== F2-TC005 · search_higher 同一安全口径 ===============

/// search_higher 工具（tools.rs L648）零自有过滤，直接委托
/// SearchRepository::search —— 本测试锁定该依赖契约：
/// ① 源码层：search_higher 分支必须只调 SearchRepository::search（不得新造过滤器）；
/// ② 行为层：pending/rejected 均不返回，confirmed 返回。
#[test]
fn f2_tc005_search_higher_shares_repository_gate() {
    let state = setup("tc005");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let mut conn = state.0.lock().unwrap();

    let p = seed(&conn, pid, "QWERTY甲", "UNIQUE-SHARE-PENDING-XYZ", false); // pending
    let r = seed(&conn, pid, "QWERTY乙", "UNIQUE-SHARE-REJECTED-XYZ", false);
    intelligence::memory_confirmation::reject_memory(&conn, pid, r).unwrap();
    let c = seed(&conn, pid, "QWERTY丙", "UNIQUE-SHARE-CONFIRMED-XYZ", true);
    app_lib::repository::search::rebuild_profile(&mut conn, pid).unwrap();

    // 行为层：search_higher 的实际后端（不带 entity_types 过滤 = 工具默认形态）
    let hits = SearchRepository::new(&conn).search(pid, "UNIQUE-SHARE", None, 10).unwrap();
    let mem_hits: Vec<i64> = hits.iter().filter(|h| h.entity_type == "memory").map(|h| h.entity_id).collect();
    assert!(mem_hits.contains(&c), "confirmed 可见：{mem_hits:?}");
    assert!(!mem_hits.contains(&p), "pending 不可见（§四：search_higher 与 Repository 同口径）");
    assert!(!mem_hits.contains(&r), "rejected 不可见");

    // 源码层：search_higher 分支零 Memory 权限逻辑（§四：权限集中在 Repository）
    let tools_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/tools.rs"),
    )
    .unwrap();
    let branch = tools_rs
        .split("\"search_higher\" => {")
        .nth(1)
        .and_then(|s| s.split("}\"").next())
        .unwrap_or_default();
    assert!(
        branch.contains("SearchRepository::new(conn)\n                .search") || branch.contains(".search(profile_id, q, ets.as_deref(), limit)"),
        "search_higher 必须委托 SearchRepository::search（不得自造过滤器）"
    );
    assert!(
        !branch.contains("memory_records"),
        "search_higher 分支不得直接查 memory_records（§四：状态权限集中在 Repository）"
    );
    let _ = (p, r);
}
