//! DEV-AI-ARCH-001 · Architecture Gate（ARCH-TC01~TC04）。
//!
//! 任务书 §4：先建立架构 Gate——从生产入口出发的**可达性**验证，
//! 不是简单全仓 grep 零命中（legacy run_chat_turn / legacy tests 允许引用）。
//!
//! - ARCH-TC01：ai_start_run 的 production main 必须可达 run_agent_turn；
//! - ARCH-TC02：production Global Agent Planning 完成后不得依赖
//!   PLAN_DRAFT_INSTRUCTION / PLANNER_TURN_PROTOCOL / build_planning_instruction /
//!   compile_production_plan（PRODUCTION REACHABILITY，非全仓零命中）；
//! - ARCH-TC03：Global Agent 正式写入工具只有 execute_higher_actions——
//!   production tool 面禁止直接 SQL / 直接 create_goal / create_task repository；
//! - ARCH-TC04：Dedicated Planner 允许存在，但 GLOBAL_AGENT_LIVE_REACHABILITY = 0
//!  （从生产入口静态不可达）。
//!
//! 纪律：纯源码级 Gate（读 src/ 断言），零 Provider、零 DB fixture。

use serde_json::Value as J;

fn src(rel: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"))
}

/// 提取 lib.rs 中某个顶层 async fn 的函数体区域（到下一个顶层
/// `\nasync fn ` 或文件尾）——PRODUCTION REACHABILITY 的区域口径。
fn fn_region(lib: &str, sig: &str) -> String {
    let start = lib.find(sig).unwrap_or_else(|| panic!("{sig} 必须存在于 lib.rs"));
    let end = lib[start..]
        .find("\nasync fn ")
        .map(|i| start + i)
        .unwrap_or(lib.len());
    lib[start..end].to_string()
}

/// lib.rs 中 legacy `run_chat_turn` 的函数体区域（死代码；零调用者由
/// dev0077_3 治理 `callers == 1`（仅定义）保证）。
fn legacy_run_chat_turn_region(lib: &str) -> String {
    fn_region(lib, "async fn run_chat_turn(")
}

/// Dedicated Planner 生产链符号（§A2）。
const BANNED_PLANNER_SYMBOLS: [&str; 4] = [
    "compile_production_plan(",
    "build_planning_instruction(",
    "PLAN_DRAFT_INSTRUCTION",
    "PLANNER_TURN_PROTOCOL",
];

// ==================== ARCH-TC01 · production main 可达 run_agent_turn ====================

#[test]
fn arch_tc01_ai_start_run_reaches_run_agent_turn() {
    let lib = src("src/lib.rs");
    let entry = fn_region(&lib, "async fn ai_start_run(");
    assert!(
        entry.contains("ai::agent::run_agent_turn("),
        "ARCH-TC01：ai_start_run（production main）必须可达 run_agent_turn"
    );
    assert!(
        !entry.contains("run_chat_turn("),
        "ARCH-TC01：ai_start_run 不得路由 legacy run_chat_turn（Turn Interpreter 已退役为死代码）"
    );
    // run_agent_turn 在 agent.rs 有真实定义（可达性另一端）
    let agent = src("src/ai/agent.rs");
    assert!(agent.contains("pub async fn run_agent_turn("), "run_agent_turn 定义必须存在");
}

// ==================== ARCH-TC02 · production Planner 生产链 reachability = 0 ====================

#[test]
fn arch_tc02_production_zero_dedicated_planner_reachability() {
    let agent = src("src/ai/agent.rs");
    let lib = src("src/lib.rs");

    // ① agent.rs = production Global Agent 主链：全文件零引用（含注释）
    for banned in BANNED_PLANNER_SYMBOLS {
        assert!(
            !agent.contains(banned),
            "ARCH-TC02：agent.rs（production Agent 主链）残留 Dedicated Planner 生产链引用 {banned}"
        );
    }

    // ② lib.rs：生产入口 ai_start_run 区域零引用
    let entry = fn_region(&lib, "async fn ai_start_run(");
    for banned in BANNED_PLANNER_SYMBOLS {
        assert!(
            !entry.contains(banned),
            "ARCH-TC02：lib.rs 生产入口 ai_start_run 不得依赖 {banned}"
        );
    }

    // ③ PRODUCTION REACHABILITY 口径：lib.rs 中 banned 符号的**全部**命中
    //    必须落在 legacy run_chat_turn 死代码区域内（区域外 = 生产可达面）
    let legacy = legacy_run_chat_turn_region(&lib);
    for banned in BANNED_PLANNER_SYMBOLS {
        let total = lib.matches(banned).count();
        let in_legacy = legacy.matches(banned).count();
        assert_eq!(
            total, in_legacy,
            "ARCH-TC02：lib.rs 中 {banned} 出现 {total} 次，其中仅 {in_legacy} 次位于 legacy run_chat_turn 死代码内——生产区域存在泄漏"
        );
    }

    // ④ legacy 允许存在：planner.rs 保留定义（production 不可达，tests 可用）
    let planner = src("src/ai/planner.rs");
    assert!(planner.contains("pub fn compile_production_plan("), "planner.rs legacy 定义保留（允许存在）");
    assert!(planner.contains("pub const PLAN_DRAFT_INSTRUCTION:"), "legacy 协议常量保留");
}

