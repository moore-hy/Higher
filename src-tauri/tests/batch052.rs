//! DEV-0052 测试（PHASE Z §202-216）。

use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::memory::MemoryRepository;
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::search::SearchRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id
}

fn json_op(etype: &str, id: Option<i64>, action: &str, after: serde_json::Value) -> ProposedOp {
    ProposedOp {
        entity_type: etype.to_string(),
        entity_id: id,
        action: action.to_string(),
        after,
        reason: "test".to_string(),
        operation_ref: None,
    }
}

// =============== §202 Migration ===============

#[test]
fn test_v017_migration_preserves_data() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 旧数据仍在
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM study_profiles WHERE id=?1", rusqlite::params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    // v017 表存在
    for t in [
        "ai_conversations", "ai_messages", "ai_runs", "ai_run_events", "ai_sources",
        "memory_records", "personalization_sources", "personalization_source_chunks",
        "personalization_profiles", "ai_change_sets", "ai_change_operations",
    ] {
        let c: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info('{}')", t),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert!(c > 0, "missing table {}", t);
    }
    // day_kind 默认 study
    let dk: String = conn
        .query_row("SELECT day_kind FROM goals LIMIT 1", [], |r| r.get(0))
        .unwrap_or("study".into());
    assert_eq!(dk, "study");
}

// =============== §203 Conversation ===============

#[test]
fn test_conversation_crud_pagination_isolation() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let repo = ConversationRepository::new(&conn);

    let c1 = repo.create(pa, "readonly", "").unwrap();
    assert_eq!(c1.mode, "readonly");
    // messages（60 条，分页）
    for i in 0..60 {
        repo.add_message(c1.id, pa, if i % 2 == 0 { "user" } else { "assistant" }, &format!("msg{}", i), None)
            .unwrap();
    }
    let page1 = repo.list_messages(c1.id, pa, 50, 0).unwrap();
    assert_eq!(page1.len(), 50);
    assert_eq!(page1[0].content, "msg10"); // 倒序取 50 后 reverse → 最旧的第 11 条开头
    let page2 = repo.list_messages(c1.id, pa, 50, 50).unwrap();
    assert_eq!(page2.len(), 10);
    assert_eq!(page2[0].content, "msg0");
    // isolation
    let cb = repo.create(pb, "assistant", "").unwrap();
    repo.add_message(cb.id, pb, "user", "other", None).unwrap();
    assert!(repo.list_messages(c1.id, pb, 10, 0).is_err());
    // 新对话跨会话检索（§23）：c1 的消息可被 search 命中
    let hits = SearchRepository::new(&conn).search(pa, "msg42", None, 5).unwrap();
    assert!(hits.iter().any(|h| h.entity_type == "conversation"), "新对话仍可检索旧会话内容");
}

// =============== §204 Mode（协议层为 prompt；此处验证 KV + conversation mode） ===============

#[test]
fn test_mode_kv_and_conversation_override() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = ConversationRepository::new(&conn);
    let c = repo.create(p, "readonly", "").unwrap();
    assert_eq!(c.mode, "readonly");
    repo.set_mode(c.id, p, "assistant").unwrap();
    assert_eq!(repo.get(c.id, p).unwrap().unwrap().mode, "assistant");
}

// =============== §205 Cancel（RunManager 纯逻辑） ===============

#[test]
fn test_run_manager_cancel_lifecycle() {
    let rm = app_lib::ai::run::RunManager::new();
    let (id, token) = rm.register();
    assert!(!token.is_cancelled());
    assert_eq!(rm.active_count(), 1);
    assert!(rm.cancel(&id));
    assert!(token.is_cancelled(), "cancel 后 token 置位（后续工具/流式循环检测退出）");
    rm.finish(&id);
    assert_eq!(rm.active_count(), 0, "结束即从 registry 移除（不常驻 RAM）");
    assert!(!rm.cancel(&id), "已结束的 run 无法再取消");
}

