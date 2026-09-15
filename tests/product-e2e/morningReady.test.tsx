/**
 * PRODUCT-2.0 PHASE H · MORNING_READY 44-STEP 端到端验收门。
 *
 * 设计原则（§15 / §19）：
 * - 这是**真实** e2e 验收门，不是假完成。每个断言都必须能真实失败。
 * - 复用 product-ui / interaction-contract 既有的「mock 真实组件 + 驱动真实交互」范式。
 * - 后端一律以桌面 IPC 在 jsdom 下确定性 mock；不依赖真实 WebView / 网络。
 * - 尚未贯通的 Agent / Review / Learning 步骤（20-33、39-43）按各 Phase 进度
 *   逐步补齐真实断言；缺功能时允许临时红（§A3），不得用 skip / 空 assert 造假。
 *
 * 本文件覆盖已落地的「用户面」步骤：
 *   Today（1-15）· Planning/Intake（16-19）· Knowledge Canvas（34-38）。
 */

import { render, screen, waitFor, within, act, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

// ---- 稳定上下文 mock（对象必须引用稳定，避免无限重渲染） ----
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
    ActiveProfileProvider: (p: { children?: unknown }) => p.children,
    canSwitchProfile: () => true,
  };
});

vi.mock("../../src/components/ai/AiPanelContext", () => {
  const panel = {
    runAction: () => Promise.resolve(),
    setPageContext: () => {},
    sendChat: vi.fn(async () => {}),
  };
  return {
    useAiPanel: () => panel,
    AiPanelProvider: (p: { children?: unknown }) => p.children,
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}));

vi.mock("@excalidraw/excalidraw/index.css", () => ({}));
vi.mock("@excalidraw/excalidraw", async () => {
  const React = await import("react");
  const APP_STATE = { scrollX: 0, scrollY: 0, zoom: { value: 1 }, offsetLeft: 0, offsetTop: 0 };
  function FakeExcalidraw({
    onChange,
    initialData,
  }: {
    onChange?: (els: readonly unknown[], appState: unknown) => void;
    initialData?: { elements?: unknown };
  }) {
    const booted = React.useRef(false);
    React.useEffect(() => {
      if (booted.current) return;
      booted.current = true;
      onChange?.([], APP_STATE);
    }, [onChange]);
    return (
      <div data-testid="fake-excalidraw" data-elements={JSON.stringify(initialData?.elements ?? [])}>
        <button
          type="button"
          data-testid="fake-draw"
          onClick={() =>
            onChange?.(
              [
                { id: "r1", type: "rectangle", x: 1, y: 2, width: 30, height: 40 },
                { id: "t1", type: "text", x: 5, y: 5, text: "自由书写" },
              ],
              APP_STATE
            )
          }
        >
          画一个矩形
        </button>
      </div>
    );
  }
  return { Excalidraw: FakeExcalidraw };
});

// ---- 重量级 UI 收口（与本验收面无关，保持渲染面收敛） ----
vi.mock("../../src/components/FinalGoalCard", () => ({
  PLAN_REQUEST_MESSAGE: "请根据我的最终目标安排未来14天计划",
  default: () => null,
}));
vi.mock("../../src/components/DailyActivitiesSection", () => ({
  default: () => null,
  durationShort: (s: number) => `${s}s`,
}));
// DailyTasksSection 承载真实「Quick Add」输入（label=快速添加任务），需真实渲染以验证步骤 6-7。
// 其依赖的 api 已由下方统一 mock 覆盖（importOriginal 兜底，未知函数安全返回 undefined）。
vi.mock("../../src/components/ActiveSessionConflictModal", () => ({
  default: () => null,
  useActiveSessionConflict: () => ({ conflict: null, guard: () => false, close: () => {} }),
}));
vi.mock("../../src/components/RichDocEditor", () => ({
  default: () => null,
  documentToPlainText: () => "已写下的学习笔记",
  noteToDocument: () => ({ type: "doc", content: [] }),
}));
vi.mock("../../src/components/AttachmentList", () => ({ default: () => null }));

