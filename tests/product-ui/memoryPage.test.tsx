import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CognitiveMemoryDashboard, CognitiveMemoryRow } from "../../src/types";

/**
 * COGNITIVE CORE V1.2 §25 / §33 —— Memory 页交互契约。
 *
 * 覆盖任务书 §33 锁定的 UI 断言中**属于 Memory 页**的部分：
 *   UI-12 Memory empty state has no demo rows
 *   （并同时守住 §25 的页面层级、§36 的「无编造数据」）
 *
 * 分层原则：记忆真值（压力分类 / 到期队列 / 理由顺序）由 Rust 集成测试
 * `memory_engine_v1`（含 ME-11）负责；本文件只验证 **UI 是否诚实地渲染**。
 */

vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = {
    activeProfile: Object.freeze({ id: 7, name: "测试档案", profile_type: "kaoyan" }),
    gate: Object.freeze({ phase: "active" }),
    refreshKey: 0,
  };
  return { useActiveProfile: () => ctx };
});

const H = vi.hoisted(() => ({ getMemoryDashboard: vi.fn() }));

vi.mock("../../src/api", () => ({
  getMemoryDashboard: H.getMemoryDashboard,
}));

import Memory, {
  formatMemoryReason,
  MEMORY_DUE_LIMIT,
  MEMORY_KIND_LABEL,
  MEMORY_STATUS_LABEL,
} from "../../src/pages/Memory";

// ============================ 夹具 ============================

function row(
  over: Partial<Omit<CognitiveMemoryRow, "unit">> & {
    id?: number;
    unit?: Partial<CognitiveMemoryRow["unit"]>;
  } = {}
): CognitiveMemoryRow {
  const { id = 1, unit: unitOver, ...rowOver } = over;
  const base: CognitiveMemoryRow = {
    unit: {
      id,
      profile_id: 7,
      linked_learning_item_id: 100 + id,
      memory_key: `k${id}`,
      memory_kind: "definition",
      stability: 12.5,
      difficulty: 4.2,
      retrievability: 0.93,
      last_review_at: "2026-09-01 00:00:00",
      next_review_at: "2026-09-20 00:00:00",
      desired_retention: 0.9,
      review_count: 3,
      lapse_count: 0,
      fsrs_state_json: {},
      created_at: "2026-08-01 00:00:00",
      updated_at: "2026-09-01 00:00:00",
    },
    learning_item_label: "一元二次方程求根公式",
    overdue_days: -2,
    status: "due",
  };
  return {
    ...base,
    ...rowOver,
    unit: { ...base.unit, ...(unitOver ?? {}) },
  };
}

function dashboard(over: Partial<CognitiveMemoryDashboard> = {}): CognitiveMemoryDashboard {
  return {
    profile_id: 7,
    generated_at: "2026-09-17 01:00:00",
    pressure: {
      total_units: 2,
      due_count: 1,
      high_risk_count: 1,
      next_due_at: "2026-09-18 00:00:00",
      oldest_due_at: "2026-09-15 00:00:00",
      status: "watch",
    },
    due_units: [],
    upcoming_units: [],
    rationale: [],
    ...over,
  };
}

function renderMemory() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <Memory />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function page(): HTMLElement {
  const el = document.querySelector(".hc-memory");
  expect(el).not.toBeNull();
  return el as HTMLElement;
}

async function waitLoaded() {
  await waitFor(() => {
    expect(screen.queryByText("正在读取你的记忆状态…")).not.toBeInTheDocument();
  });
}

beforeEach(() => {
  H.getMemoryDashboard.mockReset();
});

// ============================================================
// UI-12 — 空状态没有任何 demo 行
// ============================================================

