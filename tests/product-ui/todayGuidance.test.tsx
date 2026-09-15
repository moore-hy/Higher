import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

/**
 * PRODUCT-2.0 §22 / §30B / §64 —— Today 单一学习引导面交互契约。
 *
 * 覆盖（Critical UI，§64 禁止 PENDING HUMAN）：
 *   LEARN-TC001  no active -> at most ONE Start Here recommendation
 *   LEARN-TC002  active session -> Start Here hidden, Active Study Bar wins
 *   LEARN-TC003  manual Task Start works regardless of recommendation
 *   LEARN-TC004  dismiss/swap recommendation does not mutate plan/task
 *   CONTINUE-TC001 recent ended session visible as Continue Last
 *   §22.1 Header 不出现 AI安排（已移至底部次级入口）
 *   §22.2 不重复「新建任务」主按钮
 *   §22.3 Quick Add：Enter 创建 title + today
 *   §22.5 Active Study Bar 结束一击 → 非阻塞「已保存」
 *   Today Start（开始学习）= 一击创建 Quick Session
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
  getDailyLearningReport: vi.fn(),
  listLearningItemsByProfile: vi.fn(async () => []),
  getGoalTree: vi.fn(async () => null),
  getActiveSession: vi.fn(async () => null),
  isPlanningReviewDue: vi.fn(async () => false),
  getPlanningReviewRisk: vi.fn(async () => "unknown"),
  listRecentSessionsByProfile: vi.fn(async () => []),
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

const SESSION = {
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
};

const REPORT = {
  date: "2026-09-15",
  planned_minutes: 25,
  unestimated_task_count: 0,
  actual_minutes: 0,
  planned_task_actual_minutes: 0,
  task_total: 1,
  task_completed: 0,
  task_completion_rate: 0,
  day_goal: null,
  day_goal_id: null,
  day_goal_progress: null,
  time_execution_rate: null,
  tasks: [TASK],
  activities: [],
};

function mockReport(over: Record<string, unknown> = {}) {
  vi.mocked(api.getDailyLearningReport).mockResolvedValue({
    ...REPORT,
    ...over,
  } as never);
}

function renderToday() {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <Routes>
        <Route path="/" element={<Today />} />
        <Route path="/learn/:id" element={<div data-testid="learn-page">学习工作区</div>} />
      </Routes>
    </MemoryRouter>
  );
}

async function waitLoaded() {
  await waitFor(() => expect(screen.queryByText("加载中…")).not.toBeInTheDocument());
}

describe("LEARN-TC001 — 同一时刻最多一个 Start Here 主建议", () => {
  it("无 active：Start Here 只渲染一次，且「开始学习」是其中唯一主推荐按钮", async () => {
    mockReport();
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

  it("Start Here 默认不展开理由（禁止自动展开，§30B）", async () => {
    mockReport();
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).queryByRole("list")).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(within(card).getByRole("button", { name: "为什么？" }));
    expect(within(card).getByRole("list")).toBeInTheDocument();
  });

  it("Start Here 推荐的是今日高优先任务，且展示预计时长", async () => {
    mockReport();
    renderToday();
    await waitLoaded();

    const card = screen.getAllByLabelText("从这里开始")[0];
    expect(within(card).getByText("学习极限定义")).toBeInTheDocument();
    expect(within(card).getByText("预计 25 分钟")).toBeInTheDocument();
  });
});

describe("LEARN-TC002 — 有 active 时 Start Here 让位给 Active Study Bar", () => {
  it("active session 存在 → 无 Start Here，Active Study Bar 可见且可一击结束", async () => {
    mockReport();
    vi.mocked(api.getActiveSession).mockResolvedValue(SESSION as never);
    renderToday();
    await waitLoaded();

    expect(screen.queryByLabelText("从这里开始")).not.toBeInTheDocument();
    const bar = screen.getByLabelText("正在学习");
    expect(within(bar).getByText("正在学习")).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "继续" })).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "结束" })).toBeInTheDocument();
  });

  it("§22.5 结束一击：endSession 被调用，结束后显示非阻塞「已保存」", async () => {
    mockReport();
    // 结束后 refresh 会重新拉 active：第二次起必须为 null，否则 bar 会「复活」
    vi.mocked(api.getActiveSession)
      .mockResolvedValueOnce(SESSION as never)
      .mockResolvedValue(null);
    vi.mocked(api.endSession).mockResolvedValue({
      ...SESSION,
      status: "completed",
      ended_at: "2026-09-15 01:32:00",
      duration_seconds: 1920,
    } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(within(screen.getByLabelText("正在学习")).getByRole("button", { name: "结束" }));

    await waitFor(() => expect(api.endSession).toHaveBeenCalledWith(77));
    const saved = await screen.findByLabelText("学习已保存");
    expect(within(saved).getByText(/已保存 32 分钟/)).toBeInTheDocument();
    // 结束后不再有 active bar（无 Modal 阻塞）
    expect(screen.queryByLabelText("正在学习")).not.toBeInTheDocument();
  });
});

describe("LEARN-TC003 / Today Start — 一击开始学习", () => {
  it("Start Here「开始学习」一击 → startTaskSession + 跳转学习页", async () => {
    mockReport();
    vi.mocked(api.startTaskSession).mockResolvedValue({ ...SESSION, id: 900, task_id: 11 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    const card = screen.getAllByLabelText("从这里开始")[0];
    await user.click(within(card).getByRole("button", { name: "开始学习" }));

    await waitFor(() => expect(api.startTaskSession).toHaveBeenCalledWith(11));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("Header「开始学习」一击 → 立即创建 Quick Session（不强制任何字段）", async () => {
    mockReport({ tasks: [] });
    vi.mocked(api.startQuickSession).mockResolvedValue({ ...SESSION, id: 901 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    // 作用域限定 Header：Start Here 内也有一颗「开始学习」
    const header = document.querySelector(".today-head") as HTMLElement;
    await user.click(within(header).getByRole("button", { name: "开始学习" }));

    await waitFor(() => expect(api.startQuickSession).toHaveBeenCalledWith(1));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });
});

describe("LEARN-TC004 — 换建议不改动正式数据", () => {
  it("「换一个」只切换展示，不创建/结束/修改任何 Session 或 Task", async () => {
    mockReport({ tasks: [{ ...TASK, id: 11 }, { ...TASK, id: 12, title: "背单词" }] });
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

describe("CONTINUE-TC001 — 继续上次学习并入 Start Here", () => {
  it("近期已结束 Session → 显示「继续上次」候选，不额外堆第二张卡", async () => {
    mockReport({ tasks: [] });
    vi.mocked(api.listRecentSessionsByProfile).mockResolvedValue([
      {
        ...SESSION,
        id: 500,
        title: "英语四级 · 翻译",
        status: "completed",
        ended_at: new Date(Date.now() - 60_000).toISOString(),
        duration_seconds: 1500,
      },
    ] as never);

    renderToday();
    await waitLoaded();

    const cards = screen.getAllByLabelText("从这里开始");
    expect(cards).toHaveLength(1);
    expect(within(cards[0]).getByText("继续上次")).toBeInTheDocument();
    expect(within(cards[0]).getByText("英语四级 · 翻译")).toBeInTheDocument();
  });
});

describe("§22 Today 结构收口", () => {
  it("§22.1：Header 不再出现「AI安排」，但底部次级入口仍保留 AI 能力", async () => {
    mockReport();
    renderToday();
    await waitLoaded();

    const aiPlan = screen.getByRole("button", { name: /AI安排/ });
    // 必须不在 Header 内
    const header = document.querySelector(".today-head");
    expect(header).not.toBeNull();
    expect(header!.contains(aiPlan)).toBe(false);
    expect(screen.getByRole("button", { name: /AI复盘今天/ })).toBeInTheDocument();
  });

  it("§22.2：「新建任务」主按钮只出现一次（Header），Task 区不重复", async () => {
    mockReport();
    renderToday();
    await waitLoaded();

    const newTaskButtons = screen.getAllByRole("button", { name: /新建任务/ });
    expect(newTaskButtons).toHaveLength(1);
  });

  it("§22.3：Quick Add 输入 Enter 创建 title + today", async () => {
    mockReport({ tasks: [] });
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
    // 只传 title（+ today），不强制 Goal / Knowledge / priority / estimated_minutes
    const payload = vi.mocked(api.createTaskV2).mock.calls[0][0] as Record<string, unknown>;
    expect(payload.goalId).toBeUndefined();
    expect(payload.learningItemId).toBeUndefined();
    expect(payload.estimatedMinutes).toBeUndefined();
    expect(payload.priority).toBeUndefined();
    expect(typeof payload.plannedDate).toBe("string");
  });

  it("§22.4：Task Row 可一击开始（不打开详情）", async () => {
    mockReport();
    vi.mocked(api.startTaskSession).mockResolvedValue({ ...SESSION, id: 902, task_id: 11 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => expect(api.startTaskSession).toHaveBeenCalledWith(11));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("§22.4：正在学习的任务在 Task Row 显示「继续」并直接回到该 Session", async () => {
    mockReport();
    vi.mocked(api.getActiveSession).mockResolvedValue({ ...SESSION, task_id: 11 } as never);

    renderToday();
    await waitLoaded();

    const user = userEvent.setup();
    // 作用域限定任务列表：Active Study Bar 里也有一颗「继续」
    const list = document.querySelector(".today__tasklist") as HTMLElement;
    expect(list).not.toBeNull();
    await user.click(within(list).getByRole("button", { name: "继续" }));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });
});
