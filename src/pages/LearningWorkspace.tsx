import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  attachSession,
  confirmSessionDuration,
  correctSessionTime,
  createChildLearningItem,
  createRootLearningItem,
  deleteSession,
  endSession,
  getLearningItemPath,
  getLearningTotals,
  getSession,
  listAllTasksByProfile,
  listAttachmentsBySession,
  listLearningItemsByProfile,
  listGoalsByProfile,
  listTodayTasksByProfile,
  organizeSessionIntoKnowledge,
  startQuickSession,
  startTaskSession,
  updateLearningItemContent,
  updateSessionDocument,
  updateSessionTitle,
} from "../api";
import AttachmentList from "../components/AttachmentList";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import { durationShort } from "../components/DailyActivitiesSection";
import { minutesShort } from "../components/DailyTasksSection";
import RichDocEditor, {
  documentToPlainText,
  noteToDocument,
} from "../components/RichDocEditor";
import type { JSONContent } from "@tiptap/react";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { Goal, LearningAttachment, LearningItem, LearningTotals, StudySession, Task } from "../types";
import { formatDurationCompact, formatDurationTimer } from "../utils";

type SaveStatus = "saved" | "dirty" | "saving" | "error";

/** End Sheet 展开的子面板（§60 五个整理选项中的三个需要表单）。 */
type SheetTab = "none" | "link" | "new" | "append";