describe("UI-12 — Memory empty state has no demo rows", () => {
  it("一条 MemoryUnit 都没有时，只渲染任务书锁定文案，且不存在任何数据行", async () => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        pressure: {
          total_units: 0,
          due_count: 0,
          high_risk_count: 0,
          next_due_at: null,
          oldest_due_at: null,
          status: "insufficient",
        },
      })
    );

    renderMemory();
    await waitLoaded();

    expect(
      screen.getByText("Higher 还没有足够的回忆记录来建立你的记忆节奏。")
    ).toBeInTheDocument();
    expect(
      screen.getByText("当你在学习中完成主动回忆后，这里会逐渐形成复习安排。")
    ).toBeInTheDocument();

    // 空状态**不得**出现任何队列 / 行 / 理由
    const root = page();
    expect(root.querySelectorAll(".hc-memrow")).toHaveLength(0);
    expect(root.querySelector(".hc-memlist")).toBeNull();
    expect(root.querySelector(".hc-memwhy")).toBeNull();
    expect(root.querySelector(".hc-memsum")).toBeNull();
  });

  it("即使是后端数据自相矛盾（insufficient 却带了行），UI 也必须走空状态且一行不渲染", async () => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        pressure: {
          total_units: 0,
          due_count: 0,
          high_risk_count: 0,
          next_due_at: null,
          oldest_due_at: null,
          status: "insufficient",
        },
        due_units: [row({ id: 1 }), row({ id: 2 })],
        upcoming_units: [row({ id: 3 })],
        rationale: [{ code: "memory_due", value: "due=2", trend: "caution" }],
      })
    );

    renderMemory();
    await waitLoaded();

    const root = page();
    expect(root.querySelectorAll(".hc-memrow")).toHaveLength(0);
    expect(screen.queryByText("一元二次方程求根公式")).not.toBeInTheDocument();
    expect(root.textContent).not.toContain("现在有 2 个知识点");
  });

  it("空状态不得出现任何百分比或编造的风险句式（§36）", async () => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        pressure: {
          total_units: 0,
          due_count: 0,
          high_risk_count: 0,
          next_due_at: null,
          oldest_due_at: null,
          status: "insufficient",
        },
      })
    );
    renderMemory();
    await waitLoaded();

    const text = page().textContent ?? "";
    expect(text).not.toContain("%");
    for (const forbidden of ["87%", "高遗忘风险", "记忆强度", "掌握度", "已 6 天"]) {
      expect(text).not.toContain(forbidden);
    }
  });
});

// ============================================================
// §25 — 有数据时的页面层级与「只展示真实字段」
// ============================================================

