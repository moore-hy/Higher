import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

/**
 * PRODUCT-2.0 §24 / §26.3 / §64 —— Planning Intake 交互契约。
 *
 * Critical UI（§64 禁止 PENDING HUMAN）：入口按钮点击、草稿保存。
 *
 * 硬断言：**Intake 只写草稿**——绝不触碰 goals / tasks / planning_blueprints。
 * 正式写入只能发生在 ChangeSet 预览 + 用户确认之后（§26.1 / §26.3）。
 */

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

vi.mock("../../src/components/ai/AiPanelContext", () => {
  const panel = {
    runAction: () => Promise.resolve(),
    setPageContext: () => {},
    sendChat: vi.fn(async () => {}),
  };
  return { useAiPanel: () => panel, AiPanelProvider: (p: { children?: unknown }) => p.children };
});

vi.mock("../../src/api", () => ({
  getPlanningIntakeDraft: vi.fn(async () => null),
  savePlanningIntakeDraft: vi.fn(),
  setPlanningIntakeStatus: vi.fn(async () => undefined),
  discardPlanningIntakeDraft: vi.fn(async () => undefined),
  importPersonalizationFiles: vi.fn(async () => []),
  writeExportFile: vi.fn(async () => undefined),
  // 以下一律不得被调用（正式数据写入路径）
  createGoal: vi.fn(),
  createTaskV2: vi.fn(),
  updateTaskV2: vi.fn(),
  applyAiChangeSet: vi.fn(),
  createPlanningBlueprint: vi.fn(),
}));

import * as api from "../../src/api";
import { useAiPanel } from "../../src/components/ai/AiPanelContext";
import PlanningIntake from "../../src/components/PlanningIntake";

const FORMAL_WRITE_FNS = [
  api.createGoal,
  api.createTaskV2,
  api.updateTaskV2,
  api.applyAiChangeSet,
  api.createPlanningBlueprint,
] as const;

function expectNoFormalWrites() {
  for (const fn of FORMAL_WRITE_FNS) {
    expect(vi.mocked(fn)).not.toHaveBeenCalled();
  }
}

describe("§24.1 空规划页 — 三个入口", () => {
  it("显示「准备开始你的规划」并给出三个入口，主按钮为「和 AI 一起填写」", async () => {
    render(<PlanningIntake profileId={1} />);
    await waitFor(() => expect(api.getPlanningIntakeDraft).toHaveBeenCalledWith(1));

    const card = screen.getByLabelText("准备开始你的规划");
    expect(within(card).getByText("准备开始你的规划")).toBeInTheDocument();

    const primary = within(card).getByRole("button", { name: "和 AI 一起填写" });
    expect(primary.className).toContain("btn--primary");
    expect(within(card).getByRole("button", { name: "导入规划任务书" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "直接告诉 AI 目标" })).toBeInTheDocument();
  });

  it("「和 AI 一起填写」一击 → 打开 AI 面板（sendChat），不产生任何正式写入", async () => {
    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "和 AI 一起填写" }));

    const { sendChat } = useAiPanel();
    await waitFor(() => expect(vi.mocked(sendChat)).toHaveBeenCalled());
    expectNoFormalWrites();
  });

  it("「直接告诉 AI 目标」一击 → sendChat，不产生任何正式写入", async () => {
    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "直接告诉 AI 目标" }));

    const { sendChat } = useAiPanel();
    await waitFor(() => expect(vi.mocked(sendChat)).toHaveBeenCalled());
    expectNoFormalWrites();
  });
});