/** UTC datetime（SQLite）→ 本地 HH:MM。 */
function utcHHMM(raw: string | null): string {
  if (!raw) return "—";
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return raw;
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** UTC datetime（SQLite）→ datetime-local 输入值（本地时区）。 */
function utcToLocalInput(raw: string | null): string {
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

/** 追加到知识正文的模板（§64）：日期 + Session 标题 + 全文（纯文本投影）。 */
function buildAppendText(title: string, plainText: string): string {
  const dateLabel = new Date().toLocaleDateString("zh-CN");
  return `\n\n## ${dateLabel} 学习记录：${title}\n${plainText.trim()}`;
}

/**
 * Learning Workspace 最终版（BATCH-04 / DEV-0043「Study First / Archive Later」）。
 *
 * - 进入即写；debounce 自动保存；学习中不显示任何 Goal/归档/验证/分类打扰（§56）
 * - 结束顺序（§57-58）：flush note → endSession → Session 永久成为历史 → End Sheet
 *   （关闭 Sheet 也不丢记录）
 * - End Sheet（§59-66）：统计 + 「你想如何整理本次学习？」五选项 + 开始下一个
 * - 历史 Session 重开（§67-69）：编辑器照常可编辑 + [保存并关闭] + ⋯ 低频菜单
 *   （修正学习时间 / 删除这条学习记录 §70）
 */
export default function LearningWorkspace() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const { activeProfile } = useActiveProfile();

  const [session, setSession] = useState<StudySession | null>(null);
  const [item, setItem] = useState<LearningItem | null>(null);
  const [taskTitle, setTaskTitle] = useState<string | null>(null);
  /** DEV-0049：Tiptap 文档（初始由 note_document_json 或旧 note 构造 §3.4） */
  const [doc, setDoc] = useState<JSONContent | null>(null);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved");
  const [attachments, setAttachments] = useState<LearningAttachment[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  /** DEV-0054 Start Guard：开始下一个的 Active Session 冲突弹窗 */
  const { conflict: startConflict, guard: guardStart, close: closeStart } =
    useActiveSessionConflict();

  /** 本次访问内刚结束（§57 endSession 已成功）：显示结束后视图 */
  const [justEnded, setJustEnded] = useState(false);
  /** End Sheet（§59） */
  const [endSheetOpen, setEndSheetOpen] = useState(false);
  const [sheetTab, setSheetTab] = useState<SheetTab>("none");
  const [busy, setBusy] = useState(false);

  /** DEV-0055 §93-99 Completion 反馈：结束后聚合（今天累计 / 今日任务 / 知识归属两级） */
  const [endTotals, setEndTotals] = useState<LearningTotals | null>(null);
  const [endPath, setEndPath] = useState<string | null>(null);

  /** End Sheet：整理候选与新建表单 */
  const [items, setItems] = useState<LearningItem[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [linkSearch, setLinkSearch] = useState("");
  const [newName, setNewName] = useState("");
  const [newParent, setNewParent] = useState<number | null>(null);
  const [newGoalId, setNewGoalId] = useState<number | "">("");
  const [appendSearch, setAppendSearch] = useState("");
  const [appendTarget, setAppendTarget] = useState<LearningItem | null>(null);

  /** 开始下一个（§66）：今天未完成 Task + 快速学习 */
  const [nextOpen, setNextOpen] = useState(false);
  const [todayTasks, setTodayTasks] = useState<Task[]>([]);

  /** Header 标题编辑（§54；Esc 还原 = 不提交） */
  const [titleEditing, setTitleEditing] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const titleEscapedRef = useRef(false);

  /** 历史视图 ⋯ 菜单（§69-70） */
  const [menuOpen, setMenuOpen] = useState(false);
  const [timeFixOpen, setTimeFixOpen] = useState(false);
  const [fixStart, setFixStart] = useState("");
  const [fixEnd, setFixEnd] = useState("");
  const [deleteOpen, setDeleteOpen] = useState(false);

  const { runAction: aiRunAction, setPageContext } = useAiPanel();

  const timerRef = useRef<number | null>(null);
  const latestNote = useRef("");
  /** 最新文档与纯文本投影（flush 用，避免闭包过期） */
  const latestDoc = useRef<JSONContent | null>(null);
  const latestPlain = useRef("");

  /** 历史 Session（status=completed 且非本次刚结束）重开 → 历史记录编辑模式（§67-68） */
  const isHistory = session != null && session.status === "completed" && !justEnded;

  const load = useCallback(async () => {
    if (!sessionId || !activeProfile) return;
    setLoading(true);
    setError("");
    setJustEnded(false);
    setEndSheetOpen(false);
    setSheetTab("none");
    try {
      const s = await getSession(Number(sessionId));
      if (!s) {
        setError("学习会话不存在");
        return;
      }
      setSession(s);
      // §3.4 兼容：note_document_json 优先；NULL → 旧 note（v2 JSON/纯文本）构造；都空 → 空文档
      let initialDoc: JSONContent;
      if (s.note_document_json) {
        try {
          initialDoc = JSON.parse(s.note_document_json) as JSONContent;
        } catch {
          initialDoc = noteToDocument(s.note);
        }
      } else {
        initialDoc = noteToDocument(s.note);
      }
      setDoc(initialDoc);
      latestDoc.current = initialDoc;
      const plain0 = documentToPlainText(initialDoc);
      latestPlain.current = plain0;
      latestNote.current = plain0;
      const [itemList, atts, goalList, taskList] = await Promise.all([
        listLearningItemsByProfile(activeProfile.id),
        listAttachmentsBySession(Number(sessionId)),
        listGoalsByProfile(activeProfile.id),
        listAllTasksByProfile(activeProfile.id).catch(() => [] as Task[]),
      ]);
      setAttachments(atts);
      setItems(itemList);
      setGoals(goalList);
      const it = itemList.find((i) => i.id === s.learning_item_id) ?? null;
      setItem(it);
      setTaskTitle(s.task_id != null ? taskList.find((t) => t.id === s.task_id)?.title ?? null : null);
      let pathLabel = s.learning_item_id?.toString() ?? "自由学习";
      if (it) {
        try {
          pathLabel = await getLearningItemPath(it.id);
        } catch {
          pathLabel = it.name;
        }
      }
      setPageContext({
        page: "learning",
        pageLabel: "学习工作区",
        learningItemId: s.learning_item_id,
        sessionId: s.id,
        sessionTitle: s.title,
        knowledgePath: pathLabel,
        learningName: it?.name,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionId, activeProfile, setPageContext]);

  useEffect(() => {
    load();
  }, [load]);

  /** 文档变化 → debounce 保存（§9：note 纯文本投影 + document JSON 同事务原子写） */
  const handleDocChange = useCallback(
    (next: JSONContent, plainText: string) => {
      setDoc(next);
      latestDoc.current = next;
      latestPlain.current = plainText;
      if (!session) return;
      setSaveStatus("dirty");
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => void flushNote(), 900);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session]
  );

  const flushNote = useCallback(async () => {
    if (!session) return false;
    const docNow = latestDoc.current;
    const plain = latestPlain.current;
    if (docNow == null) return true;
    setSaveStatus("saving");
    try {
      // §10：两字段同一条命令原子写入（成功或失败一起）
      await updateSessionDocument(session.id, plain, JSON.stringify(docNow));
      latestNote.current = plain;
      setSaveStatus("saved");
      return true;
    } catch {
      setSaveStatus("error");
      return false;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  useEffect(() => {
    const handler = (e: BeforeUnloadEvent) => {
      if (latestNote.current !== latestPlain.current) {
        e.preventDefault();
      }
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, []);

  /** 拉取「开始下一个」候选（今天未完成 Task） */
  const loadNextCandidates = useCallback(async () => {
    if (!activeProfile) return;
    try {
      const list = await listTodayTasksByProfile(activeProfile.id);
      setTodayTasks(list.filter((t) => t.status !== "completed"));
    } catch {
      setTodayTasks([]);
    }
  }, [activeProfile]);

  /** 标题保存（§54：Enter/失焦生效；Esc 还原） */
  async function commitTitle() {
    if (!session) return;
    const next = titleDraft.trim();
    setTitleEditing(false);
    if (!next || next === session.title) return;
    try {
      await updateSessionTitle(session.id, next);
      setSession({ ...session, title: next });
    } catch (e) {
      setError(String(e));
    }
  }

  /** 结束学习（§57-58）：flush → 成功 → endSession → Session 永久成为历史 → End Sheet */
  async function requestEnd() {
    if (!session) return;
    setError("");
    const ok = await flushNote();
    if (!ok) {
      setError("笔记尚未保存，暂不结束学习。请重试保存后再结束。");
      return;
    }
    try {
      const s = await endSession(session.id);
      setSession(s);
      setJustEnded(true);
      setEndSheetOpen(true);
      void loadNextCandidates();
      void loadEndFeedback(s);
    } catch (e) {
      setError(String(e));
    }
  }

  /** §95-97：结束后拉取今天累计 + 今日任务 + 知识归属（最多两级） */
  async function loadEndFeedback(s: StudySession) {
    if (!activeProfile) return;
    try {
      setEndTotals(await getLearningTotals(activeProfile.id));
    } catch {
      setEndTotals(null);
    }
    await refreshEndPath(s.learning_item_id);
  }

  /** item 完整路径 → 最多两级（§97：`数学 / 高等数学`） */
  async function refreshEndPath(itemId: number | null) {
    if (itemId == null) {
      setEndPath(null);
      return;
    }
    try {
      const full = await getLearningItemPath(itemId);
      setEndPath(full.split(" > ").slice(0, 2).join(" / "));
    } catch {
      setEndPath(null);
    }
  }

  /** §98「现在整理」：现有知识选择器（organize_session_into_knowledge，只建关联） */
  async function organizeCompletion(itemId: number) {
    if (!session || !activeProfile) return;
    setBusy(true);
    setError("");
    try {
      await organizeSessionIntoKnowledge(activeProfile.id, session.id, itemId);
      const s = await getSession(session.id);
      if (s) setSession(s);
      setItem(items.find((i) => i.id === itemId) ?? null);
      await refreshEndPath(itemId);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 历史模式：保存并关闭（§68） */
  async function saveAndClose() {
    const ok = await flushNote();
    if (!ok) {
      setError("笔记尚未保存。请重试后再关闭。");
      return;
    }
    navigate(-1);
  }

  /** C 新建知识（§63）：名称* + 父节点可选 + Goal 可选（默认不关联） */
  async function createAndLink() {
    if (!session || !activeProfile) return;
    if (!newName.trim()) {
      setError("请填写新知识名称");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const goalId = newGoalId === "" ? null : Number(newGoalId);
      const created = newParent
        ? await createChildLearningItem(activeProfile.id, newParent, goalId, newName.trim())
        : await createRootLearningItem(activeProfile.id, goalId, newName.trim());
      await attachSession(session.id, created.id, null);
      const [s, itemList] = await Promise.all([
        getSession(session.id),
        listLearningItemsByProfile(activeProfile.id),
      ]);
      if (s) setSession(s);
      setItems(itemList);
      setItem(created);
      closeSheet();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** D 追加到已有知识正文（§64）：用户确认 Preview 后才写入 content */
  async function appendToItem() {
    if (!session || !appendTarget) return;
    setBusy(true);
    setError("");
    try {
      const addition = buildAppendText(session.title, latestPlain.current);
      await updateLearningItemContent(appendTarget.id, (appendTarget.content ?? "") + addition);
      await attachSession(session.id, appendTarget.id, null);
      const s = await getSession(session.id);
      if (s) setSession(s);
      setItem(appendTarget);
      closeSheet();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function closeSheet() {
    setEndSheetOpen(false);
    setSheetTab("none");
    setLinkSearch("");
    setNewName("");
    setNewParent(null);
    setNewGoalId("");
    setAppendSearch("");
    setAppendTarget(null);
    setError("");
  }

  /** 开始下一个（§66）：从任务 / 快速学习；Start Guard 冲突 → 弹窗 */
  async function startNext(fn: () => Promise<StudySession>) {
    setError("");
    try {
      const s = await fn();
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guardStart(e)) return;
      setError(String(e));
    }
  }

  /** 修正学习时间（§69）：低频；后端重算 duration 并标记 corrected */
  async function applyTimeFix() {
    if (!session || !fixStart) return;
    setBusy(true);
    setError("");
    try {
      const s = await correctSessionTime(
        session.id,
        localInputToDbUtc(fixStart),
        fixEnd ? localInputToDbUtc(fixEnd) : null
      );
      setSession(s);
      setTimeFixOpen(false);
      setMenuOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** DEV-0057 §107：>12h 结束 → needs_review；用户确认「无误」→ confirmed（时间数据不动） */
  async function confirmDuration() {
    if (!session) return;
    setBusy(true);
    setError("");
    try {
      const s = await confirmSessionDuration(session.id);
      setSession(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §107「修正时间」：直接打开既有修正 Modal（预填当前起止时间） */
  function openTimeFix() {
    if (!session) return;
    setFixStart(utcToLocalInput(session.started_at));
    setFixEnd(utcToLocalInput(session.ended_at));
    setTimeFixOpen(true);
  }

  /** 删除这条学习记录（§70）：确认后删除，回学习规划 */
  async function removeSession() {
    if (!session) return;
    setBusy(true);
    setError("");
    try {
      await deleteSession(session.id);
      navigate("/planning");
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (isHistory || justEnded || !session) return;
    const t = window.setInterval(() => setTick((x) => x + 1), 1000);
    return () => window.clearInterval(t);
  }, [isHistory, justEnded, session]);

  // DEV-0059 §6.3/§6.4：活跃 Session Timer 每秒真实更新（HH:MM:SS 统一格式）；
  // 历史/刚结束 Session 不跳动，显示 Compact 中文。
  const elapsedLabel = useMemo(() => {
    if (!session) return "";
    if (isHistory || justEnded) return formatDurationCompact(session.duration_seconds);
    const start = new Date(session.started_at.replace(" ", "T") + "Z").getTime();
    const secs = Math.max(0, Math.floor((Date.now() - start) / 1000));
    return formatDurationTimer(secs);
  }, [session, isHistory, justEnded, tick]);

  if (loading) {
    return (
      <div className="page">
        <p className="muted">加载中…</p>
      </div>
    );
  }

  if (error && !session) {
    return (
      <div className="page">
        <div className="alert alert--error">{error}</div>
        <button className="btn" onClick={() => navigate("/")}>返回今日任务</button>
      </div>
    );
  }

  // ===== 统计（基于当前文档：纯文本投影字数 + 媒体计数） =====
  const plainNow = latestPlain.current;
  const noteLen = plainNow.replace(/\s/g, "").length;
  let imgCount = 0;
  let vidCount = 0;
  const countMedia = (n: JSONContent | null | undefined) => {
    if (!n) return;
    if (n.type === "higherImage") imgCount++;
    else if (n.type === "higherVideo") vidCount++;
    for (const c of n.content ?? []) countMedia(c);
  };
  countMedia(doc);
  const linkFiltered = linkSearch.trim()
    ? items.filter((i) => i.name.toLowerCase().includes(linkSearch.trim().toLowerCase()))
    : items;
  const appendFiltered = appendSearch.trim()
    ? items.filter((i) => i.name.toLowerCase().includes(appendSearch.trim().toLowerCase()))
    : items;
  const appendPreviewText = session ? buildAppendText(session.title, plainNow) : "";
  const sourceLabels: string[] = [];
  if (taskTitle && session && taskTitle !== session.title) sourceLabels.push(`任务「${taskTitle}」`);
  if (item && session && item.name !== session.title) sourceLabels.push(`知识「${item.name}」`);

  /** 开始下一个列表（End Sheet 与结束后视图共用） */
  const nextList = (
    <div className="lw-next">
      <button
        className="lw-next__item lw-next__item--quick"
        onClick={() => void startNext(() => startQuickSession(activeProfile!.id))}
      >
        ⚡ 快速学习
      </button>
      {todayTasks.length === 0 ? (
        <span className="muted lw-next__empty">今天没有未完成任务</span>
      ) : (
        todayTasks.slice(0, 8).map((t) => (
          <button
            key={t.id}
            className="lw-next__item"
            onClick={() => void startNext(() => startTaskSession(t.id))}
          >
            ☐ {t.title}
            {t.planned_time ? <span className="muted"> · {t.planned_time}</span> : null}
          </button>
        ))
      )}
    </div>
  );

  /**
   * §95-99 Completion 第一层反馈（End Sheet 顶部 + 结束后视图共用）。
   * 禁止烟花 / XP / 排名 / 努力分（§99）——只显示事实。
   * inSheet=true 时「以后整理」直接关闭 Sheet；结束后视图只提供「现在整理」。
   * DEV-0057 §101-107：刚结束且 needs_review（>12h）→ 不祝贺，改确认/修正分支。
   */
  const durationNeedsReview =
    justEnded && session?.duration_review_state === "needs_review";

  const renderCompletion = (inSheet: boolean) =>
    durationNeedsReview ? (
      <div className="completefb completefb--review">
        <div className="completefb__title completefb__title--review">学习已结束</div>
        {session && <div className="completefb__name">{session.title}</div>}
        <div className="completefb__duration">
          记录时长 {durationShort(session?.duration_seconds ?? 0)}
        </div>
        <div className="alert completefb__review-alert">
          这个时长较长，请确认是否准确。
        </div>
        <div className="btn-row">
          <button
            className="btn btn--small btn--primary"
            disabled={busy}
            onClick={() => void confirmDuration()}
          >
            {busy ? "确认中…" : "确认无误"}
          </button>
          <button className="btn btn--small" onClick={openTimeFix}>
            修正时间
          </button>
        </div>
        {inSheet && (
          <button
            className="btn btn--small completefb__review-later"
            onClick={closeSheet}
          >
            暂不处理
          </button>
        )}
      </div>
    ) : (
      <div className="completefb">
        <div className="completefb__title">学习完成 ✓</div>
        {session && <div className="completefb__name">{session.title}</div>}
        <div className="completefb__duration">
          {Math.max(1, Math.round((session?.duration_seconds ?? 0) / 60))} 分钟
        </div>
        <div className="completefb__row">
          <span>
            今天累计 {minutesShort(Math.round((endTotals?.today_seconds ?? 0) / 60))}
          </span>
          <span>
            今日任务 {endTotals?.today_tasks_completed ?? 0}/{endTotals?.today_tasks_total ?? 0}
          </span>
        </div>
        {endPath ? (
          <div className="completefb__path">已记录到 {endPath}</div>
        ) : (
          <div className="completefb__unassigned">
            <span>这次学习还没有整理进知识体系</span>
            <div className="btn-row">
              {inSheet && (
                <button className="btn btn--small" onClick={closeSheet}>
                  以后整理
                </button>
              )}
              <button
                className="btn btn--small btn--primary"
                onClick={() => {
                  setEndSheetOpen(true);
                  setSheetTab("link");
                }}
              >
                现在整理
              </button>
            </div>
          </div>
        )}
      </div>
    );

  return (
    <div className="page page--wide lw">
      {/* ===== Header（§54）===== */}
      <header className="lw__header">
        <div className="lw__title-wrap">
          {titleEditing ? (
            <input
              className="lw__title-input"
              value={titleDraft}
              autoFocus
              onChange={(e) => setTitleDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void commitTitle();
                if (e.key === "Escape") {
                  titleEscapedRef.current = true;
                  setTitleEditing(false);
                }
              }}
              onBlur={() => {
                if (titleEscapedRef.current) {
                  titleEscapedRef.current = false;
                  return;
                }
                void commitTitle();
              }}
            />
          ) : (
            <button
              className="lw__title-btn"
              title="点击修改标题"
              onClick={() => {
                setTitleDraft(session?.title ?? "");
                setTitleEditing(true);
              }}
            >
              {session?.title}
            </button>
          )}
          <div className="lw__meta muted">
            开始 {utcHHMM(session?.started_at ?? null)}
            {session?.time_corrected ? " · 已手动修正" : ""}
            {sourceLabels.length > 0 && ` · 来源：${sourceLabels.join(" · ")}`}
          </div>
        </div>
        <div className="lw__header-right">
          <span className="lw__timer">{elapsedLabel}</span>
          {isHistory ? (
            <>
              <button className="btn btn--primary" onClick={() => void saveAndClose()}>
                保存并关闭
              </button>
              <div className="taskmenu">
                <button className="taskmenu__btn" onClick={() => setMenuOpen(!menuOpen)}>
                  ⋯
                </button>
                {menuOpen && (
                  <div className="taskmenu__pop">
                    <button
                      onClick={() => {
                        setMenuOpen(false);
                        setFixStart(utcToLocalInput(session?.started_at ?? null));
                        setFixEnd(utcToLocalInput(session?.ended_at ?? null));
                        setTimeFixOpen(true);
                      }}
                    >
                      修正学习时间…
                    </button>
                    <button
                      className="taskmenu__danger"
                      onClick={() => {
                        setMenuOpen(false);
                        setDeleteOpen(true);
                      }}
                    >
                      删除这条学习记录…
                    </button>
                  </div>
                )}
              </div>
            </>
          ) : (
            !justEnded && (
              <button className="btn btn--primary" onClick={requestEnd}>
                结束学习
              </button>
            )
          )}
        </div>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={startConflict}
        onClose={closeStart}
        onResolved={() => void loadNextCandidates()}
      />

      {justEnded ? (
        /* ===== 结束后视图（§61 默认 Primary 关闭 Sheet 后；§95-99 Completion 第一层）===== */
        <section className="card lw-ended">
          {renderCompletion(false)}
          <div className="lw-ended__stats">
            <span>笔记 {noteLen} 字</span>
            <span>
              图片 {imgCount} 张{vidCount > 0 ? ` · 视频 ${vidCount}` : ""}
            </span>
            <span>附件 {attachments.length} 个</span>
          </div>
          <div className="btn-row">
            <button className="btn btn--primary" onClick={() => navigate("/")}>
              返回今日
            </button>
            <button className="btn" onClick={() => setNextOpen(!nextOpen)}>
              开始下一个
            </button>
          </div>
          {nextOpen && nextList}
          {/* AI 两入口（保持不动） */}
          <div className="btn-row">
            <button className="btn" onClick={() => void aiRunAction("session_analysis")}>
              ✨ AI 分析本次学习
            </button>
            <button className="btn" onClick={() => void aiRunAction("knowledge_organize")}>
              ✨ AI 帮我整理知识
            </button>
          </div>
          {/* 结束后仍可回看全文（只读） */}
          <div className="lw-ended__review">
            <RichDocEditor
              profileId={activeProfile?.id ?? 0}
              learningItemId={session?.learning_item_id ?? null}
              sessionId={session?.id ?? null}
              initialDocument={doc}
              initialLegacyNote={session?.note ?? null}
              onChange={() => {}}
              readOnly
            />
          </div>
        </section>
      ) : (
        /* ===== 主编辑区（active 与历史编辑模式共用，§55/§68）===== */
        <section className="card lw-editor">
          <div className="lw-editor__head">
            <h2 className="card__title">{isHistory ? "学习笔记（历史记录）" : "本次学习笔记"}</h2>
            <span className={"knowledge__save-status knowledge__save-status--" + saveStatus}>
              {saveStatus === "dirty" && "未保存…"}
              {saveStatus === "saving" && "正在保存…"}
              {saveStatus === "saved" && "已保存 ✓"}
              {saveStatus === "error" && "保存失败 · 点击重试"}
            </span>
            {saveStatus === "error" && (
              <button className="btn btn--small" onClick={() => void flushNote()}>
                重试
              </button>
            )}
          </div>
          {activeProfile && (
            <RichDocEditor
              profileId={activeProfile.id}
              learningItemId={item?.id ?? null}
              sessionId={session?.id ?? null}
              initialDocument={doc}
              initialLegacyNote={session?.note ?? null}
              onChange={handleDocChange}
            />
          )}
        </section>
      )}

      {/* 附件区（§8：本次 Session 的附件索引/文件管理；正文引用经插入/同步移除联动） */}
      <section className="card">
        <div className="lw-att__head">
          <h2 className="card__title">本次附件</h2>
          <span className="muted" style={{ fontSize: 11 }}>
            索引区：可「插入正文」；删除附件会同步移除正文引用
          </span>
        </div>
        <AttachmentList
          attachments={attachments}
          onChanged={(next) => {
            // §8：被删除的附件 → 同步移除正文所有引用 Node（选"同步移除"实现，无破损引用）
            const removed = attachments.filter((a) => !next.some((n) => n.id === a.id));
            for (const r of removed) {
              window.dispatchEvent(
                new CustomEvent("higher:detach-attachment", { detail: { id: r.id } })
              );
            }
            setAttachments(next);
          }}
          readOnly={false}
          onInsert={(att) => {
            // 插入当前光标（编辑器经 window 事件接收；结束后只读时忽略）
            const kind = att.attachment_type === "video" ? "video" : "image";
            window.dispatchEvent(
              new CustomEvent("higher:insert-attachment", {
                detail: { id: att.id, kind, name: att.file_name },
              })
            );
          }}
        />
      </section>

      {/* ============ End Sheet（§59-66 + DEV-0055 §93-99 Completion 反馈） ============ */}
      {endSheetOpen && session && (
        <div className="modal-overlay" onClick={closeSheet}>
          <div className="modal modal--endsheet" onClick={(e) => e.stopPropagation()}>
            {/* §95-99：Completion 第一层（学习完成 ✓ + 名称 + 分钟 + 今天累计/任务 + 知识归属） */}
            {renderCompletion(true)}

            <div className="endsheet__title">你想如何整理本次学习？</div>
            <p className="evmodal__note muted">
              学习记录已经保存。不整理也完全可以——之后仍可在学习规划中找到它并整理。
            </p>

            {/* A 默认 Primary（§61 + §98）：返回今日（不强迫整理）。
                DEV-0063 Human Runtime Repair：必须真正返回 Today「/」——复用结束后视图/
                错误兜底同款原 navigate("/") handler；closeSheet 保留全部状态清理。 */}
            <button
              className="btn btn--primary btn--block"
              onClick={() => {
                closeSheet();
                navigate("/");
              }}
            >
              返回今日
            </button>

            {/* B 关联到已有知识（§62 / §98「现在整理」= organize_session_into_knowledge） */}
            <button
              className={"btn btn--block" + (sheetTab === "link" ? " endsheet__opt--open" : "")}
              onClick={() => setSheetTab(sheetTab === "link" ? "none" : "link")}
            >
              关联到已有知识
            </button>
            {sheetTab === "link" && (
              <div className="endsheet__panel">
                <input
                  className="modal__input taskmodal__search"
                  value={linkSearch}
                  onChange={(e) => setLinkSearch(e.target.value)}
                  placeholder="搜索知识…"
                />
                <div className="taskmodal__list">
                  {linkFiltered.slice(0, 30).map((i) => (
                    <button
                      key={i.id}
                      className="taskmodal__item"
                      disabled={busy}
                      onClick={() => void organizeCompletion(i.id)}
                    >
                      {i.name}
                    </button>
                  ))}
                  {linkFiltered.length === 0 && <p className="muted">没有匹配的知识</p>}
                </div>
                <p className="muted" style={{ fontSize: 11 }}>
                  只建立学习记录 → 知识的关联，不会修改知识正文。
                </p>
              </div>
            )}

            {/* C 新建知识（§63） */}
            <button
              className={"btn btn--block" + (sheetTab === "new" ? " endsheet__opt--open" : "")}
              onClick={() => setSheetTab(sheetTab === "new" ? "none" : "new")}
            >
              新建知识
            </button>
            {sheetTab === "new" && (
              <div className="endsheet__panel">
                <label className="modal__field">
                  名称 *
                  <input
                    className="modal__input"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="如：极限的计算方法"
                  />
                </label>
                <label className="modal__field">
                  父节点（可选）
                  <select
                    className="modal__input"
                    value={newParent ?? ""}
                    onChange={(e) => setNewParent(e.target.value ? Number(e.target.value) : null)}
                  >
                    <option value="">作为顶级知识</option>
                    {items.slice(0, 60).map((i) => (
                      <option key={i.id} value={i.id}>
                        放在「{i.name}」下
                      </option>
                    ))}
                  </select>
                </label>
                <label className="modal__field">
                  长期目标（可选）
                  <select
                    className="modal__input"
                    value={newGoalId}
                    onChange={(e) => setNewGoalId(e.target.value ? Number(e.target.value) : "")}
                  >
                    <option value="">不关联目标</option>
                    {goals.map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.name}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="modal__actions">
                  <button
                    className="btn btn--primary"
                    disabled={busy || !newName.trim()}
                    onClick={() => void createAndLink()}
                  >
                    {busy ? "创建中…" : "创建并关联"}
                  </button>
                </div>
              </div>
            )}

            {/* D 追加到已有知识正文（§64：必须先 Preview，确认才写入） */}
            <button
              className={"btn btn--block" + (sheetTab === "append" ? " endsheet__opt--open" : "")}
              onClick={() => setSheetTab(sheetTab === "append" ? "none" : "append")}
            >
              追加到已有知识正文
            </button>
            {sheetTab === "append" && (
              <div className="endsheet__panel">
                <input
                  className="modal__input taskmodal__search"
                  value={appendSearch}
                  onChange={(e) => setAppendSearch(e.target.value)}
                  placeholder="搜索要追加到的知识…"
                />
                <div className="taskmodal__list">
                  {appendFiltered.slice(0, 30).map((i) => (
                    <button
                      key={i.id}
                      className={
                        "taskmodal__item" + (appendTarget?.id === i.id ? " taskmodal__item--active" : "")
                      }
                      onClick={() => setAppendTarget(i)}
                    >
                      {i.name}
                    </button>
                  ))}
                  {appendFiltered.length === 0 && <p className="muted">没有匹配的知识</p>}
                </div>
                {appendTarget && (
                  <div className="endsheet__preview">
                    <div className="endsheet__preview-title">
                      将追加到「{appendTarget.name}」正文末尾：
                    </div>
                    <pre className="endsheet__pre">{appendPreviewText}</pre>
                    <div className="modal__actions">
                      <button
                        className="btn btn--primary"
                        disabled={busy}
                        onClick={() => void appendToItem()}
                      >
                        {busy ? "写入中…" : "确认追加"}
                      </button>
                      <button className="btn" onClick={() => setAppendTarget(null)}>
                        重选知识
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* E AI 帮我整理后再决定（§65：只 Proposal，用户决定；Sheet 保留） */}
            <button className="btn btn--block" onClick={() => void aiRunAction("knowledge_organize")}>
              ✨ AI 帮我整理后再决定
            </button>
            <p className="muted" style={{ fontSize: 11 }}>
              AI 只会给出整理建议（Proposal），接受 / 修改 / 拒绝都由你决定。
            </p>

            {/* 开始下一个（§66） */}
            <div className="endsheet__next">
              <button className="btn btn--block" onClick={() => setNextOpen(!nextOpen)}>
                开始下一个
              </button>
              {nextOpen && nextList}
            </div>
          </div>
        </div>
      )}

      {/* ===== 修正学习时间（§69：低频入口） ===== */}
      {timeFixOpen && session && (
        <div className="modal-overlay" onClick={() => setTimeFixOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">修正学习时间</div>
            <p className="evmodal__note muted">
              修改开始 / 结束时间后，学习时长会重新计算，并标记「已手动修正」。
            </p>
            <label className="modal__field">
              开始时间
              <input
                className="modal__input"
                type="datetime-local"
                value={fixStart}
                onChange={(e) => setFixStart(e.target.value)}
              />
            </label>
            <label className="modal__field">
              结束时间
              <input
                className="modal__input"
                type="datetime-local"
                value={fixEnd}
                onChange={(e) => setFixEnd(e.target.value)}
              />
            </label>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                disabled={busy || !fixStart}
                onClick={() => void applyTimeFix()}
              >
                {busy ? "保存中…" : "确认修正"}
              </button>
              <button className="btn" onClick={() => setTimeFixOpen(false)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ===== 删除这条学习记录（§70） ===== */}
      {deleteOpen && session && (
        <div className="modal-overlay" onClick={() => setDeleteOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除这条学习记录？</div>
            <ul className="lw-del-preview">
              <li>标题：{session.title}</li>
              <li>
                时间：{utcHHMM(session.started_at)}
                {session.ended_at ? ` - ${utcHHMM(session.ended_at)}` : ""} ·{" "}
                {formatDurationCompact(session.duration_seconds)}
              </li>
              <li>附件：{attachments.length} 个（只属于本次学习的附件会一并删除）</li>
            </ul>
            <p className="evmodal__note muted">知识正文不受影响。此操作不可撤销。</p>
            <div className="modal__actions">
              <button className="btn btn--primary" disabled={busy} onClick={() => void removeSession()}>
                {busy ? "删除中…" : "删除"}
              </button>
              <button className="btn" onClick={() => setDeleteOpen(false)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
