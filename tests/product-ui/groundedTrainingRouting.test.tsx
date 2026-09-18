import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type { LearningStateSnapshot, NextLearningAction, StudySession } from "../../src/types";

/**
 * GROUNDED LEARNING BRIDGE V1 · W6 —— 统一训练续接路由（§11.2）。
 *
 * ```text
 * GB-ROUTE-01 active TrainingRun resumes through /train
 * GB-ROUTE-02 free StudySession resumes through /learn
 * GB-ROUTE-03 returning to Today does not escape an active TrainingRun into legacy workspace
 * GB-ROUTE-04 no second StudySession is created by resume
 * ```
 *
 * # 分层
 *
 * 「这条会话到底由哪一条训练拥有」是**后端事实**，由 Rust
 * `src-tauri/tests/grounded_training_routing.rs` 用真实 DB 负责。
 * 本文件只验 **UI 是否按那一个事实正确路由** —— 即 §11.2 的锁定规则：
 *
 * ```text
 * active_training_run_id != null → /train/:trainingRunId
 * active_training_run_id == null → /learn/:sessionId
 * ```
 *
 * # 为什么要专门覆盖「快照失败」那一路
 *
 * 有 active session 时，Next Action 卡整体让位给 Active Study Bar（LEARN-TC002），
 * 因此 `continue_session` 载荷只在**状态快照取不到、但推荐取到了**这种部分失败下
 * 才会被点击。那正是 GB-ROUTE-03 说的场景：用户在 Today 上点「开始」，
 * 结果被丢进 legacy 工作区，正在进行的训练就此被架空。
 */

// ---- 引用稳定的上下文 mock（对象每次新建会引发无限重渲染）----
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

// 这些区块与本套用例无关（内部含大量 API 依赖），保持渲染面收敛。
vi.mock("../../src/components/DailyActivitiesSection", () => ({
  default: () => null,
  durationShort: (s: number) => `${s}s`,
}));

vi.mock("../../src/components/ActiveSessionConflictModal", () => ({
  default: () => null,
  useActiveSessionConflict: () => ({ conflict: null, guard: () => false, close: () => {} }),
}));

vi.mock("../../src/api", () => {
  const companionIdle = () => ({
    profile_id: 1,
    profile: {
      id: 1,
      profile_id: 1,
      companion_id: "haven-companion",
      archetype: "sprout-guide",
      nickname: null,
      personality_seed: 11,
      created_at: "2026-09-18 01:00:00",
      updated_at: "2026-09-18 01:00:00",
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
      updated_at: "2026-09-18 01:00:00",
    },
    behavior: "idle",
    readiness: "NOT_READY",
    available_durations: [],
    open_expedition: null,
    ready_expedition: null,
    memory_count: 0,
    dialogue: { event: "first_visit_today", variant: 0, text: "今天也一起吧。" },
    nudge_available: true,
  });

  return {
    materializeRecurringRolling: vi.fn(async () => 0),
    // ---- Today 只消费这两个入口 ----
    getLearningState: vi.fn(),
    getNextLearningAction: vi.fn(),
    getTodayCoachSnapshot: vi.fn(async () => null),
    getCompanionState: vi.fn(async () => companionIdle()),
    interactCompanion: vi.fn(async () => companionIdle()),
    startCompanionExpedition: vi.fn(async () => companionIdle()),
    settleCompanionExpeditions: vi.fn(async () => 0),
    collectCompanionReturn: vi.fn(),
    getCompanionMemories: vi.fn(async () => []),
    getCompanionLearningNudge: vi.fn(async () => null),
    // ---- 参考数据 ----
    listLearningItemsByProfile: vi.fn(async () => []),
    getGoalTree: vi.fn(async () => null),
    syncNotifications: vi.fn(async () => undefined),
    // ---- 续接**不得**触碰的写入口（GB-ROUTE-04 就在盯这几个）----
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
  };
});

import * as api from "../../src/api";
import Today from "../../src/pages/Today";

// ============================ fixtures ============================

const SESSION: StudySession = {
  id: 77,
  profile_id: 1,
  goal_id: null,
  task_id: null,
  learning_item_id: null,
  title: "光合作用",
  started_at: "2026-09-18 01:00:00",
  ended_at: null,
  duration_seconds: null,
  status: "active",
  note: null,
  note_document_json: null,
  created_at: "2026-09-18 01:00:00",
  updated_at: "2026-09-18 01:00:00",
  time_corrected: 0,
  activity_kind: "planned",
  duration_review_state: "normal",
};

const TRAINING_RUN_ID = 42;

/** 字段与 Rust `LearningStateSnapshot` 严格一致（含 W6 新字段）。 */
function snapshot(over: Partial<LearningStateSnapshot> = {}): LearningStateSnapshot {
  return {
    profile_id: 1,
    generated_at: "2026-09-18T01:00:00Z",
    local_date: "2026-09-18",
    profile: { profile_id: 1, name: "测试档案", has_confirmed_personalization: false },
    today: {
      date: "2026-09-18",
      planned_minutes: 0,
      actual_minutes: 0,
      planned_task_actual_minutes: 0,
      task_total: 0,
      task_completed: 0,
      task_completion_rate: null,
      unestimated_task_count: 0,
      needs_review_count: 0,
      learning_status: "unplanned",
      day_goal: null,
      day_goal_id: null,
    },
    today_tasks: [],
    today_activities: [],
    active_session: null,
    // W6 §11.1：默认「不由训练拥有」。
    active_training_run_id: null,
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
    review_state: { due: false, risk_state: "unknown", open_review_id: null, open_review_status: null },
    learning_evidence: {
      evidence_generated_at: "2026-09-18T01:00:00Z",
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
        open_task_today: 0,
        today_task_total: 0,
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
      updated_at: "2026-09-18T00:00:00Z",
    },
    ...over,
  };
}

