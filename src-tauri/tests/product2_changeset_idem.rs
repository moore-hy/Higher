//! PRODUCT-2.0 §26.4 —— ChangeSet Apply Idempotency（P0）。
//!
//! 前端防双击只是第一层；后端必须 authoritative。同一个 `change_set_id`：
//!
//! ```text
//! 第一次 apply → transaction commit → status=applied
//! 第二次 apply → already_applied / same readback
//! ```
//!
//! 第二次绝不能重复 Task / Blueprint / recurring rule / delete。
//!
//! 覆盖：CHANGESET-IDEM-TC001..004。

use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
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

fn op(etype: &str, id: Option<i64>, action: &str, after: serde_json::Value) -> ProposedOp {
    ProposedOp {
        entity_type: etype.to_string(),
        entity_id: id,
        action: action.to_string(),
        after,
        reason: "test".to_string(),
        operation_ref: None,
    }
}

fn count_tasks(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1",
        rusqlite::params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn status_of(conn: &Connection, cs: i64) -> String {
    conn.query_row(
        "SELECT status FROM ai_change_sets WHERE id=?1",
        rusqlite::params![cs],
        |r| r.get(0),
    )
    .unwrap()
}

fn task_ids(conn: &Connection, profile_id: i64) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT id FROM tasks WHERE profile_id=?1 ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![profile_id], |r| r.get::<_, i64>(0))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// FIRST_PLAN_REVIEW：一次「首次规划」= ONE ChangeSet（§26.1），
/// 含多条 task create —— 用来验证重复 apply 不产生重复行。
fn plan_ops() -> Vec<ProposedOp> {
    vec![
        op(
            "task",
            None,
            "create",
            serde_json::json!({ "title": "第1天：极限定义", "status": "pending" }),
        ),
        op(
            "task",
            None,
            "create",
            serde_json::json!({ "title": "第2天：导数计算", "status": "pending" }),
        ),
        op(
            "task",
            None,
            "create",
            serde_json::json!({ "title": "第3天：中值定理", "status": "pending" }),
        ),
    ]
}

#[test]
fn changeset_idem_tc001_double_invoke_single_write() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = ChangeSetRepository::new(&conn);

    let cs = repo
        .create(p, None, None, "首次规划", "ONE ChangeSet", &plan_ops())
        .unwrap();

    // 第一次：真正落库
    let first = repo.apply(cs, p, false).unwrap();
    assert!(first, "第一次 apply 必须真正落库（Ok(true)）");
    assert_eq!(status_of(&conn, cs), "applied");
    assert_eq!(count_tasks(&conn, p), 3, "第一次 apply 写入 3 条 Task");
    let ids_once = task_ids(&conn, p);

    // 第二次：already_applied —— 不得报错、不得重复写入
    let second = repo.apply(cs, p, false).unwrap();
    assert!(!second, "第二次 apply 必须是 already_applied（Ok(false)）");
    assert_eq!(count_tasks(&conn, p), 3, "第二次 apply 绝不能重复 Task");
    assert_eq!(task_ids(&conn, p), ids_once, "readback 必须完全一致");
    assert_eq!(status_of(&conn, cs), "applied", "状态保持 applied");
}

