//! DEV-0051 测试：v016 迁移 / Document CRUD+原子性 / 附件 / Timeline / Media-only /
//! Legacy 附件 / safe_delete 守卫 / AI Context 读 Documents（§64-74）。

use app_lib::repository::attachment::AttachmentRepository;
use app_lib::repository::knowledge_document::KnowledgeDocumentRepository;
use app_lib::repository::knowledge_workspace::KnowledgeWorkspaceRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection) -> i64 {
    let p = StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    // command 层 create_study_profile 会 ensure_final（v015）；测试直接补齐
    let _ = app_lib::repository::goal::GoalRepository::new(conn).ensure_final(p).unwrap();
    p
}

fn mk_item(conn: &Connection, p: i64, name: &str) -> i64 {
    let goal = app_lib::repository::goal::GoalRepository::new(conn)
        .create(p, "G", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_root(goal.id, name, None)
        .unwrap()
        .id
}

/// 手工搭 v015 库（复刻 run_migrations 至 15），供 v015→v016 真实迁移测试。
fn setup_v015() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL,
         executed_at TEXT NOT NULL DEFAULT (datetime('now')));",
    )
    .unwrap();
    let ups: Vec<fn(&Connection) -> rusqlite::Result<()>> = vec![
        app_lib::migrations::v001_initial::up,
        app_lib::migrations::v002_core_models::up,
        app_lib::migrations::v003_planning::up,
        app_lib::migrations::v004_evaluations::up,
        app_lib::migrations::v005_study_profiles::up,
        app_lib::migrations::v006_learning_item_content::up,
        app_lib::migrations::v007_feedbacks::up,
        app_lib::migrations::v008_adjustments::up,
        app_lib::migrations::v009_learning_attachments::up,
        app_lib::migrations::v010_recurring_tasks::up,
        app_lib::migrations::v011_task_lifecycle::up,
        app_lib::migrations::v012_ux_convergence::up,
        app_lib::migrations::v013_profile_first::up,
        app_lib::migrations::v014_session_rich_document::up,
        app_lib::migrations::v015_goal_tree_mastery::up,
    ];
    for v in ups {
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        v(&tx).unwrap();
        tx.commit().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    }
    for i in 1..=15 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![i],
        )
        .unwrap();
    }
    conn
}

// =============== §64/§65/§24 Migration ===============

