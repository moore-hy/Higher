import { readFileSync } from "node:fs";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CognitiveProgressView } from "../../src/types";

/**
 * COGNITIVE CORE V1.2 §26 / §33 —— Progress 页（四轴）交互契约。
 *
 * 覆盖任务书 §33 锁定的 UI 断言中**属于 Progress 页**的部分：
 *   UI-13 Progress has no global efficiency/mastery score
 *
 * 并同时守住 §26 的四条硬约束：
 *   1. 四轴固定且顺序固定；
 *   2. 证据不足的轴显示「证据不足」，**不画假图**；
 *   3. 任何图表都走既有 `recharts`（结构断言）；
 *   4. 轴口径披露走 Radix 原语（结构 + 交互断言）。
 *
 * 真值由 Rust 侧负责（同源投影 + memory_engine_v1 ME-11）；本文件只验 UI。
 */

vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = {
    activeProfile: Object.freeze({ id: 7, name: "测试档案", profile_type: "kaoyan" }),
    gate: Object.freeze({ phase: "active" }),
    refreshKey: 0,
  };
  return { useActiveProfile: () => ctx };
});

const H = vi.hoisted(() => ({ getCognitiveProgress: vi.fn() }));

vi.mock("../../src/api", () => ({
  getCognitiveProgress: H.getCognitiveProgress,
}));

import CognitiveProgress, {
  PROGRESS_AXES,
  PROGRESS_AXIS_EXPLAIN,
  PROGRESS_DIFFICULTY_ZH,
} from "../../src/pages/CognitiveProgress";

const SRC = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");

// ============================ 夹具 ============================

function view(over: Partial<CognitiveProgressView> = {}): CognitiveProgressView {
  const base: CognitiveProgressView = {
    profile_id: 7,
    generated_at: "2026-09-17 01:00:00",
    window_days: 30,
    volume: {
      available: true,
      observed_minutes_7d: 180,
      observed_minutes_30d: 760,
      active_days_30d: 14,
      nested_windows: true,
      reason_code: null,
    },
    quality: {
      available: true,
      recall_success: 12,
      recall_partial: 5,
      recall_failure: 3,
      hint_requests: 4,
      hint_uses: 4,
      reason_code: null,
    },
    difficulty: {
      available: false,
      buckets: [],
      reason_code: "no_protocol_sessions",
    },
    adaptation: {
      available: true,
      recall_to_independent: 2,
      application_to_independent: 1,
      acquisition_to_understood: 3,
      items_improved: 4,
      items_examined: 9,
      reason_code: null,
    },
  };
  return { ...base, ...over };
}

function renderProgress() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <CognitiveProgress />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function page(): HTMLElement {
  const el = document.querySelector(".hc-progress");
  expect(el).not.toBeNull();
  return el as HTMLElement;
}

async function waitLoaded() {
  await waitFor(() => {
    expect(screen.queryByText("正在整理学习数据…")).not.toBeInTheDocument();
  });
}

beforeEach(() => {
  H.getCognitiveProgress.mockReset();
  H.getCognitiveProgress.mockResolvedValue(view());
});

// ============================================================
// UI-13 — 没有全局效率 / 掌握度分数
// ============================================================

