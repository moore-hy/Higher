import { describe, expect, it } from "vitest";
import {
  buildStartHereCandidates,
  CONTINUE_LAST_WINDOW_MS,
  nextStartHere,
  parsePlannedMinutes,
  pickStartHere,
  rankStartHere,
  START_HERE_CATEGORY_RANK,
  type StartHereCandidate,
} from "../../src/learning/startHere";
import type { DailyTaskRow, StudySession } from "../../src/types";

/**
 * PRODUCT-2.0 §0B.2 / §0C.5 —— Start Here 排序引擎单测。
 *
 * 覆盖：
 *   LEARN-TC001  no active -> at most ONE Start Here recommendation
 *   LEARN-TC003  manual Task Start works regardless of recommendation
 *   LEARN-TC004  dismiss/swap recommendation does not mutate plan/task
 *   LEARN-TC005  Continue Last creates NEW session, never reopens ended session
 *   LEARN-TC012  available_minutes uses same guidance engine
 *   LEARN-TC018  provider offline -> deterministic guidance/data still works
 *   LEARN-TC024  profile isolation for all guidance
 *   LEARN-TC025  no guidance render loop repeatedly calls LLM（引擎为纯函数）
 *   CONTINUE-TC001..005（§22.6）
 */

const T0 = new Date("2026-09-15T02:00:00Z").getTime();
const DAY = 24 * 60 * 60 * 1000;

function task(over: Partial<DailyTaskRow> & { id: number }): DailyTaskRow {
  return {
    title: `任务 ${over.id}`,
    status: "planned",
    planned_time: null,
    estimated_minutes: null,
    task_kind: "structured",
    priority: "normal",
    goal_id: null,
    learning_item_id: null,
    knowledge_name: null,
    deep_link: "",
    ...over,
  };
}

function session(over: Partial<StudySession> & { id: number }): StudySession {
  return {
    profile_id: 1,
    goal_id: null,
    task_id: null,
    learning_item_id: null,
    title: `学习 ${over.id}`,
    started_at: "2026-09-15 00:00:00",
    ended_at: "2026-09-15 00:30:00",
    duration_seconds: 1800,
    status: "completed",
    note: null,
    note_document_json: null,
    created_at: "2026-09-15 00:00:00",
    updated_at: "2026-09-15 00:30:00",
    time_corrected: 0,
    ...over,
  };
}

function candidate(over: Partial<StartHereCandidate> & { id: string }): StartHereCandidate {
  return {
    kind: "today_task",
    title: "t",
    subtitle: null,
    reasons: [],
    action: { type: "start_quick" },
    deadlineMinutes: null,
    priorityRank: 1,
    minutesFit: Number.MAX_SAFE_INTEGER,
    recency: 0,
    stableId: 0,
    ...over,
  };
}

describe("§0B.2 类别优先级", () => {
  it("类别优先级表冻结为 continue_last(3) < today_task(5) < quick_study(6)", () => {
    expect(START_HERE_CATEGORY_RANK.continue_last).toBeLessThan(
      START_HERE_CATEGORY_RANK.today_task
    );
    expect(START_HERE_CATEGORY_RANK.today_task).toBeLessThan(
      START_HERE_CATEGORY_RANK.quick_study
    );
  });

  it("LEARN-TC001：任意候选集合只产出唯一主建议（不是数组）", () => {
    const picked = pickStartHere([
      candidate({ id: "a", kind: "today_task" }),
      candidate({ id: "b", kind: "today_task" }),
      candidate({ id: "c", kind: "quick_study" }),
    ]);
    expect(picked).not.toBeNull();
    expect(Array.isArray(picked)).toBe(false);
    expect(picked!.id).toBe("a");
  });

  it("continue_last 优先于 today_task 与 quick_study", () => {
    const ranked = rankStartHere([
      candidate({ id: "quick", kind: "quick_study" }),
      candidate({ id: "task", kind: "today_task" }),
      candidate({ id: "cont", kind: "continue_last" }),
    ]);
    expect(ranked.map((c) => c.id)).toEqual(["cont", "task", "quick"]);
  });
});

