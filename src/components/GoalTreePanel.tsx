import { useEffect, useState } from "react";
import {
  createGoalNode,
  deleteGoalNode,
  getGoalTree,
  updateGoal,
} from "../api";
import type { GoalTree, GoalTreeNode } from "../types";

/**
 * 目标树面板（DEV-0050 / PHASE-B §25-26）。
 * 文件树形态：Final → Year → Month → Day；展开/折叠/缩进/层级标签/Hover 操作。
 * 不做卡片墙 / 分页 / 三栏 / 甘特图。
 * Final：编辑 + 创建年目标；Year：编辑/创建月/删除；Month：编辑/创建日/删除；Day：编辑/删除/新建任务。
 */
export default function GoalTreePanel({
  profileId,
  onRefresh,
  onCreateTaskForGoal,
  goalPathOf,
}: {
  profileId: number;
  onRefresh?: () => void;
  /** Day 节点「新建任务」→ TaskModal 预填 goal */
  onCreateTaskForGoal?: (goalId: number) => void;
  /** 供外部读取当前树（下一步面包屑用） */
  goalPathOf?: (goalId: number) => string[];
}) {
  const [tree, setTree] = useState<GoalTree | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  // 节点操作态
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const [editFor, setEditFor] = useState<GoalTreeNode | null>(null);
  const [editName, setEditName] = useState("");
  const [createFor, setCreateFor] = useState<GoalTreeNode | null>(null); // 父节点
  const [createLevel, setCreateLevel] = useState<"year" | "month" | "day">("year");
  const [createPeriod, setCreatePeriod] = useState("");
  const [createName, setCreateName] = useState("");

  const load = async () => {
    try {
      const t = await getGoalTree(profileId);
      setTree(t);
      if (goalPathOf) {
        // 构建父链 map 供外部
        const path = new Map<number, string[]>();
        const walk = (n: GoalTreeNode, acc: string[]) => {
          const next = [...acc, shortLabel(n)];
          path.set(n.id, next);
          for (const c of n.children) walk(c, next);
        };
        walk(t.final_goal, []);
        goalPathResolver.map = path;
      }
    } catch (e) {
      setError(String(e));
    }
  };
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId]);

  if (error && !tree) return <p className="muted">{error}</p>;
  if (!tree) return <p className="muted">加载目标…</p>;

  async function doCreate() {
    if (!createFor) return;
    setBusy(true);
    setError("");
    try {
      await createGoalNode(profileId, createLevel, createFor.id, createName.trim() || defaultName(), null, createPeriod || null);
      setCreateFor(null);
      setCreateName("");
      setCreatePeriod("");
      await load();
      onRefresh?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function defaultName() {
    if (createLevel === "year") return `${createPeriod} 年目标`;
    if (createLevel === "month") return `${createPeriod} 月目标`;
    return `${createPeriod} 日目标`;
  }

  async function doDelete(g: GoalTreeNode) {
    if (!window.confirm(`删除目标「${g.name}」？关联任务会保留（取消目标关联）。`)) return;
    setBusy(true);
    setError("");
    try {
      await deleteGoalNode(g.id);
      await load();
      onRefresh?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function doEdit() {
    if (!editFor) return;
    setBusy(true);
    setError("");
    try {
      await updateGoal(editFor.id, editName.trim() || editFor.name, editFor.description ?? undefined);
      setEditFor(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const renderNode = (n: GoalTreeNode, depth: number) => {
    const level = n.goal_level ?? "legacy";
    const label =
      level === "final"
        ? "最终目标"
        : level === "year"
          ? `${(n.period_start ?? "").slice(0, 4)} · 年目标`
          : level === "month"
            ? `${(n.period_start ?? "").slice(5, 7)}月 · 月目标`
            : level === "day"
              ? `${(n.period_start ?? "").slice(5)} · 日目标`
              : "目标";
    return (
      <div key={n.id} className="gtree__node-wrap">
        <div className="gtree__node" style={{ paddingLeft: depth * 16 }}>
          <span className={`gtree__level-tag gtree__level-tag--${level}`}>{label}</span>
          <span className="gtree__name">{n.name}</span>
          <span className="gtree__actions">
            {level === "final" && (
              <button
                className="gtree__act"
                title="创建年目标"
                onClick={() => {
                  setCreateFor(n);
                  setCreateLevel("year");
                  setCreatePeriod(String(new Date().getFullYear()));
                  setCreateName("");
                }}
              >
                + 年
              </button>
            )}
            {level === "year" && (
              <button
                className="gtree__act"
                title="创建月目标"
                onClick={() => {
                  setCreateFor(n);
                  setCreateLevel("month");
                  const now = new Date();
                  setCreatePeriod(`${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`);
                  setCreateName("");
                }}
              >
                + 月
              </button>
            )}
            {level === "month" && (
              <button
                className="gtree__act"
                title="创建日目标"
                onClick={() => {
                  setCreateFor(n);
                  setCreateLevel("day");
                  setCreatePeriod(new Date().toISOString().slice(0, 10));
                  setCreateName("");
                }}
              >
                + 日
              </button>
            )}
            <button
              className="gtree__act"
              title="编辑"
              onClick={() => {
                setEditFor(n);
                setEditName(n.name);
              }}
            >
              编辑
            </button>
            {level === "day" && onCreateTaskForGoal && (
              <button
                className="gtree__act"
                title="从该日目标创建任务"
                onClick={() => onCreateTaskForGoal(n.id)}
              >
                + 任务
              </button>
            )}
            {level !== "final" && (
              <button className="gtree__act gtree__act--danger" title="删除" onClick={() => void doDelete(n)}>
                删除
              </button>
            )}
          </span>
        </div>
        {n.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  };

  return (
    <div className="gtree">
      <div className="gtree__head">
        <h2 className="card__title">目标</h2>
      </div>
      {error && (
        <div className="alert alert--error" onClick={() => setError("")}>
          {error}（点击关闭）
        </div>
      )}
      <div className="gtree__body">
        {renderNode(tree.final_goal, 0)}
        {tree.legacy_goals.length > 0 && (
          <p className="muted" style={{ fontSize: 11, marginTop: 8 }}>
            另有 {tree.legacy_goals.length} 个历史目标（旧版数据，已保留，不参与目标树层级）
          </p>
        )}
      </div>

      {/* 新建子目标 */}
      {createFor && (
        <div className="modal-overlay" onClick={() => setCreateFor(null)}>
          <div className="modal modal--quick" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">
              新建{createLevel === "year" ? "年" : createLevel === "month" ? "月" : "日"}目标 ·{" "}
              {createFor.name}
            </div>
            <label className="modal__field">
              {createLevel === "year" ? "年份" : createLevel === "month" ? "月份" : "日期"}
              <input
                className="modal__input"
                type={createLevel === "year" ? "number" : createLevel === "month" ? "month" : "date"}
                value={createPeriod}
                onChange={(e) => setCreatePeriod(e.target.value)}
              />
            </label>
            <label className="modal__field">
              名称（留空自动生成）
              <input
                className="modal__input"
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                placeholder={defaultName()}
              />
            </label>
            <div className="btn-row">
              <button className="btn btn--primary" disabled={busy} onClick={() => void doCreate()}>
                创建
              </button>
              <button className="btn" onClick={() => setCreateFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 编辑 */}
      {editFor && (
        <div className="modal-overlay" onClick={() => setEditFor(null)}>
          <div className="modal modal--quick" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">编辑目标</div>
            <label className="modal__field">
              名称
              <input
                className="modal__input"
                value={editName}
                autoFocus
                onChange={(e) => setEditName(e.target.value)}
              />
            </label>
            <div className="btn-row">
              <button className="btn btn--primary" disabled={busy} onClick={() => void doEdit()}>
                保存
              </button>
              <button className="btn" onClick={() => setEditFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
      {/* menuFor 预留（当前操作全部内联按钮，无浮层需要） */}
      <span style={{ display: "none" }}>{menuFor}</span>
    </div>
  );
}

/** 父链解析（供「下一步」面包屑）：组件加载时填充。 */
export const goalPathResolver: { map: Map<number, string[]> } = { map: new Map() };

function shortLabel(n: GoalTreeNode): string {
  const level = n.goal_level ?? "";
  if (level === "final") return n.name;
  if (level === "year") return (n.period_start ?? "").slice(0, 4);
  if (level === "month") return `${(n.period_start ?? "").slice(5, 7)}月`;
  if (level === "day") return (n.period_start ?? "").slice(5);
  return n.name;
}