describe("UI-13 — Progress has no global efficiency/mastery score", () => {
  it("渲染结果里没有百分比，也没有任何总分 / 综合效率 / 掌握度语言", async () => {
    renderProgress();
    await waitLoaded();

    const text = page().textContent ?? "";
    expect(text).not.toContain("%");
    for (const forbidden of [
      "综合效率",
      "效率分",
      "总分",
      "总评",
      "掌握度",
      "熟练度",
      "得分",
      "评级",
      "Score",
      "score",
    ]) {
      expect(text).not.toContain(forbidden);
    }
  });

  it("前端 DTO 没有任何跨轴聚合字段（结构断言）", () => {
    const ts = SRC("../../src/types.ts");
    const block =
      /export interface CognitiveProgressView \{[\s\S]*?\n\}/.exec(ts)?.[0] ?? "";
    expect(block).not.toBe("");
    const fields = Array.from(block.matchAll(/^\s{2}(\w+)\??:/gm)).map((m) => m[1]);
    expect(fields).toContain("volume");
    expect(fields).toContain("quality");
    expect(fields).toContain("difficulty");
    expect(fields).toContain("adaptation");
    for (const f of fields) {
      expect(f).not.toMatch(/score|efficiency|overall|mastery|grade|rating|total/i);
    }
  });

  it("后端视图同样没有任何跨轴聚合字段（结构断言）", () => {
    const rs = SRC("../../src-tauri/src/cognitive/progress_projection.rs");
    const block =
      /pub struct CognitiveProgressView \{[\s\S]*?\n\}/.exec(rs)?.[0] ?? "";
    expect(block).not.toBe("");
    const fields = Array.from(block.matchAll(/pub (\w+):/g)).map((m) => m[1]);
    expect(fields).toContain("volume");
    expect(fields).toContain("quality");
    expect(fields).toContain("difficulty");
    expect(fields).toContain("adaptation");
    for (const f of fields) {
      expect(f).not.toMatch(/score|efficiency|overall|mastery|grade|rating/i);
    }
  });

  it("四条轴固定且顺序固定（Volume → Difficulty → Quality → Adaptation）", async () => {
    renderProgress();
    await waitLoaded();

    expect(PROGRESS_AXES.map((a) => a.key)).toEqual([
      "volume",
      "difficulty",
      "quality",
      "adaptation",
    ]);

    const rendered = Array.from(page().querySelectorAll("[data-axis]")).map((e) =>
      e.getAttribute("data-axis")
    );
    expect(rendered).toEqual(["volume", "difficulty", "quality", "adaptation"]);

    for (const axis of PROGRESS_AXES) {
      expect(screen.getByRole("heading", { name: new RegExp(axis.title) })).toBeInTheDocument();
    }
  });
});

// ============================================================
// §26 — 证据不足的轴不画假图
// ============================================================

describe("§26 — 证据不足的轴只显示「证据不足」", () => {
  it("Difficulty 在窗口内没有完成过训练块时显示证据不足 + 真实原因，且没有图表", async () => {
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="difficulty"]') as HTMLElement;
    expect(axis.dataset.available).toBe("false");
    expect(within(axis).getByText("证据不足")).toBeInTheDocument();
    // W7：训练块已经会真实落库，因此这句话必须说「本窗口没有完成过训练块」，
    // 而不能再说「协议会话还没有开始记录」（那是一句已经过期的假话）。
    expect(within(axis).getByText(/这个窗口内还没有完成过训练块/)).toBeInTheDocument();
    expect(axis.querySelector(".hc-chart")).toBeNull();
  });

  it("Adaptation 没有历史证据时显示证据不足，绝不把「没有证据」说成「没有进步」", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        adaptation: {
          available: false,
          recall_to_independent: 0,
          application_to_independent: 0,
          acquisition_to_understood: 0,
          items_improved: 0,
          items_examined: 0,
          reason_code: "no_historical_evidence",
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="adaptation"]') as HTMLElement;
    expect(axis.dataset.available).toBe("false");
    expect(within(axis).getByText("证据不足")).toBeInTheDocument();
    expect(within(axis).getByText(/还没有足够久的历史证据/)).toBeInTheDocument();
    expect(axis.textContent).not.toContain("没有进步");
  });

  it("未知理由码不渲染任何说明文本（绝不抛机器串）", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        difficulty: {
          available: false,
          buckets: [],
          reason_code: "weird_code" as never,
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="difficulty"]') as HTMLElement;
    expect(within(axis).getByText("证据不足")).toBeInTheDocument();
    expect(axis.textContent).not.toContain("weird_code");
    expect(axis.querySelector(".hc-axis__missing-hint")).toBeNull();
  });
});

// ============================================================
// W7 §12 — Difficulty 来自**真实完成**的训练块
// ============================================================

