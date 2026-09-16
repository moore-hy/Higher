//! PRODUCT-2.0 §35 / §37 / §38 —— Knowledge Canvas 数据层。
//!
//! 关键约束：
//! - §35.1 二进制不进 JSON/DB 列：image/video/file 必须关联 Higher attachment
//! - §38 revision 单调递增 + 基线冲突拒绝（不覆盖更新的内容，也不清 dirty）
//! - §35 learning_item_id UNIQUE（一个节点一块画布）
//! - 档案隔离

use app_lib::repository::attachment::AttachmentRepository;
use app_lib::repository::knowledge_canvas::KnowledgeCanvasRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_root_for_profile(profile_id, None, name, None)
        .unwrap()
        .id
}

/// §35.1：二进制走 Higher attachment storage；embeds.attachment_id 有 FK，
/// 因此测试必须建**真实** attachment 记录（不能塞不存在的 id）。
fn mk_attachment(conn: &Connection, profile_id: i64, item_id: i64, name: &str) -> i64 {
    AttachmentRepository::new(conn)
        .create(
            profile_id,
            Some(item_id),
            None,
            "image",
            name,
            &format!("attachments/{profile_id}/{name}"),
            Some("image/png"),
            "",
        )
        .unwrap()
        .id
}

#[test]
fn migration_v031_creates_canvas_tables() {
    let conn = setup();
    for t in ["knowledge_canvases", "knowledge_canvas_embeds"] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![t],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "v031 应创建 {t}");
    }
    // DAILY EXPERIENCE V1 §PHASE 4：v032（micro_learning_events）+ v033（companion_skill）已追加
    assert_eq!(app_lib::migrations::latest_version(), 33);
}

#[test]
fn empty_canvas_is_none_and_save_round_trips() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);

    assert!(
        repo.get(p, item).unwrap().is_none(),
        "未创建 → None（不隐式落库）"
    );

    let saved = repo
        .save(p, item, "[{\"type\":\"text\"}]", Some("{\"zoom\":1}"), 0)
        .unwrap();
    assert_eq!(saved.revision, 1, "首次保存 revision=1");
    assert_eq!(saved.elements_json, "[{\"type\":\"text\"}]");

    // 同一节点再保存 → 复用同一行，revision 递增（UNIQUE(learning_item_id)）
    let saved2 = repo
        .save(
            p,
            item,
            "[{\"type\":\"text\"},{\"type\":\"rectangle\"}]",
            None,
            1,
        )
        .unwrap();
    assert_eq!(saved2.revision, 2);
    assert_eq!(saved2.id, saved.id, "一个节点一块画布");
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_canvases WHERE learning_item_id=?1",
            rusqlite::params![item],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

/// §38：迟到的旧响应（base_revision 落后）必须被拒绝，绝不覆盖新内容。
#[test]
fn stale_base_revision_is_rejected_without_data_loss() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);

    repo.save(p, item, "[\"v1\"]", None, 0).unwrap();

    // 正常前进
    repo.save(p, item, "[\"v2\"]", None, 1).unwrap();

    // 迟到的旧请求：基于 r1 的保存 → 拒绝
    let err = repo.save(p, item, "[\"stale\"]", None, 1).unwrap_err();
    assert!(
        err.contains("r1") && err.contains("r2"),
        "必须说明冲突版本：{err}"
    );

    let cur = repo.get(p, item).unwrap().unwrap();
    assert_eq!(cur.elements_json, "[\"v2\"]", "更新的内容不得被旧响应覆盖");
    assert_eq!(cur.revision, 2, "revision 不因失败而前进");

    // 未创建的节点用非 0 基线 → 也拒绝（前端状态过期）
    let item2 = mk_item(&conn, p, "导数");
    assert!(repo.save(p, item2, "[]", None, 3).is_err());
    assert!(repo.get(p, item2).unwrap().is_none());
}

