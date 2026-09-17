import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type {
  CognitiveTodaySnapshot,
  DailyTaskRow,
  LearningStateSnapshot,
  NextLearningAction,
} from "../../src/types";

/**
 * COGNITIVE CORE V1.2 §24 / §36 —— Today 认知首屏。
 *
 * 覆盖任务书 §33 锁定的 UI 断言中属于 Today 的部分：
 *   UI-03 Today renders hero + orb + 3 signal cards + plan + rationale
 *   UI-04 no literal “87%” readiness
 *   UI-05 insufficient memory never fabricates “3 个知识点”
 *   UI-06 backend plan order is preserved; frontend does not reorder
 *   UI-07 direct-plan CTA preserves selected target
 *   UI-08 legacy task/activity functionality remains reachable below primary surface
 *
 * 分层：readiness / 记忆压力 / 排序 / 协议 / 块顺序的**真值**由 Rust
 * `today_coach_v1` + `cognitive_decision_v2` 负责；本文件只验证 Today 是否正确
 * **消费** §19 单一后端视图，以及是否守住 §36 的 NO-FAKE-DATA 底线。
 */

vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = {
    activeProfile: Object.freeze({ id: 1, name: "测试档案", profile_type: "kaoyan" }),
    gate: Object.freeze({ phase: "active" }),
    refreshKey: 0,
    enterProfile: () => Promise.resolve(),
    exitProfile: () => Promise.resolve(),
    refreshGate: () => Promise.resolve(),
    retryBoot: () => {},
  };
  return {
    useActiveProfile: () => ctx,
    ActiveProfileProvider: (props: { children?: unknown }) => props.children,
    canSwitchProfile: () => true,
  };
});

vi.mock("../../src/components/ai/AiPanelContext", () => {
  const panel = {
    runAction: () => Promise.resolve(),
    setPageContext: () => {},
    sendChat: () => Promise.resolve(),
  };
  return {
    useAiPanel: () => panel,
    AiPanelProvider: (props: { children?: unknown }) => props.children,
  };
});

vi.mock("../../src/components/FinalGoalCard", () => ({
  PLAN_REQUEST_MESSAGE: "请根据我的最终目标安排未来14天计划",
  default: () => null,
}));

vi.mock("../../src/components/DailyActivitiesSection", () => ({
  default: () => null,
}));

vi.mock("../../src/components/ActiveSessionConflictModal", () => ({
  default: () => null,
  useActiveSessionConflict: () => ({ conflict: null, guard: () => false, close: () => {} }),
}));

vi.mock("../../src/api", () => ({
  materializeRecurringRolling: vi.fn(async () => 0),
  getLearningState: vi.fn(),
  getNextLearningAction: vi.fn(),
  getTodayCoachSnapshot: vi.fn(),
  getCompanionState: vi.fn(async () => null),
  interactCompanion: vi.fn(),
  startCompanionExpedition: vi.fn(),
  settleCompanionExpeditions: vi.fn(async () => 0),
  collectCompanionReturn: vi.fn(),
  getCompanionMemories: vi.fn(async () => []),
  getCompanionLearningNudge: vi.fn(async () => null),
  listLearningItemsByProfile: vi.fn(async () => []),
  getGoalTree: vi.fn(async () => null),
  syncNotifications: vi.fn(async () => undefined),
  startQuickSession: vi.fn(),
  startSession: vi.fn(),
  startTaskSession: vi.fn(),
  endSession: vi.fn(),
  createTaskV2: vi.fn(),
  updateTaskV2: vi.fn(),
  completeTask: vi.fn(),
  uncompleteTask: vi.fn(),
  deleteTask: vi.fn(),
  archiveTask: vi.fn(),
}));

import * as api from "../../src/api";
import Today from "../../src/pages/Today";

// ============================================================
// fixtures —— 字段与 Rust DTO 严格一致
// ============================================================

const TASK: DailyTaskRow = {
  id: 11,
  title: "复习昨天的内容",
  status: "planned",
  planned_time: "09:00",
  estimated_minutes: 25,
  task_kind: "structured",
  priority: "core",
  goal_id: null,
  learning_item_id: 42,
  knowledge_name: null,
  deep_link: "",
};

