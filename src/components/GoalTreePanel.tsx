import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  createGoalNode,
  deleteGoalNode,
  getGoalTree,
  listAllTasksByProfile,
  listSessionsByGoal,
  updateGoal,
} from "../api";
import type { GoalTree, GoalTreeNode, StudySession, Task } from "../types";
import { durationShort } from "./DailyActivitiesSection";

/**
 * 目标树面板（DEV-0050 §25-26 / DEV-0053 §44-47、§112-113）。
 * 文件树形态：Final → Year → Month → Day；展开/折叠/缩进/层级标签/Hover 操作。
 * 选中 Goal 节点（点击名称）→ 树下方 Detail 区显示两个 tab：
 *   目标任务（现有 tasks 查询按 goal 过滤）/ 学习记录（list_sessions_by_goal；极简行点击进 Session）。
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
  const navigate = useNavigate();
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

  // §112：选中节点的 Detail（目标任务 / 学习记录）
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detailTab, setDetailTab] = useState<"tasks" | "sessions">("tasks");
  const [goalTasks, setGoalTasks] = useState<Task[]>([]);
  const [goalSessions, setGoalSessions] = useState<StudySession[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);

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

  /** §45/§112：选中 Goal → 下方 Detail 加载 目标任务 + 学习记录 */
  async function selectGoal(n: GoalTreeNode) {
    setSelectedId(n.id);
    setDetailTab("tasks");
    setDetailLoading(true);
    try {
      const [allTasks, sessions] = await Promise.all([
        listAllTasksByProfile(profileId).catch(() => [] as Task[]),
        listSessionsByGoal(profileId, n.id, 50).catch(() => [] as StudySession[]),
      ]);
      // 只保留属于该节点的任务（现有 tasks 查询 + goal 过滤）
      setGoalTasks(
        allTasks
          .filter((t) => t.goal_id === n.id && t.archived_at == null)
          .sort((a, b) => (a.planned_date ?? "9999") < (b.planned_date ?? "9999") ? -1 : 1)
      );
      setGoalSessions(sessions);
    } catch (e) {
      setError(String(e));
    } finally {
      setDetailLoading(false);
    }
  }

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
      if (selectedId === g.id) setSelectedId(null);
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

  const selectedNode = selectedId != null ? findNode(tree.final_goal, selectedId) : null;

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
        <div className={"gtree__node" + (n.id === selectedId ? " gtree__node--selected" : "")} style={{ paddingLeft: depth * 16 }}>
          <span className={`gtree__level-tag gtree__level-tag--${level}`}>{label}</span>
          <button
            className={"gtree__name" + (n.id === selectedId ? " gtree__name--selected" : "")}
            title="点击查看该目标的目标任务与学习记录"
            onClick={() => void selectGoal(n)}
          >
            {n.name}
          </button>
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

      {/* §44-47/§112-113：选中 Goal 节点的 Detail 区（目标任务 / 学习记录） */}
      {selectedNode && (
        <div className="gtree__detail">
          <div className="gtree__detail-head">
            <span className="gtree__detail-name" title={selectedNode.name}>
              {selectedNode.name}
            </span>
            <div className="gtree__detail-tabs">
              <button
                className={"gtree__tab" + (detailTab === "tasks" ? " gtree__tab--active" : "")}
                onClick={() => setDetailTab("tasks")}
              >
                目标任务
              </button>
              <button
                className={"gtree__tab" + (detailTab === "sessions" ? " gtree__tab--active" : "")}
                onClick={() => setDetailTab("sessions")}
              >
                学习记录
              </button>
              <button
                className="gtree__tab gtree__tab--close"
                title="收起详情"
                onClick={() => setSelectedId(null)}
              >
                ✕
              </button>
            </div>
          </div>
          <div className="gtree__detail-body">
            {detailLoading ? (
              <p className="muted" style={{ fontSize: 12 }}>加载中…</p>
            ) : detailTab === "tasks" ? (
              goalTasks.length === 0 ? (
                <p className="muted" style={{ fontSize: 12 }}>这个目标下还没有任务。</p>
              ) : (
                <ul className="gtree__rows">
                  {goalTasks.map((t) => (
                    <li key={t.id} className="gtree__row">
                      <span className={"gtree__row-title" + (t.status === "completed" ? " gtree__row-title--done" : "")}>
                        {t.status === "completed" ? "✓ " : "○ "}
                        {t.title}
                      </span>
                      <span className="gtree__row-meta">
                        {t.planned_date ? t.planned_date.slice(5) : "未排期"}
                      </span>
                    </li>
                  ))}
                </ul>
              )
            ) : goalSessions.length === 0 ? (
              <p className="muted" style={{ fontSize: 12 }}>这个目标还没有学习记录。</p>
            ) : (
              <ul className="gtree__rows">
                {goalSessions.map((s) => (
                  <li key={s.id} className="gtree__row">
                    <button
                      className="gtree__row-title gtree__row-link"
                      title="打开这条学习记录"
                      onClick={() => navigate(`/learn/${s.id}`)}
                    >
                      {s.title || `学习记录 #${s.id}`}
                    </button>
                    <span className="gtree__row-meta">{durationShort(s.duration_seconds)}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}

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

/** 在树中按 id 查找节点。 */
function findNode(node: GoalTreeNode, id: number): GoalTreeNode | null {
  if (node.id === id) return node;
  for (const c of node.children) {
    const hit = findNode(c, id);
    if (hit) return hit;
  }
  return null;
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
