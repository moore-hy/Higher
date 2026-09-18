import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type {
  BlockCompletionState,
  GroundedMaterialView,
  TrainingBlockRun,
  TrainingRun,
  TrainingSessionView,
} from "../../src/types";

/**
 * GROUNDED LEARNING BRIDGE V1 · P4.4 —— **重开恢复**（页面级）。
 *
 * ```text
 * TrainingRun exists
 * app/page reloads
 * ↓
 * Today → Continue
 * ↓
 * same run · same current block · same persisted snapshot
 * ```
 *
 * # 这一条为什么必须在**页面**层面验
 *
 * 「续接路由回到哪条训练」由 `groundedTrainingRouting.test.tsx`（GB-ROUTE）负责；
 * 「快照是否真的落库」由 Rust 侧负责。这里盯的是剩下那一件事，也是 P4.4 的原话：
 *
 * > No recomputation of grounded material for an already-created block.
 * > The snapshot is historical truth.
 *
 * 页面重开的正确行为是**纯读**：读回那条已存在的 run、读回它已经指向的当前块、
 * 读回那个块**已经落库的**快照，然后原样呈现。任何「看起来更合理」的重算、
 * 重排、重启、重置都会被这条用例抓住。
 *
 * # 「重开」在测试里就是换一个空的 QueryClient
 *
 * 桌面应用重启 = 进程内存里的缓存全没了。所以每次 `mount()` 都新建 QueryClient，
 * 不留任何跨次缓存 —— 这正是真实重开时页面能看到的东西。
 */

vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = { activeProfile: Object.freeze({ id: 7, name: "测试档案" }) };
  return {
    useActiveProfile: () => ctx,
    ActiveProfileProvider: (props: { children?: unknown }) => props.children,
    canSwitchProfile: () => true,
  };
});

vi.mock("../../src/api", () => ({
  getTrainingSession: vi.fn(),
  getBlockGroundedMaterial: vi.fn(),
  startTrainingRun: vi.fn(),
  startTrainingBlock: vi.fn(),
  recordTrainingInteraction: vi.fn(),
  advanceTrainingBlock: vi.fn(),
  completeTrainingRun: vi.fn(),
  abandonTrainingRun: vi.fn(),
}));

import * as api from "../../src/api";
import TrainingExperience from "../../src/pages/TrainingExperience";

// ============================ fixtures ============================

const PROFILE_ID = 7;
const RUN_ID = 42;

const EXCERPT = "光合作用把光能转成化学能，并释放氧气。";
const REFERENCE = "叶绿体是光合作用的主要场所。";
const CUE = "叶绿体";

const GOAL_A = "先回忆一遍光合作用";
const GOAL_B = "按线索补全光反应";

/** 已完成的第 1 段（`ordinal = 1`）。 */
const BLOCK_A: TrainingBlockRun = {
  id: 101,
  profile_id: PROFILE_ID,
  training_run_id: RUN_ID,
  ordinal: 1,
  protocol_id: "free_recall",
  is_break: false,
  goal: GOAL_A,
  planned_minutes: 8,
  memory_unit_id: 5,
  status: "completed",
  started_at: "2026-09-18 09:00:00",
  ended_at: "2026-09-18 09:08:00",
  created_at: "2026-09-18 08:59:00",
  updated_at: "2026-09-18 09:08:00",
};

/** 正在进行中的第 2 段（`ordinal = 2`）—— 重开后必须回到这一块。 */
const BLOCK_B: TrainingBlockRun = {
  ...BLOCK_A,
  id: 202,
  ordinal: 2,
  protocol_id: "cued_recall",
  goal: GOAL_B,
  planned_minutes: 12,
  status: "active",
  started_at: "2026-09-18 09:09:00",
  ended_at: null,
};

const RUN: TrainingRun = {
  id: RUN_ID,
  profile_id: PROFILE_ID,
  study_session_id: 77,
  learning_item_id: 9,
  mode: "copilot",
  status: "active",
  current_block_ordinal: 2,
  plan_snapshot_json: "{}",
  started_at: "2026-09-18 09:00:00",
  ended_at: null,
  created_at: "2026-09-18 08:59:00",
  updated_at: "2026-09-18 09:09:00",
};

const COMPLETIONS: BlockCompletionState[] = [
  {
    block_run_id: BLOCK_A.id,
    rule_kind: "at_least_one_recall_outcome",
    rule_zh: "至少有一次真实的回忆结果",
    satisfied: true,
    reason: "rule_satisfied_by_interaction_outcome",
  },
  {
    block_run_id: BLOCK_B.id,
    rule_kind: "at_least_one_recall_outcome",
    rule_zh: "至少有一次真实的回忆结果",
    satisfied: false,
    reason: "no_qualifying_outcome_yet",
  },
];