describe("§0C.5 同类别 tie-break（顺序不可调换）", () => {
  it("#1 hard deadline 更近优先；无时间的排在有时间之后", () => {
    const ranked = rankStartHere([
      candidate({ id: "late", deadlineMinutes: 20 * 60 }),
      candidate({ id: "none", deadlineMinutes: null }),
      candidate({ id: "early", deadlineMinutes: 9 * 60 }),
    ]);
    expect(ranked.map((c) => c.id)).toEqual(["early", "late", "none"]);
  });

  it("#2 同级同 deadline → priority 更高优先（core=0 < normal=1 < accumulation=2）", () => {
    const ranked = rankStartHere([
      candidate({ id: "acc", priorityRank: 2 }),
      candidate({ id: "core", priorityRank: 0 }),
      candidate({ id: "normal", priorityRank: 1 }),
    ]);
    expect(ranked.map((c) => c.id)).toEqual(["core", "normal", "acc"]);
  });

  it("#3 同级同 priority → 与 available_minutes 更匹配优先", () => {
    const ranked = rankStartHere([
      candidate({ id: "far", minutesFit: 40 }),
      candidate({ id: "fit", minutesFit: 3 }),
    ]);
    expect(ranked[0].id).toBe("fit");
  });

  it("#4 以上都相同 → 最近一次中断/未完成更近优先", () => {
    const ranked = rankStartHere([
      candidate({ id: "old", recency: 1000 }),
      candidate({ id: "recent", recency: 9000 }),
    ]);
    expect(ranked[0].id).toBe("recent");
  });

  it("#5 完全并列 → stableId 稳定排序（可复现，不依赖输入顺序）", () => {
    const a = candidate({ id: "a", stableId: 7 });
    const b = candidate({ id: "b", stableId: 3 });
    expect(rankStartHere([a, b])[0].id).toBe("b");
    expect(rankStartHere([b, a])[0].id).toBe("b");
  });

  it("排序不可用 LLM 决定：对同一输入永远给出同一顺序（LEARN-TC018 离线可算）", () => {
    const input = [
      candidate({ id: "x", deadlineMinutes: 600, priorityRank: 1 }),
      candidate({ id: "y", deadlineMinutes: 600, priorityRank: 0 }),
    ];
    const r1 = rankStartHere(input).map((c) => c.id);
    const r2 = rankStartHere(input).map((c) => c.id);
    expect(r1).toEqual(r2);
    expect(r1).toEqual(["y", "x"]);
  });
});

describe("§30B「换一个」循环", () => {
  it("从当前候选循环到下一个，末尾回卷", () => {
    const list = [
      candidate({ id: "a", stableId: 1 }),
      candidate({ id: "b", stableId: 2 }),
      candidate({ id: "c", stableId: 3 }),
    ];
    expect(nextStartHere(list, "a")!.id).toBe("b");
    expect(nextStartHere(list, "b")!.id).toBe("c");
    expect(nextStartHere(list, "c")!.id).toBe("a");
  });

  it("LEARN-TC004：换建议不改动任何候选（无 plan/task 变更副作用）", () => {
    const list = [
      candidate({ id: "a", stableId: 1, action: { type: "start_task", taskId: 11 } }),
      candidate({ id: "b", stableId: 2, action: { type: "start_task", taskId: 22 } }),
    ];
    const snapshot = JSON.stringify(list);
    nextStartHere(list, "a");
    pickStartHere(list);
    expect(JSON.stringify(list)).toBe(snapshot);
  });

  it("空候选返回 null", () => {
    expect(pickStartHere([])).toBeNull();
    expect(nextStartHere([], "a")).toBeNull();
  });
});

