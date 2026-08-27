//! DEV-0076 Phase F.1 · Memory Consistency Repair 专项测试（§十三 F1-TC001~005）。
//!
//! 确认门安全边界（任务书结语）：
//! ```text
//! AI发现 → pending → 用户确认 → confirmed → FTS索引 → AI未来读取
//! ```
//! 任何未确认内容都不能进入 AI 长期认知。本套件验证：
//! - pending 不进 FTS / AI 检索 / AI context（TC001/TC003）
//! - confirm 才建立检索入口（TC002）
//! - legacy 收口不再产生 'active'（TC004：动态模拟 + 静态源码防回退）
//! - 用户亲手编辑 = 用户事实直接 confirmed（TC005）

use app_lib::ai::intelligence::{self, memory as pi_memory};
use app_lib::ai::workflow::AgentWorkflowPayload;
use app_lib::db::DbState;
use app_lib::repository::memory::{MemoryRecord, MemoryRepository};
use app_lib::repository::personalization::PersonalizationRepository;
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
        .create("F1", None, None, None, None, None)
        .unwrap()
        .id
}

/// 一条 explicit 记忆候选（§十二：AI 侧唯一创建入口）。
fn pending_item(key: &str, value: &str, excerpt: &str) -> pi_memory::ExtractedMemory {
    pi_memory::ExtractedMemory {
        kind: "explicit".into(),
        memory_type: "user_fact".into(),
        category: "学习".into(),
        key: key.into(),
        value: value.into(),
        excerpt: excerpt.into(),
        importance: 4,
        confidence: "high".into(),
    }
}

