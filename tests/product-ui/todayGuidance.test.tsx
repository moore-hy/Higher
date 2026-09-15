import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type {
  DailyTaskRow,
  LearningStateSnapshot,
  NextLearningAction,
  StudySession,
} from "../../src/types";

/**
 * HIGHER CLOSED LOOP V1 §PHASE 4 / §PHASE 8 —— Today 单一学习引导面交互契约。
 *
 * 分层（任务书 §PHASE 9）：
 * - 推荐**逻辑**（类别优先级 / tie-break / Time Budget / Recovery）由 Rust
 *   `src-tauri/tests/closed_loop_core.rs` 的真实集成测试负责；
 * - 本文件只负责 **UI 验收**：Today 是否正确消费
 *   `LearningStateSnapshot` + `NextLearningAction`，以及交互契约是否保持。
 *
 * 覆盖：
 *   LEARN-TC001  no active -> at most ONE Next Action 卡
 *   LEARN-TC002  active session -> Start Here 让位给 Active Study Bar
 *   LEARN-TC003  manual Task Start works regardless of recommendation
 *   LEARN-TC004  「换一个」只在 alternates 内切换，不改任何正式数据
 *   CONTINUE-TC001 旧合约（continue_last 徽标）
 *   §PHASE 3    时间预算四档可切换，且 30 秒档不出现「开始学习」按钮
 *   §22.1/§22.2/§22.3/§22.4 Today 结构收口
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
  // ---- CLOSED LOOP V1：Today 只消费这两个入口 ----
  getLearningState: vi.fn(),
  getNextLearningAction: vi.fn(),
  // ---- 参考数据 ----
  listLearningItemsByProfile: vi.fn(async () => []),
  getGoalTree: vi.fn(async () => null),
  syncNotifications: vi.fn(async () => undefined),
  startQuickSession: vi.fn(),
  startSession: vi.fn(),
  startTaskSession: vi.fn(),
  endSession: vi.fn(),
  // DailyTasksSection 依赖
  createTaskV2: vi.fn(),
  updateTaskV2: vi.fn(),
  completeTask: vi.fn(),
  uncompleteTask: vi.fn(),
  deleteTask: vi.fn(),
  archiveTask: vi.fn(),
}));

import * as api from "../../src/api";
import Today from "../../src/pages/Today";

const TASK: DailyTaskRow = {
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

const TASK_B: DailyTaskRow = { ...TASK, id: 12, title: "背单词", priority: "normal" };

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

/** PHASE 1：LearningStateSnapshot 夹具（字段与 Rust 端严格一致）。 */
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
    review_state: { due: false, risk_state: "unknown", open_review_id: null, open_review_status: null },
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
    ...over,
  };
}

/** PHASE 2：NextLearningAction 夹具。 */
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
    reasons: ["今天计划中优先级最高（核心）。", "计划时间 09:00。"],
    is_primary: true,
    micro_action_only: false,
    alternates: [
      {
        action_type: "planned_task",
        reason_code: "today_task_in_plan",
        source_entity: { kind: "task", task_id: 12 },
        estimated_minutes: 25,
        execution_payload: {
          kind: "start_task",
          task_id: 12,
          learning_item_id: null,
          session_id: null,
          review_id: null,
          entry_slice: false,
          suggested_minutes: 25,
        },
        title: "背单词",
        subtitle: "预计 25 分钟",
        reasons: ["今天计划中的任务。"],
      },
    ],
    ...over,
  };
}

function mockClosedLoop(
  snap: LearningStateSnapshot = snapshot(),
  act: NextLearningAction = action()
) {
  vi.mocked(api.getLearningState).mockResolvedValue(snap as never);
  vi.mocked(api.getNextLearningAction).mockResolvedValue(act as never);
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

describe("LEARN-TC001 — 同一时刻最多一个 Next Action 主建议", () => {
  it("无 active：Next Action 卡只渲染一次，且「开始学习」是其中唯一主推荐按钮", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const cards = screen.getAllByLabelText("从这里开始");
    expect(cards).toHaveLength(1);
    // 页面正文里「从这里开始」标签也只出现一次
    expect(screen.getAllByText("从这里开始")).toHaveLength(1);

    const card = cards[0];
    expect(within(card).getByRole("button", { name: "开始学习" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "换一个" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "为什么？" })).toBeInTheDocument();
  });

  it("Next Action 默认不展开理由（禁止自动展开，§30B）", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).queryByRole("list")).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(within(card).getByRole("button", { name: "为什么？" }));
    expect(within(card).getByRole("list")).toBeInTheDocument();
  });

  it("Next Action 展示后端给出的标题与预计时长", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByText("学习极限定义")).toBeInTheDocument();
    expect(within(card).getByText("预计 25 分钟")).toBeInTheDocument();
  });
});