#[test]
fn changeset_idem_tc002_retry_after_lost_response() {
    let conn = setup();
    let p = mk_profile(&conn, "P2");
    let repo = ChangeSetRepository::new(&conn);

    let cs = repo
        .create(p, None, None, "首次规划", "retry", &plan_ops())
        .unwrap();

    // 模拟：第一次 apply 已提交，但响应在网络上丢失 → 前端重试
    assert!(repo.apply(cs, p, false).unwrap());
    let after_first = task_ids(&conn, p);
    let applied_at_1: String = conn
        .query_row(
            "SELECT applied_at FROM ai_change_sets WHERE id=?1",
            rusqlite::params![cs],
            |r| r.get(0),
        )
        .unwrap();

    // 重试（同一 change_set_id）
    let retried = repo.apply(cs, p, false).unwrap();
    assert!(!retried, "重试必须是幂等空操作");
    assert_eq!(task_ids(&conn, p), after_first, "重试不得新增 Task");
    let applied_at_2: String = conn
        .query_row(
            "SELECT applied_at FROM ai_change_sets WHERE id=?1",
            rusqlite::params![cs],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(applied_at_1, applied_at_2, "重试不得改写 applied_at");
}

#[test]
fn changeset_idem_tc002b_double_apply_does_not_duplicate_delete() {
    let conn = setup();
    let p = mk_profile(&conn, "P2b");
    let repo = ChangeSetRepository::new(&conn);

    // 先建立一条真实 Task（走一次正常 apply）
    let seed = repo
        .create(
            p,
            None,
            None,
            "种子",
            "",
            &[op(
                "task",
                None,
                "create",
                serde_json::json!({ "title": "要被删除的任务", "status": "pending" }),
            )],
        )
        .unwrap();
    assert!(repo.apply(seed, p, false).unwrap());
    let target = task_ids(&conn, p);
    assert_eq!(target.len(), 1);

    // 删除包：单条 delete 重复 apply 必须只删一次
    let del = repo
        .create(
            p,
            None,
            None,
            "删除包",
            "",
            &[op("task", Some(target[0]), "delete", serde_json::json!({}))],
        )
        .unwrap();
    assert!(repo.apply(del, p, false).unwrap());
    assert_eq!(count_tasks(&conn, p), 0, "delete 生效一次");

    // 第二次 apply：不得报错（已删除再删会触发的错误必须被幂等短路掉）
    let again = repo.apply(del, p, false).unwrap();
    assert!(!again, "delete 包第二次 apply 必须是 already_applied");
    assert_eq!(count_tasks(&conn, p), 0, "不得出现重复 delete 造成的破坏");
}

#[test]
fn changeset_idem_tc003_rollback_on_mid_error_then_retry_succeeds() {
    let conn = setup();
    let p = mk_profile(&conn, "P3");
    let repo = ChangeSetRepository::new(&conn);

    // 第 2 条指向不存在的实体 → 整包回滚，0 写入
    let bad = repo
        .create(
            p,
            None,
            None,
            "半成功包",
            "",
            &[
                op(
                    "task",
                    None,
                    "create",
                    serde_json::json!({ "title": "会成功的", "status": "pending" }),
                ),
                op(
                    "task",
                    Some(9_999_999),
                    "update",
                    serde_json::json!({ "title": "不存在的实体" }),
                ),
            ],
        )
        .unwrap();

    let err = repo.apply(bad, p, false).unwrap_err();
    assert!(!err.is_empty(), "中途失败必须返回错误：{err}");
    assert_eq!(count_tasks(&conn, p), 0, "§26.4 TC003：中途失败整包回滚");
    assert_eq!(
        status_of(&conn, bad),
        "waiting_approval",
        "失败后状态未变为 applied（可安全重试）"
    );

    // 修正后以新 ChangeSet 重试（真实链路：Agent 重新提案），只落一次
    let good = repo
        .create(
            p,
            None,
            None,
            "修正包",
            "",
            &[
                op(
                    "task",
                    None,
                    "create",
                    serde_json::json!({ "title": "会成功的", "status": "pending" }),
                ),
                op(
                    "task",
                    None,
                    "create",
                    serde_json::json!({ "title": "第二条", "status": "pending" }),
                ),
            ],
        )
        .unwrap();
    assert!(repo.apply(good, p, false).unwrap(), "修正后应用成功");
    assert_eq!(count_tasks(&conn, p), 2, "修正后写入 2 条");

    assert!(!repo.apply(good, p, false).unwrap(), "再次重试幂等");
    assert_eq!(count_tasks(&conn, p), 2, "不得重复");

    // 失败包依然不能被应用（未被误标 applied）
    assert!(repo.apply(bad, p, false).is_err(), "未修正的坏包仍应失败");
    assert_eq!(count_tasks(&conn, p), 2);
}

#[test]
fn changeset_idem_tc004_cross_profile_reject() {
    let conn = setup();
    let p1 = mk_profile(&conn, "P1");
    let p2 = mk_profile(&conn, "P2");
    let repo = ChangeSetRepository::new(&conn);

    let cs = repo
        .create(p1, None, None, "P1 的规划", "", &plan_ops())
        .unwrap();

    // 用另一个档案 apply → 必须拒绝，且 0 写入（两个档案都干净）
    let err = repo.apply(cs, p2, false).unwrap_err();
    assert!(
        err.contains("不属于当前档案") || err.contains("不存在"),
        "跨档案必须拒绝：{err}"
    );
    assert_eq!(count_tasks(&conn, p1), 0, "P1 未被误写");
    assert_eq!(count_tasks(&conn, p2), 0, "P2 未被误写");
    assert_eq!(status_of(&conn, cs), "waiting_approval", "状态未被污染");

    // 正确档案仍可正常应用
    assert!(repo.apply(cs, p1, false).unwrap());
    assert_eq!(count_tasks(&conn, p1), 3);
}

#[test]
fn changeset_idem_rejected_still_cannot_apply() {
    // §158-159 语义保持：rejected 不是「已应用」，必须继续拒绝（不得被幂等短路）
    let conn = setup();
    let p = mk_profile(&conn, "P5");
    let repo = ChangeSetRepository::new(&conn);

    let cs = repo
        .create(p, None, None, "会被拒绝的包", "", &plan_ops())
        .unwrap();
    repo.reject(cs, p).unwrap();

    assert!(
        repo.apply(cs, p, false).is_err(),
        "rejected 状态不得 Apply（也不能被 already_applied 短路放行）"
    );
    assert_eq!(count_tasks(&conn, p), 0);
}