/// DB 中 status='active' 的行数（§一：业务状态必须为零）。
fn count_active(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND status='active'",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// =============== F1-TC001 · pending 不可搜索 ===============

/// create_pending_memory 后：DB 存在，但 search_memory（FTS + 候选两路）均 0。
#[test]
fn f1_tc001_pending_not_searchable() {
    let state = setup("tc001");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    let id = intelligence::memory_confirmation::create_memory_proposal(
        &conn, profile_id, &pending_item("考研计划", "正在准备2028考研", "我要准备2028考研"),
    )
    .unwrap();

    // DB 存在（pending 期间数据不丢，供用户确认）
    let repo = MemoryRepository::new(&conn);
    assert_eq!(repo.get(id, profile_id).unwrap().unwrap().status, "pending_confirmation");
    assert_eq!(repo.list_pending(profile_id).unwrap().len(), 1);

    // search 不可见（FTS 未写入 + 候选口径 confirmed——pending 两路都被隔离）
    assert!(
        repo.search(profile_id, "2028考研", 10).unwrap().is_empty(),
        "pending 不得进入 AI 检索（search_memory）"
    );
    assert!(
        repo.search(profile_id, "", 10).unwrap().iter().all(|m| m.id != id),
        "空查询候选路也不得返回 pending"
    );
    // FTS 物理未写入（search_index 无 memory 行）
    let fts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index WHERE profile_id=?1 AND entity_type='memory'",
            params![profile_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts, 0, "pending 不写 FTS");
}

// =============== F1-TC002 · confirm 后可搜索 ===============

/// pending → confirm → status=confirmed 且 FTS/检索入口建立。
#[test]
fn f1_tc002_confirm_builds_retrieval_entry() {
    let state = setup("tc002");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    let id = intelligence::memory_confirmation::create_memory_proposal(
        &conn, profile_id, &pending_item("学习习惯", "用户偏好晚上学习", "我晚上学习效率高"),
    )
    .unwrap();
    let repo = MemoryRepository::new(&conn);
    assert!(repo.search(profile_id, "晚上学习", 10).unwrap().is_empty(), "确认前不可检索");

    intelligence::memory_confirmation::confirm_memory(&conn, profile_id, id).unwrap();
    assert_eq!(repo.get(id, profile_id).unwrap().unwrap().status, "confirmed");

    // FTS 已建立 + 检索可见
    let hits = repo.search(profile_id, "晚上学习", 10).unwrap();
    assert!(hits.iter().any(|m| m.id == id), "confirm 后进入 AI 检索");
    let fts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index WHERE profile_id=?1 AND entity_type='memory'",
            params![profile_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts, 1, "confirm 写入 FTS 索引");
}

// =============== F1-TC003 · AI 读取只能 confirmed ===============

/// pending A + confirmed B 并存：AI 读取（active_memories + 轮首注入块）
/// 只含 B；A 仅存在于待确认区（用户可见、AI 不可见）。
#[test]
fn f1_tc003_ai_reads_confirmed_only() {
    let state = setup("tc003");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    // A：未确认候选
    let a = intelligence::memory_confirmation::create_memory_proposal(
        &conn, profile_id,
        &pending_item("候选A", "A-未确认的推断内容-XYZ", "A 的原话"),
    )
    .unwrap();
    // B：候选 → 用户确认
    let b = intelligence::memory_confirmation::create_memory_proposal(
        &conn, profile_id,
        &pending_item("记忆B", "B-已确认的长期记忆-ABC", "B 的原话"),
    )
    .unwrap();
    intelligence::memory_confirmation::confirm_memory(&conn, profile_id, b).unwrap();

    // AI 读取口径：active_memories 只有 B
    let actives = pi_memory::active_memories(&conn, profile_id, 10);
    assert_eq!(actives.len(), 1, "AI 长期记忆只有 confirmed");
    assert_eq!(actives[0].id, b);
    assert!(!actives.iter().any(|m| m.id == a), "pending A 不进 AI 读取");

    // 轮首注入块（build_injection = Decision 输入增强层）只含 B 内容
    let injection = intelligence::intelligence_builder::build_injection(
        &conn, profile_id, &AgentWorkflowPayload::default(), "帮我安排复习",
    );
    assert!(injection.contains("B-已确认的长期记忆-ABC"), "confirmed B 注入 AI context");
    assert!(!injection.contains("A-未确认的推断内容-XYZ"), "pending A 不得注入 AI context");

    // 管理口径（list_active）两者都可见（用户可在设置页处理 A）
    let managed = MemoryRepository::new(&conn).list_active(profile_id).unwrap();
    assert_eq!(managed.len(), 2, "管理口径 = confirmed + pending");
}

// =============== F1-TC004 · legacy 路径不产生 active ===============

/// 两层验证：
/// ① 动态：按 lib.rs 旧 Memory Extract 收口的同款字段构造写入（AI 生成路径）
///    → 落库一律 pending_confirmation，DB 无任何 'active' 行；
/// ② 静态：lib.rs 源码不得再出现 memory `repo.insert(&rec)`（写 'active' 的
///    旧入口已删除，防回退）。
#[test]
fn f1_tc004_legacy_path_never_produces_active() {
    let state = setup("tc004");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    // ① 动态模拟 legacy 收口写入（与 lib.rs Memory Extract 相同构造：
    // user_fact / user_message / normalize 后 key —— 走 create_pending_memory）
    for (key, value) in [
        ("chat::考研", "用户准备2028考研"),
        ("chat::习惯", "用户偏好晚上学习"),
    ] {
        let rec = MemoryRecord {
            id: 0,
            profile_id,
            memory_type: "user_fact".into(),
            category: "chat".into(),
            memory_key: key.into(),
            memory_value: value.into(),
            source_kind: "user_message".into(),
            source_ref: format!("conversation:{}", 1),
            source_excerpt: value.into(),
            importance: 4,
            confidence: "medium".into(),
            status: "pending_confirmation".into(),
            valid_from: None,
            valid_to: None,
            supersedes_id: None,
            created_at: String::new(),
            updated_at: String::new(),
            last_used_at: None,
        };
        if !rec.memory_value.is_empty() {
            MemoryRepository::new(&conn).create_pending_memory(&rec).unwrap();
        }
    }
    assert_eq!(count_active(&conn, profile_id), 0, "legacy 收口不得产生 'active' 状态");
    assert_eq!(
        MemoryRepository::new(&conn).list_pending(profile_id).unwrap().len(),
        2,
        "AI 生成的记忆一律以待确认身份落库"
    );

    // ② 静态源码防回退：lib.rs 无 memory insert('active') 旧入口
    let lib_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .unwrap();
    assert!(
        !lib_rs.contains("repo.insert(&rec)"),
        "lib.rs 不得保留 memory repo.insert(&rec)（写 'active' 的旧入口，§十）"
    );
    assert!(
        lib_rs.contains("create_pending_memory(&rec)"),
        "lib.rs legacy 收口必须走 create_pending_memory（§十）"
    );
}

// =============== F1-TC005 · user_edit 直接 confirmed ===============

/// 用户主动编辑画像（personalization user_edit）= 用户事实：
/// source_kind='user_edit' 且 status='confirmed'（不是 AI 推断，无需确认门）。
#[test]
fn f1_tc005_user_edit_directly_confirmed() {
    let state = setup("tc005");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    PersonalizationRepository::new(&conn)
        .user_edit(profile_id, "# 手工编辑档案\n正在准备2028考研")
        .unwrap();

    let row: (String, String) = conn
        .query_row(
            "SELECT source_kind, status FROM memory_records
             WHERE profile_id=?1 AND memory_key='私人化档案（用户编辑）'",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row.0, "user_edit", "用户亲手编辑 → source_kind=user_edit");
    assert_eq!(row.1, "confirmed", "用户事实直接 confirmed（§十一）");
    // 且立即可被 AI 检索（confirmed 进入 FTS）
    let hits = MemoryRepository::new(&conn).search(profile_id, "手工编辑档案", 10).unwrap();
    assert!(!hits.is_empty(), "user_edit 记忆即时进入 AI 长期读取");
    assert_eq!(count_active(&conn, profile_id), 0);
}