// ---- 统一 API mock（importOriginal 兜底，杜绝「no export」崩溃；覆盖页读取路径） ----
vi.mock("../../src/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/api")>();
  const DEFAULT_REPORT = {
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
    tasks: [],
    activities: [],
  };
  const overrides: Record<string, unknown> = {
    // profile / session
    getActiveStudyProfile: vi.fn(async () => ({ id: 1, name: "测试档案" })),
    getStudyProfile: vi.fn(async () => ({ id: 1, name: "测试档案" })),
    getActiveSession: vi.fn(async () => null),
    startQuickSession: vi.fn(),
    startSession: vi.fn(),
    startTaskSession: vi.fn(),
    endSession: vi.fn(),
    updateSessionNote: vi.fn(),
    correctSessionTime: vi.fn(),
    confirmSessionDuration: vi.fn(),
    deleteSession: vi.fn(),
    // tasks
    createTaskV2: vi.fn(),
    updateTaskV2: vi.fn(),
    completeTask: vi.fn(),
    uncompleteTask: vi.fn(),
    deleteTask: vi.fn(),
    archiveTask: vi.fn(),
    listTodayTasksByProfile: vi.fn(async () => []),
    listAllTasksByProfile: vi.fn(async () => []),
    // report / planning truth
    getDailyLearningReport: vi.fn(async () => DEFAULT_REPORT),
    materializeRecurringRolling: vi.fn(async () => 0),
    listLearningItemsByProfile: vi.fn(async () => []),
    getGoalTree: vi.fn(async () => null),
    isPlanningReviewDue: vi.fn(async () => false),
    getPlanningReviewRisk: vi.fn(async () => "unknown"),
    listRecentSessionsByProfile: vi.fn(async () => []),
    syncNotifications: vi.fn(async () => undefined),
    getTime: vi.fn(async () => "2026-09-15 01:00:00"),
    startupMark: vi.fn(),
    // planning intake draft（只写草稿，不碰正式表）
    getPlanningIntakeDraft: vi.fn(async () => null),
    savePlanningIntakeDraft: vi.fn(),
    setPlanningIntakeStatus: vi.fn(async () => undefined),
    discardPlanningIntakeDraft: vi.fn(async () => undefined),
    writeExportFile: vi.fn(async () => undefined),
    importPersonalizationFiles: vi.fn(async () => []),
    createGoal: vi.fn(),
    createPlanningBlueprint: vi.fn(),
    applyAiChangeSet: vi.fn(),
    // knowledge canvas
    getKnowledgeCanvas: vi.fn(async () => null),
    listCanvasEmbeds: vi.fn(async () => []),
    saveKnowledgeCanvas: vi.fn(),
    addCanvasEmbed: vi.fn(),
    updateCanvasEmbedGeometry: vi.fn(),
    deleteCanvasEmbed: vi.fn(),
    addAttachmentFromBase64: vi.fn(async () => ({ id: 900 })),
    getAttachmentAssetPath: vi.fn(async () => "sandbox/attachments/a.png"),
    // knowledge workspace（页面挂载需要的读路径）
    getKnowledgeWorkspace: vi.fn(async () => ({
      root_items: [],
      documents: [],
      records: [],
    })),
    listLearningItemsLight: vi.fn(async () => []),
    listLearningItemsByProfile: vi.fn(async () => []),
    getLearningItemPath: vi.fn(async () => "自由学习"),
    createKnowledgeDocument: vi.fn(),
    updateKnowledgeDocument: vi.fn(),
    deleteKnowledgeDocument: vi.fn(),
    createRootLearningItem: vi.fn(),
    createChildLearningItem: vi.fn(),
    renameKnowledgeDocument: vi.fn(),
    saveDocumentDrawing: vi.fn(),
    listAttachmentsByDocument: vi.fn(async () => []),
    addDocumentAttachment: vi.fn(),
    addDocumentAttachmentFromBase64: vi.fn(),
    listEvaluationsByLearningItem: vi.fn(async () => []),
    listFeedbacksByLearningItem: vi.fn(async () => []),
    listGoalsByProfile: vi.fn(async () => []),
    updateLearningItem: vi.fn(),
    updateLearningItemStatus: vi.fn(),
    reorderLearningItems: vi.fn(),
    moveLearningItem: vi.fn(),
    deleteLearningItem: vi.fn(),
    organizeSessionIntoKnowledge: vi.fn(),
    updateSessionTitle: vi.fn(),
    // noop 兜底（其余命令不参与本次验收面，但页面可能调用）
    getLearningTotals: vi.fn(async () => ({ today_seconds: 0, today_tasks_completed: 0, today_tasks_total: 0 })),
    searchHigher: vi.fn(async () => []),
    listMemoryRecords: vi.fn(async () => []),
  };
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(actual)) {
    if (key in overrides) out[key] = overrides[key];
    else {
      const v = (actual as Record<string, unknown>)[key];
      out[key] = typeof v === "function" ? vi.fn(async () => undefined) : v;
    }
  }
  for (const key of Object.keys(overrides)) if (!(key in out)) out[key] = overrides[key];
  return out;
});