describe("§24.2 / §24.4 导入规划任务书", () => {
  it("粘贴任务书 → 保存为草稿：写 structured_json + completeness_json，且只写 draft 表", async () => {
    vi.mocked(api.savePlanningIntakeDraft).mockResolvedValue({
      id: 1,
      profile_id: 1,
      source_kind: "taskbook",
      raw_text: "…",
      structured_json: "{}",
      completeness_json: JSON.stringify({ filled: 2, total: 30, missing: ["为什么"] }),
      status: "ready",
      created_at: "2026-09-15 00:00:00",
      updated_at: "2026-09-15 00:00:00",
    } as never);

    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "导入规划任务书" }));
    const ta = screen.getByLabelText("规划任务书内容");
    await user.type(ta, "# 1 我的目标\n目标是什么：通过英语四级\n为什么：毕业要求");
    await user.click(screen.getByRole("button", { name: "保存为草稿" }));

    await waitFor(() => expect(api.savePlanningIntakeDraft).toHaveBeenCalledTimes(1));
    const payload = vi.mocked(api.savePlanningIntakeDraft).mock.calls[0][0] as Record<
      string,
      unknown
    >;
    expect(payload.profileId).toBe(1);
    expect(payload.sourceKind).toBe("taskbook");
    expect(payload.status).toBe("ready");
    expect(typeof payload.structuredJson).toBe("string");
    expect(typeof payload.completenessJson).toBe("string");
    // 解析结果确实是结构化的
    const structured = JSON.parse(payload.structuredJson as string);
    expect(structured.sections["1 我的目标"]["目标是什么"]).toBe("通过英语四级");

    // 关键：正式数据零写入
    expectNoFormalWrites();
  });

  it("空内容 → 提示且不写草稿", async () => {
    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "导入规划任务书" }));
    await user.click(screen.getByRole("button", { name: "保存为草稿" }));

    await waitFor(() => expect(screen.getByText("请先粘贴任务书内容。")).toBeInTheDocument());
    expect(api.savePlanningIntakeDraft).not.toHaveBeenCalled();
    expectNoFormalWrites();
  });

  it("提示支持 .md / .txt / .docx / .pdf（§24.4）", async () => {
    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "导入规划任务书" }));
    expect(screen.getByText(/\.md \/ \.txt \/ \.docx \/ \.pdf/)).toBeInTheDocument();
  });
});

describe("§24.2 模板下载", () => {
  it("「下载任务书模板」取消保存 → 不写文件、不报错", async () => {
    render(<PlanningIntake profileId={1} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "下载任务书模板" }));

    await waitFor(() => expect(api.writeExportFile).not.toHaveBeenCalled());
    expectNoFormalWrites();
  });
});

describe("草稿展示与丢弃", () => {
  it("已有草稿 → 展示来源/状态/完成度与缺口字段", async () => {
    vi.mocked(api.getPlanningIntakeDraft).mockResolvedValue({
      id: 9,
      profile_id: 1,
      source_kind: "taskbook",
      raw_text: "…",
      structured_json: "{}",
      completeness_json: JSON.stringify({
        filled: 28,
        total: 30,
        missing: ["目标日期", "预算"],
      }),
      status: "ready",
      created_at: "2026-09-15 00:00:00",
      updated_at: "2026-09-15 00:00:00",
    } as never);

    render(<PlanningIntake profileId={1} />);

    expect(await screen.findByText("当前草稿")).toBeInTheDocument();
    expect(screen.getByText(/来源 规划任务书/)).toBeInTheDocument();
    expect(screen.getByText(/完成 28\/30/)).toBeInTheDocument();
    expect(screen.getByText(/还缺：目标日期、预算/)).toBeInTheDocument();
  });

  it("「丢弃草稿」只删草稿，不触碰正式计划", async () => {
    vi.mocked(api.getPlanningIntakeDraft).mockResolvedValue({
      id: 9,
      profile_id: 1,
      source_kind: "chat",
      raw_text: null,
      structured_json: null,
      completeness_json: null,
      status: "draft",
      created_at: "2026-09-15 00:00:00",
      updated_at: "2026-09-15 00:00:00",
    } as never);
    const onChanged = vi.fn();

    render(<PlanningIntake profileId={1} onChanged={onChanged} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "丢弃草稿" }));

    await waitFor(() => expect(api.discardPlanningIntakeDraft).toHaveBeenCalledWith(1));
    expectNoFormalWrites();
    expect(onChanged).toHaveBeenCalled();
  });
});
