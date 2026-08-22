//! DEV-0063 UI source-contract regression（§44-§47）。
//!
//! 只锁定 UI 契约（不证明真实点击——那属于 Human Click Runtime）：
//! U01-U03  Task Menu（visible = Edit + Delete；handler/modal 路径保留）
//! U04-U07  Proposal Truth（Update Diff 候选来自 keys(after_json)）
//! U08-U13  Runtime Invariants（handler 仍存在 / Proposal 真值来源）
//!
//! 纯 read_src 源码契约（TASK §44 允许）；无 Provider、无 DB、无网络。

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

// ==================== U01-U03 · Task Menu（§45） ====================

#[test]
fn u01_visible_menu_edit_delete_only() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    // ⋯ 菜单弹出块内只允许 编辑 / 删除 两个 button
    let pop = sec.split("taskmenu__pop").nth(1).unwrap_or_default();
    let pop = pop.split("</div>").next().unwrap_or_default();
    assert!(pop.contains("编辑"), "U01: 编辑存在");
    assert!(pop.contains("删除"), "U01: 删除存在");
    assert!(
        !pop.contains("调整日期") && !pop.contains("调整目标") && !pop.contains("调整知识") && !pop.contains("修改类型"),
        "U01: 旧四项快捷入口不得继续作为独立 visible menu item"
    );
    assert!(pop.contains("taskmenu__sep"), "U01: 分组分隔线");
}

#[test]
fn u02_edit_handler_modal_path_preserved() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    // 菜单编辑 → setEditing(t)（同一 TaskFormModal mode="edit"）
    assert!(
        sec.contains("setMenuFor(null); setEditing(t);"),
        "U02: 菜单编辑走 setEditing 原路径"
    );
    assert!(sec.contains("TaskFormModal"), "U02: 原编辑 Modal 组件仍被使用");
    assert!(sec.contains("mode=\"edit\""), "U02: edit 模式存在");
    // 编辑 Modal 内全部既有字段继续可编辑（§23）
    for field in ["任务名称", "日期", "时间", "预计分钟", "任务类型", "优先级", "目标（可选）", "关联知识"] {
        assert!(sec.contains(field), "U02: 编辑 Modal 字段存在：{field}");
    }
    assert!(sec.contains("updateTaskV2"), "U02: 原 update API 保留");
}

#[test]
fn u03_delete_handler_confirm_path_preserved() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    assert!(
        sec.contains("setMenuFor(null);") && sec.contains("setDeleting(t);"),
        "U03: 菜单删除走 setDeleting 原路径"
    );
    // 确认 Modal（禁止点击即删除）
    assert!(sec.contains("删除「"), "U03: 删除确认 Modal 存在");
    assert!(sec.contains("取消"), "U03: 取消按钮存在");
    assert!(sec.contains("handleDelete"), "U03: 原删除 handler 保留");
    assert!(sec.contains("deleteTask") && sec.contains("archiveTask"), "U03: 原 API 链保留");
}

// ==================== U04-U07 · Proposal Truth（§46/§32-§33） ====================

#[test]
fn u04_update_diff_candidates_from_after_keys() {
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    assert!(
        csr.contains("if (op.action === \"update\")"),
        "U04: update 分支存在"
    );
    assert!(
        csr.contains("Object.keys(after)"),
        "U04: 候选字段来自 keys(after_json)"
    );
    // update 分支不遍历 before keys（唯一锚点 function diffRows，避免 line83 统计处误命中）
    let upd = csr.split("function diffRows").nth(1).unwrap_or_default();
    let upd = upd.split("// create / delete").next().unwrap_or_default();
    assert!(!upd.contains("Object.keys(before)"), "U04: update 不用 before keys 做候选");
}

#[test]
fn u05_missing_after_key_not_rendered_delete() {
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    // update 分支只 push add/clear/mod，绝不 push del
    let upd = csr.split("function diffRows").nth(1).unwrap_or_default();
    let upd = upd.split("// create / delete").next().unwrap_or_default();
    assert!(!upd.contains("\"del\""), "U05: update 分支禁止渲染删除");
}

#[test]
fn u06_after_null_explicit_clear() {
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    assert!(
        csr.contains("if (newV === null)") && csr.contains("未设置"),
        "U06: after=null → 显式清空（before → 未设置）"
    );
}

#[test]
fn u07_same_value_hidden() {
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    let upd = csr.split("function diffRows").nth(1).unwrap_or_default();
    let upd = upd.split("// create / delete").next().unwrap_or_default();
    assert!(
        upd.contains("if (oldV === newV) continue"),
        "U07: after == before 不展示"
    );
}

// ==================== U08-U13 · Runtime Invariants（§47） ====================

#[test]
fn u08_proposal_trigger_from_changeset_event() {
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    assert!(panel.contains("ai://changeset"), "U08: Proposal 仍由真实 ai://changeset 驱动");
}