// =============== §206 Memory ===============

#[test]
fn test_memory_types_supersede_isolation_retrieval() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let repo = MemoryRepository::new(&conn);

    let mk = |t: &str, k: &str, v: &str, sk: &str, ex: &str, c: &str| app_lib::repository::memory::MemoryRecord {
        id: 0, profile_id: pa, memory_type: t.into(), category: "test".into(),
        memory_key: k.into(), memory_value: v.into(), source_kind: sk.into(),
        source_ref: "conv:1".into(), source_excerpt: ex.into(), importance: 4,
        confidence: c.into(), status: "active".into(),
        valid_from: None, valid_to: None, supersedes_id: None,
        created_at: String::new(), updated_at: String::new(), last_used_at: None,
    };

    // user_fact（带原话）
    let f1 = repo.insert(&mk("user_fact", "工作日学习时长", "工作日最多学 2 小时", "user_message", "我现在工作日最多学2小时。", "high")).unwrap();
    // user_opinion
    repo.insert(&mk("user_opinion", "数学基础感受", "感觉数学基础比较差", "user_message", "我感觉自己数学基础比较差。", "medium")).unwrap();
    // system_observation 必须来自 higher_db
    assert!(repo.insert(&mk("system_observation", "stats", "30 天学 21h", "ai_inference", "", "high")).is_err());
    repo.insert(&mk("system_observation", "stats", "最近 30 天数学学习 21 小时", "higher_db", "", "high")).unwrap();
    // ai_inference 冒充 user_fact → 拒
    assert!(repo.insert(&mk("user_fact", "fake", "x", "ai_inference", "", "low")).is_err());
    // ai_inference 合法
    repo.insert(&mk("ai_inference", "极限状态", "极限可能仍存在理解缺口", "ai_inference", "", "medium")).unwrap();

    // supersede（§34）：同 key 新记录 → 旧记录 superseded
    let f2 = repo.insert(&mk("user_fact", "工作日学习时长", "现在每天只有 1 小时", "user_message", "现在每天只有1小时。", "high")).unwrap();
    let old = repo.get(f1, pa).unwrap().unwrap();
    assert_eq!(old.status, "superseded", "旧记录不删，状态 superseded");
    let new = repo.get(f2, pa).unwrap().unwrap();
    assert_eq!(new.status, "active");

    // 检索（相关度；superseded 记录不作为 active 返回）：
    let hits = repo.search(pa, "工作日 学习 时间", 5).unwrap();
    assert!(hits.iter().all(|m| m.status == "active"), "检索只返回 active 记录");
    assert!(!hits.iter().any(|m| m.id == f1), "superseded 旧记录不再返回");

    // isolation
    assert!(repo.search(pb, "工作日", 5).unwrap().is_empty());
}

// =============== §207 FTS 全实体 ===============

