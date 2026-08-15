import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  addLearningAttachment,
  createChildLearningItem,
  createRootLearningItem,
  deleteLearningItem,
  getLearningItemPath,
  getLearningItemStats,
  listAttachmentsByItem,
  listEvaluationsByLearningItem,
  listFeedbacksByLearningItem,
  listGoalsByProfile,
  listLearningItemsByGoal,
  listSessionsByLearningItem,
  moveLearningItem,
  reorderLearningItems,
  saveDrawingAttachment,
  startSession,
  updateLearningItem,
  updateLearningItemContent,
  updateLearningItemStatus,
  updateSessionNote,
} from "../api";
import AiProposalReview from "../components/AiProposalReview";
import AttachmentList from "../components/AttachmentList";
import DrawModal from "../components/DrawModal";
import EvaluationModal from "../components/EvaluationModal";
import FeedbackCard from "../components/FeedbackCard";
import FeedbackModal from "../components/FeedbackModal";
import KnowledgeFlow from "../components/KnowledgeFlow";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type {
  Evaluation,
  Feedback,
  Goal,
  KnowledgeNodeStats,
  LearningAttachment,
  LearningItem,
  StudySession,
} from "../types";
import {
  MASTERY_LABELS,
  MASTERY_STATUSES,
  EVALUATION_TYPE_LABELS,
  OUTCOME_LABELS,
  FEEDBACK_TYPE_LABELS,
} from "../types";
import type { MasteryStatus, EvaluationType, Outcome, FeedbackType } from "../types";
import { formatDateTime, formatDuration } from "../utils";

/** 后代集合（移动目标排除；DEV-0034）。 */
function descendantsOf(items: LearningItem[], id: number): Set<number> {
  const out = new Set<number>();
  const stack = [id];
  while (stack.length) {
    const cur = stack.pop()!;
    for (const it of items) {
      if (it.parent_id === cur && !out.has(it.id)) {
        out.add(it.id);
        stack.push(it.id);
      }
    }
  }
  return out;
}

/** 树节点（LearningItem + children）。 */
interface TreeNode extends LearningItem {
  children: TreeNode[];
}

/** 把扁平列表组装成森林（按 parent_id）。 */
function buildTree(items: LearningItem[]): TreeNode[] {
  const byId = new Map<number, TreeNode>();
  items.forEach((i) => byId.set(i.id, { ...i, children: [] }));
  const roots: TreeNode[] = [];
  byId.forEach((node) => {
    if (node.parent_id == null) {
      roots.push(node);
    } else {
      const parent = byId.get(node.parent_id);
      if (parent) parent.children.push(node);
      else roots.push(node);
    }
  });
  return roots;
}

/** 客户端计算节点完整层级路径（如 "数学 > 高等数学 > 极限"）。 */
function computeFullPath(items: LearningItem[], id: number): string {
  const byId = new Map<number, LearningItem>();
  items.forEach((i) => byId.set(i.id, i));
  const chain: string[] = [];
  let current = byId.get(id);
  const visited = new Set<number>();
  while (current && !visited.has(current.id)) {
    visited.add(current.id);
    chain.push(current.name);
    if (current.parent_id == null) break;
    current = byId.get(current.parent_id);
  }
  return chain.reverse().join(" > ");
}

/** 自动保存延迟（TASK 建议 800ms ~ 1500ms）。 */
const AUTOSAVE_DELAY_MS = 1000;

type SaveStatus = "saved" | "dirty" | "saving" | "error";

/**
 * 知识体系工作区 V1（DEV-0010）。
 *
 * 左侧：知识树（搜索 / 展开折叠 / ··· 菜单 / 新建）
 * 右侧：知识编辑器（面包屑 / 标题 / 掌握状态 / 学习统计 / 正文 / 自动保存）
 *
 * 产品语言：对用户呈现"知识 / 知识体系"，代码内部仍为 LearningItem。
 */