/** 后端给出的「续接这条会话」动作（`continue_session`）。 */
function continueAction(trainingRunId: number | null): NextLearningAction {
  return {
    profile_id: 1,
    local_date: "2026-09-18",
    action_type: "active_session",
    reason_code: "active_session_in_progress",
    source_entity: { kind: "session", session_id: SESSION.id },
    estimated_minutes: null,
    source_task_estimate_minutes: null,
    available_minutes: null,
    execution_payload: {
      kind: "continue_session",
      task_id: null,
      learning_item_id: null,
      session_id: SESSION.id,
      // W6 §11.2：由后端给出的「回哪一条训练」，UI 不重算。
      training_run_id: trainingRunId,
      review_id: null,
      entry_slice: false,
      suggested_minutes: 0,
    },
    title: "光合作用",
    subtitle: "已有一条进行中的学习记录",
    reasons: ["当前已经有一条正在进行的学习记录。"],
    is_primary: true,
    micro_action_only: false,
    micro_action: null,
    alternates: [],
  };
}

// ============================ harness ============================

function renderToday() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<Today />} />
          <Route path="/learn/:id" element={<div data-testid="learn-page">学习工作区</div>} />
          <Route
            path="/train/:trainingRunId"
            element={<div data-testid="train-page">训练体验</div>}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function waitLoaded() {
  await waitFor(() => expect(screen.queryByText("加载中…")).not.toBeInTheDocument());
}

/** 当前落在哪一个页面（两个目标页各自有唯一 testid）。 */
function landedOn(): "train" | "learn" | null {
  if (screen.queryByTestId("train-page")) return "train";
  if (screen.queryByTestId("learn-page")) return "learn";
  return null;
}

// ============================ GB-ROUTE-01 ============================

describe("§11.2 续接路由 · Active Study Bar", () => {
  it("GB-ROUTE-01：会话由未终结的训练拥有 → 「继续」回 /train/:trainingRunId", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(
      snapshot({ active_session: SESSION, active_training_run_id: TRAINING_RUN_ID }) as never,
    );
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      continueAction(TRAINING_RUN_ID) as never,
    );

    renderToday();
    await waitLoaded();

    const bar = screen.getByLabelText("正在学习");
    const user = userEvent.setup();
    await user.click(within(bar).getByRole("button", { name: "继续" }));

    await waitFor(() => expect(landedOn()).toBe("train"));
    // 绝不能同时落到 legacy 工作区（两个都不出现才算真的只落一处）。
    expect(screen.queryByTestId("learn-page")).not.toBeInTheDocument();
  });

  // ============================ GB-ROUTE-02 ============================

  it("GB-ROUTE-02：自由学习（无 owning run）→ 「继续」回 /learn/:sessionId", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(
      snapshot({ active_session: SESSION, active_training_run_id: null }) as never,
    );
    vi.mocked(api.getNextLearningAction).mockResolvedValue(continueAction(null) as never);

    renderToday();
    await waitLoaded();

    const bar = screen.getByLabelText("正在学习");
    const user = userEvent.setup();
    await user.click(within(bar).getByRole("button", { name: "继续" }));

    await waitFor(() => expect(landedOn()).toBe("learn"));
    expect(screen.queryByTestId("train-page")).not.toBeInTheDocument();
  });

  // ============================ GB-ROUTE-04 ============================

  it("GB-ROUTE-04：「继续」只是导航，不创建任何新的 StudySession", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(
      snapshot({ active_session: SESSION, active_training_run_id: TRAINING_RUN_ID }) as never,
    );
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      continueAction(TRAINING_RUN_ID) as never,
    );

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(
      within(screen.getByLabelText("正在学习")).getByRole("button", { name: "继续" }),
    );
    await waitFor(() => expect(landedOn()).toBe("train"));

    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
  });
});

// ============================ GB-ROUTE-03 ============================

describe("§11.2 续接路由 · 状态快照部分失败时不得逃回 legacy", () => {
  it("GB-ROUTE-03：快照取不到但推荐指回训练 → 「开始」仍回 /train，不进 legacy 工作区", async () => {
    // 真实的**部分失败**：状态快照那次 IPC 挂了，但推荐取到了。
    vi.mocked(api.getLearningState).mockRejectedValue(new Error("LearningState 拉取失败 (IPC)"));
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      continueAction(TRAINING_RUN_ID) as never,
    );

    renderToday();
    await waitLoaded();

    // 失败必须可见（不静默），但推荐卡仍然给出唯一主入口。
    expect(await screen.findByRole("alert")).toHaveTextContent(/LearningState 拉取失败/);
    const card = screen.getAllByLabelText("从这里开始")[0];

    const user = userEvent.setup();
    await user.click(within(card).getByRole("button", { name: "开始" }));

    // 关键断言：进入正在进行的**训练**，而不是被丢进 legacy 工作区。
    await waitFor(() => expect(landedOn()).toBe("train"));
    expect(screen.queryByTestId("learn-page")).not.toBeInTheDocument();
  });

  it("GB-ROUTE-03b：同一路失败下若推荐说这是自由学习 → 才回 /learn", async () => {
    vi.mocked(api.getLearningState).mockRejectedValue(new Error("LearningState 拉取失败 (IPC)"));
    vi.mocked(api.getNextLearningAction).mockResolvedValue(continueAction(null) as never);

    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    const user = userEvent.setup();
    await user.click(within(card).getByRole("button", { name: "开始" }));

    await waitFor(() => expect(landedOn()).toBe("learn"));
    expect(screen.queryByTestId("train-page")).not.toBeInTheDocument();
  });
});