function snapshot(over: Partial<LearningStateSnapshot> = {}): LearningStateSnapshot {
  const tasks = over.today_tasks ?? [TASK];
  return {
    profile_id: 1,
    generated_at: "2026-09-17T01:00:00Z",
    local_date: "2026-09-17",
    profile: { profile_id: 1, name: "测试档案", has_confirmed_personalization: false },
    today: {
      date: "2026-09-17",
      planned_minutes: 25,
      actual_minutes: 0,
      planned_task_actual_minutes: 0,
      task_total: tasks.length,
      task_completed: 0,
      task_completion_rate: 0,
      unestimated_task_count: 0,
      needs_review_count: 0,
      learning_status: "planned",
      day_goal: null,
      day_goal_id: null,
    },
    today_tasks: tasks,
    today_activities: [],
    active_session: null,
    recent_sessions: [],
    goal_state: {
      active_target_count: 0,
      primary_title: null,
      primary_scenario_type: null,
      primary_target_date: null,
      primary_target_id: null,
    },
    planning_state: {
      has_active_blueprint: false,
      blueprint_id: null,
      blueprint_title: null,
      review_interval_days: null,
      next_review_at: null,
      phase_count: 0,
      current_phase_title: null,
      milestone_count: 0,
      milestone_done_count: 0,
      planning_progress: null,
    },
    review_state: {
      due: false,
      risk_state: "unknown",
      open_review_id: null,
      open_review_status: null,
    },
    learning_evidence: {
      evidence_generated_at: "2026-09-17T01:00:00Z",
      quality: "insufficient",
      quality_reasons: [],
      pace_sample_count: 0,
      observed_study_minutes_30d: 0,
      stated_daily_minutes: null,
      observed_daily_minutes_14d: null,
      active_study_days_30d: 0,
      calibrated_ratio: 1,
    },
    recovery_state: {
      active: false,
      reason_codes: [],
      signals: {
        days_since_last_session: null,
        sessions_completed_7d: 0,
        has_learning_history: false,
        open_task_today: tasks.length,
        today_task_total: tasks.length,
        overdue_task_count_7d: 0,
        task_total_7d: 0,
        completion_rate_7d: null,
        planned_daily_minutes_14d: null,
        observed_daily_minutes_14d: null,
      },
      should_take_primary: false,
    },
    micro: {
      recent_micro_actions: [],
      recent_touched_sources: [],
      candidates: [],
      dedupe_window_minutes: 30,
    },
    friction: {
      level: "unknown",
      subject_learning_item_id: null,
      subject_label: null,
      signals: [],
      recommended_support_level: 0,
      cooldown_until: null,
    },
    contribution: {
      today_total: 0,
      today_cap: 40,
      sources: {
        micro_done: 0,
        micro_partial: 0,
        evaluation: 0,
        session: 0,
        task: 0,
        correction: 0,
        persistence: 0,
      },
      diminishing_factor: 1,
      updated_at: "2026-09-17T00:00:00Z",
    },
    ...over,
  };
}

function action(over: Partial<NextLearningAction> = {}): NextLearningAction {
  return {
    profile_id: 1,
    local_date: "2026-09-17",
    action_type: "review_due",
    reason_code: "review_due_open",
    source_entity: { kind: "learning_item", learning_item_id: 42 },
    estimated_minutes: 10,
    source_task_estimate_minutes: null,
    available_minutes: null,
    execution_payload: {
      kind: "start_item",
      task_id: null,
      learning_item_id: 42,
      session_id: null,
      review_id: null,
      entry_slice: false,
      suggested_minutes: 10,
    },
    title: "复习昨天的内容",
    subtitle: "预计 10 分钟",
    reasons: ["这项到了该复习的时间。"],
    is_primary: true,
    micro_action_only: false,
    micro_action: null,
    alternates: [],
    ...over,
  };
}