#[test]
fn canvas_is_profile_scoped() {
    let conn = setup();
    let p1 = mk_profile(&conn, "P1");
    let p2 = mk_profile(&conn, "P2");
    let item = mk_item(&conn, p1, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);

    repo.save(p1, item, "[\"p1\"]", None, 0).unwrap();
    assert!(repo.get(p1, item).unwrap().is_some());
    assert!(repo.get(p2, item).unwrap().is_none(), "跨档案不可见");
}

/// §35.1：二进制必须走 attachment；link 必须有 url。
#[test]
fn embed_requires_attachment_or_url() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);

    // image 无 attachment → 拒绝（禁止 base64 塞 JSON）
    let err = repo
        .add_embed(
            p,
            item,
            "image",
            None,
            None,
            Some("图"),
            0.0,
            0.0,
            100.0,
            100.0,
        )
        .unwrap_err();
    assert!(err.contains("attachment"), "必须要求 attachment：{err}");

    // link 无 url → 拒绝
    let err2 = repo
        .add_embed(
            p,
            item,
            "link",
            None,
            None,
            Some("站点"),
            0.0,
            0.0,
            100.0,
            60.0,
        )
        .unwrap_err();
    assert!(err2.contains("URL"), "link 必须有 URL：{err2}");

    // 非法 kind → 拒绝
    assert!(repo
        .add_embed(
            p,
            item,
            "iframe",
            None,
            Some("https://x"),
            None,
            0.0,
            0.0,
            1.0,
            1.0
        )
        .is_err());

    // 合法 link
    let link = repo
        .add_embed(
            p,
            item,
            "link",
            None,
            Some("https://example.com/a"),
            Some("示例站"),
            10.0,
            20.0,
            320.0,
            120.0,
        )
        .unwrap();
    assert_eq!(link.kind, "link");
    assert_eq!(link.z_index, 1, "首个叠加 z_index=1");

    // 合法 attachment 型（file card，§35.1 必须是真实 attachment）
    let att = mk_attachment(&conn, p, item, "讲义.pdf");
    let f = repo
        .add_embed(
            p,
            item,
            "file",
            Some(att),
            None,
            Some("讲义.pdf"),
            0.0,
            0.0,
            200.0,
            80.0,
        )
        .unwrap();
    assert_eq!(f.z_index, 2, "z_index 递增");

    let list = repo.list_embeds(p, item).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, link.id, "按 z_index 排序");
}

#[test]
fn embed_geometry_update_and_delete() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);

    let e = repo
        .add_embed(
            p,
            item,
            "link",
            None,
            Some("https://e.com"),
            None,
            0.0,
            0.0,
            10.0,
            10.0,
        )
        .unwrap();
    repo.update_embed_geometry(p, e.id, 100.0, 200.0, 300.0, 150.0)
        .unwrap();
    let got = repo.list_embeds(p, item).unwrap();
    assert_eq!(
        (got[0].x, got[0].y, got[0].width, got[0].height),
        (100.0, 200.0, 300.0, 150.0)
    );

    // 跨档案删除无效（0 行受影响，不报错也不误删）
    let p2 = mk_profile(&conn, "P2");
    repo.delete_embed(p2, e.id).unwrap();
    assert_eq!(
        repo.list_embeds(p, item).unwrap().len(),
        1,
        "他人档案不得删除"
    );

    repo.delete_embed(p, e.id).unwrap();
    assert_eq!(repo.list_embeds(p, item).unwrap().len(), 0);
}

#[test]
fn canvas_survives_reload_with_same_elements() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let elements = "[{\"type\":\"freedraw\",\"id\":\"a\"},{\"type\":\"text\",\"id\":\"b\"}]";

    KnowledgeCanvasRepository::new(&conn)
        .save(p, item, elements, Some("{\"scrollX\":0}"), 0)
        .unwrap();

    // 模拟「重新打开节点」：新连接、重新读取
    let reloaded = KnowledgeCanvasRepository::new(&conn)
        .get(p, item)
        .unwrap()
        .unwrap();
    assert_eq!(
        reloaded.elements_json, elements,
        "reload 必须拿到同一批元素"
    );
    assert_eq!(reloaded.app_state_json.as_deref(), Some("{\"scrollX\":0}"));
}