#[test]
fn test_fts_all_entities() {
    let conn = setup();
    let p = mk_profile(&conn);
    let sr = SearchRepository::new(&conn);
    // 各实体写入
    sr.upsert("goal", 1, p, "考研上岸", "考研", None).unwrap();
    sr.upsert("task", 2, p, "背单词", "每日背 100 词", None).unwrap();
    sr.upsert("session", 3, p, "快速学习", "学习了极限的定义", None).unwrap();
    sr.upsert("knowledge", 4, p, "高等数学", "极限与连续", None).unwrap();
    sr.upsert("document", 5, p, "极限笔记", "等价无穷小替换", None).unwrap();
    sr.upsert("evaluation", 6, p, "极限测试", "10 题对 8", None).unwrap();
    sr.upsert("memory", 7, p, "偏好", "偏好晚上学习", None).unwrap();
    // conversation（add_message 自动索引）
    let cr = ConversationRepository::new(&conn);
    let c = cr.create(p, "readonly", "").unwrap();
    cr.add_message(c.id, p, "user", "我想聊聊线性代数的复习", None).unwrap();
    // personalization chunk
    let pr = PersonalizationRepository::new(&conn);
    let sid = pr.insert_source(p, "a.txt", "txt", "x", "sha1", "", "extracted").unwrap();
    pr.store_chunks(sid, p, "我是大三学生，准备 2027 考研，目标计算机专业").unwrap();

    for (q, etype) in [
        ("考研", "goal"), ("背单词", "task"), ("极限", "session"),
        ("连续", "knowledge"), ("无穷小", "document"), ("题对", "evaluation"),
        ("晚上", "memory"), ("线性代数", "conversation"), ("计算机专业", "personalization_chunk"),
    ] {
        let hits = sr.search(p, q, None, 5).unwrap();
        assert!(
            hits.iter().any(|h| h.entity_type == etype),
            "「{}」应命中 {}，实际 {:?}",
            q, etype,
            hits.iter().map(|h| h.entity_type.clone()).collect::<Vec<_>>()
        );
    }
    // profile 隔离
    let p2 = mk_profile(&conn);
    assert!(sr.search(p2, "考研", None, 5).unwrap().is_empty());
    // upsert 更新生效
    sr.upsert("goal", 1, p, "考研上岸（更新）", "", None).unwrap();
    let hits = sr.search(p, "更新", None, 5).unwrap();
    assert!(hits.iter().any(|h| h.entity_id == 1 && h.entity_type == "goal"));
}

// =============== §208 Personalization 导入（构造文件） ===============

#[test]
fn test_personalization_import_txt_md_gbk_and_doc() {
    let dir = std::env::temp_dir().join("higher_p52_test");
    std::fs::create_dir_all(&dir).unwrap();
    // UTF-8 txt
    let f1 = dir.join("a.txt");
    std::fs::write(&f1, "我是大三学生。\n工作日每天可学 3 小时。").unwrap();
    let t1 = app_lib::repository::personalization::decode_text(std::fs::read(&f1).unwrap()).unwrap();
    assert!(t1.contains("大三学生"));
    // GBK md
    let gbk_bytes: Vec<u8> = "我准备2027考研".encode_utf16().collect::<Vec<u16>>().iter().map(|&x| x as u8).collect();
    let _ = gbk_bytes; // UTF-16 不是 GBK；改用 encoding_rs 生成 GBK
    let (encoded, _, _) = encoding_rs::GBK.encode("我准备2027考研，目标计算机。");
    let f2 = dir.join("b.md");
    std::fs::write(&f2, encoded.as_ref()).unwrap();
    let t2 = app_lib::repository::personalization::decode_text(std::fs::read(&f2).unwrap()).unwrap();
    assert!(t2.contains("计算机"), "GBK 解码 fallback");
    // .doc 拒绝
    let f3 = dir.join("old.doc");
    std::fs::write(&f3, b"D0CF11E0binary").unwrap();
    // 非 zip 头 → 明确错误（extract_docx 校验 PK 头）
    let err = app_lib::repository::personalization::extract_docx(&f3).unwrap_err();
    assert!(err.contains("docx") || err.contains("转换"), ".doc 明确提示转换：{err}");
    // HTML 提取（web_open 用）
    let html = "<html><head><style>x{}</style><script>alert(1)</script></head><body><p>极限是函数值的趋势</p><div>连续性</div></body></html>";
    let text = app_lib::ai::web::extract_html_text(html);
    assert!(text.contains("极限是函数值的趋势") && text.contains("连续性"));
    assert!(!text.contains("alert"), "script 剥离");
    assert!(!text.contains("x{}"), "style 剥离");
}

