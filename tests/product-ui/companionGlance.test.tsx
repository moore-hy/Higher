import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CompanionExpedition,
  CompanionState,
  LearningStateSnapshot,
  NextLearningAction,
  StudySession,
} from "../../src/types";

/**
 * HIGHER 1.0 §M6 —— HOME / TODAY PRODUCT INTEGRATION（Companion / World glance）。
 *
 * 分层（与 todayGuidance.test.tsx 同一纪律）：
 * - **后端语义**（状态机 / 就绪度 / 确定性返回 / 0 Cloud）由 Rust 真实集成测试
 *   `companion_skill.rs` / `companion_world.rs` 负责，本文件不重复；
 * - 本文件只验收**前端契约**：Today 是否正确组合 Hermero hero、是否遵守
 *   §M6-A..F 的产品规则，以及是否**没有**把推荐/真相搬到前端。
 *
 * 覆盖：
 *   M6-A  顶层 hero 同时承载 Companion glance 与 Primary Next Action
 *   M6-B  优先级（未收取返回 > 进行中远征 > 新事件 > 当前状态）+ 一个 CTA
 *   M6-C  学习动作仍是 canonical（时间档仍回后端重取，前端不排序）
 *   M6-D  邀请非自动弹出 / 只在主动互动后出现 / 谢绝零证据 / 已开始学习不邀请
 *   M6-E  今日任务与今日活动仍在 hero 之下
 *   M6-F  无原始 JSON、无 XP 经济、companion 卡内不做第一屏第二个 btn--primary
 *   §M5-C 不出现能量 / 学习币 / 燃料钱包语言
 */

vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = {
    activeProfile: Object.freeze({ id: 1, name: "测试档案" }),
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

// 今日活动区与本套用例无关（内部含大量 API 依赖），保持渲染面收敛。
vi.mock("../../src/components/DailyActivitiesSection", () => ({
  default: () => null,
  durationShort: (s: number) => `${s}s`,
}));

vi.mock("../../src/components/ActiveSessionConflictModal", () => ({
  default: () => null,
  useActiveSessionConflict: () => ({ conflict: null, guard: () => false, close: () => {} }),
}));

vi.mock("../../src/api", () => ({
  materializeRecurringRolling: vi.fn(async () => 0),
  getLearningState: vi.fn(),
  getNextLearningAction: vi.fn(),
  /** COGNITIVE CORE V1.2 §19：本套用例只验证 companion 契约 → 认知视图返回「尚无快照」 */
  getTodayCoachSnapshot: vi.fn(async () => null),
  getCompanionState: vi.fn(),
  interactCompanion: vi.fn(),
  startCompanionExpedition: vi.fn(),
  settleCompanionExpeditions: vi.fn(),
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
// fixtures
// ============================================================

const TASK = {
  id: 11,
  title: "学习极限定义",
  status: "planned",
  planned_time: "09:00",
  estimated_minutes: 25,
  task_kind: "structured",
  priority: "core",
  goal_id: null,
  learning_item_id: null,
  knowledge_name: null,
  deep_link: "",
};

const SESSION: StudySession = {
  id: 77,
  profile_id: 1,
  goal_id: null,
  task_id: null,
  learning_item_id: null,
  title: "快速学习",
  started_at: "2026-09-15 01:00:00",
  ended_at: null,
  duration_seconds: null,
  status: "active",
  note: null,
  note_document_json: null,
  created_at: "2026-09-15 01:00:00",
  updated_at: "2026-09-15 01:00:00",
  time_corrected: 0,
  activity_kind: "unplanned",
  duration_review_state: "normal",
};

function snapshot(over: Partial<LearningStateSnapshot> = {}): LearningStateSnapshot {
  const tasks = over.today_tasks ?? [TASK];
  return {
    profile_id: 1,
    generated_at: "2026-09-15T01:00:00Z",
    local_date: "2026-09-15",
    profile: { profile_id: 1, name: "测试档案", has_confirmed_personalization: false },
    today: {
      date: "2026-09-15",
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
      evidence_generated_at: "2026-09-15T01:00:00Z",
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
      updated_at: "2026-09-15T00:00:00Z",
    },
    ...over,
  };
}

function action(over: Partial<NextLearningAction> = {}): NextLearningAction {
  return {
    profile_id: 1,
    local_date: "2026-09-15",
    action_type: "planned_task",
    reason_code: "today_task_core_priority",
    source_entity: { kind: "task", task_id: 11 },
    estimated_minutes: 25,
    source_task_estimate_minutes: 25,
    available_minutes: null,
    execution_payload: {
      kind: "start_task",
      task_id: 11,
      learning_item_id: null,
      session_id: null,
      review_id: null,
      entry_slice: false,
      suggested_minutes: 25,
    },
    title: "学习极限定义",
    subtitle: "预计 25 分钟",
    reasons: ["今天计划中优先级最高（核心）。"],
    is_primary: true,
    micro_action_only: false,
    micro_action: null,
    alternates: [],
    ...over,
  };
}

/** 与 Rust `CompanionState` 字段严格一致的夹具。 */
function companionState(over: Partial<CompanionState> = {}): CompanionState {
  const base: CompanionState = {
    profile_id: 1,
    profile: {
      id: 1,
      profile_id: 1,
      companion_id: "haven-companion",
      archetype: "sprout-guide",
      nickname: null,
      personality_seed: 11,
      created_at: "2026-09-15 01:00:00",
      updated_at: "2026-09-15 01:00:00",
    },
    world: {
      id: 1,
      profile_id: 1,
      expedition_readiness: "NOT_READY",
      readiness_updated_at: null,
      current_scene: "home",
      current_behavior: "idle",
      last_interaction_at: null,
      last_nudge_at: null,
      updated_at: "2026-09-15 01:00:00",
    },
    behavior: "idle",
    readiness: "NOT_READY",
    available_durations: [],
    open_expedition: null,
    ready_expedition: null,
    memory_count: 0,
    dialogue: { event: "first_visit_today", variant: 0, text: "今天也一起吧。" },
    nudge_available: true,
  };
  return { ...base, ...over };
}

function expedition(over: Partial<CompanionExpedition> = {}): CompanionExpedition {
  return {
    id: 501,
    profile_id: 1,
    status: "running",
    started_at: "2026-09-15 01:00:00",
    duration_seconds: 1200,
    // 「现在」+ 18 分钟（夹具时间必须由测试注入，绝不在产品里算）
    finished_at: new Date(Date.now() + 18 * 60 * 1000).toISOString().replace("T", " ").slice(0, 19),
    readiness_tier_at_start: "READY_SHORT",
    seed: 4242,
    theme: "General",
    collected_at: null,
    ...over,
  };
}

const NUDGE = {
  text: "都来了，要不要顺手做一个很小的？",
  action_type: "planned_task",
  reason_code: "today_task_core_priority",
  title: "学习极限定义",
  estimated_minutes: 25,
  suggested_minutes: 25,
};

function mockLearning(
  snap: LearningStateSnapshot = snapshot(),
  act: NextLearningAction = action()
) {
  vi.mocked(api.getLearningState).mockResolvedValue(snap as never);
  vi.mocked(api.getNextLearningAction).mockResolvedValue(act as never);
}

function mockCompanion(state: CompanionState = companionState()) {
  vi.mocked(api.getCompanionState).mockResolvedValue(state as never);
  vi.mocked(api.interactCompanion).mockResolvedValue(state as never);
  vi.mocked(api.startCompanionExpedition).mockResolvedValue(state as never);
}

function renderToday() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<Today />} />
          <Route path="/learn/:id" element={<div data-testid="learn-page">学习工作区</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

async function waitLoaded() {
  await waitFor(() => expect(screen.queryByText("加载中…")).not.toBeInTheDocument());
}

function glance(): HTMLElement {
  return screen.getByLabelText("伙伴");
}

beforeEach(() => {
  vi.mocked(api.getCompanionLearningNudge).mockResolvedValue(null);
  vi.mocked(api.collectCompanionReturn).mockReset();
  vi.mocked(api.settleCompanionExpeditions).mockResolvedValue(0 as never);
});

// ============================================================
// §M6-A — 顶层 hero 组合
// ============================================================

describe("§M6-A — 顶层 hero：一个连贯区域同时承载两条动机", () => {
  it("M6A-01：Companion glance 与 Primary Next Action 同处 .today-hero，且各自完整", async () => {
    mockLearning();
    mockCompanion();
    renderToday();
    await waitLoaded();

    const hero = document.querySelector(".today-hero") as HTMLElement;
    expect(hero).not.toBeNull();
    // 两条动机都在同一个 hero 区域内（不埋没任何一条）
    expect(hero.contains(glance())).toBe(true);
    expect(hero.contains(screen.getAllByLabelText("从这里开始")[0])).toBe(true);
    // 学习侧仍是完整的「现在做什么 / 大约多久 / 为什么 / 开始 / 3-10-25」
    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByText("学习极限定义")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "开始" })).toBeInTheDocument();
    expect(within(card).getByRole("group", { name: "时间预算" })).toBeInTheDocument();
  });

  it("M6A-02：hero 位于 Secondary Actions / 今日任务 / 今日活动之前（§M6-E：任务保持次级）", async () => {
    mockLearning();
    mockCompanion();
    renderToday();
    await waitLoaded();

    const hero = document.querySelector(".today-hero") as HTMLElement;
    const secondary = document.querySelector(".today__secondary") as HTMLElement;
    const tasksHeading = screen.getByRole("heading", { name: "今日任务" });
    const activitiesHeading = screen.getByRole("heading", { name: "今日活动" });

    const before = (a: Element, b: Element) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

    expect(before(hero, secondary)).toBe(true);
    expect(before(secondary, tasksHeading)).toBe(true);
    expect(before(tasksHeading, activitiesHeading)).toBe(true);
  });

  it("M6A-03：Companion 读取失败时静默让位，学习启动面完全不受影响（不得阻塞核心产品）", async () => {
    mockLearning();
    vi.mocked(api.getCompanionState).mockRejectedValue(new Error("companion ipc failed"));

    renderToday();
    await waitLoaded();

    expect(screen.queryByLabelText("伙伴")).not.toBeInTheDocument();
    // 学习面照常可用
    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByRole("button", { name: "开始" })).toBeInTheDocument();
  });
});