#[test]
fn deleting_learning_item_cascades_canvas() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let repo = KnowledgeCanvasRepository::new(&conn);
    repo.save(p, item, "[\"x\"]", None, 0).unwrap();
    repo.add_embed(
        p,
        item,
        "link",
        None,
        Some("https://e.com"),
        None,
        0.0,
        0.0,
        1.0,
        1.0,
    )
    .unwrap();

    conn.execute(
        "DELETE FROM learning_items WHERE id=?1",
        rusqlite::params![item],
    )
    .unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_canvases", [], |r| r.get(0))
        .unwrap();
    let m: i64 = conn
        .query_row("SELECT COUNT(*) FROM knowledge_canvas_embeds", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!((n, m), (0, 0), "节点删除后画布与叠加层级联清理");
}

/// §40 / CANVAS-TC015：叠加层与历史 attachment 是**弱引用**，画布侧清理不得连坐删文件。
#[test]
fn canvas_teardown_never_deletes_existing_attachments() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let att = mk_attachment(&conn, p, item, "老笔记.png");
    let repo = KnowledgeCanvasRepository::new(&conn);

    let e = repo
        .add_embed(
            p,
            item,
            "image",
            Some(att),
            None,
            Some("老笔记.png"),
            0.0,
            0.0,
            10.0,
            10.0,
        )
        .unwrap();

    // ① 删叠加层 → 历史附件仍在（§40：不删历史文件）
    repo.delete_embed(p, e.id).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_attachments WHERE id=?1",
            rusqlite::params![att],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "CANVAS-TC015: 删叠加层不得删掉历史附件");

    // ② 删画布 → 历史附件仍在
    repo.save(p, item, "[]", None, 0).unwrap();
    repo.add_embed(
        p,
        item,
        "image",
        Some(att),
        None,
        None,
        0.0,
        0.0,
        10.0,
        10.0,
    )
    .unwrap();
    repo.delete(p, item).unwrap();
    let n2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_attachments WHERE id=?1",
            rusqlite::params![att],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n2, 1, "CANVAS-TC015: 删画布不得删掉历史附件");
}

/// 附件被删除时，叠加层保留但降级为「无二进制引用」（ON DELETE SET NULL）。
#[test]
fn deleting_attachment_keeps_overlay_but_drops_binary_reference() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let item = mk_item(&conn, p, "极限");
    let att = mk_attachment(&conn, p, item, "示意图.png");
    let repo = KnowledgeCanvasRepository::new(&conn);
    let e = repo
        .add_embed(
            p,
            item,
            "image",
            Some(att),
            None,
            None,
            0.0,
            0.0,
            10.0,
            10.0,
        )
        .unwrap();

    conn.execute(
        "DELETE FROM learning_attachments WHERE id=?1",
        rusqlite::params![att],
    )
    .unwrap();

    let list = repo.list_embeds(p, item).unwrap();
    assert_eq!(list.len(), 1, "叠加层保留（不因附件消失而丢布局）");
    assert_eq!(list[0].id, e.id);
    assert_eq!(list[0].attachment_id, None, "二进制引用被置空");
}

// ==================== §34 / §39 / §0C.7 UI 接线契约（源码级） ====================

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