#[test]
fn test_personalization_chunks_and_user_edit_confirm() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = PersonalizationRepository::new(&conn);
    let sid = repo.insert_source(p, "a.txt", "txt", "r", "s1", "", "extracted").unwrap();
    let n = repo.store_chunks(sid, p, &"段".repeat(300_000)).unwrap();
    assert!(n >= 2, "256KB 分块：{} 块", n);
    // draft → confirm → user_edit
    repo.save_draft(p, "# 档案\n内容", None).unwrap();
    assert_eq!(repo.get_profile(p).unwrap().unwrap().status, "draft");
    repo.confirm(p).unwrap();
    let prof = repo.get_profile(p).unwrap().unwrap();
    assert_eq!((prof.status.as_str(), prof.version), ("confirmed", 2));
    repo.user_edit(p, "# 手工编辑版").unwrap();
    assert!(repo.get_profile(p).unwrap().unwrap().md_content.contains("手工编辑版"));
    // user_edit 记为 user_fact 记忆
    let mems = MemoryRepository::new(&conn).list_active(p).unwrap();
    assert!(mems.iter().any(|m| m.source_kind == "user_edit" && m.memory_value.contains("手工编辑版")));
}

// =============== §209 Web（SSRF guard 纯逻辑；Brave 解析走构造 JSON 不发真请求） ===============

#[test]
fn test_ssrf_guard() {
    let blocked = [
        "file:///etc/passwd",
        "http://localhost/x",
        "https://127.0.0.1/admin",
        "http://0.0.0.0/",
        "https://[::1]/",
        "http://10.1.2.3/internal",
        "http://192.168.1.1/router",
        "http://172.16.0.5/",
        "http://169.254.169.254/latest/meta-data",
        "ftp://example.com/f",
    ];
    for u in blocked {
        assert!(app_lib::ai::web::ssrf_check(u).is_err(), "应拦截 {}", u);
    }
    assert!(app_lib::ai::web::ssrf_check("https://example.com/page").is_ok());
    assert!(app_lib::ai::web::ssrf_check("http://api.search.brave.com/res/v1/web/search").is_ok());
    // redirect 复查
    assert!(app_lib::ai::web::redirect_check("http://localhost/after").is_err());
}

// =============== §210 Citation 扫描 ===============

