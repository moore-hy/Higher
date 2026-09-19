import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

/**
 * HOTFIX-TEST-TIME-01 —— 消除对真实系统日期的依赖。
 *
 * `elapsed` 由 `Date.now() - started_at` 推导（LearningWorkspace 的 elapsedLabel），
 * 一旦 fixture 把 started_at 写成硬编码的绝对时刻，真实时间往前走，
 * 计时器就会漂移：超过 100 小时后变成 `100:36:14`，撞破 HH:MM:SS 契约。
 *
 * 因此这里把「现在」钉死在一个固定锚点上，started_at 由锚点反推（now - 5 分钟），
 * 使计时器恒为 `00:05:00`，与运行测试的真实日期无关。
 */
const FIXED_NOW = new Date("2026-09-15T01:05:00.000Z");
const SESSION_START_OFFSET_MS = 5 * 60 * 1000;

/**
 * Rust 侧时间列格式 `YYYY-MM-DD HH:MM:SS`（UTC）。
 * 必须按 UTC 序列化：组件用 `started_at.replace(" ", "T") + "Z"` 解析它。
 */
function toUtcColumn(d: Date): string {
  return d.toISOString().slice(0, 19).replace("T", " ");
}

const SESSION_STARTED_AT = toUtcColumn(
  new Date(FIXED_NOW.getTime() - SESSION_START_OFFSET_MS)
);

// 只伪造 Date，setTimeout / setInterval 保持真实：
// userEvent 与 waitFor 都依赖真实定时器推进，假定时器会把它们拖死。
beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(FIXED_NOW);
});
afterEach(() => {
  vi.useRealTimers();
});

/**
 * PRODUCT-2.0 §8A / §23.5 / §46A.3 — Learning Suite：P0 DATA SAFETY 的 UI 侧契约。
 *
 * 断言的是**用户可见行为**，不是实现细节：
 *   1. 笔记保存失败时，学习时长仍然落库（endSession 先于 note flush）
 *   2. 结束后不出现「必须完成」的阻塞式 Modal
 *   3. 双击「结束学习」只产生一次 finalization（Pending 防重复提交）
 *   4. 结束后能看到已保存的真实时长（ReadBack）
 */

function makeSession(over: Record<string, unknown>) {
  return {
    id: 7,
    profile_id: 1,
    goal_id: null,
    task_id: null,
    learning_item_id: null,
    title: "快速学习",
    started_at: SESSION_STARTED_AT,
    ended_at: null,
    duration_seconds: null,
    status: "active",
    note: null,
    note_document_json: null,
    created_at: SESSION_STARTED_AT,
    updated_at: SESSION_STARTED_AT,
    time_corrected: 0,
    activity_kind: "unplanned",
    duration_review_state: "normal",
    ...over,
  };
}

// 关键：mock 返回的对象/函数必须**引用稳定**。
// 若每次调用都新建对象，LearningWorkspace 的 `load` useCallback 依赖会每次变化，
// 触发 useEffect 反复重跑 load()，把 justEnded 一直重置掉（同时造成无限重载）。
// 因此在工厂闭包内建一次性单例（工厂体只在模块首次被请求时执行一次）。
vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const profile = Object.freeze({ id: 1, name: "测试档案" });
  const gate = Object.freeze({ phase: "active" });
  const ctx = {
    activeProfile: profile,
    gate,
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
  };
  return {
    useAiPanel: () => panel,
    AiPanelProvider: (props: { children?: unknown }) => props.children,
  };
});

// 重量级 UI 依赖：本套用例只关心「结束」链路，不关心编辑器/附件面板内部实现。
vi.mock("../../src/components/RichDocEditor", () => ({
  default: () => null,
  documentToPlainText: () => "已写下的学习笔记",
  noteToDocument: () => ({ type: "doc", content: [] }),
}));
vi.mock("../../src/components/AttachmentList", () => ({ default: () => null }));
vi.mock("../../src/components/DailyActivitiesSection", () => ({
  default: () => null,
  durationShort: (s: number) => `${s}s`,
}));
vi.mock("../../src/components/DailyTasksSection", () => ({
  default: () => null,
  minutesShort: (m: number) => `${m}m`,
}));
vi.mock("../../src/components/ActiveSessionConflictModal", () => ({
  default: () => null,
  useActiveSessionConflict: () => ({ conflict: null, guard: () => false, close: () => {} }),
}));