#[test]
fn test_v016_migration_legacy_content() {
    let conn = setup_v015();
    let p = mk_profile(&conn);
    let i1 = mk_item(&conn, p, "有内容");
    let i2 = mk_item(&conn, p, "空内容");
    let i3 = mk_item(&conn, p, "空白内容");
    conn.execute(
        "UPDATE learning_items SET content='abc' WHERE id=?1",
        rusqlite::params![i1],
    )
    .unwrap();
    conn.execute(
        "UPDATE learning_items SET content='' WHERE id=?1",
        rusqlite::params![i2],
    )
    .unwrap();
    conn.execute(
        "UPDATE learning_items SET content='   ' WHERE id=?1",
        rusqlite::params![i3],
    )
    .unwrap();

    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v016_knowledge_documents::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // §64：非空 → 旧知识正文（content_text=原样）；原 content 保留
    let (title, text): (String, String) = conn
        .query_row(
            "SELECT title, content_text FROM knowledge_documents WHERE learning_item_id=?1",
            rusqlite::params![i1],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((title.as_str(), text.as_str()), ("旧知识正文", "abc"));
    let doc_json: String = conn
        .query_row(
            "SELECT content_document_json FROM knowledge_documents WHERE learning_item_id=?1",
            rusqlite::params![i1],
            |r| r.get(0),
        )
        .unwrap();
    assert!(doc_json.contains(r#""type":"doc""#) && doc_json.contains(r#""text":"abc""#), "纯文本转 Tiptap JSON");
    let origin: String = conn
        .query_row("SELECT content FROM learning_items WHERE id=?1", rusqlite::params![i1], |r| r.get(0))
        .unwrap();
    assert_eq!(origin, "abc", "§23 旧 content 不清空");

    // §65：'' / 空白 不生成
    for iid in [i2, i3] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM knowledge_documents WHERE learning_item_id=?1",
                rusqlite::params![iid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "空内容不得生成垃圾文档");
    }

    // §24：重复执行 v016 不产生重复「旧知识正文」
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v016_knowledge_documents::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_documents WHERE learning_item_id=?1 AND title='旧知识正文'",
            rusqlite::params![i1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "防重复迁移");

    // 多行 content → 多 paragraph
    conn.execute(
        "UPDATE learning_items SET content='l1
l2' WHERE id=?1",
        rusqlite::params![i2],
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v016_knowledge_documents::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let j: String = conn
        .query_row(
            "SELECT content_document_json FROM knowledge_documents WHERE learning_item_id=?1",
            rusqlite::params![i2],
            |r| r.get(0),
        )
        .unwrap();
    assert!(j.matches("paragraph").count() >= 2, "多行 → 多段落");
}

// =============== §66/§67 Document CRUD ===============

#[test]
fn test_document_crud_and_isolation() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let ia = mk_item(&conn, pa, "A");
    let ib = mk_item(&conn, pb, "B");
    let repo = KnowledgeDocumentRepository::new(&conn);

    // Create（默认标题）
    let d = repo.create(pa, ia, "").unwrap();
    assert_eq!(d.title, "未命名文档");
    // Get / List
    assert_eq!(repo.get(d.id, pa).unwrap().unwrap().id, d.id);
    assert_eq!(repo.list_by_item(pa, ia).unwrap().len(), 1);
    // Update（原子：text+json 同行）
    let up = repo
        .update(d.id, pa, "新标题", "纯文本", Some(r#"{"type":"doc"}"#))
        .unwrap();
    assert_eq!((up.title.as_str(), up.content_text.as_str()), ("新标题", "纯文本"));
    assert_eq!(up.content_document_json.as_deref(), Some(r#"{"type":"doc"}"#));
    // Rename
    let rn = repo.rename(d.id, pa, "再改名").unwrap();
    assert_eq!(rn.title, "再改名");
    // 空标题回退未命名
    let rn2 = repo.rename(d.id, pa, "  ").unwrap();
    assert_eq!(rn2.title, "未命名文档");

    // Profile isolation：pb 读/改/删 pa 的文档 → 拒
    assert!(repo.get(d.id, pb).unwrap().is_none());
    assert!(repo.update(d.id, pb, "x", "y", None).is_err());
    assert!(repo.delete(d.id, pb).is_err());
    // 跨 profile item：pb 的文档挂 ia → 拒
    assert!(repo.create(pb, ia, "跨").is_err());
    // Missing / Invalid
    assert!(repo.get(99999, pa).unwrap().is_none());
    assert!(repo.create(pa, 99999, "无item").is_err());

    // Delete
    repo.delete(d.id, pa).unwrap();
    assert!(repo.get(d.id, pa).unwrap().is_none());
}

#[test]
fn test_document_update_atomicity() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    let repo = KnowledgeDocumentRepository::new(&conn);
    let d = repo.create(p, i, "D").unwrap();
    // 单条 UPDATE 语句本身原子；验证成功路径 text/json 一致 + 失败路径（跨档案）两者都不变
    repo.update(d.id, p, "T1", "TEXT1", Some(r#"{"v":1}"#)).unwrap();
    let bad = repo.update(d.id, p + 999, "T2", "TEXT2", None);
    assert!(bad.is_err());
    let after = repo.get(d.id, p).unwrap().unwrap();
    assert_eq!((after.title.as_str(), after.content_text.as_str()), ("T1", "TEXT1"));
    assert_eq!(after.content_document_json.as_deref(), Some(r#"{"v":1}"#));
}

// =============== §68 Attachment ===============

#[test]
fn test_document_attachments_and_delete_cleanup() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    let repo = KnowledgeDocumentRepository::new(&conn);
    let att = AttachmentRepository::new(&conn);
    let d = repo.create(p, i, "D").unwrap();

    // 三类 owner 附件（DB 层；文件本体由 command 层，此处验证归属与列）
    let img = att
        .create_for_document(p, i, d.id, "image", "a.png", "1/item/a.png", Some("image/png"), "")
        .unwrap();
    let vid = att
        .create_for_document(p, i, d.id, "video", "b.mp4", "1/item/b.mp4", Some("video/mp4"), "")
        .unwrap();
    let draw = att
        .create_for_document(p, i, d.id, "drawing", "画图.png", "1/item/c.png", Some("image/png"), "")
        .unwrap();
    for a in [&img, &vid, &draw] {
        assert_eq!(a.document_id, Some(d.id));
        assert_eq!(a.session_id, None, "文档附件 session=NULL");
        assert_eq!(a.learning_item_id, Some(i));
        assert_eq!(a.profile_id, p);
    }
    assert_eq!(att.list_by_document(d.id).unwrap().len(), 3);
    // 跨档案 / 错 item 拒绝
    let p2 = mk_profile(&conn);
    assert!(att.create_for_document(p2, i, d.id, "image", "x", "x", None, "").is_err());
    let i2 = mk_item(&conn, p, "I2");
    assert!(att.create_for_document(p, i2, d.id, "image", "x", "x", None, "").is_err());

    // 删除 Document → attachment DB 不残留（FK CASCADE）
    let paths = repo.attachment_paths(d.id, p).unwrap();
    assert_eq!(paths.len(), 3);
    repo.delete(d.id, p).unwrap();
    assert_eq!(att.list_by_document(d.id).unwrap().len(), 0, "附件行已清");
    assert_eq!(att.list_by_learning_item(i).unwrap().len(), 0);
}

// =============== §69/§70/§71 Timeline / Media-only / Legacy ===============

#[test]
fn test_workspace_timeline_order_media_only_and_legacy() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "进程管理");
    let ws = KnowledgeWorkspaceRepository::new(&conn);

    // Document A updated 10:00 / Session A started 11:00 / Document B updated 12:00
    let da = KnowledgeDocumentRepository::new(&conn).create(p, i, "DocA").unwrap();
    conn.execute(
        "UPDATE knowledge_documents SET updated_at='2026-08-16 10:00:00' WHERE id=?1",
        rusqlite::params![da.id],
    )
    .unwrap();
    let sa = StudySessionRepository::new(&conn).start(i, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET title='SessionA', started_at='2026-08-16 11:00:00' WHERE id=?1",
        rusqlite::params![sa.id],
    )
    .unwrap();
    let db_ = KnowledgeDocumentRepository::new(&conn).create(p, i, "DocB").unwrap();
    conn.execute(
        "UPDATE knowledge_documents SET updated_at='2026-08-16 12:00:00' WHERE id=?1",
        rusqlite::params![db_.id],
    )
    .unwrap();

    let data = ws.get(p, i).unwrap();
    // 前端合并排序：documents(updated_at) + sessions(started_at) 倒序
    let mut entries: Vec<(String, String)> = data
        .documents
        .iter()
        .map(|d| (d.updated_at.clone(), format!("doc:{}", d.title)))
        .chain(data.sessions.iter().map(|s| (s.started_at.clone(), format!("ses:{}", s.title))))
        .collect();
    entries.sort();
    let names: Vec<&str> = entries.iter().rev().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, vec!["doc:DocB", "ses:SessionA", "doc:DocA"], "§69 时间倒序");
}

#[test]
fn test_session_media_only_not_empty() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    let s = StudySessionRepository::new(&conn).start(i, None).unwrap();
    StudySessionRepository::new(&conn).end(s.id, Some("")).unwrap();
    // note 空 + 4 图片
    for n in 0..4 {
        AttachmentRepository::new(&conn)
            .create(p, Some(i), Some(s.id), "image", &format!("p{n}.png"), &format!("1/item/p{n}.png"), None, "")
            .unwrap();
    }
    let data = KnowledgeWorkspaceRepository::new(&conn).get(p, i).unwrap();
    let ses = &data.sessions[0];
    assert_eq!(ses.note_plain, "");
    assert_eq!(ses.image_count, 4, "§70 media-only → 显示 图片 4 的数据源");
    assert_eq!(ses.attachment_count, 4);
}

#[test]
fn test_legacy_item_attachments_surface() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    let att = AttachmentRepository::new(&conn);
    // legacy：item 挂、无 session、无 document
    att.create(p, Some(i), None, "image", "old.png", "1/item/old.png", None, "").unwrap();
    att.create(p, Some(i), None, "video", "old.mp4", "1/item/old.mp4", None, "").unwrap();
    // 非 legacy：带 session 的
    let s = StudySessionRepository::new(&conn).start(i, None).unwrap();
    att.create(p, Some(i), Some(s.id), "image", "s.png", "1/item/s.png", None, "").unwrap();

    let data = KnowledgeWorkspaceRepository::new(&conn).get(p, i).unwrap();
    assert_eq!(data.legacy_attachments.len(), 2, "§71 legacy 节点级附件单列");
    assert!(data.legacy_attachments.iter().all(|a| a.session_id.is_none() && a.document_id.is_none()));
}

// =============== §72 Safe Delete ===============

#[test]
fn test_safe_delete_document_guard() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    let item_repo = LearningItemRepository::new(&conn);
    let doc_repo = KnowledgeDocumentRepository::new(&conn);

    let d = doc_repo.create(p, i, "D").unwrap();
    // 有文档 → 拒绝
    let err = item_repo.safe_delete(i).unwrap_err().to_string();
    assert!(err.contains("仍包含文档"), "§51 提示：{err}");
    // 删文档后 → 按原规则通过
    doc_repo.delete(d.id, p).unwrap();
    item_repo.safe_delete(i).unwrap();
    let gone: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items WHERE id=?1", rusqlite::params![i], |r| r.get(0))
        .unwrap();
    assert_eq!(gone, 0);
}

// =============== §73/§74 AI Context 读 Documents ===============

#[test]
fn test_ai_knowledge_context_reads_documents() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    // learning_items.content 为空；Document 存在文字
    KnowledgeDocumentRepository::new(&conn)
        .update(
            KnowledgeDocumentRepository::new(&conn).create(p, i, "进程文档").unwrap().id,
            p,
            "进程文档",
            "进程是资源分配的基本单位",
            Some(r#"{"type":"doc"}"#),
        )
        .unwrap();

    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            profile_id: p,
            action: app_lib::ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(i),
            user_instruction: None,
            date: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("知识文档"), "knowledge_detail 读取 knowledge_documents");
    assert!(ctx.contains("进程是资源分配的基本单位"));
    assert!(!ctx.contains("legacy learning_items.content"), "有 Document 时不双注 legacy content");
}

#[test]
fn test_ai_knowledge_context_fallback_when_no_documents() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    conn.execute("UPDATE learning_items SET content='legacy 正文' WHERE id=?1", rusqlite::params![i]).unwrap();
    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            profile_id: p,
            action: app_lib::ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(i),
            user_instruction: None,
            date: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("legacy 正文"), "无 Document → fallback learning_items.content");
}

