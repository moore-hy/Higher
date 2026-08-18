import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  addDocumentAttachment,
  addDocumentAttachmentFromBase64,
  correctSessionTime,
  createChildLearningItem,
  createKnowledgeDocument,
  createRootLearningItem,
  deleteKnowledgeDocument,
  deleteLearningItem,
  deleteSession,
  getKnowledgeWorkspace,
  getLearningItemPath,
  listEvaluationsByLearningItem,
  listFeedbacksByLearningItem,
  listGoalsByProfile,
  listLearningItemsByGoal,
  listLearningItemsLight,
  listUnassignedSessions,
  moveLearningItem,
  organizeSessionIntoKnowledge,
  reorderLearningItems,
  renameKnowledgeDocument,
  saveDocumentDrawing,
  startSession,
  updateKnowledgeDocument,
  updateLearningItem,
  updateLearningItemStatus,
  updateSessionTitle,
} from "../api";
import AttachmentList from "../components/AttachmentList";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import EvaluationModal from "../components/EvaluationModal";
import FeedbackCard from "../components/FeedbackCard";
import FeedbackModal from "../components/FeedbackModal";
import KnowledgeFlow from "../components/KnowledgeFlow";
import RichDocEditor, {
  noteToDocument,
  type MediaSaveAdapter,
} from "../components/RichDocEditor";
import { durationShort, studyClockHHMM } from "../components/DailyActivitiesSection";
import type { JSONContent } from "@tiptap/react";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type {
  Evaluation,
  Feedback,
  Goal,
  KnowledgeDocument,
  KnowledgeWorkspaceData,
  LightLearningItem,
  StudySession,
} from "../types";
import {
  MASTERY_LABELS,
  MASTERY_STATUSES,
  EVALUATION_TYPE_LABELS,
  OUTCOME_LABELS,
} from "../types";
import type { MasteryStatus, EvaluationType, Outcome } from "../types";
import { formatDateTime, formatDurationCompact, studyDayOf, todayDate } from "../utils";