vi.mock("../../src/api", () => ({
  // 读路径
  getSession: vi.fn(),
  listLearningItemsByProfile: vi.fn(async () => []),
  listAttachmentsBySession: vi.fn(async () => []),
  listGoalsByProfile: vi.fn(async () => []),
  listAllTasksByProfile: vi.fn(async () => []),
  listTodayTasksByProfile: vi.fn(async () => []),
  getLearningItemPath: vi.fn(async () => "自由学习"),
  getLearningTotals: vi.fn(async () => ({
    today_seconds: 1500,
    today_tasks_completed: 1,
    today_tasks_total: 3,
  })),
  // 写路径
  endSession: vi.fn(),
  updateSessionDocument: vi.fn(),
  updateSessionTitle: vi.fn(),
  organizeSessionIntoKnowledge: vi.fn(),
  attachSession: vi.fn(),
  createChildLearningItem: vi.fn(),
  createRootLearningItem: vi.fn(),
  startQuickSession: vi.fn(),
  startTaskSession: vi.fn(),
  startSession: vi.fn(),
  correctSessionTime: vi.fn(),
  confirmSessionDuration: vi.fn(),
  deleteSession: vi.fn(),
  updateLearningItemContent: vi.fn(),
  // §M1-B / §M1-D：结束后「再来一点 / 看看下一步」必须重新读取的后端入口
  getLearningState: vi.fn(async () => ({})),
  getNextLearningAction: vi.fn(),
}));

import * as api from "../../src/api";
import LearningWorkspace from "../../src/pages/LearningWorkspace";

const endSessionMock = vi.mocked(api.endSession);
const updateSessionDocumentMock = vi.mocked(api.updateSessionDocument);
const getSessionMock = vi.mocked(api.getSession);
const getLearningStateMock = vi.mocked(api.getLearningState);
const getNextLearningActionMock = vi.mocked(api.getNextLearningAction);
const startQuickSessionMock = vi.mocked(api.startQuickSession);

/** §M1-D 结束后视图的最小 NextLearningAction（字段与 Rust 投影一一对应）。 */
function makeAction(over: Record<string, unknown> = {}) {
  return {
    profile_id: 1,
    local_date: "2026-09-15",
    action_type: "quick_study",
    reason_code: "quick_study",
    source_entity: { kind: "none" },
    estimated_minutes: 3,
    source_task_estimate_minutes: null,
    available_minutes: null,
    execution_payload: {
      kind: "start_quick",
      task_id: null,
      learning_item_id: null,
      session_id: null,
      review_id: null,
      entry_slice: false,
      suggested_minutes: 3,
    },
    title: "快速学习",
    subtitle: null,
    reasons: ["不绑定任务，点一下就开始计时。"],
    is_primary: true,
    micro_action_only: false,
    micro_action: null,
    alternates: [],
    ...over,
  };
}

function renderWorkspace() {
  return render(
    <MemoryRouter initialEntries={["/learn/7"]}>
      <Routes>
        <Route path="/" element={<div data-testid="today-page">Today</div>} />
        <Route path="/learn/:sessionId" element={<LearningWorkspace />} />
      </Routes>
    </MemoryRouter>
  );
}

/** 等待工作区完成首屏加载（结束按钮出现即视为 loaded）。 */
async function waitLoaded() {
  await waitFor(() => expect(screen.getByTestId("learning-end")).toBeInTheDocument());
}