describe("planned_time 解析", () => {
  it("HH:MM → 当天分钟；非法返 null", () => {
    expect(parsePlannedMinutes("09:30")).toBe(570);
    expect(parsePlannedMinutes("00:00")).toBe(0);
    expect(parsePlannedMinutes("23:59")).toBe(1439);
    expect(parsePlannedMinutes("")).toBeNull();
    expect(parsePlannedMinutes(null)).toBeNull();
    expect(parsePlannedMinutes("25:00")).toBeNull();
    expect(parsePlannedMinutes("abc")).toBeNull();
  });
});

describe("CONTINUE — 继续上次学习（§22.6）", () => {
  it("CONTINUE-TC001：近期已结束 Session 可见为主要候选", () => {
    const tasks = [task({ id: 1 })];
    const recent = [session({ id: 100, title: "英语四级 · 翻译", status: "completed" })];
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks,
      recentSessions: recent,
      now: T0,
    });
    const cont = cands.find((c) => c.kind === "continue_last");
    expect(cont).toBeDefined();
    expect(cont!.title).toBe("英语四级 · 翻译");
    expect(pickStartHere(cands)!.kind).toBe("continue_last");
  });

  it("CONTINUE-TC002 / LEARN-TC005：点击产生 NEW session 动作，绝不「恢复」已结束 Session", () => {
    const recent = [session({ id: 100, task_id: 5, status: "completed" })];
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 5 })],
      recentSessions: recent,
      now: T0,
    });
    const cont = cands.find((c) => c.kind === "continue_last")!;
    // 动作用于「新建」，不携带被继续的 Session id
    expect(cont.action.type).toBe("start_task");
    expect(cont.action).toEqual({ type: "start_task", taskId: 5 });
    expect(JSON.stringify(cont.action)).not.toContain("100");
  });

  it("CONTINUE-TC003：引擎从不修改输入（旧 Session 原样保留）", () => {
    const recent = [session({ id: 100, status: "completed" })];
    const snapshot = JSON.stringify(recent);
    buildStartHereCandidates({ profileId: 1, tasks: [], recentSessions: recent, now: T0 });
    expect(JSON.stringify(recent)).toBe(snapshot);
  });

  it("CONTINUE-TC004：关联任务已完成 → 安全降级（有知识项走知识项，否则走快速学习）", () => {
    const withItem = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 5, status: "completed" })],
      recentSessions: [
        session({ id: 100, task_id: 5, learning_item_id: 77, status: "completed" }),
      ],
      now: T0,
    }).find((c) => c.kind === "continue_last")!;
    expect(withItem.action).toEqual({ type: "start_item", learningItemId: 77, taskId: null });

    const noItem = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 5, status: "completed" })],
      recentSessions: [session({ id: 101, task_id: 5, status: "completed" })],
      now: T0,
    }).find((c) => c.kind === "continue_last")!;
    expect(noItem.action).toEqual({ type: "start_quick" });
  });

  it("CONTINUE-TC005 / LEARN-TC024：绝不跨档案推荐", () => {
    const recent = [session({ id: 200, profile_id: 2, status: "completed" })];
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [],
      recentSessions: recent,
      now: T0,
    });
    expect(cands.find((c) => c.kind === "continue_last")).toBeUndefined();
  });

  it("超出 7 天窗口的已结束 Session 不算「近期」", () => {
    const old = new Date(T0 - CONTINUE_LAST_WINDOW_MS - 60_000).toISOString();
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [],
      recentSessions: [session({ id: 300, ended_at: old, status: "completed" })],
      now: T0,
    });
    expect(cands.find((c) => c.kind === "continue_last")).toBeUndefined();
  });

  it("active（未结束）Session 不构成 Continue Last 候选", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [],
      recentSessions: [session({ id: 400, status: "active", ended_at: null })],
      now: T0,
    });
    expect(cands.find((c) => c.kind === "continue_last")).toBeUndefined();
  });

  it("多条已结束时取最近一条", () => {
    const fresh = new Date(T0 - 1000).toISOString();
    const older = new Date(T0 - 5 * DAY).toISOString();
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [],
      recentSessions: [
        session({ id: 500, title: "旧", ended_at: older, status: "completed" }),
        session({ id: 501, title: "新", ended_at: fresh, status: "completed" }),
      ],
      now: T0,
    });
    expect(cands.find((c) => c.kind === "continue_last")!.title).toBe("新");
  });
});