/** 后代集合（移动目标排除；DEV-0034）。 */
function descendantsOf(items: LightLearningItem[], id: number): Set<number> {
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

/** 树节点（LightLearningItem + children；DEV-0057 §153-155 树不再携带 content 正文）。 */
interface TreeNode extends LightLearningItem {
  children: TreeNode[];
}

/** 把扁平列表组装成森林（按 parent_id）。 */
function buildTree(items: LightLearningItem[]): TreeNode[] {
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
function computeFullPath(items: LightLearningItem[], id: number): string {
  const byId = new Map<number, LightLearningItem>();
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

// ---------- DEV-0051 Workspace V2 工具 ----------

/** UTC datetime（SQLite）→ 本地 HH:MM（时间线卡左侧时间）。 */
function utcHHMM(raw: string | null | undefined): string {
  if (!raw) return "—";
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return raw;
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** UTC datetime（SQLite）→ 本地 yyyy-MM-dd HH:mm（Header 统计行「最近」）。 */
function fmtFullDateTime(raw: string | null | undefined): string {
  if (!raw) return "—";
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return raw;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** UTC datetime（SQLite）→ datetime-local 输入值（本地时区；修正时间用）。 */
function utcToLocalInput(raw: string | null | undefined): string {
  if (!raw) return "";
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** datetime-local 输入值（本地）→ 后端 SQLite "YYYY-MM-DD HH:MM:SS"（UTC）。 */
function localInputToDbUtc(value: string): string {
  const d = new Date(value);
  if (isNaN(d.getTime())) return value;
  return d.toISOString().slice(0, 19).replace("T", " ");
}

/** 昨天日期 YYYY-MM-DD（本地时区；时间线分组）。 */
function yesterdayDate(): string {
  const d = new Date();
  d.setDate(d.getDate() - 1);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** 时间线分组标题：今天 / 昨天 / 更早（学习日 = UTC+8 日历日）。 */
function dayGroupLabel(at: string): string {
  const day = studyDayOf(at);
  if (day === todayDate()) return "今天";
  if (day === yesterdayDate()) return "昨天";
  return "更早";
}

/** 粗计 JSON 中某节点类型出现次数（图片 / 代码块摘要计数）。 */
function countNodeType(json: string | null, type: string): number {
  if (!json) return 0;
  const needle = `"type":"${type}"`;
  let count = 0;
  let i = json.indexOf(needle);
  while (i !== -1) {
    count++;
    i = json.indexOf(needle, i + needle.length);
  }
  return count;
}

/** 文档自动保存延迟（DEV-0051 §37：900ms debounce）。 */
const DOC_SAVE_DELAY_MS = 900;

type SaveStatus = "saved" | "dirty" | "saving" | "error";

/** 文档待保存快照（含 profileId，卸载兜底不依赖闭包）。 */
interface PendingDocSave {
  profileId: number;
  id: number;
  title: string;
  plain: string;
  json: string | null;
}

/**
 * 知识体系工作区 V2（DEV-0051）。
 *
 * 左侧：知识树（搜索 / 展开折叠 / ··· 菜单 / 新建 / 拖拽）
 * 右侧：Workspace（§27 Header → §28-34 内容时间线 → §36-37 文档详情模式）
 *
 * 产品语言：对用户呈现"知识 / 知识体系"，代码内部仍为 LearningItem。
 */
function Knowledge() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const goalIdParam = searchParams.get("goal");
  const itemParam = searchParams.get("item");
  const fromParam = searchParams.get("from");

  const { activeProfile, refreshKey } = useActiveProfile();

  const [goals, setGoals] = useState<Goal[]>([]);
  /** DEV-0057 §153-155：树数据源 = light 列表（无 content；正文仅在打开 item 时加载） */
  const [items, setItems] = useState<LightLearningItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const goalId = goalIdParam ? Number(goalIdParam) : null;

  // 选中节点与标题
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [title, setTitle] = useState("");

  // DEV-0051 §49：Workspace 聚合（文档 / 学习记录 / Legacy 附件 / 统计）
  const [workspace, setWorkspace] = useState<KnowledgeWorkspaceData | null>(null);

  // 记录验证 Modal（DEV-0012：知识详情直接验证，自动带入当前 Goal + Item）
  const [showEvalModal, setShowEvalModal] = useState(false);

  // 需要关注（DEV-0013：当前知识的 open Feedback + 记录入口）
  const [itemFeedbacks, setItemFeedbacks] = useState<Feedback[]>([]);
  const [showFeedbackModal, setShowFeedbackModal] = useState(false);
  const [recentEvals, setRecentEvals] = useState<Evaluation[]>([]);

  const [viewMode, setViewMode] = useState<"workspace" | "graph">("workspace");

  // DEV-0053 §50-52：未归类学习（learning_item_id IS NULL 的 Session；虚拟入口，非 Knowledge Node）
  const [unassigned, setUnassigned] = useState<StudySession[]>([]);
  const [showUnassigned, setShowUnassigned] = useState(false);
  /** 「整理进知识」选择器（当前目标下的知识节点） */
  const [organizeFor, setOrganizeFor] = useState<StudySession | null>(null);
  const [organizeSearch, setOrganizeSearch] = useState("");
  const [organizeBusy, setOrganizeBusy] = useState(false);

  // DEV-0051 §36-37：文档详情模式
  const [editingDoc, setEditingDoc] = useState<KnowledgeDocument | null>(null);
  const [docTitle, setDocTitle] = useState("");
  const [docTitleFocus, setDocTitleFocus] = useState(false);
  const [docSave, setDocSave] = useState<SaveStatus>("saved");

  // 时间线卡片「更多 ⋯」菜单（session / document）
  const [sessionMenuFor, setSessionMenuFor] = useState<number | null>(null);
  const [docMenuFor, setDocMenuFor] = useState<number | null>(null);
  const [renamingSessionId, setRenamingSessionId] = useState<number | null>(null);
  const [renameSessionValue, setRenameSessionValue] = useState("");
  const [renamingDocId, setRenamingDocId] = useState<number | null>(null);
  const [renameDocValue, setRenameDocValue] = useState("");
  const [timeFixFor, setTimeFixFor] = useState<{
    id: number;
    start: string;
    end: string;
  } | null>(null);

  // DEV-0022：AI 统一进入右侧 Panel；并上报当前知识上下文
  const { runAction: aiRunAction, setPageContext } = useAiPanel();

  // DEV-0055 §172：开始学习 Start Guard（与 Today/Planning 同一 ActiveSessionConflict 模式）
  const { conflict: startConflict, guard: guardStart, close: closeStart } =
    useActiveSessionConflict();

  // DEV-0041：小屏（<900px）知识树切换为 Drawer
  const [treeDrawerOpen, setTreeDrawerOpen] = useState(false);

  // 左树交互状态
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [search, setSearch] = useState("");
  const [menuForId, setMenuForId] = useState<number | null>(null);
  // DEV-0034 §65：树节点「移动到…」
  const [treeMoveFor, setTreeMoveFor] = useState<LightLearningItem | null>(null);
  // DEV-0305：拖拽排序/改父子
  const [dragOverId, setDragOverId] = useState<number | null>(null);
  const [creatingRoot, setCreatingRoot] = useState(false);
  const [rootName, setRootName] = useState("");
  const [addingChildFor, setAddingChildFor] = useState<number | null>(null);
  const [childName, setChildName] = useState("");
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");

  // ---- 文档自动保存（refs 避免闭包过期）----
  const docTimerRef = useRef<number | null>(null);
  const pendingDocRef = useRef<PendingDocSave | null>(null);
  const docTitleRef = useRef("");

  const selectedItem = useMemo(
    () => items.find((i) => i.id === selectedId) ?? null,
    [items, selectedId]
  );

  /** 立即执行待保存的文档（切节点 / 返回列表 / 卸载前调用，禁止丢内容）。 */
  const flushDocSave = useCallback(async () => {
    const pending = pendingDocRef.current;
    if (!pending) return;
    pendingDocRef.current = null;
    if (docTimerRef.current != null) {
      window.clearTimeout(docTimerRef.current);
      docTimerRef.current = null;
    }
    setDocSave("saving");
    try {
      const updated = await updateKnowledgeDocument(
        pending.profileId,
        pending.id,
        pending.title,
        pending.plain,
        pending.json
      );
      if (pendingDocRef.current == null) setDocSave("saved");
      setWorkspace((w) =>
        w
          ? {
              ...w,
              documents: w.documents.map((d) => (d.id === updated.id ? updated : d)),
            }
          : w
      );
    } catch (e) {
      // 失败不静默：恢复 pending 让用户可重试
      pendingDocRef.current = pending;
      setDocSave("error");
      setError(String(e));
    }
  }, []);

  // 卸载前尽力保存（正常路径 debounce 已落库；此为兜底）
  useEffect(() => {
    return () => {
      const pending = pendingDocRef.current;
      if (pending) {
        void updateKnowledgeDocument(
          pending.profileId,
          pending.id,
          pending.title,
          pending.plain,
          pending.json
        ).catch(() => {});
      }
    };
  }, []);

  const handleDocChange = useCallback(
    (doc: JSONContent, plainText: string) => {
      if (editingDoc == null || activeProfile == null) return;
      setDocSave("dirty");
      pendingDocRef.current = {
        profileId: activeProfile.id,
        id: editingDoc.id,
        title: docTitleRef.current.trim() || editingDoc.title,
        plain: plainText,
        json: JSON.stringify(doc),
      };
      if (docTimerRef.current != null) window.clearTimeout(docTimerRef.current);
      docTimerRef.current = window.setTimeout(() => {
        void flushDocSave();
      }, DOC_SAVE_DELAY_MS);
    },
    [editingDoc, activeProfile, flushDocSave]
  );

  /** §49：Workspace 聚合加载（selectedItem 变化 / 增删后刷新）。 */
  const loadWorkspace = useCallback(
    async (itemId: number) => {
      if (!activeProfile) return;
      try {
        const w = await getKnowledgeWorkspace(activeProfile.id, itemId);
        setWorkspace(w);
      } catch (e) {
        setError(String(e));
      }
    },
    [activeProfile]
  );

  /** 切换选中节点：先保证未保存文档落库，再切换。 */
  const selectNode = useCallback(
    async (id: number, expandAncestors = false) => {
      await flushDocSave();
      setError("");
      setEditingDoc(null);
      setShowUnassigned(false);
      const item = items.find((i) => i.id === id);
      if (!item) return;
      setSelectedId(id);
      setTitle(item.name);
      setWorkspace(null);
      setSessionMenuFor(null);
      setDocMenuFor(null);
      setRenamingSessionId(null);
      setRenamingDocId(null);
      setTimeFixFor(null);
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
      // DEV-0022：上报当前知识上下文（item_id + 面包屑路径）
      let pathLabel = item.name;
      try {
        pathLabel = await getLearningItemPath(id);
      } catch {
        /* 退化用名称 */
      }
      setPageContext({
        page: "knowledge",
        pageLabel: "知识体系",
        learningItemId: id,
        knowledgePath: pathLabel,
      });
      await loadWorkspace(id);
    },
    [items, flushDocSave, loadWorkspace, setPageContext]
  );

  /** 验证 / 问题 / 学习记录变更后刷新证据与聚合。 */
  async function reloadItemEvidence(id: number) {
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
    void loadWorkspace(id);
  }

  // ---- 数据加载 ----
  // 注意：goals 必须无条件加载（否则无 ?goal= 参数直接进入时
  // 会因 goals 为空而误显示"没有学习目标"——DEV-0011 修复的真实 Bug）。
  // DEV-0057 §153-155：树数据源 = list_learning_items_light（不含 content 正文）。
  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const goalList = await listGoalsByProfile(activeProfile!.id);
      setGoals(goalList);
      // DEV-0059 §6.10：Goal 是可筛选维度，不是权限门——无 goal 参数时加载该档案全部
      // learning_items（含 goal_id=NULL 的节点）
      const lightList = await listLearningItemsLight(activeProfile!.id);
      setItems(goalId == null ? lightList : lightList.filter((i) => i.goal_id === goalId));
      // 当前选中节点失效（被删 / 切档）时清空工作区
      if (selectedId != null && !lightList.some((i) => i.id === selectedId)) {
        setSelectedId(null);
        setTitle("");
        setWorkspace(null);
        setEditingDoc(null);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [goalId, activeProfile, refreshKey, selectedId]);

  useEffect(() => {
    void flushDocSave();
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  // DEV-0053 §51：未归类学习列表与计数（Profile Scope，独立于当前 goal 刷新）
  const reloadUnassigned = useCallback(async () => {
    if (!activeProfile) return;
    try {
      setUnassigned(await listUnassignedSessions(activeProfile.id, 50));
    } catch {
      /* 保持原值 */
    }
  }, [activeProfile]);

  useEffect(() => {
    void reloadUnassigned();
  }, [reloadUnassigned, refreshKey]);

  /** §52：整理进知识（只更新 session.learning_item_id，不复制笔记） */
  async function organizeUnassigned(itemId: number) {
    if (!organizeFor || !activeProfile) return;
    setOrganizeBusy(true);
    setError("");
    try {
      await organizeSessionIntoKnowledge(activeProfile.id, organizeFor.id, itemId);
      setOrganizeFor(null);
      setOrganizeSearch("");
      await reloadUnassigned();
    } catch (e) {
      setError(String(e));
    } finally {
      setOrganizeBusy(false);
    }
  }

  // DEV-0059 §6.10：Goal 筛选为可选维度；无 goal 参数时显示全部（含 goal_id=NULL 节点），
  // 不再强制自动选中第一个 goal（避免把 Goal 当成 Knowledge 权限门）。

  // ?item= 参数：自动定位并打开指定知识节点（今日任务"打开知识"入口）
  // DEV-0051 §43：?from=knowledge&item=N（学习后回来）→ 选中后清掉导航参数（保留 goal）
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
      if (fromParam === "knowledge") {
        const next = new URLSearchParams(searchParams);
        next.delete("from");
        next.delete("item");
        setSearchParams(next, { replace: true });
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, itemParam, fromParam]);

  // ---- 树操作 ----
  async function handleCreateRoot() {
    const name = rootName.trim();
    if (!name || !activeProfile) return;
    try {
      // §6.10：goalId 可为 null（无 Goal 档案 → goal_id NULL）
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
    if (!name || !activeProfile) return;
    try {
      // §6.10：goalId 可为 null（后端 create_child_for_profile 继承 parent.goal_id）
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

  /**
   * DEV-0057 §153-155：light 树没有 description 字段，而 update_learning_item
   * 会整字段覆盖 description（null = 清空）——重命名提交前按需取回一次原值。
   */
  async function renameItemPreservingDesc(id: number, name: string) {
    let description: string | undefined;
    if (goalId != null) {
      try {
        const full = await listLearningItemsByGoal(goalId);
        description = full.find((i) => i.id === id)?.description ?? undefined;
      } catch {
        /* 取回失败不阻塞重命名 */
      }
    }
    await updateLearningItem(id, name, description);
  }

  async function handleRename(id: number) {
    const name = renameValue.trim();
    const item = items.find((i) => i.id === id);
    if (!name || !item || name === item.name) {
      setRenamingId(null);
      return;
    }
    try {
      await renameItemPreservingDesc(id, name);
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
    // DEV-0057 §153-155：light 树无 content，旧的 hasContent 二次确认随之移除；
    // 正文事实仍由后端 safe_delete 的关联数据保护。
    if (!window.confirm(`确认删除「${item.name}」？`)) return;
    try {
      await deleteLearningItem(id);
      if (selectedId === id) {
        setSelectedId(null);
        setTitle("");
        setWorkspace(null);
        setEditingDoc(null);
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
      await renameItemPreservingDesc(selectedItem.id, name);
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

  // ---- 开始学习 / 新建文档（§35） ----

  async function handleStartSession() {
    if (selectedItem == null) return;
    try {
      const s = await startSession(selectedItem.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      // §172：已有进行中学习 → Start Guard 弹窗（继续/结束/取消），不再裸抛错误
      if (guardStart(e)) return;
      setError(String(e));
    }
  }

  async function handleCreateDocument() {
    if (selectedItem == null || activeProfile == null) return;
    setError("");
    try {
      await flushDocSave();
      const created = await createKnowledgeDocument(activeProfile.id, selectedItem.id);
      await loadWorkspace(selectedItem.id);
      setEditingDoc(created);
      setDocTitle(created.title);
      setDocTitleFocus(true);
      setDocSave("saved");
    } catch (e) {
      setError(String(e));
    }
  }

  // ---- 文档详情模式（§36-37） ----

  function openDocumentDetail(doc: KnowledgeDocument) {
    setDocMenuFor(null);
    setEditingDoc(doc);
    setDocTitle(doc.title);
    setDocTitleFocus(false);
    setDocSave("saved");
  }

  async function handleBackToContent() {
    await flushDocSave();
    setEditingDoc(null);
    setDocTitle("");
    if (selectedId != null) void loadWorkspace(selectedId);
  }

  // 标题跟随输入；Enter / 失焦 → rename
  useEffect(() => {
    docTitleRef.current = docTitle;
  }, [docTitle]);

  async function commitDocTitle() {
    setDocTitleFocus(false);
    if (editingDoc == null || activeProfile == null) return;
    const name = docTitle.trim();
    if (!name || name === editingDoc.title) return;
    try {
      const updated = await renameKnowledgeDocument(activeProfile.id, editingDoc.id, name);
      setEditingDoc(updated);
      setWorkspace((w) =>
        w
          ? {
              ...w,
              documents: w.documents.map((d) => (d.id === updated.id ? updated : d)),
            }
          : w
      );
    } catch (e) {
      setError(String(e));
    }
  }

  function handleDocTitleKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  }

  /** §15 persistence adapter：编辑器媒体 → 文档附件链。 */
  const docSaveMedia = useCallback<MediaSaveAdapter>(
    async (args) => {
      if (activeProfile == null || selectedItem == null || editingDoc == null) {
        throw new Error("文档未打开");
      }
      const att =
        args.base64 != null
          ? await addDocumentAttachmentFromBase64(
              activeProfile.id,
              selectedItem.id,
              editingDoc.id,
              args.kind,
              args.fileName,
              args.mime,
              args.base64
            )
          : await addDocumentAttachment(
              activeProfile.id,
              selectedItem.id,
              editingDoc.id,
              args.kind,
              args.sourcePath!
            );
      return { id: att.id, file_name: att.file_name };
    },
    [activeProfile, selectedItem, editingDoc]
  );

  /** §15 画图适配器：文档链（禁止回退 Session 链）。 */
  const docSaveDrawing = useCallback(
    async (dataBase64: string) => {
      if (activeProfile == null || selectedItem == null || editingDoc == null) {
        throw new Error("文档未打开");
      }
      const att = await saveDocumentDrawing(
        activeProfile.id,
        selectedItem.id,
        editingDoc.id,
        dataBase64
      );
      return { id: att.id, file_name: att.file_name };
    },
    [activeProfile, selectedItem, editingDoc]
  );

  /** 编辑器初始文档：content_document_json 优先；NULL → 纯文本构造。 */
  const editingDocInitial = useMemo<JSONContent | null>(() => {
    if (editingDoc == null) return null;
    if (editingDoc.content_document_json) {
      try {
        return JSON.parse(editingDoc.content_document_json) as JSONContent;
      } catch {
        /* 回退纯文本构造 */
      }
    }
    return noteToDocument(editingDoc.content_text);
  }, [editingDoc]);

  // ---- 时间线卡片操作（§32-34） ----

  async function commitSessionTitle(id: number) {
    const name = renameSessionValue.trim();
    setRenamingSessionId(null);
    if (!name) return;
    try {
      await updateSessionTitle(id, name);
      if (selectedId != null) await loadWorkspace(selectedId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function commitTimeFix() {
    if (timeFixFor == null || !timeFixFor.start || selectedId == null) return;
    try {
      await correctSessionTime(
        timeFixFor.id,
        localInputToDbUtc(timeFixFor.start),
        timeFixFor.end ? localInputToDbUtc(timeFixFor.end) : null
      );
      setTimeFixFor(null);
      await loadWorkspace(selectedId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeSessionCard(id: number) {
    setSessionMenuFor(null);
    if (!window.confirm("删除这条学习记录？时间与笔记记录将一并删除，无法恢复。")) return;
    try {
      await deleteSession(id);
      if (selectedId != null) await loadWorkspace(selectedId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function commitDocCardRename(id: number) {
    const name = renameDocValue.trim();
    setRenamingDocId(null);
    if (!name || activeProfile == null) return;
    try {
      await renameKnowledgeDocument(activeProfile.id, id, name);
      if (selectedId != null) await loadWorkspace(selectedId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeDocCard(id: number) {
    setDocMenuFor(null);
    if (activeProfile == null) return;
    const doc = workspace?.documents.find((d) => d.id === id);
    if (!doc) return;
    if (!window.confirm(`删除文档「${doc.title}」？该操作无法恢复。`)) return;
    try {
      await deleteKnowledgeDocument(activeProfile.id, id);
      if (selectedId != null) await loadWorkspace(selectedId);
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

  /** §30：document × session 合并时间线（倒序；UTC 字符串比较即可）。 */
  type TimelineRow =
    | {
        kind: "session";
        at: string;
        s: KnowledgeWorkspaceData["sessions"][number];
      }
    | { kind: "document"; at: string; d: KnowledgeDocument };
  const timeline = useMemo<TimelineRow[]>(() => {
    if (!workspace) return [];
    const rows: TimelineRow[] = [
      ...workspace.documents.map((d) => ({
        kind: "document" as const,
        at: d.updated_at,
        d,
      })),
      ...workspace.sessions.map((s) => ({
        kind: "session" as const,
        at: s.started_at,
        s,
      })),
    ];
    rows.sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0));
    return rows;
  }, [workspace]);

  // DEV-0059 §6.10：Goal 不再是 Knowledge 权限门——无 Goal 档案也正常渲染知识树
  // （items 已含 goal_id=NULL 节点；树空态见下方"还没有知识内容"）

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
        {/* §6.10：Goal 是可选筛选维度；有 Goal 才显示选择器（含「全部」） */}
        {goals.length > 0 && (
          <div className="knowledge__goal">
            <span className="knowledge__goal-label">筛选目标</span>
            <select
              className="knowledge__goal-select"
              value={goalId ?? ""}
              onChange={(e) => {
                setSelectedId(null);
                setWorkspace(null);
                setEditingDoc(null);
                const v = e.target.value;
                if (v === "") {
                  const next = new URLSearchParams(searchParams);
                  next.delete("goal");
                  setSearchParams(next, { replace: true });
                } else {
                  setSearchParams({ goal: v });
                }
              }}
            >
              <option value="">全部（含无目标）</option>
              {goals.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </div>
        )}

        {/* DEV-0053 §50-51：未归类学习虚拟入口（非 Knowledge Node；Quick Study 未归类 Session） */}
        <div className="kws__unassigned">
          <button
            className={
              "kws__unassigned-btn" + (showUnassigned ? " kws__unassigned-btn--active" : "")
            }
            onClick={() => setShowUnassigned(true)}
            title="没有关联任何知识的学习记录（快速学习「先学，再归档」）"
          >
            未归类学习 ({unassigned.length})
          </button>
        </div>

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
              <p className="knowledge__empty-title">还没有知识内容</p>
              <p className="muted">
                你可以先新建一个主题，也可以先去快速学习，稍后再整理。
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
                (viewMode === "workspace" && !showUnassigned ? " review-window__item--active" : "")
              }
              onClick={() => {
                setShowUnassigned(false);
                setViewMode("workspace");
              }}
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

        {showUnassigned ? (
          /* ============ 未归类学习列表（DEV-0053 §51-52；极简行 + 整理进知识） ============ */
          <div className="kws kws--unassigned">
            <div className="kws__detail-bar">
              <button className="kws__back" onClick={() => setShowUnassigned(false)}>
                ← 返回知识
              </button>
              <span className="muted" style={{ fontSize: 12 }}>
                未关联任何知识的学习记录（{unassigned.length} 条，最多显示 50）
              </span>
            </div>
            {unassigned.length === 0 ? (
              <p className="muted kws__empty">
                没有未归类学习。快速学习结束后选择知识，或在这里整理。
              </p>
            ) : (
              <ul className="kws__unassigned-list">
                {unassigned.map((s) => (
                  <li key={s.id} className="actrow">
                    <button
                      className="actrow__main"
                      onClick={() => navigate(`/learn/${s.id}`)}
                      title="打开这条学习记录"
                    >
                      <span className="actrow__title">{s.title || `学习记录 #${s.id}`}</span>
                      <span className="actrow__time">
                        {studyDayOf(s.started_at).slice(5)} {studyClockHHMM(s.started_at)} ·{" "}
                        {durationShort(s.duration_seconds)}
                      </span>
                    </button>
                    <button
                      className="btn btn--small"
                      onClick={() => {
                        setOrganizeFor(s);
                        setOrganizeSearch("");
                      }}
                    >
                      整理进知识
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : viewMode === "graph" ? (
          <KnowledgeFlow
            items={items}
            profileId={activeProfile?.id ?? null}
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
              await renameItemPreservingDesc(item.id, name);
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
        ) : editingDoc != null ? (
          /* ============ 文档详情模式（DEV-0051 §36-37） ============ */
          <div className="kws kws--doc">
            <div className="kws__detail-bar">
              <button className="kws__back" onClick={() => void handleBackToContent()}>
                ← 返回内容
              </button>
              <span
                className={
                  "knowledge__save-status knowledge__save-status--" + docSave
                }
              >
                {docSave === "dirty" && "未保存…"}
                {docSave === "saving" && "正在保存…"}
                {docSave === "saved" && "已保存 ✓"}
                {docSave === "error" && (
                  <>
                    保存失败{" "}
                    <button
                      className="knowledge__retry"
                      onClick={() => void flushDocSave()}
                    >
                      重试
                    </button>
                  </>
                )}
              </span>
            </div>
            <div className="knowledge__breadcrumb">
              {breadcrumb.split(" > ").join(" › ")} › {editingDoc.title}
            </div>
            <div className="knowledge__title-row">
              <input
                className="knowledge__title"
                value={docTitle}
                autoFocus={docTitleFocus}
                onChange={(e) => setDocTitle(e.target.value)}
                onBlur={() => void commitDocTitle()}
                onKeyDown={handleDocTitleKeyDown}
                spellCheck={false}
              />
            </div>
            <RichDocEditor
              key={"kdoc-" + editingDoc.id}
              profileId={activeProfile?.id ?? 0}
              learningItemId={selectedItem.id}
              sessionId={null}
              initialDocument={editingDocInitial}
              initialLegacyNote={null}
              onChange={handleDocChange}
              saveMedia={docSaveMedia}
              saveDrawing={docSaveDrawing}
            />
          </div>
        ) : (
          /* ============ Workspace V2 主区（DEV-0051 §27-34 / §45-46） ============ */
          <div className="kws">
            {/* Header（§27） */}
            <div className="kws__head">
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
              {workspace != null &&
                (workspace.session_count > 0 || workspace.documents.length > 0) && (
                  <div className="kws__stats">
                    累计学习 {formatDurationCompact(workspace.study_seconds)} · 学习{" "}
                    {workspace.session_count} 次 · 文档 {workspace.documents.length} 篇 · 最近{" "}
                    {fmtFullDateTime(workspace.last_studied_at)}
                  </div>
                )}
              <div className="kws__actions">
                <button
                  className="knowledge__eval-btn knowledge__eval-btn--first"
                  onClick={() => void handleStartSession()}
                  title="开始学习这个知识"
                >
                  开始学习
                </button>
                <button
                  className="knowledge__eval-btn knowledge__eval-btn--first"
                  onClick={() => void handleCreateDocument()}
                  title="新建一篇文档（DEV-0051 §35）"
                >
                  + 新建文档
                </button>
                <button
                  className="knowledge__eval-btn knowledge__eval-btn--first"
                  onClick={() => void runAiCheck()}
                  title="AI 分析当前知识"
                >
                  ✨ AI 分析
                </button>
                <button
                  className="knowledge__eval-btn knowledge__eval-btn--first"
                  onClick={() => setShowEvalModal(true)}
                  title="记录一次验证"
                >
                  记录验证
                </button>
              </div>
            </div>

            {/* 内容时间线（§28-34） */}
            {workspace == null ? (
              <p className="muted kws__loading">加载中…</p>
            ) : workspace.documents.length === 0 && workspace.sessions.length === 0 ? (
              <p className="muted kws__empty">
                还没有内容——新建一篇文档，或开始一次学习。
              </p>
            ) : (
              <div className="kws__timeline">
                {timeline.map((row, idx) => {
                  const label = dayGroupLabel(row.at);
                  const prevLabel = idx > 0 ? dayGroupLabel(timeline[idx - 1].at) : null;
                  const showGroup = label !== prevLabel;
                  if (row.kind === "session") {
                    const s = row.s;
                    const note = (s.note_plain || "").trim();
                    const mediaParts: string[] = [];
                    if (s.image_count > 0) mediaParts.push(`图片 ${s.image_count}`);
                    if (s.video_count > 0) mediaParts.push(`视频 ${s.video_count}`);
                    const noteLines = note
                      .split(/\r?\n/)
                      .map((l) => l.trim())
                      .filter(Boolean);
                    return (
                      <Fragment key={"s" + s.id}>
                        {showGroup && <div className="kws__group-title">{label}</div>}
                        <article className="kws__card">
                          <div className="kws__card-meta">
                            <span className="kws__card-time">
                              {utcHHMM(s.started_at)}
                            </span>
                            <span className="kws__card-kind">学习记录</span>
                            {s.status === "active" && (
                              <span className="kws__card-flag">进行中</span>
                            )}
                          </div>
                          {renamingSessionId === s.id ? (
                            <div className="kws__rename">
                              <input
                                autoFocus
                                className="modal__input"
                                value={renameSessionValue}
                                onChange={(e) => setRenameSessionValue(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") void commitSessionTitle(s.id);
                                  if (e.key === "Escape") setRenamingSessionId(null);
                                }}
                              />
                              <div className="btn-row">
                                <button
                                  className="btn btn--small btn--primary"
                                  onClick={() => void commitSessionTitle(s.id)}
                                >
                                  保存
                                </button>
                                <button
                                  className="btn btn--small"
                                  onClick={() => setRenamingSessionId(null)}
                                >
                                  取消
                                </button>
                              </div>
                            </div>
                          ) : (
                            <>
                              <div className="kws__card-title">
                                {s.title || selectedItem.name}
                              </div>
                              <div className="kws__card-sub">
                                学习 {formatDurationCompact(s.duration_seconds)}
                              </div>
                              {note ? (
                                <>
                                  <p className="kws__card-summary">
                                    {noteLines.slice(0, 3).join("\n")}
                                    {noteLines.length > 3 ? " …" : ""}
                                  </p>
                                  {mediaParts.length > 0 && (
                                    <div className="kws__card-media">
                                      {mediaParts.join(" · ")}
                                    </div>
                                  )}
                                </>
                              ) : mediaParts.length > 0 ? (
                                <div className="kws__card-media">
                                  {mediaParts.join(" · ")}
                                </div>
                              ) : (
                                <div className="kws__card-summary kws__card-summary--muted">
                                  （无笔记）
                                </div>
                              )}
                              <div className="kws__card-actions">
                                <button
                                  className="kws__card-btn"
                                  title="在学习工作区打开这条记录"
                                  onClick={() =>
                                    navigate(
                                      `/learn/${s.id}?from=knowledge&item=${selectedItem.id}`
                                    )
                                  }
                                >
                                  打开
                                </button>
                                <button
                                  className="kws__card-btn"
                                  title="再次开始学习这个知识"
                                  onClick={() => void handleStartSession()}
                                >
                                  继续学习
                                </button>
                                <button
                                  className="kws__card-btn"
                                  onClick={() =>
                                    setSessionMenuFor(
                                      sessionMenuFor === s.id ? null : s.id
                                    )
                                  }
                                >
                                  更多 ⋯
                                </button>
                              </div>
                            </>
                          )}
                          {sessionMenuFor === s.id && (
                            <>
                              <div
                                className="kws__menu-backdrop"
                                onClick={() => setSessionMenuFor(null)}
                              />
                              <div className="kws__menu">
                                <button
                                  className="kws__menu-item"
                                  onClick={() => {
                                    setSessionMenuFor(null);
                                    setRenamingSessionId(s.id);
                                    setRenameSessionValue(s.title);
                                  }}
                                >
                                  修改标题
                                </button>
                                <button
                                  className="kws__menu-item"
                                  onClick={() => {
                                    setSessionMenuFor(null);
                                    setTimeFixFor({
                                      id: s.id,
                                      start: utcToLocalInput(s.started_at),
                                      end: "",
                                    });
                                  }}
                                >
                                  修正时间
                                </button>
                                <button
                                  className="kws__menu-item kws__menu-item--danger"
                                  onClick={() => void removeSessionCard(s.id)}
                                >
                                  删除
                                </button>
                              </div>
                            </>
                          )}
                        </article>
                      </Fragment>
                    );
                  }
                  const d = row.d;
                  const text = (d.content_text || "").trim();
                  const previewLines = text
                    ? text
                        .split(/\r?\n/)
                        .map((l) => l.trim())
                        .filter(Boolean)
                        .slice(0, 2)
                    : [];
                  const preview = previewLines.join(" ");
                  const images = countNodeType(d.content_document_json, "higherImage");
                  const codes = countNodeType(d.content_document_json, "codeBlock");
                  const docMeta: string[] = [];
                  if (images > 0) docMeta.push(`图片 ${images}`);
                  if (codes > 0) docMeta.push(`代码块 ${codes}`);
                  return (
                    <Fragment key={"d" + d.id}>
                      {showGroup && <div className="kws__group-title">{label}</div>}
                      <article className="kws__card">
                        <div className="kws__card-meta">
                          <span className="kws__card-time">{utcHHMM(d.updated_at)}</span>
                          <span className="kws__card-kind kws__card-kind--doc">文档</span>
                        </div>
                        {renamingDocId === d.id ? (
                          <div className="kws__rename">
                            <input
                              autoFocus
                              className="modal__input"
                              value={renameDocValue}
                              onChange={(e) => setRenameDocValue(e.target.value)}
                              onKeyDown={(e) => {
                                if (e.key === "Enter") void commitDocCardRename(d.id);
                                if (e.key === "Escape") setRenamingDocId(null);
                              }}
                            />
                            <div className="btn-row">
                              <button
                                className="btn btn--small btn--primary"
                                onClick={() => void commitDocCardRename(d.id)}
                              >
                                保存
                              </button>
                              <button
                                className="btn btn--small"
                                onClick={() => setRenamingDocId(null)}
                              >
                                取消
                              </button>
                            </div>
                          </div>
                        ) : (
                          <>
                            <div className="kws__card-title">{d.title}</div>
                            {preview ? (
                              <p className="kws__card-summary">
                                {preview.length > 120 ? preview.slice(0, 120) + "…" : preview}
                              </p>
                            ) : docMeta.length > 0 ? null : (
                              <div className="kws__card-summary kws__card-summary--muted">
                                （空白文档）
                              </div>
                            )}
                            {docMeta.length > 0 && (
                              <div className="kws__card-media">{docMeta.join(" · ")}</div>
                            )}
                            <div className="kws__card-actions">
                              <button
                                className="kws__card-btn"
                                onClick={() => openDocumentDetail(d)}
                              >
                                打开
                              </button>
                              <button
                                className="kws__card-btn"
                                onClick={() => openDocumentDetail(d)}
                              >
                                编辑
                              </button>
                              <button
                                className="kws__card-btn"
                                onClick={() =>
                                  setDocMenuFor(docMenuFor === d.id ? null : d.id)
                                }
                              >
                                更多 ⋯
                              </button>
                            </div>
                          </>
                        )}
                        {docMenuFor === d.id && (
                          <>
                            <div
                              className="kws__menu-backdrop"
                              onClick={() => setDocMenuFor(null)}
                            />
                            <div className="kws__menu">
                              <button
                                className="kws__menu-item"
                                onClick={() => {
                                  setDocMenuFor(null);
                                  setRenamingDocId(d.id);
                                  setRenameDocValue(d.title);
                                }}
                              >
                                重命名
                              </button>
                              <button
                                className="kws__menu-item kws__menu-item--danger"
                                onClick={() => void removeDocCard(d.id)}
                              >
                                删除
                              </button>
                            </div>
                          </>
                        )}
                      </article>
                    </Fragment>
                  );
                })}
              </div>
            )}

            {/* Legacy 附件区（§45-46） */}
            {workspace != null && workspace.legacy_attachments.length > 0 && (
              <div className="kws__legacy">
                <div className="kws__legacy-head">
                  未归入文档的附件 · {workspace.legacy_attachments.length}
                </div>
                <AttachmentList
                  attachments={workspace.legacy_attachments}
                  onChanged={(list) =>
                    setWorkspace((w) =>
                      w ? { ...w, legacy_attachments: list } : w
                    )
                  }
                />
              </div>
            )}

            {/* 学习证据（DEV-0028 §89：最近验证 + 问题，默认折叠，不抢主区） */}
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
          </div>
        )}
        {error && <div className="knowledge__error">{error}</div>}
      </main>

      {/* 统一验证 Modal（自动带入当前 Profile/Goal + 选中 LearningItem） */}
      {showEvalModal && selectedItem && (
        <EvaluationModal
          profileId={activeProfile?.id ?? 0}
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

      {/* DEV-0051 §33：学习记录「修正时间」小 Modal */}
      {timeFixFor && (
        <div className="modal-overlay" onClick={() => setTimeFixFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">修正学习时间</div>
            <div className="kws__fixrow">
              <label>开始</label>
              <input
                type="datetime-local"
                value={timeFixFor.start}
                onChange={(e) =>
                  setTimeFixFor((t) => (t ? { ...t, start: e.target.value } : t))
                }
              />
              <label>结束</label>
              <input
                type="datetime-local"
                value={timeFixFor.end}
                onChange={(e) =>
                  setTimeFixFor((t) => (t ? { ...t, end: e.target.value } : t))
                }
              />
            </div>
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void commitTimeFix()}>
                保存
              </button>
              <button className="btn" onClick={() => setTimeFixFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* DEV-0053 §52：未归类学习「整理进知识」（选 Knowledge Node → organize_session_into_knowledge） */}
      {organizeFor && (
        <div className="modal-overlay" onClick={() => setOrganizeFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">
              把「{organizeFor.title || `学习记录 #${organizeFor.id}`}」整理进哪个知识？
            </div>
            <p className="evmodal__note muted">
              只更新这条学习记录的知识关联，笔记不会被复制或修改。
            </p>
            <div className="taskmodal__picker">
              <input
                className="modal__input taskmodal__search"
                value={organizeSearch}
                onChange={(e) => setOrganizeSearch(e.target.value)}
                placeholder="搜索知识…"
              />
            </div>
            <div className="taskmodal__list">
              {(organizeSearch.trim()
                ? items.filter((i) =>
                    i.name.toLowerCase().includes(organizeSearch.trim().toLowerCase())
                  )
                : items
              )
                .slice(0, 30)
                .map((i) => (
                  <button
                    key={i.id}
                    className="taskmodal__item"
                    disabled={organizeBusy}
                    onClick={() => void organizeUnassigned(i.id)}
                  >
                    {i.name}
                  </button>
                ))}
              {items.length === 0 && (
                <span className="muted" style={{ fontSize: 12 }}>
                  当前目标下还没有知识节点，先在左侧新建。
                </span>
              )}
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setOrganizeFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
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

      {/* DEV-0055 §172：开始学习 Start Guard 冲突弹窗（与 Today/Planning 同模式） */}
      <ActiveSessionConflictModal
        conflict={startConflict}
        onClose={closeStart}
        onResolved={() => void refresh()}
      />
    </div>
  );

  /** ✨ AI 分析当前知识（DEV-0022 统一入口；DEV-0051 合并为单一「✨ AI 分析」）。 */
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