// ============================================================
// §M6-B — Companion glance 优先级 + 一个 CTA
// ============================================================

describe("§M6-B — Companion glance 优先级（未收取返回 > 进行中远征 > 新事件 > 当前状态）", () => {
  it("M6B-01：有未收取返回 → 显示「回来了 · 查看」，点击走 collect_companion_return", async () => {
    mockLearning();
    const ready = expedition({ status: "ready", collected_at: null });
    mockCompanion(companionState({ behavior: "returning", ready_expedition: ready }));
    vi.mocked(api.collectCompanionReturn).mockResolvedValue({
      expedition: { ...ready, status: "collected", collected_at: "2026-09-15 05:00:00" },
      memory: {
        id: 9,
        profile_id: 1,
        kind: "expedition_return",
        title: "一颗被水磨圆的小石头",
        body: "它没走远，就在屋子边上转了一圈。",
        source_type: "expedition",
        source_id: 501,
        created_at: "2026-09-15 05:00:00",
      },
      dialogue: { event: "expedition_return", variant: 1, text: "我把这个带回来了。" },
      nudge: null,
    } as never);

    renderToday();
    await waitLoaded();

    expect(within(glance()).getByTestId("companion-line")).toHaveTextContent("回来了");
    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "回来了 · 查看" }));

    await waitFor(() => expect(api.collectCompanionReturn).toHaveBeenCalledWith(1, 501));
    // §M5-E：结果是内联面板（不是 Modal），且只呈现本地化文本
    const panel = await within(glance()).findByTestId("companion-return");
    expect(within(panel).getByText("一颗被水磨圆的小石头")).toBeInTheDocument();
    expect(within(panel).getByText("它没走远，就在屋子边上转了一圈。")).toBeInTheDocument();
  });

  it("M6B-02：有进行中的远征 → 只陈述剩余时间，不出现「回来了 · 查看」", async () => {
    mockLearning();
    const open = expedition();
    mockCompanion(
      companionState({
        behavior: "expedition",
        open_expedition: open,
        world: { ...companionState().world, current_scene: "wilds", current_behavior: "expedition" },
      })
    );

    renderToday();
    await waitLoaded();

    expect(within(glance()).getByTestId("companion-line")).toHaveTextContent(/远行中/);
    expect(within(glance()).getByTestId("companion-line")).toHaveTextContent(/还有约/);
    expect(within(glance()).queryByRole("button", { name: "回来了 · 查看" })).not.toBeInTheDocument();
    // 「看看回来了吗」= 显式检查（无后台 tick，§M5-B）
    expect(within(glance()).getByRole("button", { name: "看看回来了吗" })).toBeInTheDocument();
  });

  it("M6B-03：无远征时回到当前状态，且同一时刻只有一个 companion CTA", async () => {
    mockLearning();
    mockCompanion(
      companionState({ behavior: "resting", readiness: "NOT_READY" })
    );
    renderToday();
    await waitLoaded();

    expect(within(glance()).getByTestId("companion-line")).toHaveTextContent("安静地待着");
    const ctas = glance().querySelectorAll(".companion-glance__cta");
    expect(ctas).toHaveLength(1);
    expect(glance().querySelectorAll(".btn--primary")).toHaveLength(0);
  });

  it("M6B-04：出门档位完全来自后端 available_durations（前端不自造档位）", async () => {
    mockLearning();
    mockCompanion(
      companionState({ readiness: "READY_LONG", available_durations: [1200, 3600, 10800] })
    );
    renderToday();
    await waitLoaded();

    const trip = within(glance()).getByRole("group", { name: "出门" });
    const buttons = within(trip).getAllByRole("button");
    expect(buttons.map((b) => b.textContent)).toEqual(["20 分钟", "1 小时", "3 小时"]);

    const user = userEvent.setup();
    await user.click(within(trip).getByRole("button", { name: "3 小时" }));
    await waitFor(() => expect(api.startCompanionExpedition).toHaveBeenCalledWith(1, 10800));
  });

  it("M6B-05：NOT_READY 时不出现任何出门档位（远征不可用）", async () => {
    mockLearning();
    mockCompanion(companionState({ readiness: "NOT_READY", available_durations: [] }));
    renderToday();
    await waitLoaded();

    expect(within(glance()).queryByRole("group", { name: "出门" })).not.toBeInTheDocument();
  });
});

