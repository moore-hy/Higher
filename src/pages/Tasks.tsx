import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  completeTask,
  createTask,
  listAllTasksByProfile,
  listGoalsByProfile,
  listLearningItems,
  listPlans,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { Goal, LearningItem, Plan, Task } from "../types";
import { formatDateTime, todayDate } from "../utils";

/**
 * 根据 parent_id 链计算 Learning Item 的完整层级路径，
 * 如 "数学 > 高等数学 > 极限"。使用 Map 自底向上回溯，visited 防御循环引用。
 */
function computeFullPath(items: LearningItem[], id: number): string {
  const byId = new Map<number, LearningItem>();
  items.forEach((i) => byId.set(i.id, i));
  const parts: string[] = [];
  const visited = new Set<number>();
  let current = byId.get(id);
  while (current) {
    if (visited.has(current.id)) break;
    visited.add(current.id);
    parts.unshift(current.name);
    if (current.parent_id == null) break;
    current = byId.get(current.parent_id);
  }
  return parts.join(" > ");
}

function Tasks() {
  // 支持 Planning 页面「+ 创建任务」按钮通过 URL 预填表单
  const [searchParams] = useSearchParams();

  const { activeProfile, refreshKey } = useActiveProfile();

  const [tasks, setTasks] = useState<Task[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const [itemId, setItemId] = useState<number | "">("");
  const [title, setTitle] = useState("");
  const [plannedDate, setPlannedDate] = useState(todayDate());
  const [planId, setPlanId] = useState<number | "">("");

  // 标记 URL 预填参数是否已应用（仅应用一次，避免覆盖用户后续选择）
  const appliedUrlParams = useRef(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      // Tasks 页没有 goal 选择器，因此先加载全部 goal，再逐个加载计划并合并
      const [taskList, itemList, goalList] = await Promise.all([
        listAllTasksByProfile(activeProfile!.id),
        listLearningItems(),
        listGoalsByProfile(activeProfile!.id),
      ]);
      const planLists = await Promise.all(goalList.map((g) => listPlans(g.id)));
      setTasks(taskList);
      setItems(itemList);
      setGoals(goalList);
      setPlans(planLists.flat());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 数据就绪后根据 URL 参数预填：?plan=PLAN_ID&item=ITEM_ID&goal=GOAL_ID
  useEffect(() => {
    if (appliedUrlParams.current) return;
    if (items.length === 0) return; // 等待学习对象加载完成

    appliedUrlParams.current = true;

    const planParam = searchParams.get("plan");
    const itemParam = searchParams.get("item");

    let nextItemId: number | "" = "";
    let nextPlanId: number | "" = "";

    // 显式指定的 item 优先级最高
    if (itemParam) {
      const id = Number(itemParam);
      if (items.some((i) => i.id === id)) nextItemId = id;
    }

    if (planParam) {
      const pid = Number(planParam);
      const plan = plans.find((p) => p.id === pid);
      if (plan) {
        nextPlanId = pid;
        // 未显式指定 item 时，若计划绑定了 learning_item_id 则自动选中
        if (!nextItemId && plan.learning_item_id != null) {
          const id = plan.learning_item_id;
          if (items.some((i) => i.id === id)) nextItemId = id;
        }
      }
    }

    setItemId((prev) => (nextItemId ? nextItemId : prev === "" && items.length > 0 ? items[0].id : prev));
    setPlanId((prev) => (nextPlanId ? nextPlanId : prev));
  }, [items, plans, searchParams]);

  // 学习对象 id → 完整路径（缓存，避免每次渲染重复回溯）
  const itemPath = useMemo(() => {
    const map = new Map<number, string>();
    items.forEach((i) => map.set(i.id, computeFullPath(items, i.id)));
    return map;
  }, [items]);

  const goalName = (id: number) =>
    goals.find((g) => g.id === id)?.name ?? `#${id}`;

  // 任务列表里展示计划标签：优先用计划标题，找不到则回退到 #id
  const planLabel = (id: number | null) => {
    if (id == null) return "";
    const p = plans.find((x) => x.id === id);
    return p ? `${goalName(p.goal_id)} / ${p.title}` : `#${id}`;
  };

  // 选择计划：若计划绑定了 learning_item_id，自动选中对应学习对象
  const handlePlanChange = (value: string) => {
    if (!value) {
      setPlanId("");
      return;
    }
    const pid = Number(value);
    setPlanId(pid);
    const plan = plans.find((p) => p.id === pid);
    if (plan?.learning_item_id != null) {
      setItemId(plan.learning_item_id);
    }
  };

  const handleCreate = async () => {
    if (!itemId) {
      setError("请先选择学习对象");
      return;
    }
    if (!title.trim()) {
      setError("任务标题不能为空");
      return;
    }
    setError("");
    try {
      const item = items.find((i) => i.id === Number(itemId));
      await createTask({
        profileId: item?.profile_id ?? activeProfile!.id,
        goalId: item?.goal_id ?? null,
        title: title.trim(),
        plannedDate: plannedDate || null,
        learningItemId: Number(itemId),
        planId: planId ? Number(planId) : null,
      });
      setTitle("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleComplete = async (id: number) => {
    setError("");
    try {
      await completeTask(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const itemName = (id: number | null) =>
    id == null ? "未关联知识" : items.find((i) => i.id === id)?.name ?? `#${id}`;

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">任务</h1>
        <p className="page__subtitle">计划做什么（与实际 Session 分离）</p>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      <section className="card">
        <h2 className="card__title">创建任务</h2>
        {items.length === 0 ? (
          <p className="muted">请先去「学习对象」页创建一个学习对象。</p>
        ) : (
          <div className="form-stack">
            <label className="field-label">
              学习对象
              <select
                className="input"
                value={itemId}
                onChange={(e) => setItemId(e.target.value ? Number(e.target.value) : "")}
              >
                {items.map((i) => (
                  <option key={i.id} value={i.id}>
                    {itemPath.get(i.id) ?? i.name}
                  </option>
                ))}
              </select>
            </label>
            <input
              className="input"
              placeholder="任务标题（如：复习极限定义）"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <label className="field-label">
              计划日期
              <input
                className="input"
                type="date"
                value={plannedDate}
                onChange={(e) => setPlannedDate(e.target.value)}
              />
            </label>
            <label className="field-label">
              所属计划（可选）
              <select
                className="input"
                value={planId}
                onChange={(e) => handlePlanChange(e.target.value)}
              >
                <option value="">不关联计划</option>
                {plans.map((p) => (
                  <option key={p.id} value={p.id}>
                    {goalName(p.goal_id)} / {p.title}
                  </option>
                ))}
              </select>
            </label>
            <button className="btn btn--primary" onClick={handleCreate}>
              创建任务
            </button>
          </div>
        )}
      </section>

      <section className="card">
        <h2 className="card__title">所有任务</h2>
        {loading ? (
          <p className="muted">加载中…</p>
        ) : tasks.length === 0 ? (
          <p className="muted">还没有任务。</p>
        ) : (
          <ul className="task-list">
            {tasks.map((t) => (
              <li
                key={t.id}
                className={"task-list__item" + (t.status === "completed" ? " task-list__item--done" : "")}
              >
                <div className="task-list__main">
                  <span className="task-list__title">{t.title}</span>
                  <span className="task-list__meta">
                    {itemName(t.learning_item_id)} · 计划 {t.planned_date ?? "未计划"}
                  </span>
                  {t.plan_id != null && (
                    <span className="muted">所属计划: {planLabel(t.plan_id)}</span>
                  )}
                </div>
                <div className="task-list__actions">
                  <span className={"badge " + (t.status === "completed" ? "badge--done" : "badge--pending")}>
                    {t.status === "completed" ? "已完成" : "待完成"}
                  </span>
                  {t.status !== "completed" && (
                    <button
                      className="btn btn--small"
                      onClick={() => handleComplete(t.id)}
                    >
                      标记完成
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

export default Tasks;