/** 该块**已经落库**的接地材料快照。 */
const PERSISTED_MATERIAL: GroundedMaterialView = {
  material: {
    version: 1,
    status: "ready",
    protocol_id: "cued_recall",
    prompt_text: null,
    cue_text: CUE,
    source_excerpt: EXCERPT,
    reference_text: REFERENCE,
    worked_steps: [],
    hidden_step_index: null,
    practice_prompt: null,
    transfer_prompt: null,
    generated_by: "deterministic",
    provenance: [{ source_id: 71, revision_id: 72, section_id: 73, chunk_id: 74 }],
    unavailable_reason: null,
  },
  provenance_labels: [
    { source_id: 71, display_name: "生物笔记.md", section_id: 73, section_title: "第三章" },
  ],
};

function session(over: Partial<TrainingSessionView> = {}): TrainingSessionView {
  return {
    run: RUN,
    blocks: [BLOCK_A, BLOCK_B],
    interactions: [],
    completions: COMPLETIONS,
    ...over,
  };
}

// ============================ harness ============================

/** 模拟一次「应用重开」：全新 QueryClient，没有任何内存缓存可依赖。 */
function mount(
  sessionView: TrainingSessionView = session(),
  material: GroundedMaterialView = PERSISTED_MATERIAL,
) {
  vi.mocked(api.getTrainingSession).mockResolvedValue(sessionView);
  vi.mocked(api.getBlockGroundedMaterial).mockResolvedValue(material);

  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/train/${RUN_ID}`]}>
        <Routes>
          <Route path="/train/:trainingRunId" element={<TrainingExperience />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function waitLoaded() {
  await waitFor(() => expect(screen.queryByText("正在读取这次训练…")).not.toBeInTheDocument());
}

/** 页面在「重开」时**唯一**被允许调用的写入口列表。 */
const WRITE_APIS = [
  "startTrainingRun",
  "startTrainingBlock",
  "recordTrainingInteraction",
  "advanceTrainingBlock",
  "completeTrainingRun",
  "abandonTrainingRun",
] as const;

function expectNoWrites() {
  for (const name of WRITE_APIS) {
    expect(api[name], `重开不得调用写入口 ${name}`).not.toHaveBeenCalled();
  }
}

// ============================ P4-4a ============================

describe("P4.4 重开恢复 · 读的是落库事实", () => {
  it("P4-4a：重开只读回这条 run 与**当前块**的已落库快照，且一个写入口都不碰", async () => {
    mount();
    await waitLoaded();

    // 读的是 URL 里那条 run，不是「重新开一条」。
    expect(api.getTrainingSession).toHaveBeenCalledWith(PROFILE_ID, RUN_ID);
    expect(api.getTrainingSession).toHaveBeenCalledTimes(1);

    // 关键：材料是为**当前块（ordinal 2 → id 202）**读的，而且只读一次。
    await waitFor(() => expect(api.getBlockGroundedMaterial).toHaveBeenCalledTimes(1));
    expect(api.getBlockGroundedMaterial).toHaveBeenCalledWith(PROFILE_ID, BLOCK_B.id);
    // 已经结束的块不该在重开时被顺手重算。
    expect(api.getBlockGroundedMaterial).not.toHaveBeenCalledWith(PROFILE_ID, BLOCK_A.id);

    // 重开 = 纯读。重启 run / 重启块 / 写交互 / 推进 / 收口，一个都不该发生。
    expectNoWrites();

    // run 已经是 active，所以界面上不该出现「开始」（那是 ready 才有的重启入口）。
    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
  });
});

// ============================ P4-4b ============================

describe("P4.4 重开恢复 · 界面与落库事实一致", () => {
  it("P4-4b：回到同一个当前块，落库线索照常呈现，完整答案仍保持隐藏", async () => {
    mount();
    await waitLoaded();

    // 同一个当前块：它的目标就是选中的那一段（且只出现一次）。
    expect(screen.getAllByRole("heading", { level: 2, name: GOAL_B })).toHaveLength(1);

    // 落库快照被真实消费：线索来自 cue_text，不是前端编的。
    expect(await screen.findByTestId("hc-train-cue")).toHaveTextContent(CUE);

    // 诚实性在重开之后**不退化**：没有真实尝试之前，完整答案与来源不得出现。
    expect(screen.queryByTestId("hc-train-revealed-excerpt")).not.toBeInTheDocument();
    expect(screen.queryByTestId("hc-train-revealed-reference")).not.toBeInTheDocument();
    expect(screen.queryByText(EXCERPT)).not.toBeInTheDocument();
    expect(screen.queryByText(REFERENCE)).not.toBeInTheDocument();

    // 完成契约也来自后端：未满足时必须如实说「还不能算完成」。
    expect(screen.getByText("还没有符合要求的结果，所以现在还不能算完成。")).toBeInTheDocument();
  });

  it("P4-4b2：落库快照是 unavailable 时，重开也不编内容 —— 如实走无线索兜底", async () => {
    mount(session(), {
      material: {
        version: 1,
        status: "unavailable",
        protocol_id: "cued_recall",
        prompt_text: null,
        cue_text: null,
        source_excerpt: null,
        reference_text: null,
        worked_steps: [],
        hidden_step_index: null,
        practice_prompt: null,
        transfer_prompt: null,
        generated_by: "none",
        provenance: [],
        unavailable_reason: "这个块没有可用的接地材料",
      },
      provenance_labels: [],
    });
    await waitLoaded();

    // 没有 cue → 没有编造的线索行。
    expect(screen.queryByTestId("hc-train-cue")).not.toBeInTheDocument();
    expect(
      screen.getByText(
        /这个块没有绑定更具体的线索文本，所以上面只是这次编排的目标 ——\s*Higher 不会替你编一条线索。/,
      ),
    ).toBeInTheDocument();

    // 材料不可用不等于可以「补一份」。
    expect(screen.queryByText(EXCERPT)).not.toBeInTheDocument();
    expect(screen.queryByText(REFERENCE)).not.toBeInTheDocument();
    expectNoWrites();
  });
});

// ============================ P4-4c ============================

describe("P4.4 重开恢复 · 快照是历史真值", () => {
  it("P4-4c：连续两次重开渲染完全相同 —— 不做重算，也不做「更好的」重排", async () => {
    const first = mount();
    await waitLoaded();
    const cueFirst = (await screen.findByTestId("hc-train-cue")).textContent;
    const blocksFirst = screen
      .getAllByRole("listitem")
      .map((li) => li.textContent ?? "")
      .join(" | ");
    first.unmount();

    // 第二次重开：同样的落库事实，必须得到同样的界面。
    const second = mount();
    await waitLoaded();
    const cueSecond = (await screen.findByTestId("hc-train-cue")).textContent;
    const blocksSecond = screen
      .getAllByRole("listitem")
      .map((li) => li.textContent ?? "")
      .join(" | ");
    second.unmount();

    expect(cueSecond).toBe(cueFirst);
    expect(blocksSecond).toBe(blocksFirst);

    // 每次重开各读一次同一个块；两次之间没有任何第二次「生成」。
    expect(api.getBlockGroundedMaterial).toHaveBeenCalledTimes(2);
    expect(api.getBlockGroundedMaterial).toHaveBeenNthCalledWith(1, PROFILE_ID, BLOCK_B.id);
    expect(api.getBlockGroundedMaterial).toHaveBeenNthCalledWith(2, PROFILE_ID, BLOCK_B.id);
    expectNoWrites();
  });
});

// ============================ P4-4d ============================

describe("P4.4 重开恢复 · 进度不被重置", () => {
  it("P4-4d：重开停在已走到的第 2 段，不回到第 1 段、不出现「开始这一段」", async () => {
    mount();
    await waitLoaded();
    await waitFor(() => expect(api.getBlockGroundedMaterial).toHaveBeenCalled());

    // 编排里的两段都在，但「当前」只属于 ordinal 2。
    expect(screen.getByText(GOAL_A)).toBeInTheDocument();
    // GOAL_B 会出现两次：计划列表里的那一段 + 下方选中段的标题。
    expect(screen.getAllByText(GOAL_B)).toHaveLength(2);
    expect(screen.getByText(/· 当前$/)).toBeInTheDocument();
    expect(screen.getAllByText(/· 当前$/)).toHaveLength(1);

    // 第 1 段已完成 —— 重开不得把它重新激活。
    expect(screen.getByText(/8 分钟 · 已完成/)).toBeInTheDocument();
    expect(screen.getByText(/12 分钟 · 进行中/)).toBeInTheDocument();

    // 当前块已经是 active，所以「开始这一段」（pending 才有的激活入口）不该出现。
    expect(screen.queryByRole("button", { name: "开始这一段" })).not.toBeInTheDocument();

    // 提交控件必须可用：当前 + 活跃 + run 活跃 三者同时成立（否则界面在骗用户）。
    expect(screen.getByRole("button", { name: "提交" })).toBeEnabled();

    expectNoWrites();
  });
});