// ============================================================
// §M6-C — 学习动作仍是 canonical
// ============================================================

describe("§M6-C — 学习动作仍是 canonical（前端不排序）", () => {
  it("M6C-01：时间档变化只重取后端 NextAction，且 Companion 与学习卡并存时依然如此", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious" }));
    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const card = screen.getAllByLabelText("从这里开始")[0];
    await user.click(
      within(within(card).getByRole("group", { name: "时间预算" })).getByRole("button", {
        name: "3 分钟",
      })
    );

    await waitFor(() => expect(api.getNextLearningAction).toHaveBeenCalledWith(1, "3m"));
    // 前端没有自己执行任何推荐/副作用
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
    expect(api.startCompanionExpedition).not.toHaveBeenCalled();
  });

  it("M6C-02：学习开始的唯一主按钮仍是学习卡的「开始」（companion 卡内没有 btn--primary）", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious" }));
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    const start = within(card).getByRole("button", { name: "开始" });
    expect(start.className).toContain("btn--primary");
    // 整个第一屏（hero + secondary nav）内 btn--primary 只出现一次
    const hero = document.querySelector(".today-hero") as HTMLElement;
    expect(hero.querySelectorAll(".btn--primary")).toHaveLength(1);
  });
});

// ============================================================
// §M6-D — 邀请：非自动弹出 / 主动互动后 / 谢绝零证据
// ============================================================