// ==================== ARCH-TC03 · 写入工具唯一 = execute_higher_actions ====================

#[test]
fn arch_tc03_write_tool_surface_is_execute_higher_actions_only() {
    let agent_tools = src("src/ai/agent_tools.rs");

    // ① 行为级：生产工具面定义中的写类工具唯一
    let defs = app_lib::ai::agent_tools::agent_tool_definitions(false);
    let arr = defs.as_array().expect("工具定义为数组");
    assert!(!arr.is_empty(), "工具面非空");
    let mut names: Vec<&str> = arr
        .iter()
        .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
        .collect();
    names.sort_unstable();
    assert!(
        names.contains(&"execute_higher_actions"),
        "ARCH-TC03：写入工具 execute_higher_actions 必须在工具面：{names:?}"
    );
    // 写类候选（工具命名级判定：写语义动词开头的唯一写入口）
    for t in arr {
        let name = t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
        let is_write = ["create", "update", "delete", "write", "execute", "insert", "set_"]
            .iter()
            .any(|kw| name.contains(kw));
        if is_write {
            assert_eq!(
                name, "execute_higher_actions",
                "ARCH-TC03：业务写入能力只允许 execute_higher_actions，发现 {name}"
            );
        }
    }

    // ② 源码级：production tool 面禁止直 SQL 写 / 直 repository 建 goal·task
    //   （写入唯一通道 = execute_higher_action_pack，ChangeSet 审计管线）
    for banned in [
        "INSERT INTO goals",
        "INSERT INTO tasks",
        "INSERT INTO goal_targets",
        "INSERT INTO planning_blueprints",
        "GoalExecutor",
        "TaskExecutor",
        "execute_action(",
    ] {
        assert!(
            !agent_tools.contains(banned),
            "ARCH-TC03：agent_tools.rs（production tool 面）禁止 {banned}（写入唯一通道 execute_higher_actions）"
        );
    }
    assert!(
        agent_tools.contains("execute_higher_action_pack("),
        "ARCH-TC03：execute_higher_actions 工具必须经 execute_higher_action_pack 审计管线"
    );
}

// ==================== ARCH-TC04 · GLOBAL_AGENT_LIVE_REACHABILITY = 0 ====================

#[test]
fn arch_tc04_global_agent_live_reachability_zero() {
    let agent = src("src/ai/agent.rs");
    let lib = src("src/lib.rs");
    let agent_tools = src("src/ai/agent_tools.rs");
    let higher_action = src("src/ai/higher_action.rs");

    // 生产入口可达集合（静态）：ai_start_run → run_agent_turn（agent.rs 全文件）
    // → agent_tools.rs（工具面）→ higher_action.rs（pack 管线）。
    // GLOBAL_AGENT_LIVE_REACHABILITY = 0：该集合内 Dedicated Planner 生产链零引用。
    for (src_text, name) in [
        (&agent, "agent.rs"),
        (&agent_tools, "agent_tools.rs"),
        (&higher_action, "higher_action.rs"),
    ] {
        for banned in BANNED_PLANNER_SYMBOLS {
            assert!(
                !src_text.contains(banned),
                "ARCH-TC04：{name}（Global Agent 生产可达集合）命中 {banned} → GLOBAL_AGENT_LIVE_REACHABILITY != 0"
            );
        }
    }
    let entry = fn_region(&lib, "async fn ai_start_run(");
    for banned in BANNED_PLANNER_SYMBOLS {
        assert!(
            !entry.contains(banned),
            "ARCH-TC04：生产入口区域命中 {banned} → GLOBAL_AGENT_LIVE_REACHABILITY != 0"
        );
    }

    // Dedicated Planner 允许存在（legacy 域）：定义仍在 planner.rs，
    // 但不在 Global Agent 生产可达集合内（上面已证零引用）。
    let planner = src("src/ai/planner.rs");
    assert!(planner.contains("pub fn compile_production_plan("), "Dedicated Planner 允许存在（legacy）");
}

// ==================== 辅助 · 工具面 JSON 完整性（供报告核对） ====================

#[test]
fn arch_aux_tool_surface_names_dump() {
    let defs = app_lib::ai::agent_tools::agent_tool_definitions(true);
    let arr = defs.as_array().unwrap();
    let names: Vec<String> = arr
        .iter()
        .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
        .map(String::from)
        .collect();
    // 写入唯一性（同 TC03 ①，web_enabled=true 变体）
    assert!(names.iter().filter(|n| *n == "execute_higher_actions").count() == 1);
    let _v: J = serde_json::to_value(&names).unwrap();
}