function Knowledge() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const goalIdParam = searchParams.get("goal");
  const itemParam = searchParams.get("item");

  const { activeProfile, refreshKey } = useActiveProfile();

  const [goals, setGoals] = useState<Goal[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const goalId = goalIdParam ? Number(goalIdParam) : null;

  // 选中节点与编辑器状态
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved");
  const [stats, setStats] = useState<KnowledgeNodeStats | null>(null);
  const [recentEvals, setRecentEvals] = useState<Evaluation[]>([]);

  // 记录验证 Modal（DEV-0012：知识详情直接验证，自动带入当前 Goal + Item）
  const [showEvalModal, setShowEvalModal] = useState(false);

  // 需要关注（DEV-0013：当前知识的 open Feedback + 记录入口）
  const [itemFeedbacks, setItemFeedbacks] = useState<Feedback[]>([]);
  const [showFeedbackModal, setShowFeedbackModal] = useState(false);

  // DEV-0017/0018：学习记录 / 附件 / 双视图 / AI
  const [itemSessions, setItemSessions] = useState<StudySession[]>([]);
  const [expandedSession, setExpandedSession] = useState<number | null>(null);
  const [editingSession, setEditingSession] = useState<StudySession | null>(null);
  const [editNoteValue, setEditNoteValue] = useState("");
  const [itemAttachments, setItemAttachments] = useState<LearningAttachment[]>([]);
  const [showItemDraw, setShowItemDraw] = useState(false);
  const [viewMode, setViewMode] = useState<"workspace" | "graph">("workspace");
  const [organizeOpen, setOrganizeOpen] = useState(false);

  // DEV-0022：AI 统一进入右侧 Panel；并上报当前知识上下文
  const { runAction: aiRunAction, setPageContext } = useAiPanel();

  // DEV-0041：小屏（<900px）知识树切换为 Drawer
  const [treeDrawerOpen, setTreeDrawerOpen] = useState(false);

  // 左树交互状态
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [search, setSearch] = useState("");
  const [menuForId, setMenuForId] = useState<number | null>(null);
  // DEV-0034 §65：树节点「移动到…」
  const [treeMoveFor, setTreeMoveFor] = useState<LearningItem | null>(null);
  // DEV-0305：拖拽排序/改父子
  const [dragOverId, setDragOverId] = useState<number | null>(null);
  const [creatingRoot, setCreatingRoot] = useState(false);
  const [rootName, setRootName] = useState("");
  const [addingChildFor, setAddingChildFor] = useState<number | null>(null);
  const [childName, setChildName] = useState("");
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");

  // ---- 自动保存（refs 避免闭包过期）----
  const timerRef = useRef<number | null>(null);
  const pendingSaveRef = useRef<{ id: number; content: string } | null>(null);

  const selectedItem = useMemo(
    () => items.find((i) => i.id === selectedId) ?? null,
    [items, selectedId]
  );

  /** 立即执行待保存的正文（切换节点 / 删除 / 卸载前调用，禁止丢内容）。 */
  const flushSave = useCallback(async () => {
    const pending = pendingSaveRef.current;
    if (!pending) return;
    pendingSaveRef.current = null;
    if (timerRef.current != null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    setSaveStatus("saving");
    try {
      await updateLearningItemContent(pending.id, pending.content);
      // 保存期间若无新输入则标记成功；否则保持 dirty 由新定时器接管
      if (pendingSaveRef.current == null) setSaveStatus("saved");
    } catch (e) {
      // 失败不静默：恢复 pending 让用户可重试
      pendingSaveRef.current = pending;
      setSaveStatus("error");
      setError(String(e));
    }
  }, []);

  // 卸载前尽力保存（正常路径 debounce 已落库；此为兜底）
  useEffect(() => {
    return () => {
      const pending = pendingSaveRef.current;
      if (pending) {
        void updateLearningItemContent(pending.id, pending.content).catch(() => {});
      }
    };
  }, []);

  const scheduleSave = useCallback(
    (id: number, next: string) => {
      pendingSaveRef.current = { id, content: next };
      setSaveStatus("dirty");
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        void flushSave();
      }, AUTOSAVE_DELAY_MS);
    },
    [flushSave]
  );

  const handleContentChange = (next: string) => {
    setContent(next);
    if (selectedId != null) scheduleSave(selectedId, next);
  };

  /** 切换选中节点：先保证未保存正文落库，再切换。 */
  const selectNode = useCallback(
    async (id: number, expandAncestors = false) => {
      await flushSave();
      setError("");
      const item = items.find((i) => i.id === id);
      if (!item) return;
      setSelectedId(id);
      setTitle(item.name);
      setContent(item.content);
      setSaveStatus("saved");
      setMenuForId(null);
      if (expandAncestors) {
        const byId = new Map(items.map((i) => [i.id, i]));
        setExpanded((prev) => {
          const next = { ...prev };
          let cur = byId.get(id);
          while (cur && cur.parent_id != null) {
            next[cur.parent_id] = true;
            cur = byId.get(cur.parent_id);
          }
          return next;
        });
      }
      try {
        setStats(await getLearningItemStats(id));
      } catch {
        setStats(null);
      }
      try {
        // 最近验证（轻量区域，最近 5 条；按 occurred_at DESC 由后端保证）
        const evals = await listEvaluationsByLearningItem(id);
        setRecentEvals(evals.slice(0, 5));
      } catch {
        setRecentEvals([]);
      }
      try {
        const fbs = await listFeedbacksByLearningItem(id);
        setItemFeedbacks(fbs.filter((f) => f.status === "open"));
      } catch {
        setItemFeedbacks([]);
      }
      try {
        const sessions = await listSessionsByLearningItem(id, 10);
        setItemSessions(sessions);
        setExpandedSession(null);
        setEditingSession(null);
        // DEV-0022：上报当前知识上下文（item_id + 面包屑路径）
        let pathLabel = item.name;
        try {
          const p = await getLearningItemPath(id);
          pathLabel = p;
        } catch {
          /* 退化用名称 */
        }
        setPageContext({
          page: "knowledge",
          pageLabel: "知识体系",
          learningItemId: id,
          knowledgePath: pathLabel,
        });
      } catch {
        setItemSessions([]);
      }
      try {
        setItemAttachments(await listAttachmentsByItem(id));
      } catch {
        setItemAttachments([]);
      }
    },
    [items, flushSave]
  );

  /** 创建验证成功后刷新统计与最近验证（同一套 Evidence，不复制数据）。 */
  async function reloadItemEvidence(id: number) {
    try {
      setStats(await getLearningItemStats(id));
    } catch {
      /* 保持原值 */
    }
    try {
      const evals = await listEvaluationsByLearningItem(id);
      setRecentEvals(evals.slice(0, 5));
    } catch {
      /* 保持原值 */
    }
    try {
      const fbs = await listFeedbacksByLearningItem(id);
      setItemFeedbacks(fbs.filter((f) => f.status === "open"));
    } catch {
      /* 保持原值 */
    }
    try {
      setItemSessions(await listSessionsByLearningItem(id, 10));
    } catch {
      /* 保持原值 */
    }
    try {
      setItemAttachments(await listAttachmentsByItem(id));
    } catch {
      /* 保持原值 */
    }
  }

  // ---- 数据加载 ----
  // 注意：goals 必须无条件加载（否则无 ?goal= 参数直接进入时
  // 会因 goals 为空而误显示"没有学习目标"——DEV-0011 修复的真实 Bug）。
  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const goalList = await listGoalsByProfile(activeProfile!.id);
      setGoals(goalList);
      if (goalId == null) {
        setItems([]);
        return;
      }
      const itemList = await listLearningItemsByGoal(goalId);
      setItems(itemList);
      // 当前选中节点失效（被删 / 切档）时清空编辑器
      if (selectedId != null && !itemList.some((i) => i.id === selectedId)) {
        setSelectedId(null);
        setTitle("");
        setContent("");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [goalId, activeProfile, refreshKey, selectedId]);

  useEffect(() => {
    void flushSave();
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  // 无 goal 参数时默认选中第一个 active goal（单目标不强迫用户选择）
  useEffect(() => {
    if (goalId == null && goals.length > 0) {
      const first = goals.find((g) => g.status === "active") ?? goals[0];
      setSearchParams({ goal: String(first.id) }, { replace: true });
    }
  }, [goals, goalId, setSearchParams]);

  // ?item= 参数：自动定位并打开指定知识节点（今日任务"打开知识"入口）
  const itemParamApplied = useRef<string | null>(null);
  useEffect(() => {
    if (
      itemParam &&
      items.length > 0 &&
      itemParamApplied.current !== itemParam &&
      items.some((i) => i.id === Number(itemParam))
    ) {
      itemParamApplied.current = itemParam;
      void selectNode(Number(itemParam), true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, itemParam]);

  // ---- 树操作 ----
  async function handleCreateRoot() {
    const name = rootName.trim();
    if (!name || goalId == null || !activeProfile) return;
    try {
      const created = await createRootLearningItem(activeProfile.id, goalId, name);
      setRootName("");
      setCreatingRoot(false);
      await refresh();
      await selectNode(created.id);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleCreateChild(parentId: number) {
    const name = childName.trim();
    if (!name || goalId == null || !activeProfile) return;
    try {
      const created = await createChildLearningItem(activeProfile.id, parentId, goalId, name);
      setChildName("");
      setAddingChildFor(null);
      setExpanded((p) => ({ ...p, [parentId]: true }));
      await refresh();
      await selectNode(created.id);
    } catch (e) {
      setError(String(e));
    }
  }

  /** DEV-0305 §D：同级手动排序（↑/↓；写 sort_order，与拖拽共用顺序源） */
  async function handleReorder(siblings: TreeNode[], id: number, dir: -1 | 1) {
    const idx = siblings.findIndex((n) => n.id === id);
    const target = idx + dir;
    if (idx < 0 || target < 0 || target >= siblings.length) return;
    const next = [...siblings.map((s) => s.id)];
    [next[idx], next[target]] = [next[target], next[idx]];
    try {
      await reorderLearningItems(next);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleRename(id: number) {
    const name = renameValue.trim();
    const item = items.find((i) => i.id === id);
    if (!name || !item || name === item.name) {
      setRenamingId(null);
      return;
    }
    try {
      await updateLearningItem(id, name, item.description ?? undefined);
      setRenamingId(null);
      if (selectedId === id) setTitle(name);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(id: number) {
    const item = items.find((i) => i.id === id);
    setMenuForId(null);
    if (!item) return;
    const hasContent = item.content.trim().length > 0;
    const confirmed = hasContent
      ? window.confirm(
          `该知识中存在你记录的内容。\n\n删除「${item.name}」后这些内容将无法恢复。\n\n确认删除？`
        )
      : window.confirm(`确认删除「${item.name}」？`);
    if (!confirmed) return;
    try {
      await deleteLearningItem(id);
      if (selectedId === id) {
        setSelectedId(null);
        setTitle("");
        setContent("");
      }
      await refresh();
    } catch (e) {
      setError(String(e)); // safe_delete 拒绝原因（子项 / 任务 / 学习记录 / 验证记录）
    }
  }

  // 标题编辑：Enter / 失焦保存
  async function commitTitle() {
    if (selectedItem == null) return;
    const name = title.trim();
    if (!name || name === selectedItem.name) return;
    try {
      await updateLearningItem(selectedItem.id, name, selectedItem.description ?? undefined);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  function handleTitleKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  }

  async function handleMasteryChange(status: string) {
    if (selectedId == null) return;
    try {
      await updateLearningItemStatus(selectedId, status);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // ---- 渲染 ----
  const tree = useMemo(() => buildTree(items), [items]);

  const searchResults = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return null;
    return items.filter((i) => i.name.toLowerCase().includes(q));
  }, [items, search]);

  const breadcrumb = selectedItem ? computeFullPath(items, selectedItem.id) : "";

  // 无 Goal 空状态
  if (!loading && goals.length === 0) {
    return (
      <div className="page">
        <header className="page__header">
          <h1 className="page__title">知识体系</h1>
        </header>
        <div className="knowledge__empty">
          <p className="knowledge__empty-title">你的这个学习档案还没有学习目标。</p>
          <p className="muted">先建立一个目标，Higher 才能帮助你组织知识体系。</p>
          <button className="btn btn--primary" onClick={() => navigate("/goals")}>
            创建学习目标
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="knowledge">
      {/* ============ 左侧：知识树（工作区模式；<900px 可切换 Drawer，DEV-0041） ============ */}
      {viewMode === "workspace" && (
      <>
      {treeDrawerOpen && (
        <div
          className="knowledge__tree-backdrop"
          onClick={() => setTreeDrawerOpen(false)}
        />
      )}
      <aside
        className={
          "knowledge__tree" + (treeDrawerOpen ? " knowledge__tree--drawer" : "")
        }
      >
        {goals.length > 1 && (
          <div className="knowledge__goal">
            <span className="knowledge__goal-label">当前目标</span>
            <select
              className="knowledge__goal-select"
              value={goalId ?? ""}
              onChange={(e) => {
                setSelectedId(null);
                setSearchParams({ goal: e.target.value });
              }}
            >
              {goals.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </div>
        )}

        <div className="knowledge__search">
          <input
            type="text"
            className="knowledge__search-input"
            placeholder="搜索知识..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        <div className="knowledge__tree-body">
          {loading ? (
            <p className="muted knowledge__hint">加载中…</p>
          ) : searchResults != null ? (
            searchResults.length === 0 ? (
              <p className="muted knowledge__hint">没有匹配的知识。</p>
            ) : (
              <ul className="knowledge__search-list">
                {searchResults.map((i) => (
                  <li key={i.id}>
                    <button
                      className={
                        "knowledge__search-item" +
                        (i.id === selectedId ? " knowledge__search-item--active" : "")
                      }
                      onClick={() => {
                        setSearch("");
                        void selectNode(i.id, true);
                      }}
                    >
                      <span className="knowledge__search-name">{i.name}</span>
                      <span className="knowledge__search-path">
                        {computeFullPath(items, i.id)}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )
          ) : items.length === 0 ? (
            <div className="knowledge__tree-empty">
              <p className="knowledge__empty-title">开始建立你的知识体系</p>
              <p className="muted">
                你可以从最粗的分类开始，以后边学习边补充。
                <br />
                例如：数学、英语、数据结构、Linux、摄影……
              </p>
            </div>
          ) : (
            <ul className="knowledge__nodes">
              {tree.map((node) => renderNode(node, tree))}
            </ul>
          )}
        </div>

        <div className="knowledge__tree-footer">
          {creatingRoot ? (
            <input
              autoFocus
              className="knowledge__new-input"
              placeholder="知识名称，Enter 确认"
              value={rootName}
              onChange={(e) => setRootName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCreateRoot();
                if (e.key === "Escape") {
                  setCreatingRoot(false);
                  setRootName("");
                }
              }}
              onBlur={() => {
                if (rootName.trim()) void handleCreateRoot();
                else setCreatingRoot(false);
              }}
            />
          ) : (
            <button
              className="knowledge__new-root"
              onClick={() => setCreatingRoot(true)}
              disabled={goalId == null}
            >
              + 新建知识
            </button>
          )}
        </div>
      </aside>
      </>
      )}

      {/* ============ 右侧：工作区 / 知识图（DEV-0018 双视图） ============ */}
      <main className="knowledge__main">
        <div className="knowledge__viewbar">
          <div className="review-window">
            <button
              className={
                "review-window__item" +
                (viewMode === "workspace" ? " review-window__item--active" : "")
              }
              onClick={() => setViewMode("workspace")}
            >
              工作区
            </button>
            <button
              className={
                "review-window__item" +
                (viewMode === "graph" ? " review-window__item--active" : "")
              }
              onClick={() => setViewMode("graph")}
            >
              知识图
            </button>
          </div>
          {/* DEV-0041：小屏树 Drawer 切换（仅 <900px 显示） */}
          <button
            className="knowledge__tree-toggle"
            onClick={() => setTreeDrawerOpen((v) => !v)}
          >
            ☰ 树
          </button>
        </div>

        {viewMode === "graph" ? (
          <KnowledgeFlow
            items={items}
            currentId={selectedId}
            onOpen={(itemId) => {
              setViewMode("workspace");
              void selectNode(itemId, true);
            }}
            onCreateRoot={async (name) => {
              if (goalId == null || !activeProfile) return;
              await createRootLearningItem(activeProfile.id, goalId, name);
              await refresh();
            }}
            onCreateChild={async (parent, name) => {
              if (!activeProfile) return;
              await createChildLearningItem(
                activeProfile.id,
                parent.id,
                parent.goal_id,
                name
              );
              await refresh();
            }}
            onRename={async (item, name) => {
              await updateLearningItem(item.id, name, item.description ?? undefined);
              await refresh();
            }}
            onMove={async (item, newParentId) => {
              await moveLearningItem(item.id, newParentId);
              await refresh();
            }}
            onDelete={async (item) => {
              if (
                !window.confirm(
                  `删除「${item.name}」？（存在子知识 / 任务 / 学习记录 / 附件时会被拒绝，保护数据）`
                )
              )
                return;
              await deleteLearningItem(item.id);
              await refresh();
            }}
          />
        ) : selectedItem == null ? (
          <div className="knowledge__editor-empty">
            <p className="muted">选择左侧一个知识，开始整理你的学习内容。</p>
          </div>
        ) : (
          <>
            <div className="knowledge__editor-head">
              <div className="knowledge__breadcrumb">{breadcrumb}</div>
              <div className="knowledge__title-row">
                <input
                  className="knowledge__title"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  onBlur={() => void commitTitle()}
                  onKeyDown={handleTitleKeyDown}
                  spellCheck={false}
                />
                <select
                  className="knowledge__mastery"
                  value={selectedItem.mastery_status}
                  onChange={(e) => void handleMasteryChange(e.target.value)}
                  title="掌握状态"
                >
                  {MASTERY_STATUSES.map((s) => (
                    <option key={s} value={s}>
                      {MASTERY_LABELS[s as MasteryStatus]}
                    </option>
                  ))}
                </select>
              </div>
              <div className="knowledge__stats">
                {stats && stats.session_count > 0 && (
                  <>
                    <span>累计学习 {formatDuration(stats.study_seconds)}</span>
                    <span>学习 {stats.session_count} 次</span>
                    <span>最近 {formatDateTime(stats.last_studied_at)}</span>
                  </>
                )}
                {stats && stats.evaluation_count > 0 && (
                  <span>验证 {stats.evaluation_count} 次</span>
                )}
                <button
                  className="knowledge__eval-btn"
                  onClick={async () => {
                    const s = await startSession(selectedItem.id);
                    navigate(`/learn/${s.id}`);
                  }}
                  title="开始学习这个知识"
                >
                  开始学习
                </button>
                <button
                  className="knowledge__eval-btn"
                  onClick={() => void runAiCheck()}
                  title="AI 检查当前知识"
                >
                  ✨ AI 检查
                </button>
                <button
                  className="knowledge__eval-btn"
                  onClick={() => setOrganizeOpen(true)}
                  title="AI 帮我整理知识"
                >
                  ✨ AI 帮我整理
                </button>
                <button
                  className="knowledge__eval-btn"
                  onClick={() => setShowEvalModal(true)}
                  title="记录一次验证"
                >
                  记录验证
                </button>
              </div>

            </div>

            {/* 我的知识（DEV-0028 §86：最大主编辑区；引导文字仅空态 placeholder） */}
            <div className="knowledge__mine-label">我的知识</div>
            <textarea
              className="knowledge__content"
              value={content}
              onChange={(e) => handleContentChange(e.target.value)}
              placeholder={
                "记录你真正学到的东西……\n\n你可以写：\n• 自己的理解\n• 核心概念\n• 例子\n• 容易混淆的地方\n• 当前问题\n• 学习总结"
              }
              spellCheck={false}
            />
            <div className="knowledge__editor-foot">
              <span className="knowledge__foot-note">
                {selectedItem.mastery_status === "mastered"
                  ? "已标记掌握 · 继续用验证巩固"
                  : "学习后可用「记录验证」检验掌握情况"}
              </span>
              <span className={"knowledge__save-status knowledge__save-status--" + saveStatus}>
                {saveStatus === "dirty" && "未保存…"}
                {saveStatus === "saving" && "正在保存…"}
                {saveStatus === "saved" && "已保存 ✓"}
                {saveStatus === "error" && (
                  <>
                    保存失败{" "}
                    <button
                      className="knowledge__retry"
                      onClick={() => void flushSave()}
                    >
                      重试
                    </button>
                  </>
                )}
              </span>
            </div>

              {/* 学习记录（DEV-0017：Session = 学习历史，与知识正文分离） */}
              <div className="knowledge__recent-evals">
                <div className="knowledge__recent-evals-head">
                  <span className="knowledge__recent-evals-title">学习记录</span>
                  <span className="muted" style={{ fontSize: 11 }}>
                    共 {stats?.session_count ?? itemSessions.length} 次（显示最近 {itemSessions.length}）
                  </span>
                </div>
                {itemSessions.length === 0 ? (
                  <span className="muted" style={{ fontSize: 12 }}>
                    还没有学习记录。点击「开始学习」即可创建。
                  </span>
                ) : (
                  <ul className="k-sessions">
                    {itemSessions.map((s) => {
                      const isOpen = expandedSession === s.id;
                      const noteLen = (s.note ?? "").trim().length;
                      const sessAtts = itemAttachments.filter((a) => a.session_id === s.id);
                      return (
                        <li key={s.id} className="k-sessions__item">
                          <button
                            className="k-sessions__head"
                            onClick={() => setExpandedSession(isOpen ? null : s.id)}
                          >
                            <span>{formatDateTime(s.started_at)}</span>
                            <span className="muted">
                              学习 {formatDuration(s.duration_seconds)} · 笔记 {noteLen} 字
                              {sessAtts.length > 0 && ` · 附件 ${sessAtts.length}`}
                              {s.status === "active" && " · 进行中"}
                            </span>
                            <span className="k-sessions__arrow">{isOpen ? "▾" : "▸"}</span>
                          </button>
                          {isOpen && (
                            <div className="k-sessions__body">
                              <div className="muted" style={{ fontSize: 11 }}>
                                {formatDateTime(s.started_at)} → {s.ended_at ? formatDateTime(s.ended_at) : "进行中"}
                              </div>
                              {editingSession?.id === s.id ? (
                                <>
                                  <textarea
                                    className="modal__input"
                                    rows={6}
                                    value={editNoteValue}
                                    onChange={(e) => setEditNoteValue(e.target.value)}
                                  />
                                  <div className="btn-row" style={{ marginTop: 6 }}>
                                    <button
                                      className="btn btn--small btn--primary"
                                      onClick={async () => {
                                        await updateSessionNote(s.id, editNoteValue);
                                        setEditingSession(null);
                                        if (selectedId != null) void reloadItemEvidence(selectedId);
                                      }}
                                    >
                                      保存笔记
                                    </button>
                                    <button className="btn btn--small" onClick={() => setEditingSession(null)}>
                                      取消
                                    </button>
                                  </div>
                                </>
                              ) : (
                                <>
                                  {noteLen > 0 ? (
                                    <pre className="k-sessions__note">{s.note}</pre>
                                  ) : (
                                    <span className="muted" style={{ fontSize: 12 }}>
                                      本次没有笔记
                                    </span>
                                  )}
                                  <div className="btn-row" style={{ marginTop: 6 }}>
                                    <button
                                      className="btn btn--small"
                                      onClick={() => {
                                        setEditingSession(s);
                                        setEditNoteValue(s.note ?? "");
                                      }}
                                    >
                                      编辑笔记
                                    </button>
                                    {noteLen > 0 && (
                                      <button
                                        className="btn btn--small"
                                        onClick={async () => {
                                          if (!window.confirm("清空这条学习笔记？时间记录将保留。")) return;
                                          await updateSessionNote(s.id, "");
                                          if (selectedId != null) void reloadItemEvidence(selectedId);
                                        }}
                                      >
                                        清空笔记
                                      </button>
                                    )}
                                  </div>
                                </>
                              )}
                              {sessAtts.length > 0 && (
                                <div style={{ marginTop: 8 }}>
                                  <AttachmentList
                                    attachments={sessAtts}
                                    onChanged={(list) => {
                                      const removed = sessAtts.filter(
                                        (a) => !list.some((x) => x.id === a.id)
                                      );
                                      setItemAttachments((prev) =>
                                        prev.filter((a) => !removed.some((r) => r.id === a.id))
                                      );
                                    }}
                                    readOnly
                                  />
                                </div>
                              )}
                            </div>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>

              {/* 知识独立附件（DEV-0018：不要求先开始 Session） */}
              <div className="knowledge__recent-evals">
                <div className="knowledge__recent-evals-head">
                  <span className="knowledge__recent-evals-title">知识附件</span>
                  <div className="btn-row">
                    <button
                      className="knowledge__eval-btn"
                      onClick={async () => {
                        if (!activeProfile || !selectedItem) return;
                        const filters = [
                          { name: "图片", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"] },
                        ];
                        const sel = await openDialog({ multiple: false, directory: false, filters });
                        const p = Array.isArray(sel) ? sel[0] : sel;
                        if (!p) return;
                        try {
                          const att = await addLearningAttachment({
                            profileId: activeProfile.id,
                            learningItemId: selectedItem.id,
                            attachmentType: "image",
                            sourcePath: p,
                          });
                          setItemAttachments((a) => [...a, att]);
                        } catch (e) {
                          setError(String(e));
                        }
                      }}
                    >
                      上传图片
                    </button>
                    <button
                      className="knowledge__eval-btn"
                      onClick={async () => {
                        if (!activeProfile || !selectedItem) return;
                        const filters = [
                          { name: "视频", extensions: ["mp4", "webm", "mov", "mkv"] },
                        ];
                        const sel = await openDialog({ multiple: false, directory: false, filters });
                        const p = Array.isArray(sel) ? sel[0] : sel;
                        if (!p) return;
                        try {
                          const att = await addLearningAttachment({
                            profileId: activeProfile.id,
                            learningItemId: selectedItem.id,
                            attachmentType: "video",
                            sourcePath: p,
                          });
                          setItemAttachments((a) => [...a, att]);
                        } catch (e) {
                          setError(String(e));
                        }
                      }}
                    >
                      上传视频
                    </button>
                    <button className="knowledge__eval-btn" onClick={() => setShowItemDraw(true)}>
                      画图
                    </button>
                  </div>
                </div>
                <AttachmentList
                  attachments={itemAttachments.filter((a) => a.session_id == null)}
                  onChanged={(list) =>
                    setItemAttachments((prev) => [
                      ...prev.filter((a) => a.session_id != null),
                      ...list,
                    ])
                  }
                />
              </div>

              {/* 学习证据（DEV-0028 §89：最近验证 + 问题，默认折叠，不抢占正文） */}
              <details className="knowledge-evidence">
                <summary>
                  学习证据（最近验证 {recentEvals.length} · 待处理问题 {itemFeedbacks.length}）
                </summary>

                {/* 最近验证 */}
                <div className="knowledge__recent-evals">
                  <div className="knowledge__recent-evals-head">
                    <span className="knowledge__recent-evals-title">最近验证</span>
                  </div>
                  {recentEvals.length === 0 ? (
                    <div className="knowledge__recent-evals-empty">
                      <span className="muted">还没有验证记录</span>
                      <button
                        className="knowledge__eval-btn knowledge__eval-btn--first"
                        onClick={() => setShowEvalModal(true)}
                      >
                        记录第一次验证
                      </button>
                    </div>
                  ) : (
                    <ul className="knowledge__recent-evals-list">
                      {recentEvals.map((ev) => (
                        <li key={ev.id} className="knowledge__recent-evals-item">
                          <span className="knowledge__recent-evals-date">
                            {formatDateTime(ev.occurred_at)}
                          </span>
                          <span className="knowledge__recent-evals-type">
                            {EVALUATION_TYPE_LABELS[ev.evaluation_type as EvaluationType] ??
                              ev.evaluation_type}
                          </span>
                          <span
                            className={
                              "knowledge__recent-evals-outcome knowledge__recent-evals-outcome--" +
                              (ev.outcome ?? "unrated")
                            }
                          >
                            {OUTCOME_LABELS[(ev.outcome ?? "unrated") as Outcome]}
                            {ev.correct_items != null && ev.total_items != null && (
                              <>（{ev.correct_items}/{ev.total_items}）</>
                            )}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>

                {/* 需要关注（open Feedback，可安排重新学习/解决/忽略） */}
                <div className="knowledge__recent-evals">
                  <div className="knowledge__recent-evals-head">
                    <span className="knowledge__recent-evals-title">需要关注</span>
                    {itemFeedbacks.length === 0 && (
                      <button
                        className="knowledge__eval-btn knowledge__eval-btn--first"
                        onClick={() => setShowFeedbackModal(true)}
                      >
                        + 记录问题
                      </button>
                    )}
                  </div>
                  {itemFeedbacks.length === 0 ? (
                    <span className="muted" style={{ fontSize: 12 }}>
                      暂无待处理问题
                    </span>
                  ) : (
                    <ul className="review-feedback">
                      {itemFeedbacks.map((f) => (
                        <FeedbackCard
                          key={f.id}
                          feedback={f}
                          onChanged={() => {
                            if (selectedId != null) void reloadItemEvidence(selectedId);
                          }}
                        />
                      ))}
                    </ul>
                  )}
                </div>
              </details>
          </>
        )}
        {error && <div className="knowledge__error">{error}</div>}
      </main>

      {/* 统一验证 Modal（自动带入当前 Profile/Goal + 选中 LearningItem） */}
      {showEvalModal && selectedItem && (
        <EvaluationModal
          profileId={selectedItem.profile_id}
          goalId={selectedItem.goal_id}
          learningItemId={selectedItem.id}
          defaultTitle={`${selectedItem.name}验证`}
          onClose={() => setShowEvalModal(false)}
          onCreated={() => {
            setShowEvalModal(false);
            void reloadItemEvidence(selectedItem.id);
          }}
        />
      )}

      {/* 统一问题反馈 Modal（自动带入当前 Goal + 选中 LearningItem） */}
      {showFeedbackModal && selectedItem && (
        <FeedbackModal
          goalId={selectedItem.goal_id}
          learningItemId={selectedItem.id}
          defaultTitle={`${selectedItem.name}：待描述的问题`}
          onClose={() => setShowFeedbackModal(false)}
          onCreated={() => {
            setShowFeedbackModal(false);
            void reloadItemEvidence(selectedItem.id);
          }}
        />
      )}

      {/* 知识独立画图（session_id = null） */}
      {showItemDraw && activeProfile && selectedItem && (
        <DrawModal
          onClose={() => setShowItemDraw(false)}
          onSave={async (dataUrl) => {
            const att = await saveDrawingAttachment({
              profileId: activeProfile.id,
              learningItemId: selectedItem.id,
              dataBase64: dataUrl,
            });
            setShowItemDraw(false);
            setItemAttachments((a) => [...a, att]);
          }}
        />
      )}

      {/* ✨ AI 帮我整理（DEV-0021：Proposal + Diff + 用户确认） */}
      {organizeOpen && activeProfile && selectedItem && (
        <AiProposalReview
          profileId={activeProfile.id}
          action="knowledge_organize"
          learningItemId={selectedItem.id}
          items={items}
          onClose={() => setOrganizeOpen(false)}
          onApplied={() => {
            if (selectedId != null) void reloadItemEvidence(selectedId);
          }}
        />
      )}

      {/* DEV-0034 §65：树节点「移动到…」（Knowledge 选择器；排除自身与后代） */}
      {treeMoveFor && (
        <div className="modal-overlay" onClick={() => setTreeMoveFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">移动「{treeMoveFor.name}」到…</div>
            <p className="muted" style={{ fontSize: 12 }}>
              选择新的上级知识（不能移动到自身或其子节点下）。
            </p>
            <div className="taskmodal__list">
              <button
                className="taskmodal__item"
                onClick={async () => {
                  await moveLearningItem(treeMoveFor.id, null);
                  setTreeMoveFor(null);
                  await refresh();
                }}
              >
                作为顶级知识
              </button>
              {(() => {
                const banned = descendantsOf(items, treeMoveFor.id);
                return items
                  .filter(
                    (i) =>
                      i.id !== treeMoveFor.id &&
                      !banned.has(i.id) &&
                      i.goal_id === treeMoveFor.goal_id
                  )
                  .map((t) => (
                    <button
                      key={t.id}
                      className="taskmodal__item"
                      onClick={async () => {
                        await moveLearningItem(treeMoveFor.id, t.id);
                        setTreeMoveFor(null);
                        await refresh();
                      }}
                    >
                      {t.name}
                    </button>
                  ));
              })()}
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setTreeMoveFor(null)}>取消</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );

  /** ✨ AI 检查当前知识（DEV-0022：统一进入右侧 AI Panel）。 */
  async function runAiCheck() {
    if (!selectedId) return;
    await aiRunAction("knowledge_analysis");
  }

  /** 递归渲染树节点（收起操作按钮到 ··· 菜单；DEV-0305 拖拽排序/改父子）。 */
  function renderNode(node: TreeNode, parentOrder: TreeNode[]): React.ReactElement {
    const hasChildren = node.children.length > 0;
    const isOpen = !!expanded[node.id];
    const isSelected = node.id === selectedId;
    const isDropTarget = dragOverId === node.id;

    /** 拖拽落下：落到自己上=同级排序；落到其他节点=成为其子（或按 sort_hint 平级） */
    async function handleDrop(e: React.DragEvent) {
      e.preventDefault();
      e.stopPropagation();
      setDragOverId(null);
      const raw = e.dataTransfer.getData("text/knode");
      if (!raw) return;
      const srcId = Number(raw);
      if (!srcId || srcId === node.id) return;
      // 禁止拖到自身后代
      if (descendantsOf(items, srcId).has(node.id)) return;
      try {
        // 默认：成为目标节点的最后一个子节点（改父子关系）
        await moveLearningItem(srcId, node.id);
        setExpanded((p) => ({ ...p, [node.id]: true }));
        await refresh();
      } catch (err) {
        setError(String(err));
      }
    }

    return (
      <li
        key={node.id}
        className={
          "knowledge__node" +
          (isDropTarget ? " knowledge__node--drop" : "")
        }
        onDragOver={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setDragOverId(node.id);
        }}
        onDragLeave={() => setDragOverId((cur) => (cur === node.id ? null : cur))}
        onDrop={handleDrop}
      >
        <div
          className={
            "knowledge__node-row" + (isSelected ? " knowledge__node-row--active" : "")
          }
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData("text/knode", String(node.id));
            e.dataTransfer.effectAllowed = "move";
          }}
        >
          <button
            className={
              "knowledge__node-toggle" + (hasChildren ? "" : " knowledge__node-toggle--leaf")
            }
            onClick={() => setExpanded((p) => ({ ...p, [node.id]: !isOpen }))}
            tabIndex={hasChildren ? 0 : -1}
          >
            {hasChildren ? (isOpen ? "▾" : "▸") : "·"}
          </button>

          {renamingId === node.id ? (
            <input
              autoFocus
              className="knowledge__new-input knowledge__new-input--inline"
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleRename(node.id);
                if (e.key === "Escape") setRenamingId(null);
              }}
              onBlur={() => void handleRename(node.id)}
            />
          ) : (
            <button
              className="knowledge__node-name"
              onClick={() => void selectNode(node.id)}
              onDoubleClick={() => void selectNode(node.id)}
              title={node.name}
            >
              {node.name}
            </button>
          )}

          <span className="knowledge__node-order">
            <button
              className="knowledge__node-arrow"
              title="上移"
              disabled={parentOrder.findIndex((n) => n.id === node.id) === 0}
              onClick={() => void handleReorder(parentOrder, node.id, -1)}
            >
              ↑
            </button>
            <button
              className="knowledge__node-arrow"
              title="下移"
              disabled={
                parentOrder.findIndex((n) => n.id === node.id) === parentOrder.length - 1
              }
              onClick={() => void handleReorder(parentOrder, node.id, 1)}
            >
              ↓
            </button>
          </span>

          <button
            className="knowledge__node-menu-btn"
            title="更多操作"
            onClick={(e) => {
              e.stopPropagation();
              setMenuForId(menuForId === node.id ? null : node.id);
            }}
          >
            ···
          </button>

          {menuForId === node.id && (
            <>
              <div className="knowledge__menu-backdrop" onClick={() => setMenuForId(null)} />
              <div className="knowledge__menu">
                <button
                  className="knowledge__menu-item"
                  onClick={() => {
                    setMenuForId(null);
                    setExpanded((p) => ({ ...p, [node.id]: true }));
                    setAddingChildFor(node.id);
                    setChildName("");
                  }}
                >
                  新建子知识
                </button>
                <button
                  className="knowledge__menu-item"
                  onClick={() => {
                    setMenuForId(null);
                    setRenamingId(node.id);
                    setRenameValue(node.name);
                  }}
                >
                  重命名
                </button>
                <button
                  className="knowledge__menu-item"
                  onClick={() => {
                    setMenuForId(null);
                    setTreeMoveFor(node);
                  }}
                >
                  移动到…
                </button>
                <button
                  className="knowledge__menu-item knowledge__menu-item--danger"
                  onClick={() => void handleDelete(node.id)}
                >
                  删除
                </button>
              </div>
            </>
          )}
        </div>

        {isOpen && hasChildren && (
          <ul className="knowledge__nodes knowledge__nodes--child">
            {node.children.map((c) => renderNode(c, node.children))}
          </ul>
        )}

        {addingChildFor === node.id && (
          <div className="knowledge__add-child">
            <input
              autoFocus
              className="knowledge__new-input"
              placeholder="子知识名称，Enter 确认"
              value={childName}
              onChange={(e) => setChildName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCreateChild(node.id);
                if (e.key === "Escape") setAddingChildFor(null);
              }}
              onBlur={() => {
                if (childName.trim()) void handleCreateChild(node.id);
                else setAddingChildFor(null);
              }}
            />
          </div>
        )}
      </li>
    );
  }
}

export default Knowledge;