describe("§M6-D — 学习邀请非自动弹出，只在主动互动后出现", () => {
  it("M6D-01：只在挂载时**不**请求邀请（不是 auto-popup）", async () => {
    mockLearning();
    mockCompanion(companionState({ nudge_available: true }));
    renderToday();
    await waitLoaded();

    expect(api.getCompanionLearningNudge).not.toHaveBeenCalled();
    expect(screen.queryByTestId("companion-nudge")).not.toBeInTheDocument();
  });

  it("M6D-02：主动「打招呼」→ 互动 + 取出一条邀请；邀请来源逐字段是 canonical 动作", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious", nudge_available: true }));
    vi.mocked(api.getCompanionLearningNudge).mockResolvedValue(NUDGE as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));

    await waitFor(() => expect(api.interactCompanion).toHaveBeenCalledWith(1, "greet"));
    await waitFor(() => expect(api.getCompanionLearningNudge).toHaveBeenCalledTimes(1));

    const nudge = await screen.findByTestId("companion-nudge");
    expect(within(nudge).getByText(NUDGE.text)).toBeInTheDocument();
    // 标题就是 canonical NextAction 的 title（Companion 不自己排序学习任务）
    expect(within(nudge).getByText(/学习极限定义/)).toBeInTheDocument();
    // 接受按钮不是主按钮（第一屏唯一主入口仍是学习「开始」）
    expect(
      within(nudge).getByRole("button", { name: "好，做一点点" }).className
    ).not.toContain("btn--primary");
  });

  it("M6D-03：接受邀请 → 只把用户领回学习卡，不自动开 Session、不写任何学习证据", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious", nudge_available: true }));
    vi.mocked(api.getCompanionLearningNudge).mockResolvedValue(NUDGE as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));
    const nudge = await screen.findByTestId("companion-nudge");
    await user.click(within(nudge).getByRole("button", { name: "好，做一点点" }));

    // 邀请被收起，且**没有**任何 Session 被创建
    await waitFor(() =>
      expect(screen.queryByTestId("companion-nudge")).not.toBeInTheDocument()
    );
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
    expect(api.startQuickSession).not.toHaveBeenCalled();
    // 学习卡仍然在（注意力被交还给它）
    expect(screen.getAllByLabelText("从这里开始")).toHaveLength(1);
  });

  it("M6D-04：谢绝（今天先这样）→ 只写 companion 交互状态，零学习证据、零惩罚", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious", nudge_available: true }));
    vi.mocked(api.getCompanionLearningNudge).mockResolvedValue(NUDGE as never);
    // 后端在谢绝之后**仍然**报告 nudge_available=true（它只保证 180 分钟冷却，
    // 不保证「本次来访已谢绝」）—— 因此前端必须自己守住「同一来访不二次邀请」。
    vi.mocked(api.interactCompanion).mockResolvedValue(
      companionState({ behavior: "idle", nudge_available: true }) as never
    );

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));
    const nudge = await screen.findByTestId("companion-nudge");
    await user.click(within(nudge).getByRole("button", { name: "今天先这样" }));

    await waitFor(() => expect(api.interactCompanion).toHaveBeenCalledWith(1, "decline_nudge"));
    // 邀请立刻消失；同一来访不再二次邀请
    await waitFor(() =>
      expect(screen.queryByTestId("companion-nudge")).not.toBeInTheDocument()
    );
    // 只请求过一次邀请（谢绝后不得再取）
    expect(api.getCompanionLearningNudge).toHaveBeenCalledTimes(1);
    // 谢绝绝不产生任何学习副作用
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.endSession).not.toHaveBeenCalled();
  });

  it("M6D-05：已经开始学习 → 互动后不再给出学习邀请", async () => {
    mockLearning(snapshot({ active_session: SESSION }), action({ action_type: "active_session" }));
    mockCompanion(companionState({ behavior: "curious", nudge_available: true }));

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));

    await waitFor(() => expect(api.interactCompanion).toHaveBeenCalledWith(1, "greet"));
    expect(api.getCompanionLearningNudge).not.toHaveBeenCalled();
    expect(screen.queryByTestId("companion-nudge")).not.toBeInTheDocument();
  });

  it("M6D-06：伙伴自身不可用时（nudge_available=false）不请求邀请", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious", nudge_available: false }));

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));

    await waitFor(() => expect(api.interactCompanion).toHaveBeenCalledWith(1, "greet"));
    expect(api.getCompanionLearningNudge).not.toHaveBeenCalled();
  });
});

// ============================================================
// §M6-F / §M5-C — UI 质量与「没有钱包」
// ============================================================

describe("§M6-F / §M5-C — UI 质量（无原始 JSON / 无经济化语言）", () => {
  it("M6F-01：companion 卡不出现原始 JSON、不出现 XP / 能量 / 学习币 / 燃料语言", async () => {
    mockLearning();
    mockCompanion(
      companionState({
        behavior: "curious",
        readiness: "READY_MEDIUM",
        available_durations: [1200, 3600],
        memory_count: 3,
      })
    );
    vi.mocked(api.getCompanionLearningNudge).mockResolvedValue(NUDGE as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(glance()).getByRole("button", { name: "打招呼" }));
    await screen.findByTestId("companion-nudge");

    const text = glance().textContent ?? "";
    expect(text).not.toContain("{");
    expect(text).not.toMatch(/XP|经验值|能量|学习币|燃料|体力|等级|装备/);
    // 「已经带回来 N 段记忆」是事实陈述，不是可消费余额
    expect(text).toMatch(/已经带回来 3 段记忆/);
  });

  it("M6F-02：companion 卡内没有 placeholder debug 面板（无 debug/testid 泄漏标记）", async () => {
    mockLearning();
    mockCompanion(companionState({ behavior: "curious" }));
    renderToday();
    await waitLoaded();

    expect(glance().querySelectorAll("[data-debug]")).toHaveLength(0);
    expect(glance().textContent ?? "").not.toMatch(/DEBUG|TODO|FIXME|placeholder/i);
  });
});