describe("PHASE 3 — 时间预算（四档 + 30 秒档不出开始按钮）", () => {
  it("§PHASE 0.3 HOTFIX-06：Today 默认无 30 秒入口（只暴露 3m / 10m / 25m）", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    const group = within(card).getByRole("group", { name: "时间预算" });
    // 30 秒档默认隐藏：即使 Rust 端仍支持 30s，也必须待 PHASE 16 Gate 全部 VERIFIED 后才恢复。
    expect(within(group).queryByRole("button", { name: "30 秒" })).not.toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "3 分钟" })).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "10 分钟" })).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "25 分钟" })).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(within(group).getByRole("button", { name: "3 分钟" }));
    await waitFor(() =>
      expect(vi.mocked(api.getNextLearningAction)).toHaveBeenCalledWith(1, "3m")
    );
  });

  it("30 秒档：micro_action 不渲染任何「开始学习」按钮（不得创建 StudySession）", async () => {
    mockClosedLoop(
      snapshot(),
      action({
        action_type: "planned_task",
        reason_code: "micro_action_under_one_minute",
        estimated_minutes: 0,
        micro_action_only: true,
        title: "30 秒回顾一个关键点",
        subtitle: "micro action · short_recall",
        execution_payload: {
          kind: "micro_action",
          task_id: null,
          learning_item_id: null,
          session_id: null,
          review_id: null,
          entry_slice: false,
          suggested_minutes: 0,
        },
      })
    );
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).queryByRole("button", { name: "开始学习" })).not.toBeInTheDocument();
    expect(within(card).getByText(/不会创建学习记录/)).toBeInTheDocument();
  });

  it("入口切片：显式说明「任务不会因此完成」", async () => {
    mockClosedLoop(
      snapshot(),
      action({
        estimated_minutes: 3,
        execution_payload: {
          kind: "start_task",
          task_id: 11,
          learning_item_id: null,
          session_id: null,
          review_id: null,
          entry_slice: true,
          suggested_minutes: 3,
        },
      })
    );
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByText(/任务不会因此完成/)).toBeInTheDocument();
  });
});

describe("§PHASE 0.1 HOTFIX-01 / HOTFIX-02 — Next Action 错误不得静默", () => {
  it("HOTFIX-01：LearningState 成功但 NextAction IPC 失败 → 用户可见错误，推荐卡不静默消失", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(snapshot() as never);
    vi.mocked(api.getNextLearningAction).mockRejectedValue(
      new Error("NextAction 拉取失败 (IPC)")
    );

    renderToday();
    await waitLoaded();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/NextAction 拉取失败/);
    // 推荐卡不得静默“消失成空白”——错误可见即代表失败被上报（不是自动降级成假推荐）
    expect(screen.queryByLabelText("从这里开始")).not.toBeInTheDocument();
    // 必须提供「重新计算推荐」
    expect(within(alert).getByRole("button", { name: "重新计算推荐" })).toBeInTheDocument();
  });

  it("HOTFIX-02：点击「重新计算推荐」→ 只重新请求 NextAction（不自己算推荐、不触发任何副作用）", async () => {
    let fail = true;
    vi.mocked(api.getLearningState).mockResolvedValue(snapshot() as never);
    vi.mocked(api.getNextLearningAction).mockImplementation(() =>
      fail
        ? Promise.reject(new Error("NextAction 拉取失败 (IPC)"))
        : Promise.resolve(action() as never)
    );

    renderToday();
    await waitLoaded();

    const alert = await screen.findByRole("alert");
    const before = vi.mocked(api.getNextLearningAction).mock.calls.length;

    const user = userEvent.setup();
    await user.click(within(alert).getByRole("button", { name: "重新计算推荐" }));

    await waitFor(() =>
      expect(vi.mocked(api.getNextLearningAction).mock.calls.length).toBeGreaterThan(before)
    );
    // 重试成功后：推荐卡出现、错误消失
    await waitFor(() =>
      expect(screen.queryByLabelText("从这里开始")).toBeInTheDocument()
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    // 前端只重取，不自己决策——不得触发任何会话/任务副作用
    expect(api.startQuickSession).not.toHaveBeenCalled();
    expect(api.startSession).not.toHaveBeenCalled();
    expect(api.startTaskSession).not.toHaveBeenCalled();
  });
});