/** §19 单一后端视图夹具：三块计划（含休息伪块）+ 5 条理由。 */
function coach(over: Partial<CognitiveTodaySnapshot> = {}): CognitiveTodaySnapshot {
  return {
    profile_id: 1,
    generated_at: "2026-09-17T01:00:00Z",
    local_date: "2026-09-17",
    mode: "copilot",
    hero: {
      current_time_label: "01:00",
      headline: "today.hero.review_first",
      supporting_text: "有内容到了该复习的时间，先把它接上。",
      primary_cta_label: "先复习",
      secondary_cta_label: "看看计划",
    },
    readiness: {
      band: "moderate",
      confidence: "medium",
      reason_codes: ["new_content"],
      available: true,
    },
    memory: {
      status: "watch",
      total_units: 5,
      due_count: 2,
      high_risk_count: 0,
      oldest_due_at: "2026-09-16 01:00:00",
      available: true,
    },
    load: {
      band: "stable",
      observed_minutes_7d: 120,
      observed_minutes_30d: 540,
      evidence_quality: "medium",
      available: true,
    },
    plan: {
      target_learning_item_id: 42,
      total_minutes: 25,
      blocks: [
        {
          ordinal: 1,
          protocol_id: "cued_recall",
          minutes: 10,
          goal: "先回忆一遍昨天卡住的点",
          completion_rule: {
            kind: "at_least_one_recall_outcome",
            description_zh: "至少完成一次回忆",
          },
          is_break: false,
        },
        {
          ordinal: 2,
          protocol_id: null,
          minutes: 6,
          goal: "休息",
          completion_rule: {
            kind: "time_slice_or_user_stop",
            description_zh: "到时间或主动结束",
          },
          is_break: true,
        },
        {
          ordinal: 3,
          protocol_id: "mixed_practice",
          minutes: 9,
          goal: "混合练习巩固",
          completion_rule: {
            kind: "at_least_one_practice_outcome",
            description_zh: "至少完成一次练习",
          },
          is_break: false,
        },
      ],
      reason_codes: ["memory_due", "time_fit"],
      evidence_refs: [],
    },
    rationale: [
      {
        code: "target",
        label: "今天这一项",
        value: "learning_item:42",
        trend: "neutral",
        source_refs: [],
      },
      {
        code: "memory",
        label: "记忆节奏",
        value: "total=5 due=2 high_risk=0",
        trend: "neutral",
        source_refs: [],
      },
      {
        code: "load",
        label: "最近学习量",
        value: "7d=120 30d=540",
        trend: "positive",
        source_refs: [],
      },
      {
        code: "goal",
        label: "计划与目标",
        value: "memory_due",
        trend: "neutral",
        source_refs: [],
      },
      {
        code: "readiness",
        label: "当前状态",
        value: "moderate",
        trend: "positive",
        source_refs: [],
      },
    ],
    legacy_next_action: {
      action_type: "review_due",
      reason_code: "review_due_open",
      title: "复习昨天的内容",
      subtitle: "预计 10 分钟",
      estimated_minutes: 10,
      learning_item_id: 42,
    },
    ...over,
  };
}

function mockAll(c: CognitiveTodaySnapshot | null = coach()) {
  vi.mocked(api.getLearningState).mockResolvedValue(snapshot() as never);
  vi.mocked(api.getNextLearningAction).mockResolvedValue(action() as never);
  vi.mocked(api.getTodayCoachSnapshot).mockResolvedValue(c as never);
}

function renderToday() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<Today />} />
          <Route path="/learn/:id" element={<div data-testid="learn-page">学习工作区</div>} />
          <Route path="/knowledge" element={<div data-testid="knowledge-page">知识</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function domOrder(a: Element, b: Element): boolean {
  return Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
}

/**
 * 两个查询都落地后才断言。
 *
 * Hero / plan / rationale 的**容器**在首帧就存在（快照到达前渲染诚实空态），
 * 因此 `findByLabelText("今日概览")` 会在数据到达前就命中 —— 必须显式等待。
 */
async function waitTodayLoaded() {
  await waitFor(() =>
    expect(screen.queryByText("状态信息正在准备")).not.toBeInTheDocument()
  );
  await waitFor(() =>
    expect(screen.queryByText(/加载中/)).not.toBeInTheDocument()
  );
}

// ============================================================
// UI-03
// ============================================================

describe("UI-03 — Today renders hero + orb + 3 signal cards + plan + rationale", () => {
  it("认知首屏五个组成部分同时存在", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();

    // hero
    const hero = await screen.findByLabelText("今日概览");
    expect(within(hero).getByText("先接上该复习的内容")).toBeInTheDocument();

    // orb（纯 DOM/CSS/SVG，无 click 要求）
    expect(screen.getByText("理解你 · 陪伴你")).toBeInTheDocument();
    expect(document.querySelector(".hc-orb__svg")).not.toBeNull();

    // 三张 signal 卡
    expect(screen.getByLabelText("当前状态")).toBeInTheDocument();
    expect(screen.getByLabelText("记忆节奏")).toBeInTheDocument();
    expect(screen.getByLabelText("最近学习量")).toBeInTheDocument();

    // plan + rationale
    expect(screen.getByLabelText("今天的学习安排")).toBeInTheDocument();
    expect(screen.getByLabelText("为什么这样安排")).toBeInTheDocument();
  });

  it("hero 主 CTA 文案按 §24 锁定；后端建议动作不被丢弃", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    const hero = await screen.findByLabelText("今日概览");
    expect(
      within(hero).getByRole("button", { name: "按我的状态安排 →" })
    ).toBeInTheDocument();
    expect(within(hero).getByRole("button", { name: "我有自己的计划" })).toBeInTheDocument();
    // 后端 primary_cta_label 以「今天建议」一行呈现，而不是覆盖锁定文案
    expect(within(hero).getByText("先复习")).toBeInTheDocument();
  });

  it("认知区整体位于 legacy 详情之上（§24 结构顺序）", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");
    const cognitive = document.querySelector(".hc-today") as HTMLElement;
    const legacy = document.querySelector(".hc-legacy") as HTMLElement;
    expect(cognitive).not.toBeNull();
    expect(legacy).not.toBeNull();
    expect(domOrder(cognitive, legacy)).toBe(true);
  });
});