#[test]
fn test_mastery_context_reads_documents() {
    let conn = setup();
    let p = mk_profile(&conn);
    let i = mk_item(&conn, p, "I");
    KnowledgeDocumentRepository::new(&conn)
        .update(
            KnowledgeDocumentRepository::new(&conn).create(p, i, "极限笔记").unwrap().id,
            p,
            "极限笔记",
            "等价无穷小替换是我的总结",
            None,
        )
        .unwrap();
    // 关联 session（让 item 进入 mastery 关联集合）
    let s = StudySessionRepository::new(&conn).start(i, None).unwrap();
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();

    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            profile_id: p,
            action: app_lib::ai::AiAction::MasteryAssessment,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
            date: Some("2026-08-10..2026-08-16".to_string()),
        },
    )
    .unwrap();
    assert!(ctx.contains("等价无穷小替换是我的总结"), "§74 Mastery Context 含 Document 证据");
}

/// §53：AI Write Tools 仍为 0。
#[test]
fn test_ai_write_tools_still_zero() {
    for name in app_lib::ai::tools::TOOL_ALLOWLIST {
        let n = name.to_lowercase();
        assert!(
            !n.contains("create") && !n.contains("update") && !n.contains("delete") && !n.contains("write"),
            "写工具泄漏：{name}"
        );
    }
}