describe("LEARN-TC002 — 有 active 时 Next Action 让位给 Active Study Bar", () => {
  it("active session 存在 → 无推荐卡，Active Study Bar 可见且可一击结束", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(
      snapshot({ active_session: SESSION }) as never
    );
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      action({
        action_type: "active_session",
        reason_code: "active_session_in_progress",
        source_entity: { kind: "session", session_id: 77 },
        estimated_minutes: null,
        execution_payload: {
          kind: "continue_session",
          task_id: null,
          learning_item_id: null,
          session_id: 77,
          review_id: null,
          entry_slice: false,
          suggested_minutes: 0,
        },
        title: "快速学习",
        subtitle: "已有一条进行中的学习记录",
        alternates: [],
      }) as never
    );

    renderToday();
    await waitLoaded();

    expect(screen.queryByLabelText("从这里开始")).not.toBeInTheDocument();
    const bar = screen.getByLabelText("正在学习");
    expect(within(bar).getByText("正在学习")).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "继续" })).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "结束" })).toBeInTheDocument();
  });

  it("§22.5 结束一击：endSession 被调用，结束后显示非阻塞「已保存」", async () => {
    const withActive = snapshot({ active_session: SESSION });
    const withoutActive = snapshot({ active_session: null });
    vi.mocked(api.getLearningState)
      .mockResolvedValueOnce(withActive as never)
      .mockResolvedValue(withoutActive as never);
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      action({ action_type: "quick_study", alternates: [] }) as never
    );
    vi.mocked(api.endSession).mockResolvedValue({
      ...SESSION,
      status: "completed",
      ended_at: "2026-09-15 01:32:00",
      duration_seconds: 1920,
    } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(
      within(screen.getByLabelText("正在学习")).getByRole("button", { name: "结束" })
    );

    await waitFor(() => expect(api.endSession).toHaveBeenCalledWith(77));
    const saved = await screen.findByLabelText("学习已保存");
    expect(within(saved).getByText(/已保存 32 分钟/)).toBeInTheDocument();
    // 结束后不再有 active bar（Query invalidation 拿到新快照，无 Modal 阻塞）
    await waitFor(() => expect(screen.queryByLabelText("正在学习")).not.toBeInTheDocument());
  });
});