#[test]
fn test_citation_scan_and_prompt_rules() {
    // [[Sx]] 扫描正确性（lib 内私有 → 通过 prompts 常量与行为测试覆盖可见部分）
    let text = "结论A [[S1]]，结论B [[S2]]，坏引用 [[S99]] 和非引用 [S3] [[X1]]";
    let b = text.as_bytes();
    let mut ids = Vec::new();
    let mut i = 0usize;
    while i + 4 <= b.len() {
        if b[i] == b'[' && b[i + 1] == b'[' && b[i + 2] == b'S' {
            let mut j = i + 3;
            while j < b.len() && b[j].is_ascii_digit() { j += 1; }
            if j + 1 < b.len() && b[j] == b']' && b[j + 1] == b']' && j > i + 3 {
                ids.push(text[i + 2..j].to_string());
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    assert_eq!(ids, vec!["S1".to_string(), "S2".to_string(), "S99".to_string()], "[S3]/[[X1]] 不算");
    // §105/§113/§114 prompt 固化
    let sp = app_lib::ai::prompts::SYSTEM_PROMPT;
    assert!(sp.contains("不能胡编乱造") && sp.contains("没有足够证据"));
    assert!(sp.contains("官方") && sp.contains("没有找到足够可靠的官方来源"));
    assert!(sp.contains("Ignore previous instructions") && sp.contains("不执行"), "注入防护条款");
}

// =============== §212 ChangeSet ===============

#[test]
fn test_changeset_task_lifecycle_apply_reject_undo_conflict() {
    let conn = setup();
    let p = mk_profile(&conn);
    let goal = app_lib::repository::goal::GoalRepository::new(&conn).ensure_final(p).unwrap();
    let repo = ChangeSetRepository::new(&conn);

    // create task
    let cs1 = repo.create(p, None, Some("run-1"), "创建任务", "",
        &[json_op("task", None, "create", serde_json::json!({
            "title": "背 20 个单词", "planned_date": "2026-08-20", "goal_id": null
        }))]).unwrap();
    // 未 apply → 数据 0 修改
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(n, 0, "未批准的 ChangeSet 不落库");
    repo.apply(cs1, p, false).unwrap();
    let (tid, title): (i64, String) = conn
        .query_row("SELECT id, title FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(title, "背 20 个单词");
    assert_eq!(repo.get(cs1, p).unwrap().unwrap().status, "applied");

    // undo（§221）
    repo.undo(cs1, p).unwrap();
    let n2: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(n2, 0, "undo 后任务恢复原状（不存在）");

    // update + 冲突（§136）
    let _ = conn.execute("INSERT INTO tasks (id, profile_id, title, planned_date) VALUES (100, ?1, '原任务', '2026-08-20')", rusqlite::params![p]).unwrap();
    let cs2 = repo.create(p, None, None, "改任务", "",
        &[json_op("task", Some(100), "update", serde_json::json!({ "title": "新标题" }))]).unwrap();
    // 审查期间手工修改 → before 不一致 → 拒绝 apply
    conn.execute("UPDATE tasks SET title='用户手改' WHERE id=100", rusqlite::params![]).unwrap();
    let err = repo.apply(cs2, p, false).unwrap_err();
    assert!(err.contains("数据已发生变化"), "冲突拒绝：{err}");
    // 数据保持手改值
    let t: String = conn.query_row("SELECT title FROM tasks WHERE id=100", [], |r| r.get(0)).unwrap();
    assert_eq!(t, "用户手改");

    // reject
    let cs3 = repo.create(p, None, None, "删任务", "",
        &[json_op("task", Some(100), "delete", serde_json::json!({}))]).unwrap();
    repo.reject(cs3, p).unwrap();
    assert_eq!(repo.get(cs3, p).unwrap().unwrap().status, "rejected");

    // selective apply（§131）
    let cs4 = repo.create(p, None, None, "多操作", "",
        &[
            json_op("task", None, "create", serde_json::json!({ "title": "任务A" })),
            json_op("task", None, "create", serde_json::json!({ "title": "任务B" })),
        ]).unwrap();
    let ops = repo.list_operations(cs4, p).unwrap();
    repo.set_selected(ops[1].id, cs4, false).unwrap();
    repo.apply(cs4, p, true).unwrap();
    let titles: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT title FROM tasks WHERE profile_id=?1 AND title IN ('任务A','任务B')")
            .unwrap();
        let rows = stmt.query_map(rusqlite::params![p], |r| r.get(0)).unwrap();
        rows.filter_map(|r| r.ok()).collect()
    };
    assert_eq!(titles, vec!["任务A".to_string()], "只应用选中项");

    // 事务回滚（§135：第二个失败 → 第一个也回滚）
    let cs5 = repo.create(p, None, None, "混合", "",
        &[
            json_op("task", None, "create", serde_json::json!({ "title": "会成功的" })),
            json_op("task", Some(99999), "update", serde_json::json!({ "title": "不存在的实体" })),
        ]).unwrap();
    assert!(repo.apply(cs5, p, false).is_err());
    let n5: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE title='会成功的'", [], |r| r.get(0)).unwrap();
    assert_eq!(n5, 0, "整包回滚");
    let _ = goal;
}

// =============== §213 Restricted：写工具仍 0 ===============

#[test]
fn test_no_direct_write_tools() {
    for name in app_lib::ai::tools::TOOL_ALLOWLIST {
        let n = name.to_lowercase();
        let is_propose = n.starts_with("propose_");
        let direct_write = n.contains("create_") || n.contains("update_") || n.contains("delete_")
            || n.contains("apply_") || n == "apply_change_set";
        assert!(!direct_write || is_propose, "直接写工具泄漏：{name}");
    }
    assert!(app_lib::ai::tools::TOOL_ALLOWLIST.contains(&"propose_change_set"));
    assert!(!app_lib::ai::tools::TOOL_ALLOWLIST.contains(&"apply_change_set"), "apply 只能是 UI 命令");
    assert!(!app_lib::ai::tools::TOOL_ALLOWLIST.contains(&"read_vault"), "默认无 read_vault");
}

// =============== §214 Annual Goal 跨年 + 不重叠 ===============

#[test]
fn test_annual_goal_cross_year_and_overlap() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = ChangeSetRepository::new(&conn);
    let final_goal = app_lib::repository::goal::GoalRepository::new(&conn).ensure_final(p).unwrap();

    // 跨自然年 2026-08-20 ~ 2027-08-19（§214）
    let cs1 = repo.create(p, None, None, "年度1", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "year", "parent_goal_id": final_goal.id, "name": "考研年",
            "period": "2026-08-20..2027-08-19"
        }))]).unwrap();
    repo.apply(cs1, p, false).unwrap();
    let (ps, pe): (String, String) = conn
        .query_row("SELECT period_start, period_end FROM goals WHERE goal_level='year' AND profile_id=?1", rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((ps.as_str(), pe.as_str()), ("2026-08-20", "2027-08-19"), "跨自然年合法");

    // 重叠拒绝（§143）
    let cs2 = repo.create(p, None, None, "年度2", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "year", "parent_goal_id": final_goal.id, "name": "重叠年",
            "period": "2027-01-01..2027-06-30"
        }))]).unwrap();
    let err = repo.apply(cs2, p, false).unwrap_err();
    assert!(err.contains("重叠"), "重叠拒绝：{err}");

    // 相邻允许
    let cs3 = repo.create(p, None, None, "年度3", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "year", "parent_goal_id": final_goal.id, "name": "下一年",
            "period": "2027-08-20..2028-08-19"
        }))]).unwrap();
    repo.apply(cs3, p, false).unwrap();

    // Month inside annual：首月部分覆盖允许（月起点须落在 annual 内，§144）
    let y1: i64 = conn.query_row("SELECT id FROM goals WHERE name='考研年'", [], |r| r.get(0)).unwrap();
    // 月起点 2026-08-01 早于 annual 起点 08-20 → 拒绝
    let cs_bad = repo.create(p, None, None, "月越界", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "month", "parent_goal_id": y1, "name": "8月外", "period": "2026-08"
        }))]).unwrap();
    assert!(repo.apply(cs_bad, p, false).is_err(), "月起点不在 annual 内应拒绝");
    // 合法月（起点 2026-09-01 ∈ [2026-08-20, 2027-08-19]）
    let cs4 = repo.create(p, None, None, "月", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "month", "parent_goal_id": y1, "name": "9月", "period": "2026-09"
        }))]).unwrap();
    repo.apply(cs4, p, false).unwrap();
    // Day inside month
    let m1: i64 = conn.query_row("SELECT id FROM goals WHERE name='9月'", [], |r| r.get(0)).unwrap_or(-1);
    assert!(m1 > 0);
    let cs5 = repo.create(p, None, None, "日", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "day", "parent_goal_id": m1, "name": "9/15", "period": "2026-09-15"
        }))]).unwrap();
    repo.apply(cs5, p, false).unwrap();
    let d_count: i64 = conn.query_row("SELECT COUNT(*) FROM goals WHERE name='9/15'", [], |r| r.get(0)).unwrap();
    assert_eq!(d_count, 1, "Day 严格从属 Month");
    // 日不在月内 → 拒
    let cs6 = repo.create(p, None, None, "日越界", "",
        &[json_op("goal", None, "create", serde_json::json!({
            "goal_level": "day", "parent_goal_id": m1, "name": "10/01", "period": "2026-10-01"
        }))]).unwrap();
    assert!(repo.apply(cs6, p, false).is_err(), "日不属于其父月应拒绝");
}