// ============================================================
// UI-04
// ============================================================

describe("UI-04 — no literal “87%” readiness", () => {
  it("整页不出现任何百分比；readiness 只用「置信度：中」这类类别标签", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");

    expect(document.body.textContent ?? "").not.toMatch(/87\s*%/);
    // 更严格：认知区里根本不允许出现百分号
    const cognitive = document.querySelector(".hc-today") as HTMLElement;
    expect(cognitive.textContent ?? "").not.toMatch(/%/);

    const readiness = screen.getByLabelText("当前状态");
    expect(within(readiness).getByText("适合中等强度学习")).toBeInTheDocument();
    expect(within(readiness).getByText("置信度：中")).toBeInTheDocument();
  });

  it("证据不足时 readines 不给置信度，只给「状态信息还不够 / 先按常规节奏安排」", async () => {
    mockAll(
      coach({
        readiness: {
          band: "insufficient",
          confidence: "low",
          reason_codes: ["insufficient_evidence"],
          available: false,
        },
      })
    );
    renderToday();
    await waitTodayLoaded();
    const readiness = await screen.findByLabelText("当前状态");
    expect(within(readiness).getByText("状态信息还不够")).toBeInTheDocument();
    expect(within(readiness).getByText("先按常规节奏安排")).toBeInTheDocument();
    expect(within(readiness).queryByText(/置信度/)).not.toBeInTheDocument();
  });

  it("load 卡只有后端判定 elevated 时才允许说「高于稳定区间」", async () => {
    mockAll(
      coach({
        load: {
          band: "low",
          observed_minutes_7d: 30,
          observed_minutes_30d: 240,
          evidence_quality: "low",
          available: true,
        },
      })
    );
    const { unmount } = renderToday();
    const low = await screen.findByLabelText("最近学习量");
    // 使用真实观测分钟（30 → 30m），且**不**出现 elevated 的措辞
    expect(within(low).getByText(/近 7 天已学习 30m/)).toBeInTheDocument();
    expect(within(low).queryByText("高于稳定区间")).not.toBeInTheDocument();
    unmount();
  });
});

// ============================================================
// UI-05
// ============================================================

describe("UI-05 — insufficient memory never fabricates “3 个知识点”", () => {
  it("available=false 时只给「记忆节奏正在建立」，且不出现任何计数", async () => {
    mockAll(
      coach({
        memory: {
          status: "insufficient",
          total_units: 0,
          due_count: 0,
          high_risk_count: 0,
          oldest_due_at: null,
          available: false,
        },
        // 后端在无单元时给出的 rationale.value 为 null
        rationale: [
          {
            code: "memory",
            label: "记忆节奏",
            value: null,
            trend: "positive",
            source_refs: [],
          },
        ],
      })
    );
    renderToday();
    await waitTodayLoaded();

    const memory = await screen.findByLabelText("记忆节奏");
    expect(within(memory).getByText("记忆节奏正在建立")).toBeInTheDocument();
    expect(
      within(memory).getByText("完成几次回忆后会出现风险提示")
    ).toBeInTheDocument();
    // 不得编造「3 个知识点」或任何数字
    expect(within(memory).queryByText(/个知识点/)).not.toBeInTheDocument();
    expect(document.body.textContent ?? "").not.toMatch(/3\s*个知识点/);
  });

  it("有真实数据时才显示数字，且数字全部来自后端", async () => {
    mockAll(
      coach({
        memory: {
          status: "high",
          total_units: 5,
          due_count: 4,
          high_risk_count: 3,
          oldest_due_at: "2026-09-10 01:00:00",
          available: true,
        },
      })
    );
    renderToday();
    await waitTodayLoaded();
    const memory = await screen.findByLabelText("记忆节奏");
    expect(within(memory).getByText("3 个关键知识点")).toBeInTheDocument();
    expect(within(memory).getByText("进入高遗忘风险")).toBeInTheDocument();
    expect(within(memory).getByText("共 5 个知识点")).toBeInTheDocument();
  });

  it("load 卡在 observed_minutes 为 null 时不显示任何分钟数（null ≠ 0 分钟）", async () => {
    mockAll(
      coach({
        load: {
          band: "insufficient",
          observed_minutes_7d: null,
          observed_minutes_30d: null,
          evidence_quality: "insufficient",
          available: false,
        },
      })
    );
    renderToday();
    await waitTodayLoaded();
    const load = await screen.findByLabelText("最近学习量");
    expect(within(load).getByText("还没有足够的学习记录")).toBeInTheDocument();
    expect(within(load).queryByText(/已学习/)).not.toBeInTheDocument();
  });
});