describe("§25 — Memory 页层级与真实字段", () => {
  beforeEach(() => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        due_units: [row({ id: 1, overdue_days: 3, status: "due" })],
        upcoming_units: [row({ id: 2, overdue_days: -2, status: "stable" })],
        rationale: [
          { code: "memory_due", value: "due=1", trend: "neutral" },
          { code: "memory_high_risk", value: "high_risk=1", trend: "caution" },
        ],
      })
    );
  });

  it("页面层级固定：压力摘要 → 现在需要复习 → 下一次复习 → 为什么现在复习", async () => {
    renderMemory();
    await waitLoaded();

    const root = page();
    const order = Array.from(
      root.querySelectorAll(".hc-memsum, .hc-memlist, .hc-memwhy")
    ).map((e) => e.className.split(" ")[0]);

    expect(order).toEqual(["hc-memsum", "hc-memlist", "hc-memlist", "hc-memwhy"]);
    expect(screen.getByRole("heading", { name: "现在需要复习" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "下一次复习" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "为什么现在复习" })).toBeInTheDocument();
  });

  it("行内只出现真实字段：学习项名 / 记忆种类 / 时间 / 复习次数 / 状态", async () => {
    renderMemory();
    await waitLoaded();

    const rows = page().querySelectorAll(".hc-memrow");
    expect(rows).toHaveLength(2);

    const first = rows[0];
    expect(first.querySelector(".hc-memrow__label")?.textContent).toBe("一元二次方程求根公式");
    expect(first.querySelector(".hc-memrow__kind")?.textContent).toBe(
      MEMORY_KIND_LABEL.definition
    );
    expect(first.querySelector(".hc-memrow__timing")?.textContent).toBe("已逾期 3 天");
    expect(first.querySelector(".hc-memrow__count")?.textContent).toBe("复习 3 次");
    expect(first.querySelector(".hc-memrow__status")?.textContent).toBe(
      MEMORY_STATUS_LABEL.due
    );
  });

  it("尚未到期的行不得被渲染成「已逾期」（overdue_days < 0 → 展示真实的下一次复习时间）", async () => {
    renderMemory();
    await waitLoaded();

    const rows = page().querySelectorAll(".hc-memrow");
    const upcoming = rows[1];
    const timing = upcoming.querySelector(".hc-memrow__timing")?.textContent ?? "";
    expect(timing).not.toContain("已逾期");
    expect(timing.trim().length).toBeGreaterThan(0);
  });

  it("压力摘要只用真实字段，并把「最早到期」与「下一次复习」区分开", async () => {
    renderMemory();
    await waitLoaded();

    expect(screen.getByText(/共 2 个知识点 · 到期 1 · 高风险 1/)).toBeInTheDocument();
    // 有到期 → 只能讲「最早到期」，绝不能把过去的时间标成「下一次」
    expect(screen.getByText(/最早到期：/)).toBeInTheDocument();
    expect(screen.queryByText(/下一次复习：/)).not.toBeInTheDocument();
  });

  it("理由逐条无损转成中文，且绝不泄漏机器 token", async () => {
    renderMemory();
    await waitLoaded();

    const why = page().querySelector(".hc-memwhy") as HTMLElement;
    expect(within(why).getByText("现在有 1 个知识点到了该复习的时间")).toBeInTheDocument();
    expect(within(why).getByText("有 1 个知识点记忆正在变弱")).toBeInTheDocument();

    const text = why.textContent ?? "";
    expect(text).not.toContain("due=");
    expect(text).not.toContain("high_risk=");
  });

  it("无法识别的理由码 / value 一律不渲染（宁可不显示，也不抛内部串）", async () => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        due_units: [row({ id: 1 })],
        rationale: [
          { code: "memory_unknown_code", value: "xyz=1", trend: "neutral" },
          { code: "memory_due", value: "not-a-token", trend: "neutral" },
        ],
      })
    );
    renderMemory();
    await waitLoaded();

    const why = page().querySelector(".hc-memwhy") as HTMLElement;
    expect(why.querySelectorAll(".hc-memwhy__item")).toHaveLength(0);
    expect(why.textContent).not.toContain("memory_unknown_code");
    expect(why.textContent).not.toContain("xyz=1");
  });

  it("取不到真实学习项名时不回退成内部编号", async () => {
    H.getMemoryDashboard.mockResolvedValue(
      dashboard({
        due_units: [row({ id: 1, learning_item_label: null })],
      })
    );
    renderMemory();
    await waitLoaded();

    const label = page().querySelector(".hc-memrow__label")?.textContent ?? "";
    expect(label).toBe("（未命名学习项）");
    expect(label).not.toMatch(/\d/);
  });

  it("整页不出现百分比，也不出现「掌握度」这类评分语言（§25 / §36）", async () => {
    renderMemory();
    await waitLoaded();

    const text = page().textContent ?? "";
    expect(text).not.toContain("%");
    for (const forbidden of ["掌握度", "熟练度", "评分", "得分", "综合效率"]) {
      expect(text).not.toContain(forbidden);
    }
  });

  it("到期列表请求上限遵循 §25 的「初始最多 20 条」", async () => {
    renderMemory();
    await waitLoaded();
    expect(MEMORY_DUE_LIMIT).toBe(20);
    expect(H.getMemoryDashboard).toHaveBeenCalledWith(7, 20);
  });
});

// ============================================================
// 错误路径
// ============================================================

describe("§25 — 读取失败时的诚实呈现", () => {
  it("失败时给出错误 + 重试，且不渲染任何伪造数据", async () => {
    H.getMemoryDashboard.mockRejectedValue(new Error("数据库忙"));
    renderMemory();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("数据库忙");
    expect(page().querySelectorAll(".hc-memrow")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "重新尝试" })).toBeInTheDocument();
  });
});

// ============================================================
// 格式化函数的纯单测
// ============================================================

describe("formatMemoryReason — 机器 token 的无损转换", () => {
  it("识别三种已知理由码", () => {
    expect(formatMemoryReason("memory_due", "due=3")).toBe("现在有 3 个知识点到了该复习的时间");
    expect(formatMemoryReason("memory_high_risk", "high_risk=2")).toBe(
      "有 2 个知识点记忆正在变弱"
    );
    expect(formatMemoryReason("memory_calm", null)).toBe(
      "现在没有到期的记忆，按当前节奏继续就好"
    );
  });

  it("非法 / 越界 / 未知一律返回 null（绝不编造）", () => {
    expect(formatMemoryReason("memory_due", null)).toBeNull();
    expect(formatMemoryReason("memory_due", "due=0")).toBeNull();
    expect(formatMemoryReason("memory_due", "total=3 due=1")).toBeNull();
    expect(formatMemoryReason("memory_high_risk", "high_risk=-1")).toBeNull();
    expect(formatMemoryReason("something_else", "x")).toBeNull();
  });
});