/// CANVAS-TC012 / UX-TC005：双击知识图节点直接进入该节点 Canvas。
#[test]
fn canvas_ui_graph_double_click_opens_canvas() {
    let k = read_src("../src/pages/Knowledge.tsx");
    assert!(
        k.contains(r#"const KnowledgeCanvas = lazy(() => import("../features/knowledge/canvas/KnowledgeCanvas"))"#),
        "CANVAS-TC012: 画布必须 lazy 加载（Excalidraw 不进首屏 chunk）"
    );
    let open_idx = k
        .find("onOpen={(itemId) => {")
        .expect("CANVAS-TC012: KnowledgeFlow 必须保留 onOpen（图 → 节点）");
    let block = &k[open_idx..(open_idx + 400).min(k.len())];
    assert!(
        block.contains("setViewMode(\"workspace\")") && block.contains("setNodeMode(\"canvas\")"),
        "UX-TC005: 双击图节点必须直接落到 workspace + canvas，而不是要求再点「打开工作区」"
    );
    assert!(
        k.contains("<KnowledgeCanvas") && k.contains("nodeMode === \"canvas\""),
        "CANVAS-TC012: canvas 模式必须真的挂载 KnowledgeCanvas"
    );
}

/// CANVAS-TC013 / TC014：Document（Tiptap）与 Records 时间线不得被画布取代（§39）。
#[test]
fn canvas_ui_document_and_records_modes_still_exist() {
    let k = read_src("../src/pages/Knowledge.tsx");
    assert!(
        k.contains("RichDocEditor") && k.contains("editingDoc != null"),
        "CANVAS-TC013: Document 模式（Tiptap）必须仍在（§39 不迁移已有 rich docs）"
    );
    assert!(
        k.contains("kws__timeline") && k.contains("workspace.documents.length === 0"),
        "CANVAS-TC014: Records / 时间线必须仍在"
    );
    assert!(
        k.contains("画布") && k.contains("内容"),
        "§39: 节点内容必须在 Canvas 与内容（文档 / 记录）之间可切换"
    );
    // 画布分支不得替换文档分支：两者在同一个三元链里互斥存在
    assert!(
        k.contains(r#"nodeMode === "canvas" && activeProfile != null"#),
        "§39: 画布分支必须与既有内容渲染分支互斥共存"
    );
}

/// §34：直接依赖官方组件，不 fork、不复制其源码到 Higher。
#[test]
fn canvas_uses_official_excalidraw_without_forking() {
    let pkg = read_src("../package.json");
    assert!(
        pkg.contains("\"@excalidraw/excalidraw\""),
        "§34: 必须依赖官方 @excalidraw/excalidraw"
    );
    let canvas_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/features/knowledge/canvas");
    let entries = std::fs::read_dir(&canvas_dir)
        .expect("§34: canvas 目录必须存在")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    for f in &entries {
        assert!(
            f.ends_with(".ts") || f.ends_with(".tsx"),
            "§34: canvas 目录只应有我们自己的实现，出现 {f} 说明引入了第三方源码"
        );
    }
    for must in [
        "KnowledgeCanvas.tsx",
        "useKnowledgeCanvas.ts",
        "canvasSerialization.ts",
        "CanvasEmbedLayer.tsx",
        "CanvasDropzone.tsx",
    ] {
        assert!(entries.iter().any(|f| f == must), "§34: 缺少 {must}");
    }
}

/// §35.1 / §4：画布样式不得自己铺图（batch064r2 R2 全局契约延续）。
#[test]
fn canvas_styles_do_not_paint_their_own_background_image() {
    let css = read_src("../src/styles.css");
    let start = css
        .find("PRODUCT-2.0 §34-§38 Knowledge Canvas")
        .expect("画布样式块必须存在");
    let block = strip_css_comments(&css[start..]);
    assert!(
        !block.contains("background-image:") && !block.contains("background: url("),
        "§4/batch064r2: 画布样式不得铺设自己的背景图（壁纸必须继续透出）"
    );
    // 叠加层容器必须指针穿透，否则会吞掉 Excalidraw 的绘制手势
    assert!(
        block.contains(".kcanvas__overlay") && block.contains("pointer-events: none"),
        "§37: 叠加层容器必须 pointer-events: none（卡片自己吃指针）"
    );
}

/// 去掉 CSS 注释，避免「注释里写了 background-image 这个词」被误判为违规。
fn strip_css_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_comment = false;
    let mut prev = '\0';
    for ch in src.chars() {
        if !in_comment && prev == '/' && ch == '*' {
            out.pop(); // 撤回已写入的 '/'
            in_comment = true;
            prev = '\0';
            continue;
        }
        if in_comment && prev == '*' && ch == '/' {
            in_comment = false;
            prev = '\0';
            continue;
        }
        if !in_comment {
            out.push(ch);
        }
        prev = ch;
    }
    out
}
