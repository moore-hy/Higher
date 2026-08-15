//! DEV-0004 学习层级结构 - 集成测试
//!
//! 覆盖 TASK.md 测试 B/C/D/H/I + 部分 L（持久化层级）：
//! - B: 创建根节点（parent_id = NULL）
//! - C: 多层子项 数学 → 高等数学 → 极限
//! - D: 同一 Goal 下多个根节点
//! - H: Goal 隔离（list_by_goal 不串数据）
//! - I: 跨 Goal parent 防护（Repository 必须拒绝）
//! - L: 持久化（关闭重开 Goal/Item 层级仍在）
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test learning_hierarchy`

use app_lib::db::DbState;
use app_lib::repository::{
    goal::GoalRepository, learning_item::LearningItemRepository,
};
use rusqlite::Connection;

/// 创建一个默认 StudyProfile 并返回其 id（用于测试中创建 Goal）。
fn create_default_profile(conn: &Connection) -> i64 {
    use app_lib::repository::study_profile::StudyProfileRepository;
    let profile = StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap();
    profile.id
}

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    // v013 StudySessionRepository reads time_corrected; create idempotently if migration lacks it
    let tc: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='time_corrected'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if tc == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN time_corrected INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();
    }
    conn
}

#[test]
fn test_b_create_root_node_parent_id_null() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let root = item_repo.create_root(goal.id, "数学", None).unwrap();

    assert_eq!(root.goal_id, Some(goal.id));
    assert_eq!(root.name, "数学");
    assert_eq!(root.parent_id, None, "根节点 parent_id 必须为 NULL");
    assert_eq!(root.mastery_status, "not_started");
}

#[test]
fn test_c_multi_level_hierarchy() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let adv_math = item_repo
        .create_child(goal.id, math.id, "高等数学", None)
        .unwrap();
    let limit = item_repo
        .create_child(goal.id, adv_math.id, "极限", None)
        .unwrap();

    // 父子关系正确
    assert_eq!(adv_math.parent_id, Some(math.id));
    assert_eq!(limit.parent_id, Some(adv_math.id));

    // 三层都属于同一 Goal
    assert_eq!(math.goal_id, Some(goal.id));
    assert_eq!(adv_math.goal_id, Some(goal.id));
    assert_eq!(limit.goal_id, Some(goal.id));

    // get_children 验证
    let math_children = item_repo.get_children(math.id).unwrap();
    assert_eq!(math_children.len(), 1);
    assert_eq!(math_children[0].id, adv_math.id);

    let adv_math_children = item_repo.get_children(adv_math.id).unwrap();
    assert_eq!(adv_math_children.len(), 1);
    assert_eq!(adv_math_children[0].id, limit.id);

    let limit_children = item_repo.get_children(limit.id).unwrap();
    assert!(limit_children.is_empty(), "叶子节点不应有子节点");
}

#[test]
fn test_d_multiple_root_nodes_same_goal() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let english = item_repo.create_root(goal.id, "英语", None).unwrap();
    let cs408 = item_repo.create_root(goal.id, "408", None).unwrap();

    // 三个均为根节点
    assert_eq!(math.parent_id, None);
    assert_eq!(english.parent_id, None);
    assert_eq!(cs408.parent_id, None);

    // list_by_goal 返回全部 3 个
    let items = item_repo.list_by_goal(goal.id).unwrap();
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|i| i.parent_id.is_none()));
}

#[test]
fn test_h_goal_isolation() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal_a = goal_repo.create(create_default_profile(&conn), "Goal A", None).unwrap();
    let goal_b = goal_repo.create(create_default_profile(&conn), "Goal B", None).unwrap();

    let _a_root = item_repo.create_root(goal_a.id, "A 的根", None).unwrap();
    let _b_root = item_repo.create_root(goal_b.id, "B 的根", None).unwrap();
    let _a_child = item_repo
        .create_child(goal_a.id, _a_root.id, "A 的子", None)
        .unwrap();

    // list_by_goal(goal_a) 只返回 A 的 items
    let a_items = item_repo.list_by_goal(goal_a.id).unwrap();
    assert_eq!(a_items.len(), 2, "Goal A 应有 2 个 item");
    assert!(a_items.iter().all(|i| i.goal_id == Some(goal_a.id)));

    // list_by_goal(goal_b) 只返回 B 的 items
    let b_items = item_repo.list_by_goal(goal_b.id).unwrap();
    assert_eq!(b_items.len(), 1, "Goal B 应有 1 个 item");
    assert!(b_items.iter().all(|i| i.goal_id == Some(goal_b.id)));
    assert_eq!(b_items[0].name, "B 的根");
}

#[test]
fn test_i_cross_goal_parent_rejected() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal_a = goal_repo.create(create_default_profile(&conn), "Goal A", None).unwrap();
    let goal_b = goal_repo.create(create_default_profile(&conn), "Goal B", None).unwrap();

    // 在 Goal B 下创建根节点
    let b_root = item_repo.create_root(goal_b.id, "B 的根", None).unwrap();

    // 尝试用 Goal A 的 goal_id + Goal B 的 parent_id 创建子节点 —— 必须拒绝
    let result = item_repo.create_child(goal_a.id, b_root.id, "违规子项", None);
    assert!(
        result.is_err(),
        "跨 Goal parent 必须被 Repository 拒绝"
    );

    // 验证错误信息提到跨 Goal
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("跨 Goal") || err_msg.contains("goal_id") || err_msg.contains("跨档案") || err_msg.contains("不属于"),
        "错误信息应说明跨 Goal 拒绝原因，实际: {}",
        err_msg
    );

    // 数据库中没有产生非法结构
    let a_items = item_repo.list_by_goal(goal_a.id).unwrap();
    assert_eq!(a_items.len(), 0, "Goal A 不应有任何 item");
    let b_items = item_repo.list_by_goal(goal_b.id).unwrap();
    assert_eq!(b_items.len(), 1, "Goal B 仍只有 1 个根节点");
}

#[test]
fn test_i_nonexistent_parent_rejected() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "Goal", None).unwrap();

    // parent_id 指向不存在的 id —— 必须拒绝
    let result = item_repo.create_child(goal.id, 99999, "无父节点", None);
    assert!(result.is_err(), "不存在的 parent_id 必须被拒绝");
}

#[test]
fn test_l_hierarchy_persistence() {
    let temp_dir = std::env::temp_dir().join("higher_test_hierarchy_persistence");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test_hierarchy.db");
    let _ = std::fs::remove_file(&db_path);

    let goal_id;
    let math_id;
    let adv_math_id;
    let limit_id;

    // 第一次打开：建立完整层级
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        let goal_repo = GoalRepository::new(&conn);
        let item_repo = LearningItemRepository::new(&conn);

        let goal = goal_repo.create(create_default_profile(&conn), "持久化层级", None).unwrap();
        goal_id = goal.id;
        let math = item_repo.create_root(goal_id, "数学", None).unwrap();
        math_id = math.id;
        let adv = item_repo
            .create_child(goal_id, math_id, "高等数学", None)
            .unwrap();
        adv_math_id = adv.id;
        let limit = item_repo
            .create_child(goal_id, adv_math_id, "极限", None)
            .unwrap();
        limit_id = limit.id;

        // 顺便修改极限的掌握状态
        item_repo.update_status(limit_id, "learning").unwrap();
    }

    // 第二次打开：验证层级与状态全部持久化
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        let item_repo = LearningItemRepository::new(&conn);

        let items = item_repo.list_by_goal(goal_id).unwrap();
        assert_eq!(items.len(), 3, "应持久化 3 个层级 item");

        // 验证父子关系
        let math = item_repo.get(math_id).unwrap().unwrap();
        assert_eq!(math.parent_id, None);
        let adv = item_repo.get(adv_math_id).unwrap().unwrap();
        assert_eq!(adv.parent_id, Some(math_id));
        let limit = item_repo.get(limit_id).unwrap().unwrap();
        assert_eq!(limit.parent_id, Some(adv_math_id));
        assert_eq!(
            limit.mastery_status, "learning",
            "掌握状态应持久化"
        );

        // Migration 不应重复执行
        let versions: Vec<u32> = {
            let mut stmt = conn
                .prepare("SELECT version FROM schema_migrations ORDER BY version")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|v| v.ok())
                .collect()
        };
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14], "schema 版本应为 v006");
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_list_all_items_returns_across_goals() {
    // 验证旧接口 list() 仍可用（跨 Goal 返回全部）—— Task / Today 页依赖
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let g1 = goal_repo.create(create_default_profile(&conn), "G1", None).unwrap();
    let g2 = goal_repo.create(create_default_profile(&conn), "G2", None).unwrap();
    item_repo.create_root(g1.id, "A", None).unwrap();
    item_repo.create_root(g2.id, "B", None).unwrap();

    let all = item_repo.list().unwrap();
    assert_eq!(all.len(), 2, "list() 应跨 Goal 返回全部 item");
}

#[test]
fn test_mastery_status_display_in_tree_data() {
    // 验证 mastery_status 字段在层级 item 中正确返回（用于树形 UI 显示状态）
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "测试", None).unwrap();
    let root = item_repo.create_root(goal.id, "根", None).unwrap();
    let child = item_repo
        .create_child(goal.id, root.id, "子", None)
        .unwrap();

    // 修改子节点状态
    item_repo.update_status(child.id, "mastered").unwrap();

    // list_by_goal 应反映最新状态
    let items = item_repo.list_by_goal(goal.id).unwrap();
    let child_fetched = items.iter().find(|i| i.id == child.id).unwrap();
    assert_eq!(child_fetched.mastery_status, "mastered");
    let root_fetched = items.iter().find(|i| i.id == root.id).unwrap();
    assert_eq!(root_fetched.mastery_status, "not_started", "未修改的应保持默认");
}