describe("Today Task 候选", () => {
  it("已完成任务不进入候选", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 1, status: "completed" }), task({ id: 2 })],
      recentSessions: [],
      now: T0,
    });
    const ids = cands.filter((c) => c.kind === "today_task").map((c) => c.id);
    expect(ids).toEqual(["task:2"]);
  });

  it("LEARN-TC003：任务候选携带 start_task 动作，可一键手动开始（不依赖建议）", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 9, planned_time: "08:00", priority: "core" })],
      recentSessions: [],
      now: T0,
    });
    const t = cands.find((c) => c.id === "task:9")!;
    expect(t.action).toEqual({ type: "start_task", taskId: 9 });
    expect(t.deadlineMinutes).toBe(480);
    expect(t.priorityRank).toBe(0);
  });

  it("LEARN-TC012：available_minutes 参与 minutesFit 排序（同一引擎）", () => {
    const tasks = [
      task({ id: 1, estimated_minutes: 120 }),
      task({ id: 2, estimated_minutes: 20 }),
    ];
    const noMinutes = buildStartHereCandidates({
      profileId: 1,
      tasks,
      recentSessions: [],
      now: T0,
    });
    // 无 available_minutes → minutesFit 全为 MAX → 回退 stableId
    expect(pickStartHere(noMinutes.filter((c) => c.kind === "today_task"))!.id).toBe("task:1");

    const withMinutes = buildStartHereCandidates({
      profileId: 1,
      tasks,
      recentSessions: [],
      availableMinutes: 20,
      now: T0,
    });
    expect(pickStartHere(withMinutes.filter((c) => c.kind === "today_task"))!.id).toBe("task:2");
  });

  it("「最近中断更近」优先：同 priority/deadline 时取最近学过的任务", () => {
    const fresh = new Date(T0 - 60_000).toISOString();
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 1 }), task({ id: 2 })],
      recentSessions: [session({ id: 600, task_id: 2, ended_at: fresh, status: "completed" })],
      now: T0,
    });
    expect(pickStartHere(cands.filter((c) => c.kind === "today_task"))!.id).toBe("task:2");
  });
});

describe("快速学习兜底（手动学习始终可用）", () => {
  it("即使没有任何任务与历史，也永远有 quick_study 候选", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [],
      recentSessions: [],
      now: T0,
    });
    const quick = cands.find((c) => c.kind === "quick_study");
    expect(quick).toBeDefined();
    expect(quick!.action).toEqual({ type: "start_quick" });
    expect(pickStartHere(cands)!.kind).toBe("quick_study");
  });

  it("LEARN-TC025：构造候选是纯同步计算，不产生任何 Promise / 网络副作用", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 1 })],
      recentSessions: [],
      now: T0,
    });
    expect(Array.isArray(cands)).toBe(true);
    expect(cands.every((c) => typeof c.id === "string")).toBe(true);
  });

  it("每条候选都带可验证理由（「为什么？」），不含人格化结论", () => {
    const cands = buildStartHereCandidates({
      profileId: 1,
      tasks: [task({ id: 1, priority: "core", planned_time: "07:00" })],
      recentSessions: [session({ id: 700, status: "completed" })],
      now: T0,
    });
    const banned = ["懒", "自制力", "记忆力差", "注意力", "智商", "AI 觉得你应该"];
    for (const c of cands) {
      expect(c.reasons.length).toBeGreaterThan(0);
      const text = c.reasons.join(" ");
      for (const b of banned) expect(text).not.toContain(b);
    }
  });
});