describe("LEARN-TC003 / Today Start — 一击开始学习", () => {
  it("Next Action「开始学习」一击 → 按 execution_payload 执行 start_task + 跳转学习页", async () => {
    mockClosedLoop();
    vi.mocked(api.startTaskSession).mockResolvedValue({
      ...SESSION,
      id: 900,
      task_id: 11,
    } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const card = screen.getAllByLabelText("从这里开始")[0];
    await user.click(within(card).getByRole("button", { name: "开始学习" }));

    await waitFor(() => expect(api.startTaskSession).toHaveBeenCalledWith(11));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("Header「开始学习」一击 → 立即创建 Quick Session（不强制任何字段）", async () => {
    mockClosedLoop(
      snapshot({ today_tasks: [] }),
      action({
        action_type: "quick_study",
        source_entity: { kind: "none" },
        title: "快速学习",
        alternates: [],
        execution_payload: {
          kind: "start_quick",
          task_id: null,
          learning_item_id: null,
          session_id: null,
          review_id: null,
          entry_slice: false,
          suggested_minutes: 25,
        },
      })
    );
    vi.mocked(api.startQuickSession).mockResolvedValue({ ...SESSION, id: 901 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const header = document.querySelector(".today-head") as HTMLElement;
    await user.click(within(header).getByRole("button", { name: "开始学习" }));

    await waitFor(() => expect(api.startQuickSession).toHaveBeenCalledWith(1));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });
});

describe("LEARN-TC004 — 换建议不改动正式数据", () => {
  it("「换一个」只在 alternates 内切换，不创建/结束/修改任何 Session 或 Task", async () => {
    mockClosedLoop(snapshot({ today_tasks: [TASK, TASK_B] }));
    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const card = screen.getAllByLabelText("从这里开始")[0];
    const before = within(card).getByText(/学习极限定义|背单词/).textContent;
    await user.click(within(card).getByRole("button", { name: "换一个" }));

    await waitFor(() => {
      const now = within(screen.getAllByLabelText("从这里开始")[0]).getByText(
        /学习极限定义|背单词/
      );
      expect(now.textContent).not.toBe(before);
    });

    for (const fn of [
      api.startQuickSession,
      api.startSession,
      api.startTaskSession,
      api.endSession,
      api.createTaskV2,
      api.updateTaskV2,
      api.completeTask,
      api.deleteTask,
      api.archiveTask,
    ]) {
      expect(vi.mocked(fn)).not.toHaveBeenCalled();
    }
  });
});

describe("CONTINUE-TC001 — continue_last 徽标", () => {
  it("后端给出 continue_last → 卡片显示「继续上次」，不额外堆第二张卡", async () => {
    mockClosedLoop(
      snapshot({ today_tasks: [], recent_sessions: [] }),
      action({
        action_type: "continue_last",
        reason_code: "continue_last_recent_session",
        source_entity: { kind: "session", session_id: 500 },
        title: "英语四级 · 翻译",
        subtitle: "上次学习 25 分钟",
        reasons: ["你最近一次学习停在这里，接着学会更快进入状态。"],
        alternates: [],
      })
    );

    renderToday();
    await waitLoaded();

    const cards = screen.getAllByLabelText("从这里开始");
    expect(cards).toHaveLength(1);
    expect(within(cards[0]).getByText("继续上次")).toBeInTheDocument();
    expect(within(cards[0]).getByText("英语四级 · 翻译")).toBeInTheDocument();
  });
});

describe("PHASE 6 — Recovery 状态在首屏可见", () => {
  it("recovery_state.active → 当前状态行出现「恢复模式」且 Primary 是恢复动作", async () => {
    const snap = snapshot({
      recovery_state: {
        active: true,
        reason_codes: ["task_backlog"],
        signals: {
          days_since_last_session: 10,
          sessions_completed_7d: 0,
          has_learning_history: true,
          open_task_today: 1,
          today_task_total: 1,
          overdue_task_count_7d: 4,
          task_total_7d: 5,
          completion_rate_7d: 0.2,
          planned_daily_minutes_14d: 3,
          observed_daily_minutes_14d: 1,
        },
        should_take_primary: true,
      },
    });
    mockClosedLoop(
      snap,
      action({
        action_type: "recovery",
        reason_code: "recovery_task_backlog",
        estimated_minutes: 2,
        title: "回顾昨天的错题",
        subtitle: "先做 2 分钟就够",
        alternates: [],
      })
    );

    renderToday();
    await waitLoaded();

    expect(screen.getByText(/恢复模式/)).toBeInTheDocument();
    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByText("恢复")).toBeInTheDocument();
    expect(within(card).getByText("回顾昨天的错题")).toBeInTheDocument();
  });
});

describe("§22 Today 结构收口", () => {
  it("§22.1：Header 不再出现「AI安排」，但底部次级入口仍保留 AI 能力", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const aiPlan = screen.getByRole("button", { name: /AI安排/ });
    const header = document.querySelector(".today-head");
    expect(header).not.toBeNull();
    expect(header!.contains(aiPlan)).toBe(false);
    expect(screen.getByRole("button", { name: /AI复盘今天/ })).toBeInTheDocument();
  });

  it("§22.2：「新建任务」主按钮只出现一次（Header），Task 区不重复", async () => {
    mockClosedLoop();
    renderToday();
    await waitLoaded();

    const newTaskButtons = screen.getAllByRole("button", { name: /新建任务/ });
    expect(newTaskButtons).toHaveLength(1);
  });

  it("§22.3：Quick Add 输入 Enter 创建 title + today", async () => {
    mockClosedLoop(snapshot({ today_tasks: [] }), action({ alternates: [] }));
    vi.mocked(api.createTaskV2).mockResolvedValue({ id: 1 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const input = screen.getByLabelText("快速添加任务");
    expect(input).toHaveAttribute("placeholder", "今天要做什么？");
    await user.type(input, "做三道积分题{Enter}");

    await waitFor(() =>
      expect(api.createTaskV2).toHaveBeenCalledWith(
        expect.objectContaining({ profileId: 1, title: "做三道积分题" })
      )
    );
    const payload = vi.mocked(api.createTaskV2).mock.calls[0][0] as Record<string, unknown>;
    expect(payload.goalId).toBeUndefined();
    expect(payload.learningItemId).toBeUndefined();
    expect(payload.estimatedMinutes).toBeUndefined();
    expect(payload.priority).toBeUndefined();
    expect(typeof payload.plannedDate).toBe("string");
  });

  it("§22.4：Task Row 可一击开始（不打开详情）", async () => {
    mockClosedLoop();
    vi.mocked(api.startTaskSession).mockResolvedValue({
      ...SESSION,
      id: 902,
      task_id: 11,
    } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => expect(api.startTaskSession).toHaveBeenCalledWith(11));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("§22.4：正在学习的任务在 Task Row 显示「继续」并直接回到该 Session", async () => {
    vi.mocked(api.getLearningState).mockResolvedValue(
      snapshot({ active_session: { ...SESSION, task_id: 11 } }) as never
    );
    vi.mocked(api.getNextLearningAction).mockResolvedValue(
      action({ action_type: "active_session", alternates: [] }) as never
    );

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const list = document.querySelector(".today__tasklist") as HTMLElement;
    expect(list).not.toBeNull();
    await user.click(within(list).getByRole("button", { name: "继续" }));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });
});