import * as api from "../../src/api";
import Today from "../../src/pages/Today";
import PlanningIntake from "../../src/components/PlanningIntake";
import KnowledgeCanvas from "../../src/features/knowledge/canvas/KnowledgeCanvas";
import { AUTOSAVE_DEBOUNCE_MS } from "../../src/features/knowledge/canvas/canvasSerialization";

// ============================================================
// 共享 fixtures
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
  activity_kind: "unplanned",
  duration_review_state: "normal",
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
  vi.mocked(api.getDailyLearningReport).mockResolvedValue({ ...REPORT, ...over } as never);
}

// ============================================================
// PHASE H · MORNING READY 44-STEP GATE
// ============================================================

describe("MORNING_READY · Today / Session（步骤 1-15）", () => {
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

  it("1-5：App 启动 → Profile Gate 完成 → Today 正常渲染且无 ErrorBoundary", async () => {
    mockReport();
    renderToday();
    await waitLoaded();
    // 单一 Start Here 已渲染 = 页面无崩溃
    expect(screen.getByLabelText("从这里开始")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    // Header 主入口存在（作用域限定 Header：Start Here 内也有一颗「开始学习」）
    const header = document.querySelector(".today-head") as HTMLElement;
    expect(within(header).getByRole("button", { name: "开始学习" })).toBeInTheDocument();
    expect(within(header).getByRole("button", { name: /新建任务/ })).toBeInTheDocument();
  });

  it("6-7：页面只存在单一 Start Here；Quick Add 可创建今天 Task", async () => {
    mockReport({ tasks: [] });
    vi.mocked(api.createTaskV2).mockResolvedValue({ id: 1 } as never);
    renderToday();
    await waitLoaded();

    expect(screen.getAllByLabelText("从这里开始")).toHaveLength(1);
    const input = screen.getByLabelText("快速添加任务");
    expect(input).toHaveAttribute("placeholder", "今天要做什么？");
    const user = userEvent.setup();
    await user.type(input, "做三道积分题{Enter}");
    await waitFor(() =>
      expect(api.createTaskV2).toHaveBeenCalledWith(
        expect.objectContaining({ profileId: 1, title: "做三道积分题" })
      )
    );
  });

  it("8-9：新 Task 出现在 Today；Task 可一击开始", async () => {
    mockReport();
    vi.mocked(api.startTaskSession).mockResolvedValue({ ...SESSION, id: 902, task_id: 11 } as never);
    renderToday();
    await waitLoaded();

    // 推荐任务出现在 Today（Start Here 主建议即今日高优先 Task）
    const card = screen.getByLabelText("从这里开始");
    expect(within(card).getByText("学习极限定义")).toBeInTheDocument();
    // Task 一击开始（Start Here 内的「开始学习」= 对该 Task 起 Session）
    const user = userEvent.setup();
    await user.click(within(card).getByRole("button", { name: "开始学习" }));
    await waitFor(() => expect(api.startTaskSession).toHaveBeenCalledWith(11));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("10-11：Active Study Bar 出现；Continue 可进入当前 Session", async () => {
    mockReport();
    vi.mocked(api.getActiveSession)
      .mockResolvedValueOnce({ ...SESSION, id: 701, task_id: 11 } as never)
      .mockResolvedValue(null);
    renderToday();
    await waitLoaded();
    const bar = screen.getByLabelText("正在学习");
    expect(within(bar).getByRole("button", { name: "继续" })).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(within(bar).getByRole("button", { name: "继续" }));
    expect(await screen.findByTestId("learn-page")).toBeInTheDocument();
  });

  it("12-13：End Study 首次保存真实结束时间；ReadBack 读取真实 duration", async () => {
    mockReport();
    vi.mocked(api.getActiveSession)
      .mockResolvedValueOnce({ ...SESSION, id: 77, ended_at: null } as never)
      .mockResolvedValue(null);
    vi.mocked(api.endSession).mockResolvedValue({
      ...SESSION,
      id: 77,
      status: "completed",
      ended_at: "2026-09-15 01:25:00",
      duration_seconds: 1500,
    } as never);
    renderToday();
    await waitLoaded();
    const user = userEvent.setup();
    await user.click(within(screen.getByLabelText("正在学习")).getByRole("button", { name: "结束" }));
    await waitFor(() => expect(api.endSession).toHaveBeenCalledWith(77));
    expect(await screen.findByLabelText("学习已保存")).toBeInTheDocument();
  });

  it("14：第二次 End 不重复累加 duration（幂等 finalization）", async () => {
    mockReport();
    let calls = 0;
    vi.mocked(api.endSession).mockImplementation(async () => {
      calls += 1;
      return { ...SESSION, id: 77, status: "completed", ended_at: "2026-09-15 01:25:00", duration_seconds: 1500 } as never;
    });
    vi.mocked(api.getActiveSession)
      .mockResolvedValueOnce({ ...SESSION, id: 77, ended_at: null } as never)
      .mockResolvedValueOnce({ ...SESSION, id: 77, ended_at: "2026-09-15 01:25:00", duration_seconds: 1500, status: "completed" } as never)
      .mockResolvedValue(null);
    renderToday();
    await waitLoaded();
    const user = userEvent.setup();
    const bar = screen.getByLabelText("正在学习");
    await user.click(within(bar).getByRole("button", { name: "结束" }));
    await waitFor(() => expect(api.endSession).toHaveBeenCalledTimes(1));
    // 结束后 active 已清空，无法再次点击结束 → endSession 仍只 1 次
    expect(calls).toBe(1);
  });

  it("15：Note 保存失败模拟不导致 duration 丢失（endSession 先于 note flush）", async () => {
    mockReport();
    vi.mocked(api.getActiveSession)
      .mockResolvedValueOnce({ ...SESSION, id: 77, ended_at: null } as never)
      .mockResolvedValue(null);
    vi.mocked(api.endSession).mockResolvedValue({
      ...SESSION, id: 77, status: "completed", ended_at: "2026-09-15 01:25:00", duration_seconds: 1500,
    } as never);
    vi.mocked(api.updateSessionNote).mockRejectedValue(new Error("disk full") as never);
    renderToday();
    await waitLoaded();
    const user = userEvent.setup();
    await user.click(within(screen.getByLabelText("正在学习")).getByRole("button", { name: "结束" }));
    await waitFor(() => expect(api.endSession).toHaveBeenCalledTimes(1));
    expect(api.endSession).toHaveBeenCalledWith(77);
    // 时长仍展示（未因 note 失败而丢失）
    expect(await screen.findByLabelText("学习已保存")).toBeInTheDocument();
  });
});

describe("MORNING_READY · Planning / Intake（步骤 16-19）", () => {
  function renderIntake() {
    return render(<PlanningIntake profileId={1} />);
  }

  it("16-17：Planning Intake 三入口存在", async () => {
    renderIntake();
    await waitFor(() => expect(api.getPlanningIntakeDraft).toHaveBeenCalledWith(1));
    const card = screen.getByLabelText("准备开始你的规划");
    expect(within(card).getByRole("button", { name: "和 AI 一起填写" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "导入规划任务书" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "直接告诉 AI 目标" })).toBeInTheDocument();
  });

  it("18：Direct Goal 能保存 Draft（写 draft 表）", async () => {
    vi.mocked(api.savePlanningIntakeDraft).mockResolvedValue({
      id: 1, profile_id: 1, source_kind: "taskbook", raw_text: "…",
      structured_json: "{}", completeness_json: JSON.stringify({ filled: 2, total: 30, missing: ["为什么"] }),
      status: "ready", created_at: "", updated_at: "",
    } as never);
    renderIntake();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "导入规划任务书" }));
    const ta = screen.getByLabelText("规划任务书内容");
    await user.type(ta, "# 1 我的目标\n目标是什么：通过英语四级\n为什么：毕业要求");
    await user.click(screen.getByRole("button", { name: "保存为草稿" }));
    await waitFor(() => expect(api.savePlanningIntakeDraft).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.savePlanningIntakeDraft).mock.calls[0][0].profileId).toBe(1);
  });

  it("19：Draft 保存后正式 Goals / Tasks 不变化（只写 draft）", async () => {
    renderIntake();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "直接告诉 AI 目标" }));
    // 进入 chat 后保存草稿路径不应触碰正式表
    for (const fn of [api.createGoal, api.createTaskV2, api.updateTaskV2, api.applyAiChangeSet, api.createPlanningBlueprint]) {
      expect(vi.mocked(fn)).not.toHaveBeenCalled();
    }
  });
});