describe("W7 §12 — Difficulty 展示真实难度分布", () => {
  it("有合规块 → 画真实分布，且不再声称证据不足", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        difficulty: {
          available: true,
          buckets: [
            { difficulty: "light", count: 2 },
            { difficulty: "medium", count: 0 },
            { difficulty: "high", count: 1 },
          ],
          reason_code: null,
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="difficulty"]') as HTMLElement;
    expect(axis.dataset.available).toBe("true");
    expect(within(axis).queryByText("证据不足")).toBeNull();
    expect(axis.querySelector(".hc-chart")).not.toBeNull();
  });

  it("三个锁定档位有人话标签，且不给未知档位编造名字", () => {
    // §14 冻结的三档 —— 与 Rust `ProtocolDifficulty` 的 snake_case 一一对应。
    expect(PROGRESS_DIFFICULTY_ZH).toEqual({
      light: "轻度",
      medium: "中度",
      high: "重度",
    });
    // 后端没有声明的档位，前端**不猜**（返回 undefined → 渲染为空，
    // 而不是临时编一个名字造出一个假档位）。
    expect(PROGRESS_DIFFICULTY_ZH["impossible_level"]).toBeUndefined();
  });

  it("Difficulty 渲染走人话映射，绝不把 light/medium/high 原样画给用户", () => {
    const ts = SRC("../../src/pages/CognitiveProgress.tsx");
    // 标签必须经过映射表。
    expect(ts).toContain("PROGRESS_DIFFICULTY_ZH[b.difficulty]");
    // 且不允许出现旧写法（直接渲染后端 key）。
    expect(ts).not.toContain("name: b.difficulty");
  });

  it("口径说明已更新为「真实完成的训练块」，不再说协议会话尚未记录", () => {
    expect(PROGRESS_AXIS_EXPLAIN.difficulty).toContain("真实完成");
    expect(PROGRESS_AXIS_EXPLAIN.difficulty).not.toContain("才会出现分布");
  });
});

// ============================================================
// §26 / §36 — Volume 只用真实观测分钟
// ============================================================

describe("§26 — Volume 轴的诚实呈现", () => {
  it("有真实观测分钟时逐项展示，并明示 30 天窗口包含 7 天窗口", async () => {
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="volume"]') as HTMLElement;
    expect(axis.dataset.available).toBe("true");
    expect(within(axis).getByText("180 分钟")).toBeInTheDocument();
    expect(within(axis).getByText("760 分钟")).toBeInTheDocument();
    expect(within(axis).getByText("14 天")).toBeInTheDocument();
    expect(within(axis).getByText(/包含近 7 天/)).toBeInTheDocument();
  });

  it("窗口内没有有效记录时显示「暂无记录」，绝不显示「0 分钟」", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        volume: {
          available: true,
          observed_minutes_7d: null,
          observed_minutes_30d: 45,
          active_days_30d: 1,
          nested_windows: true,
          reason_code: null,
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="volume"]') as HTMLElement;
    expect(within(axis).getByText("暂无记录")).toBeInTheDocument();
    expect(within(axis).queryByText("0 分钟")).not.toBeInTheDocument();
  });

  it("完全没有学习记录时 Volume 也走「证据不足」", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        volume: {
          available: false,
          observed_minutes_7d: null,
          observed_minutes_30d: null,
          active_days_30d: 0,
          nested_windows: true,
          reason_code: "no_observed_sessions",
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="volume"]') as HTMLElement;
    expect(axis.dataset.available).toBe("false");
    expect(within(axis).getByText(/还没有完成过学习会话/)).toBeInTheDocument();
  });
});

// ============================================================
// §26 — Quality 轴
// ============================================================