#[test]
fn u09_no_prose_inferred_proposal() {
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    assert!(
        !panel.contains("includes(\"提案\")") && !panel.contains("包含\"提案\""),
        "U09: 不得由 assistant prose 推断 Proposal"
    );
    assert!(
        !panel.contains("text.includes(\"修改方案\")"),
        "U09: 不得文本关键词推断 Proposal"
    );
}

#[test]
fn u10_send_handler_exists() {
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    assert!(panel.contains("aiStartRun"), "U10: Send 主路径 aiStartRun 保留");
}

#[test]
fn u11_task_start_handler_exists() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    assert!(sec.contains("startTaskSession"), "U11: Task Start 原 API 保留");
}

#[test]
fn u12_current_session_continue_end_handlers_exist() {
    let today = read_src("../src/pages/Today.tsx");
    assert!(today.contains("endSession"), "U12: 结束 handler（endSession）保留");
    assert!(today.contains("navigate(`/learn/${active.id}`)"), "U12: 继续 handler（navigate /learn）保留");
    assert!(today.contains("today-hero"), "U12: Hero 区域存在（§20）");
}

#[test]
fn u13_planning_month_navigation_handlers_exist() {
    let cal = read_src("../src/components/PlanningCalendar.tsx");
    assert!(cal.contains("shiftMonth(-1)") && cal.contains("shiftMonth(1)"), "U13: 上/下个月 handler 保留");
    assert!(cal.contains("goToday"), "U13: 今天 handler 保留");
    // §27：today/selected 视觉 state class 保留
    assert!(cal.contains("pcal__cell--today") || read_src("../src/styles.css").contains(".pcal__cell--today"));
}

// ==================== 附加 · Design System / 冻结面契约 ====================

#[test]
fn u14_design_system_tokens_applied() {
    let css = read_src("../src/styles.css");
    for token in [
        "--h-bg: #0b0d12",
        "--h-surface-1: #121620",
        "--h-accent: #6d7cff",
        "--h-radius-md: 12px",
        "--h-space-8: 32px",
    ] {
        assert!(css.contains(token), "U14: token 存在：{token}");
    }
    assert!(css.contains("Inter, \"Segoe UI\""), "U14: 字体栈");
}

#[test]
fn u15_zindex_contract() {
    let css = read_src("../src/styles.css");
    // §13 层级（Modal 900/910 > AI Panel 200 > Menu 100 > Sidebar 20）
    let modal_overlay = css.split(".modal-overlay {").nth(1).unwrap_or_default();
    assert!(modal_overlay.contains("z-index: 900"), "U15: modal-overlay=900");
    let menu = css.split(".taskmenu__pop {").nth(1).unwrap_or_default();
    assert!(menu.contains("z-index: 100"), "U15: menu=100");
    let toast = css.split(".toast {").nth(1).unwrap_or_default();
    assert!(toast.contains("z-index: 1000"), "U15: toast=1000");
}

#[test]
fn u16_no_new_important() {
    // 本轮禁止新增 !important：本轮涉及 selector 不得出现
    let css = read_src("../src/styles.css");
    for sel in [".today-hero", ".today-banner--quiet", ".taskmenu__sep", ".csr__diff-row--clear", ".layout__nav-group"] {
        let block = css.split(sel).nth(1).unwrap_or_default();
        let block = block.split('}').next().unwrap_or_default();
        assert!(!block.contains("!important"), "U16: {sel} 无 !important");
    }
}

#[test]
fn u17_frontend_business_freeze() {
    // api.ts / types.ts 未参与本轮 diff 由 Forbidden Diff Audit 保证；
    // 这里锁定 AI Panel 关键 runtime 调用未被重写
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    for contract in ["ai://delta", "ai://run-status", "createAiConversation", "setActiveAiProfiles"] {
        assert!(panel.contains(contract), "U17: AI Panel runtime 契约保留：{contract}");
    }
}

#[test]
fn u18_endsheet_return_today_navigates() {
    // DEV-0063 Human Runtime Repair：End Sheet「返回今日」必须 closeSheet + navigate("/")
    //（复用结束后视图原 navigate("/") 业务路径；只关 Sheet 停留在学习完成页 = 回归）
    let lw = read_src("../src/pages/LearningWorkspace.tsx");
    let sheet = lw.split("endsheet__title").nth(1).unwrap_or_default();
    let sheet = sheet.split("开始下一个").next().unwrap_or_default();
    assert!(sheet.contains("返回今日"), "U18: 按钮存在");
    assert!(
        sheet.contains("closeSheet();") && sheet.contains("navigate(\"/\");"),
        "U18: 返回今日 = closeSheet + navigate(\"/\")"
    );
    // 结束后视图 / 错误兜底的返回按钮仍绑定原 navigate("/")
    assert!(
        lw.matches("onClick={() => navigate(\"/\")}").count() >= 2,
        "U18: 结束后视图与错误兜底返回今日原 handler 保留"
    );
}