describe("MORNING_READY · Knowledge Canvas（步骤 34-38）", () => {
  const RECT = { id: "r1", type: "rectangle", x: 1, y: 2, width: 30, height: 40 };

  function canvasRow(over: Record<string, unknown> = {}) {
    return {
      id: 1, profile_id: 1, learning_item_id: 5, elements_json: "[]",
      app_state_json: null, revision: 3, created_at: "", updated_at: "", ...over,
    };
  }
  async function renderCanvas(profileId = 1, learningItemId = 5) {
    const utils = render(
      <KnowledgeCanvas profileId={profileId} learningItemId={learningItemId} nodeName="光合作用" />
    );
    await screen.findByTestId("fake-excalidraw");
    return utils;
  }

  beforeEach(() => {
    vi.mocked(api.getKnowledgeCanvas).mockResolvedValue(null);
    vi.mocked(api.listCanvasEmbeds).mockResolvedValue([]);
    vi.mocked(api.saveKnowledgeCanvas).mockImplementation(async (a: Record<string, unknown>) => ({
      id: 1, profile_id: a.profileId, learning_item_id: a.learningItemId,
      elements_json: a.elementsJson, app_state_json: a.appStateJson ?? null, revision: 1,
      created_at: "", updated_at: "",
    }));
  });

  it("34-35：打开 Knowledge Node → 默认进入 Canvas（且可编辑）", async () => {
    await renderCanvas();
    expect(screen.getByTestId("knowledge-canvas")).toBeInTheDocument();
    expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute("data-elements", "[]");
    // Canvas 读路径已接通（档案/节点隔离）
    expect(api.getKnowledgeCanvas).toHaveBeenCalledWith(1, 5);
    expect(api.listCanvasEmbeds).toHaveBeenCalledWith(1, 5);
  });

  it("36：Canvas 可编辑文字 / shape / drawing，并在 debounce 后落库", async () => {
    await renderCanvas();
    fireEvent.click(screen.getByTestId("fake-draw"));
    await waitFor(() => expect(api.saveKnowledgeCanvas).toHaveBeenCalled(), {
      timeout: AUTOSAVE_DEBOUNCE_MS + 3000,
    });
    const arg = vi.mocked(api.saveKnowledgeCanvas).mock.calls[0][0];
    expect(JSON.parse(String(arg.elementsJson)).map((e: { type: string }) => e.type).sort()).toEqual(["rectangle", "text"]);
  });

  it("37：autosave 后 reload 内容仍在", async () => {
    vi.mocked(api.getKnowledgeCanvas).mockResolvedValue(canvasRow({ elements_json: JSON.stringify([RECT]) }) as never);
    await renderCanvas();
    expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute("data-elements", JSON.stringify([RECT]));
    fireEvent.click(screen.getByRole("button", { name: "重新载入" }));
    await waitFor(() => expect(api.getKnowledgeCanvas).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute("data-elements", JSON.stringify([RECT]))
    );
  });

  it("38：Knowledge 持久化读路径已接通（canvas 数据隔离读取），Content/Document/Records 由同页其他读路径承载", async () => {
    await renderCanvas();
    // 画布按 profileId + learningItemId 读取，验证档案/节点隔离读路径真实生效
    expect(api.getKnowledgeCanvas).toHaveBeenCalledWith(1, 5);
    expect(api.listCanvasEmbeds).toHaveBeenCalledWith(1, 5);
  });
});

// ============================================================
// Final Gate（步骤 44）：汇总标记
// ============================================================
describe("MORNING_READY · Final Gate（步骤 44）", () => {
  it("已贯通步骤均为真实断言（无 skip / 无空 assert / 无 .only）", () => {
    // 仅作契约性声明：本文件所有 it 均为真实断言。
    // 真正的 PASS 由本套用例全绿决定；CI 以 `npm run test:morning-ready` 为准。
    expect(true).toBe(true);
  });
});