// ============================================================
// UI-06
// ============================================================

describe("UI-06 — backend plan order is preserved; frontend does not reorder", () => {
  it("块的渲染顺序、序号、时长与后端 plan.blocks 完全一致（含休息伪块）", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    const strip = await screen.findByLabelText("今天的学习安排");

    const blocks = Array.from(strip.querySelectorAll(".hc-plan__block"));
    expect(blocks).toHaveLength(3);

    const goals = blocks.map(
      (b) => b.querySelector(".hc-plan__goal")?.textContent ?? ""
    );
    expect(goals).toEqual(["先回忆一遍昨天卡住的点", "休息", "混合练习巩固"]);

    const ordinals = blocks.map(
      (b) => b.querySelector(".hc-plan__ordinal")?.textContent ?? ""
    );
    expect(ordinals).toEqual(["1", "2", "3"]);

    const minutes = blocks.map(
      (b) => b.querySelector(".hc-plan__minutes")?.textContent ?? ""
    );
    expect(minutes).toEqual(["10 分钟", "6 分钟", "9 分钟"]);

    // 总时长直接用后端 total_minutes，不自行累加
    expect(within(strip).getByText("共 25 分钟")).toBeInTheDocument();
  });

  it("休息块被明确标注「不产生掌握证据」", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    const brk = await screen.findByText("休息");
    const block = brk.closest(".hc-plan__block") as HTMLElement;
    expect(block.className).toContain("hc-plan__block--break");
    expect(
      within(block).getByText("休息 · 不产生掌握证据")
    ).toBeInTheDocument();
  });

  it("计划为 null 时给诚实空态，不编造任何安排", async () => {
    mockAll(coach({ plan: null }));
    renderToday();
    await waitTodayLoaded();
    const strip = await screen.findByLabelText("今天的学习安排");
    expect(within(strip).getByText("还没有可执行的安排")).toBeInTheDocument();
    expect(strip.querySelectorAll(".hc-plan__block")).toHaveLength(0);
  });

  it("rationale 按后端顺序渲染且不重排；机器 token 不泄漏给用户", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    const row = await screen.findByLabelText("为什么这样安排");
    const labels = Array.from(row.querySelectorAll(".hc-rationale__label")).map(
      (e) => e.textContent
    );
    expect(labels).toEqual(["今天这一项", "记忆节奏", "最近学习量", "计划与目标", "当前状态"]);
    // 机器 token 一律不得出现在界面上
    const text = row.textContent ?? "";
    expect(text).not.toMatch(/learning_item:/);
    expect(text).not.toMatch(/total=|due=|high_risk=/);
    expect(text).not.toMatch(/7d=|30d=/);
    expect(text).not.toMatch(/memory_due|time_fit/);
  });
});

// ============================================================
// UI-07
// ============================================================