describe("§26 — Quality 轴只用真实回忆证据", () => {
  it("逐项展示成功 / 部分 / 失败与提示使用次数", async () => {
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="quality"]') as HTMLElement;
    expect(within(axis).getByText("12 次")).toBeInTheDocument();
    expect(within(axis).getByText("5 次")).toBeInTheDocument();
    expect(within(axis).getByText("3 次")).toBeInTheDocument();
    // 提示的「请求」与「使用」是两个不同的真实计数，不得合并成一个人造指标
    expect(within(axis).getByText("请求提示")).toBeInTheDocument();
    expect(within(axis).getByText("使用提示")).toBeInTheDocument();
  });

  it("没有回忆证据时走「证据不足」", async () => {
    H.getCognitiveProgress.mockResolvedValue(
      view({
        quality: {
          available: false,
          recall_success: 0,
          recall_partial: 0,
          recall_failure: 0,
          hint_requests: 0,
          hint_uses: 0,
          reason_code: "no_recall_moments",
        },
      })
    );
    renderProgress();
    await waitLoaded();

    const axis = page().querySelector('[data-axis="quality"]') as HTMLElement;
    expect(within(axis).getByText(/还没有主动回忆的记录/)).toBeInTheDocument();
    expect(axis.querySelector(".hc-chart")).toBeNull();
  });
});

// ============================================================
// §26 — 图表底座 + 轴口径披露
// ============================================================

describe("§26 — 图表与口径披露的实现底座", () => {
  it("图表一律走既有 recharts，禁止手写图表引擎", () => {
    const src = SRC("../../src/pages/CognitiveProgress.tsx");
    expect(src).toContain('from "recharts"');
    expect(src).toContain("BarChart");
    expect(src).toContain("ResponsiveContainer");
    // 不得手写 <svg> 绘图 / canvas 绘图
    expect(src).not.toContain("<svg");
    expect(src).not.toContain("<canvas");
    expect(src).not.toContain("getContext(");
  });

  it("轴口径走 Radix 原语披露（点击后出现口径说明），而不是把口径塞进图里", async () => {
    const src = SRC("../../src/pages/CognitiveProgress.tsx");
    expect(src).toContain('from "@radix-ui/themes"');
    expect(src).toContain("<Popover.");

    renderProgress();
    await waitLoaded();

    expect(screen.queryByText(PROGRESS_AXIS_EXPLAIN.volume)).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Volume 的口径说明" }));

    const pop = await screen.findByText(PROGRESS_AXIS_EXPLAIN.volume);
    expect(pop).toBeInTheDocument();
  });
});

// ============================================================
// §26 — 详情数据入口
// ============================================================

describe("§26 — 查看详细学习数据 → /data", () => {
  it("提供次级入口指向既有的 /data 详情页", async () => {
    renderProgress();
    await waitLoaded();

    const link = screen.getByRole("link", { name: "查看详细学习数据" });
    expect(link).toHaveAttribute("href", "/data");
  });
});

// ============================================================
// 错误路径
// ============================================================

describe("§26 — 读取失败时的诚实呈现", () => {
  it("失败时给出错误 + 重试，且不渲染任何伪造轴数据", async () => {
    H.getCognitiveProgress.mockRejectedValue(new Error("数据库忙"));
    renderProgress();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("数据库忙");
    expect(screen.getByRole("button", { name: "重新尝试" })).toBeInTheDocument();
    expect(page().querySelectorAll("[data-axis]")).toHaveLength(0);
  });
});

// ============================================================
// UI-14（补充）— /progress 路由升级后，兼容路由零删除
// ============================================================

describe("UI-14 — /progress 升级为四轴页，其余兼容路由零删除", () => {
  it("App.tsx 把 /progress 指向 CognitiveProgress，且不再重定向到 /planning", () => {
    const app = SRC("../../src/App.tsx");
    expect(app).toContain('<Route path="/progress" element={<CognitiveProgressPage />} />');
    expect(app).not.toMatch(/<Route path="\/progress" element=\{<Navigate/);
    // §21 / §37：Planning / Knowledge / Data / Sync 既有路由仍然存在
    for (const route of [
      '<Route path="/planning" element={<Planning />} />',
      '<Route path="/journey" element={<Planning />} />',
      '<Route path="/knowledge" element={<KnowledgePage />} />',
      '<Route path="/data" element={<DataPage />} />',
      '<Route path="/sync" element={<Sync />} />',
    ]) {
      expect(app).toContain(route);
    }
    // §21：旧页面文件不删除（只是不再挂路由）
    expect(SRC("../../src/pages/Progress.tsx")).toContain("function Progress");
  });
});