describe("Learning Suite — P0 结束链路", () => {
  it("DATA-TC003：笔记保存失败，学习时长仍然落库，且给出可重试提示", async () => {
    const user = userEvent.setup();
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    endSessionMock.mockResolvedValue(
      makeSession({
        status: "completed",
        ended_at: "2026-09-15 01:25:00",
        duration_seconds: 1500,
      }) as never
    );
    // 制造笔记保存失败。
    updateSessionDocumentMock.mockRejectedValue(new Error("disk full") as never);

    renderWorkspace();
    await waitLoaded();

    await user.click(screen.getByTestId("learning-end"));

    // 学习事实优先：endSession 必须被调用，且只调用一次。
    await waitFor(() => expect(endSessionMock).toHaveBeenCalledTimes(1));
    expect(endSessionMock).toHaveBeenCalledWith(7);

    // 笔记失败被显式暴露给用户，而不是静默吞掉，也不是阻止结束。
    await waitFor(() =>
      expect(screen.getByTestId("learning-note-save-failed")).toBeInTheDocument()
    );
    // ReadBack：结束后视图展示已保存的时长。
    expect(screen.getByText(/25 分钟/)).toBeInTheDocument();
  });

  it("§23.5：结束后不出现阻塞式 Modal，用户可直接离开", async () => {
    const user = userEvent.setup();
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    endSessionMock.mockResolvedValue(
      makeSession({
        status: "completed",
        ended_at: "2026-09-15 01:25:00",
        duration_seconds: 1500,
      }) as never
    );
    updateSessionDocumentMock.mockResolvedValue(undefined as never);

    const { container } = renderWorkspace();
    await waitLoaded();
    await user.click(screen.getByTestId("learning-end"));

    await waitFor(() => expect(endSessionMock).toHaveBeenCalledTimes(1));

    // 没有必须完成的 Modal backdrop。
    expect(container.querySelector(".modal-overlay")).toBeNull();
    // 非阻塞收尾动作存在，但都不是强制的。
    expect(screen.getByTestId("learning-post-session-optional")).toBeInTheDocument();
    // §M1-D：结束后**不把用户丢回 dashboard**，而是给出三个锁定动作。
    expect(screen.getByTestId("learning-session-end-actions")).toBeInTheDocument();
    expect(screen.getByText("再来一点")).toBeInTheDocument();
    expect(screen.getByText("看看下一步")).toBeInTheDocument();
    expect(screen.getByText("今天结束")).toBeInTheDocument();
  });

  it("Pending / Double Click：连续两次点击「结束学习」只产生一次结束", async () => {
    const user = userEvent.setup();
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    let resolveEnd: ((v: unknown) => void) | undefined;
    endSessionMock.mockImplementation(
      () =>
        new Promise((res) => {
          resolveEnd = res;
        }) as never
    );
    updateSessionDocumentMock.mockResolvedValue(undefined as never);

    renderWorkspace();
    await waitLoaded();

    const btn = screen.getByTestId("learning-end");
    await user.click(btn);
    // 第一次结束请求尚未返回时，按钮必须被锁定，第二次点击不得再次提交。
    await user.click(btn);

    await waitFor(() => expect(endSessionMock).toHaveBeenCalledTimes(1));
    expect(endSessionMock).toHaveBeenCalledTimes(1);

    resolveEnd?.(
      makeSession({
        status: "completed",
        ended_at: "2026-09-15 01:25:00",
        duration_seconds: 1500,
      })
    );
    await waitFor(() => expect(screen.getByText(/25 分钟/)).toBeInTheDocument());
    expect(endSessionMock).toHaveBeenCalledTimes(1);
  });

  it("结束过程中按钮进入禁用态（禁止重复提交）", async () => {
    const user = userEvent.setup();
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    let resolveEnd: ((v: unknown) => void) | undefined;
    endSessionMock.mockImplementation(
      () =>
        new Promise((res) => {
          resolveEnd = res;
        }) as never
    );
    updateSessionDocumentMock.mockResolvedValue(undefined as never);

    renderWorkspace();
    await waitLoaded();
    await user.click(screen.getByTestId("learning-end"));

    await waitFor(() =>
      expect(screen.getByTestId("learning-end")).toBeDisabled()
    );

    resolveEnd?.(
      makeSession({
        status: "completed",
        ended_at: "2026-09-15 01:25:00",
        duration_seconds: 1500,
      })
    );
    await waitFor(() => expect(screen.getByText(/25 分钟/)).toBeInTheDocument());
  });
});