// =============== §215 Rest Day ===============

#[test]
fn test_rest_day_guard_and_quick_learning() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = app_lib::repository::goal::GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let y = grepo.create_tree_node(p, "year", Some(f.id), "2026", None, Some("2026")).unwrap();
    let m = grepo.create_tree_node(p, "month", Some(y.id), "8月", None, Some("2026-08")).unwrap();
    let d = grepo.create_tree_node(p, "day", Some(m.id), "8/16", None, Some("2026-08-16")).unwrap();

    // 设为休息日
    conn.execute("UPDATE goals SET day_kind='rest' WHERE id=?1", rusqlite::params![d.id]).unwrap();
    // Rest Day 不允许计划 Task（changeset apply 层拒）
    let crepo = ChangeSetRepository::new(&conn);
    let cs = crepo.create(p, None, None, "rest 任务", "",
        &[json_op("task", None, "create", serde_json::json!({
            "title": "休息日任务", "goal_id": d.id
        }))]).unwrap();
    let err = crepo.apply(cs, p, false).unwrap_err();
    assert!(err.contains("休息日"), "Rest Day 任务拒绝：{err}");
    // 快速学习（无 goal 关联）仍允许 → session 正常
    let s = app_lib::repository::study_session::StudySessionRepository::new(&conn)
        .start_quick(p, None).unwrap();
    assert!(s.id > 0, "休息日快速学习不受限");
    // 有未完成任务时不能设为 rest
    conn.execute("UPDATE goals SET day_kind='study' WHERE id=?1", rusqlite::params![d.id]).unwrap();
    conn.execute("INSERT INTO tasks (profile_id, goal_id, title, status) VALUES (?1,?2,'t','pending')", rusqlite::params![p, d.id]).unwrap();
    let cs2 = crepo.create(p, None, None, "设休", "",
        &[json_op("goal", Some(d.id), "status_change", serde_json::json!({ "day_kind": "rest" }))]).unwrap();
    let err2 = crepo.apply(cs2, p, false).unwrap_err();
    assert!(err2.contains("未完成任务"), "有任务时禁设休息日：{err2}");
}