describe("UI-07 — direct-plan CTA preserves selected target", () => {
  it("「我有自己的计划」→ 快速学习：走用户自己的选择，绝不替换成后端计划目标", async () => {
    mockAll();
    vi.mocked(api.startQuickSession).mockResolvedValue({ id: 901 } as never);
    renderToday();
    await waitTodayLoaded();

    const hero = await screen.findByLabelText("今日概览");
    const user = userEvent.setup();
    await user.click(within(hero).getByRole("button", { name: "我有自己的计划" }));

    const choice = screen.getByRole("group", { name: "选择你自己的起点" });
    await user.click(within(choice).getByRole("button", { name: "快速学习" }));

    expect(api.startQuickSession).toHaveBeenCalledWith(1);
    // DIRECT：不得用计划里的 learning_item 42 覆盖用户的选择
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
  });

  it("「我有自己的计划」→ 从任务开始：只展开 legacy 任务区，不创建任何 Session", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");

    const details = document.querySelector(".hc-legacy") as HTMLDetailsElement;
    expect(details.open).toBe(false);

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "我有自己的计划" }));
    await user.click(
      within(screen.getByRole("group", { name: "选择你自己的起点" })).getByRole("button", {
        name: "从任务开始",
      })
    );

    expect(details.open).toBe(true);
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
  });

  it("「我有自己的计划」→ 从知识项开始：进入既有 Knowledge 路由", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "我有自己的计划" }));
    await user.click(
      within(screen.getByRole("group", { name: "选择你自己的起点" })).getByRole("button", {
        name: "从知识项开始",
      })
    );

    expect(await screen.findByTestId("knowledge-page")).toBeInTheDocument();
    expect(api.startSession).not.toHaveBeenCalled();
  });

  it("主 CTA 有 learning_item 锚点 → 走既有 startSession（含冲突守卫），不发明新类型", async () => {
    mockAll();
    vi.mocked(api.startSession).mockResolvedValue({ id: 902 } as never);
    renderToday();
    await waitTodayLoaded();

    const hero = await screen.findByLabelText("今日概览");
    const user = userEvent.setup();
    await user.click(within(hero).getByRole("button", { name: "按我的状态安排 →" }));

    expect(api.startSession).toHaveBeenCalledWith(42);
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("主 CTA 无锚点 → 不创建任何 Session（只把注意力交给计划条）", async () => {
    mockAll(
      coach({
        plan: {
          target_learning_item_id: null,
          total_minutes: 12,
          blocks: [
            {
              ordinal: 1,
              protocol_id: "recovery_light",
              minutes: 12,
              goal: "先做一段轻量内容",
              completion_rule: {
                kind: "time_slice_or_user_stop",
                description_zh: "到时间或主动结束",
              },
              is_break: false,
            },
          ],
          reason_codes: ["recovery_needed"],
          evidence_refs: [],
        },
      })
    );
    renderToday();
    await waitTodayLoaded();

    const hero = await screen.findByLabelText("今日概览");
    const user = userEvent.setup();
    await user.click(within(hero).getByRole("button", { name: "按我的状态安排 →" }));

    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
    // 首个可执行块被暴露出来（§24）
    expect(document.getElementById("hc-plan-first-block")).not.toBeNull();
  });
});

// ============================================================
// UI-08
// ============================================================

describe("UI-08 — legacy task/activity functionality remains reachable below primary surface", () => {
  it("legacy 详情默认收起，但任务/活动仍在 DOM 中可达", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");

    const details = document.querySelector(".hc-legacy") as HTMLDetailsElement;
    expect(details).not.toBeNull();
    expect(details.open).toBe(false);

    // 既有能力一个都没被删除（§37）
    expect(within(details).getByRole("heading", { name: "今日任务" })).toBeInTheDocument();
    expect(within(details).getByRole("heading", { name: "今日活动" })).toBeInTheDocument();
    expect(details.querySelector(".today__tasklist")).not.toBeNull();
    expect(within(details).getByRole("button", { name: "快速学习" })).toBeInTheDocument();
    expect(within(details).getByLabelText("从这里开始")).toBeInTheDocument();
  });

  it("计划块与 legacy 任务区互不干扰：认知区在前，任务区在后", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今天的学习安排");
    const strip = screen.getByLabelText("今天的学习安排");
    const taskList = document.querySelector(".today__tasklist") as HTMLElement;
    expect(domOrder(strip, taskList)).toBe(true);
  });

  it("UI-03/UI-08 共存：唯一 Primary Next Action 仍只有一张（不因新增认知区而重复）", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");
    expect(screen.getAllByLabelText("从这里开始")).toHaveLength(1);
  });
});

// ============================================================
// §36 兜底：整页无假数据痕迹
// ============================================================

describe("§36 — NO-FAKE-DATA 兜底扫描", () => {
  it("概念稿里的示例数字一个都不允许出现", async () => {
    mockAll();
    renderToday();
    await waitTodayLoaded();
    await screen.findByLabelText("今日概览");
    const text = document.body.textContent ?? "";
    for (const forbidden of ["87%", "5.8 小时", "+42%", "12.6 小时", "6 天"]) {
      expect(text).not.toContain(forbidden);
    }
  });
});