describe("Learning Suite — 基础渲染契约", () => {
  it("加载后展示当前学习标题与 HH:MM:SS 计时器（elapsed 可见）", async () => {
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    const { container } = renderWorkspace();
    await waitLoaded();

    expect(screen.getByText("快速学习")).toBeInTheDocument();
    // started_at 恒为「固定锚点 - 5 分钟」，所以计时器不再依赖真实系统日期，
    // 也就不需要只敢断言格式 —— 这里可以直接锁定真实跳动的时刻本身。
    const timer = container.querySelector(".lw__timer");
    expect(timer).not.toBeNull();
    expect(timer?.textContent ?? "").toMatch(/^\d{2}:\d{2}:\d{2}$/);
    expect(timer?.textContent ?? "").toBe("00:05:00");
  });
});

/**
 * §M1-D Session End Experience + §M1-B「再来一点」必须重算。
 *
 * 锁定规则：
 * - 结束后**不把用户丢回 dashboard**，而是展示真实事实 + 三个锁定动作；
 * - 「再来一点」「看看下一步」都必须**重新读取** LearningState 并重算 NextAction，
 *   永远不是 `pack[index + 1]`，也不是本地任务列表的下一项。
 */
describe("M1-D / M1-B — Session End Experience", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /** 结束当前学习并停在「结束后视图」（M1-D 三个动作出现的时机）。 */
  async function endAndReachPostView() {
    const user = userEvent.setup();
    getSessionMock.mockResolvedValue(makeSession({}) as never);
    endSessionMock.mockResolvedValue(
      makeSession({
        status: "completed",
        ended_at: "2026-09-15 01:25:00",
        duration_seconds: 1500,
      }) as never
    );
    updateSessionDocumentMock.mockResolvedValue(undefined as never);
    renderWorkspace();
    await waitLoaded();
    await user.click(screen.getByTestId("learning-end"));
    await waitFor(() =>
      expect(screen.getByTestId("learning-session-end-actions")).toBeInTheDocument()
    );
    return user;
  }

  it("只展示真实事实（本次时长 / 今天累计），不做无正确性证据的祝贺", async () => {
    await endAndReachPostView();

    // 真实事实：本次时长 25 分钟；今天累计来自 getLearningTotals(1500s) → 25m。
    expect(screen.getByText(/25 分钟/)).toBeInTheDocument();
    expect(screen.getByText(/今天累计 25m/)).toBeInTheDocument();
    // 没有正确性证据时，不得出现任何「对了多少 / 正确率」式结论。
    expect(screen.queryByText(/正确率/)).toBeNull();
    expect(screen.queryByText(/答对/)).toBeNull();
  });

  it("『再来一点』重新读取后端状态并重算，绝不使用本地数组下标", async () => {
    getNextLearningActionMock.mockResolvedValue(makeAction() as never);
    startQuickSessionMock.mockResolvedValue(makeSession({ id: 99 }) as never);

    const user = await endAndReachPostView();
    await user.click(screen.getByText("再来一点"));

    // 必须真正重新读取 LearningState + 重算 NextAction（0 LLM）。
    await waitFor(() => expect(getLearningStateMock).toHaveBeenCalledWith(1));
    expect(getNextLearningActionMock).toHaveBeenCalledWith(1, null);
    // 并且执行的是后端返回的 execution_payload，而不是前端自选的目标。
    await waitFor(() => expect(startQuickSessionMock).toHaveBeenCalledWith(1));
  });

  it("『看看下一步』只重新读取并展示新的下一步，不替用户开始学习", async () => {
    getNextLearningActionMock.mockResolvedValue(
      makeAction({
        action_type: "planned_task",
        title: "复习优先编码器",
        reasons: ["上次回忆未通过。"],
      }) as never
    );

    const user = await endAndReachPostView();
    await user.click(screen.getByText("看看下一步"));

    await waitFor(() =>
      expect(screen.getByTestId("learning-next-peek")).toBeInTheDocument()
    );
    expect(screen.getByText("复习优先编码器")).toBeInTheDocument();
    // 只读：不得开任何 Session。
    expect(startQuickSessionMock).not.toHaveBeenCalled();
    expect(vi.mocked(api.startTaskSession)).not.toHaveBeenCalled();
  });

  it("『今天结束』返回今日，不强行开始任何学习", async () => {
    const user = await endAndReachPostView();
    await user.click(screen.getByText("今天结束"));

    await waitFor(() => expect(screen.getByTestId("today-page")).toBeInTheDocument());
    expect(startQuickSessionMock).not.toHaveBeenCalled();
  });
});