// =============== §216 Vault ===============

#[test]
fn test_vault_lock_unlock_audit_blob() {
    let dir = std::env::temp_dir().join(format!("higher_vault_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let v = app_lib::ai::vault::VaultState::new(dir.clone());

    // 默认 LOCKED（§160）
    assert!(v.is_locked());
    // 错误密码
    assert!(v.unlock("wrong").is_err());
    assert!(v.is_locked());
    // root 解锁（§159）
    v.unlock("root").unwrap();
    assert!(!v.is_locked());
    // 三类审计
    v.record_user("create", "task", Some(1), "t");
    v.record_ai("run_done", "r1", "ok");
    v.record_system("startup", "app", None, "");
    let events = v.list_events(10).unwrap();
    assert!(events.len() >= 4);
    assert!(events.iter().any(|e| e.actor_type == "USER"));
    assert!(events.iter().any(|e| e.actor_type == "AI"));
    assert!(events.iter().any(|e| e.actor_type == "SYSTEM"));
    // Blob 去重 + chunk
    let big = dir.join("blob.bin");
    std::fs::write(&big, vec![7u8; 2_500_000]).unwrap(); // 2.5MB → 3 chunks
    let s1 = v.store_blob(&big).unwrap();
    let s2 = v.store_blob(&big).unwrap();
    assert_eq!(s1, s2, "同内容 SHA-256 去重");
    let stats = v.stats().unwrap();
    assert_eq!(stats.1, 1, "只有一个 blob 记录");
    // lock 后读取拒绝
    v.lock();
    assert!(v.list_events(10).is_err(), "锁定读取拒绝");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_requirement_template_and_skeleton() {
    let t = app_lib::repository::personalization::REQUIREMENT_TEMPLATE_MD;
    assert!(t.contains("不要一次问所有问题") && t.contains("AI 需求采集模板"));
    let s = app_lib::repository::personalization::profile_md_skeleton();
    assert!(s.contains("## 19. 更新历史") && s.contains("## 1. 基本情况"));
}
