import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { isTauriRuntime } from "../../utils/tauriEnv";
import {
  aiCancelRun,
  aiGetRunSnapshot,
  aiStartRun,
  applyAdaptationProposal,
  archiveAiConversation,
  confirmAiMemory,
  createAiConversation,
  dismissAdaptationProposal,
  getActiveAiProfiles,
  listAiConversations,
  listAiMemories,
  listAiMessages,
  listAiProviderProfiles,
  listLearningItemsByProfile,
  openExternalUrl,
  rejectAiMemory,
  setActiveAiProfiles,
  updateAiMemory,
} from "../../api";
import type { AdaptationProposalEvent } from "../../api";
import AiProposalReview from "../AiProposalReview";
import ChangeSetReview from "../ChangeSetReview";
import Markdown, { type MdCitation } from "./Markdown";
import { humanizeError, useAiPanel } from "./AiPanelContext";
import type { AiScope } from "./AiPanelContext";
import {
  bindRunId,
  confirmHydrated,
  createClientTurnId,
  initialRuntimeState,
  isBusyPhase,
  isTerminalStatus,
  legacyDeltaInto,
  legacyErrorInto,
  legacyRunStatusInto,
  reduceAiRuntimeEvent,
  stageLabel,
  startTurn,
  watchdogResolve,
} from "./runtimeState";
import type { AiRuntimeUiState, RuntimeEventPayload } from "./runtimeState";
import { useActiveProfile } from "../../contexts/ActiveProfileContext";
import { startupMark } from "../../startupTrace";
import type {
  AiConversation,
  AiMessage,
  AiProviderProfile,
  ToolTraceEntry,
  WebSource,
} from "../../types";

/** Tauri run 事件统一 payload（§16：{ run_id, data }） */
interface RunEvent<T> {
  run_id: string;
  data: T;
}

/** DEV-0076 §八：ai://memory_proposals 事件卡片项（与后端 MemoryProposalCard 对齐） */
interface MemoryProposalCardData {
  memory_id: number;
  kind: string;
  memory_type: string;
  question: string;
}

/** 消息分页大小（加载最近 50 / 「加载更早」） */
const MSG_PAGE = 50;

/** DEV-0065.1 §8/§33：唯一视觉态偏好 higher.aiPanel.mode；
 *  missing/invalid → collapsed（AI 随手可用但不抢占学习工作区）。 */
const AI_PANEL_MODE_KEY = "higher.aiPanel.mode";

/** DEV-0060.1 PART A：WebView 本地时钟 → Runtime Time Truth（YYYY-MM-DD / YYYY-MM-DD HH:mm） */
function localIsoDate(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
function localIsoDatetime(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${localIsoDate(d)} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** DEV-0077 Phase U1 §十四：建议卡证据分钟的人话展示（1680 → 28h / 760 → 12h40m） */
function fmtAdaptMinutes(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  if (h <= 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h${m}m`;
}

/**
 * Higher AI Agent Panel V2（DEV-0061R §33：Unified Higher AI——单一模式；
 * DEV-0065.1 §7-§43：两态 Expanded(340px) / Collapsed Rail(46px)——无 Closed/FAB/X，
 * Panel 恒驻 App Shell 且恒挂载（收起↔展开不重建会话/流/Proposal）。
 * - Header 三动作：历史 / 新对话 / 收起为侧栏
 * - 态偏好 higher.aiPanel.mode（expanded|collapsed；missing → collapsed）
 * - 对话主流程：DB 持久化消息（list_ai_messages）+ ai_start_run 后台流式
 *   （ai://delta 逐字、ai://source 来源、ai://changeset 提案、run-status 终态）
 * - 引用 [[S1]] → 可点击上标；消息底部来源区（open_external_url 系统浏览器打开）
 * - 保留：scope chips / 快捷 Action（AiProposalReview）/ 上下文·trace·diag 折叠
 * - Runtime 完全冻结：aiStartRun / Conversation / Streaming / Model Selector /
 *   Provider / Pending Action 零改动；Enter 发送、Shift+Enter 换行保持
 */
export default function AiPanel({
  presentation = "desktop",
}: {
  /** DEV-MOBILE-001 §74-77：desktop（默认，零行为变化）| mobile（Android 全屏，无 collapsed rail） */
  presentation?: "desktop" | "mobile";
} = {}) {
  const isMobile = presentation === "mobile";
  const {
    pageContext,
    messages,
    busy: actionBusy,
    newConversation,
    scope,
    setScope,
    proposal,
    setProposal,
    proposalItems,
    setProposalItems,
    hasPendingProposal,
    apiKeyMissing,
    pendingSendRef,
  } = useAiPanel();
  const { activeProfile, triggerRefresh } = useActiveProfile();
  const navigate = useNavigate();
  const [input, setInput] = useState("");
  // DEV-0062 §37：AI Connection dropdown（enabled ai_provider_profiles；切换 = 修改 active
  // Primary Profile，不是修改 Connection 内容；runBusy 时 disabled）
  const [conns, setConns] = useState<AiProviderProfile[]>([]);
  const [activePrimaryId, setActivePrimaryId] = useState<number | null>(null);
  const [activeControlId, setActiveControlId] = useState<number | null>(null);
  const [confirmNew, setConfirmNew] = useState(false);
  const [showProposal, setShowProposal] = useState(false);
  /** §61：上下文 chips 默认收起 */
  const [ctxOpen, setCtxOpen] = useState(false);
  /** DEV-0065.1 §33：两态 collapsed 位（expanded|collapsed；missing/invalid → collapsed） */
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(AI_PANEL_MODE_KEY) !== "expanded"
  );

  function setMode(c: boolean) {
    setCollapsed(c);
    localStorage.setItem(AI_PANEL_MODE_KEY, c ? "collapsed" : "expanded");
  }

  // 页面 AI 入口（runAction / sendChat pendingSend）触发时自动展开 rail（§29）
  useEffect(() => {
    if (actionBusy) setMode(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [actionBusy]);
  useEffect(() => {
    const expand = () => setMode(false);
    window.addEventListener("higher:aipanel-pending-send", expand);
    return () => window.removeEventListener("higher:aipanel-pending-send", expand);
  }, []);

  // ---- DEV-0052 对话主流程状态（DEV-0061R §33：Unified Higher AI——无只读/助手双模式） ----
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [convoMsgs, setConvoMsgs] = useState<AiMessage[]>([]);
  const [msgOffset, setMsgOffset] = useState(0);
  const [hasMoreMsgs, setHasMoreMsgs] = useState(false);
  const [runBusy, setRunBusy] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  /**
   * DEV-0077.3 §十八/§十九：Runtime 状态机（唯一事实源；纯 reducer，可测试）。
   * streamText 从 rt 派生（§四十三：terminal 后保留，hydrate 确认后才清）。
   */
  const [rt, setRt] = useState<AiRuntimeUiState>(initialRuntimeState);
  const rtRef = useRef(rt);
  rtRef.current = rt;
  const streamText = rt.streamText;
  const [streamSources, setStreamSources] = useState<WebSource[]>([]);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [stopped, setStopped] = useState(false);
  const [pendingChangeSet, setPendingChangeSet] = useState<{
    change_set_id: number;
    title: string;
    count: number;
  } | null>(null);
  const [showChangeSet, setShowChangeSet] = useState(false);
  /** DEV-0053 §9：requires_change_set=true 但未生成 ChangeSet 的守卫文案（null = 无） */
  const [guardMsg, setGuardMsg] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [conversations, setConversations] = useState<AiConversation[]>([]);
  // DEV-0076 §八：AI 认知卡片（本轮 run 收口产生的 Memory Proposal）
  const [memoryProposals, setMemoryProposals] = useState<MemoryProposalCardData[]>([]);
  /** 卡片「修改」内联编辑（拉取 pending 完整条目；仅编辑内容，保存后仍待用户确认） */
  const [memEdit, setMemEdit] = useState<{
    id: number;
    memory_type: string;
    category: string;
    memory_key: string;
    memory_value: string;
    source_excerpt: string;
  } | null>(null);
  const [memoryBusy, setMemoryBusy] = useState<number | null>(null);
  // DEV-0077 Phase U1 §十四：AI 调整建议卡（Proactive SuggestAdjustment → Proposal Card；
  // 应用=后端 Stored 原 intents → ONE ChangeSet；暂不=dismissed；详情=纯前端展开）
  const [adaptProposal, setAdaptProposal] = useState<AdaptationProposalEvent | null>(null);
  const [adaptDetail, setAdaptDetail] = useState(false);
  const [adaptBusy, setAdaptBusy] = useState<"apply" | "dismiss" | null>(null);
  const [adaptResult, setAdaptResult] = useState<string | null>(null);

  // 事件回调里读取最新值（listener 只注册一次）
  const runIdRef = useRef<string | null>(null);
  const profileIdRef = useRef<number | null>(null);
  const conversationIdRef = useRef<number | null>(null);
  const lastUserTextRef = useRef<string>("");

  profileIdRef.current = activeProfile?.id ?? null;

  /**
   * DEV-0077.2 §十二：hydration 请求序号——Only latest hydration may commit state。
   * 旧请求（慢返回）不得覆盖新会话/新 run 已刷新的消息列表。
   */
  const hydrationSeqRef = useRef(0);
  const commitIfLatest = useCallback((seq: number, commit: () => void) => {
    if (seq === hydrationSeqRef.current) commit();
  }, []);

  const busy = runBusy || actionBusy;

  /** §60：新消息/流式增量自动滚底 */
  const messagesRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = messagesRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [convoMsgs.length, messages.length, busy, streamText, streamError, pendingChangeSet]);

  // §37.2 Settings / Panel 共享同一 active primary truth；切换即广播同步
  const reloadConns = useCallback(() => {
    Promise.all([listAiProviderProfiles(), getActiveAiProfiles()])
      .then(([list, act]) => {
        setConns(list.filter((p) => p.enabled));
        setActivePrimaryId(act.primary_id);
        setActiveControlId(act.control_id);
      })
      .catch(() => {});
  }, []);
  useEffect(() => {
    reloadConns();
    const un = listen<void>("higher:ai-profiles-changed", () => reloadConns());
    return () => {
      void un.then((f) => f());
    };
  }, [reloadConns]);

  async function switchPrimary(next: number) {
    if (next === activePrimaryId) return;
    try {
      await setActiveAiProfiles(next, activeControlId);
      reloadConns();
    } catch {
      // 失败保持原选择（Settings 中可见错误详情）
      reloadConns();
    }
  }

  // Panel 需要展示 Proposal 时按需拉取当前档案 items（AiProposalReview 的 allowedIds）
  // DEV-0065.1：Panel 恒挂载，Proposal 出现即拉取
  useEffect(() => {
    if (proposal && proposalItems.length === 0 && activeProfile) {
      listLearningItemsByProfile(activeProfile.id)
        .then(setProposalItems)
        .catch(() => {});
    }
  }, [proposal, activeProfile, proposalItems.length, setProposalItems]);

  /** 从 DB 刷新当前会话消息（message_committed / 任何 terminal 后，§四十二/§四十四） */
  const refreshMessages = useCallback(async () => {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (pid == null || cid == null) return;
    // DEV-0077.2 §十二：登记本次 hydration 序号；仅最新请求允许提交 state
    const seq = ++hydrationSeqRef.current;
    try {
      const msgs = await listAiMessages(pid, cid, MSG_PAGE);
      commitIfLatest(seq, () => {
        setConvoMsgs(msgs);
        setMsgOffset(msgs.length);
        setHasMoreMsgs(msgs.length >= MSG_PAGE);
        // DEV-0077.3 §四十二/§四十三（UI-TC005/006）：只有确认 persisted
        // assistant message 真正出现在 convoMsgs 后才清 transient ——
        // terminal 已到但 hydrate 未完成期间流式内容保持可见（防内容消失）。
        const cur = rtRef.current;
        const rid = cur.runId;
        const hasCommitted =
          cur.committedMessageId != null
            ? msgs.some((m) => m.id === cur.committedMessageId)
            : msgs.some(
                (m) => m.role === "assistant" && rid != null && m.run_id === rid,
              );
        const next = confirmHydrated(cur, hasCommitted);
        setRt(next);
        if (next.streamText === "") setStreamSources([]);
      });
    } catch {
      /* 刷新失败保留流式内容 */
    }
  }, [commitIfLatest]);

  /**
   * §十八：runBusy 由 Runtime phase 派生（legacy 事件也经 reducer 归一）。
   * §四十五-§四十七（UI-TC007）：Event 全丢兜底 watchdog——busy 期间
   * 每 1200ms 低频查询 DB snapshot（本地 SQLite 可承受；禁 100ms 轮询）。
   */
  useEffect(() => {
    setRunBusy(isBusyPhase(rt.phase));
  }, [rt.phase]);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const cur = rtRef.current;
    if (!isBusyPhase(cur.phase) || cur.runId == null) return;
    let halted = false;
    const tick = async () => {
      if (halted) return;
      try {
        const snap = await aiGetRunSnapshot(cur.runId!);
        if (halted) return;
        if (isTerminalStatus(snap.status)) {
          // §四十六：DB 终态 → reconcile 并结束 watchdog（§三 DB=Truth）
          setRt((s) => watchdogResolve(s, snap.status));
          void refreshMessages();
        }
      } catch {
        /* run_not_found（极早期 tick）：继续等待 */
      }
    };
    const h = window.setInterval(() => void tick(), 1200);
    return () => {
      halted = true;
      window.clearInterval(h);
    };
  }, [rt.phase, rt.runId, refreshMessages]);

  /** 加载会话消息（历史抽屉点击 / 档案初始化共用） */
  const loadConversation = useCallback(async (pid: number, cid: number) => {
    const seq = ++hydrationSeqRef.current;
    const msgs = await listAiMessages(pid, cid, MSG_PAGE);
    commitIfLatest(seq, () => {
      setConvoMsgs(msgs);
      setMsgOffset(msgs.length);
      setHasMoreMsgs(msgs.length >= MSG_PAGE);
    });
  }, [commitIfLatest]);

  /** 档案切换 / 首次挂载：恢复最近会话（无则新建；DEV-0061R §33 统一 mode=assistant legacy 值） */
  useEffect(() => {
    if (!activeProfile) return;
    let cancelled = false;
    const pid = activeProfile.id;
    setConversationId(null);
    conversationIdRef.current = null;
    setConvoMsgs([]);
    // §四十九：切档案 = 更换 client_turn scope + 清 transient（不动后台 run）
    setRt(initialRuntimeState());
    setStreamSources([]);
    setStreamError(null);
    setPendingChangeSet(null);
    setGuardMsg(null);
    setStopped(false);
    setRunBusy(false);
    setMemoryProposals([]);
    setMemEdit(null);
    // DEV-0077 Phase U1 §十八：切档案/会话清理建议卡（workflow payload 中状态仍真实存在）
    setAdaptProposal(null);
    setAdaptDetail(false);
    setAdaptResult(null);
    setAdaptBusy(null);
    runIdRef.current = null;
    setRunId(null);
    (async () => {
      try {
        const convs = await listAiConversations(pid, 20);
        if (cancelled) return;
        // DEV-0077.2 Part A §五：T6 = AiPanel conversations loaded
        startupMark("t6_ai_conversations_ready");
        const latest = convs.length > 0 ? convs[0] : null;
        const conv = latest ?? (await createAiConversation(pid, "assistant"));
        if (cancelled) return;
        conversationIdRef.current = conv.id;
        setConversationId(conv.id);
        await loadConversation(pid, conv.id);
        // T7 = AiPanel latest messages hydrated
        startupMark("t7_ai_messages_ready");
      } catch {
        /* 会话初始化失败：发送时再补建 */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeProfile, loadConversation]);

  /** Tauri run 事件（§16-19；按 runId 过滤，listener 只注册一次）。
   *  DEV-0054 §97-99 Preview Guard：浏览器（vite dev/preview）不注册，从调用路径避免刷屏。 */
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unsubs: Array<() => void> = [];
    let alive = true;
    const reg = <T,>(event: string, handler: (data: T, rid: string) => void) => {
      void listen<RunEvent<T>>(event, (e) => {
        if (e.payload.run_id !== runIdRef.current) return;
        handler(e.payload.data, e.payload.run_id);
      }).then((u) => {
        if (alive) unsubs.push(u);
        else u();
      });
    };

    // DEV-0077.3 §七/§十七/§五十：canonical `ai://runtime`（Protocol v1）。
    // 过滤四元组：client_turn_id + profile_id + conversation_id（run_id 在
    // reducer 内绑定校验）——run_id 尚未返回时凭 client_turn_id 即可接收
    //（UI-TC002，防 run_id race）。
    void listen<RuntimeEventPayload>("ai://runtime", (e) => {
      const p = e.payload;
      const cur = rtRef.current;
      if (cur.activeClientTurnId == null || p.client_turn_id == null) return;
      if (p.client_turn_id !== cur.activeClientTurnId) return; // UI-TC003
      if (
        profileIdRef.current != null &&
        p.profile_id != null &&
        p.profile_id !== profileIdRef.current
      )
        return;
      if (
        conversationIdRef.current != null &&
        p.conversation_id != null &&
        p.conversation_id !== conversationIdRef.current
      )
        return;
      setRt((s) => reduceAiRuntimeEvent(s, p));
      // §四十二：message_committed 立即 refresh；§四十四：任何 terminal 都 reconcile
      if (p.kind === "message_committed" || p.kind === "terminal") {
        void refreshMessages();
      }
    }).then((u) => {
      if (alive) unsubs.push(u);
      else u();
    });

    // ---- §五十四：legacy 兼容通道（经 reducer Adapter 归一，双听过渡期） ----
    // DEV-0077.2 §九（BUG-1 修复）：agent 路径后端 payload key 为 "text"（非流式
    // 轮全文补发），legacy 路径为 "delta"——双 key 兼容；流式轮后端只走 canonical
    // delta（round_streamed 抑制 legacy 重发），两通道不会双份。
    reg<{ delta?: string; text?: string }>("ai://delta", (d, rid) => {
      const chunk = d.delta ?? d.text ?? "";
      if (chunk) setRt((s) => legacyDeltaInto(s, rid, chunk));
    });
    reg<WebSource>("ai://source", (s) => {
      setStreamSources((prev) => (prev.some((x) => x.sid === s.sid) ? prev : [...prev, s]));
    });
    reg<{ change_set_id: number; title: string; count: number }>("ai://changeset", (d) => {
      setPendingChangeSet(d);
    });
    reg<{ status: string; needs_assistant?: string; message?: string }>("ai://run-status", (d, rid) => {
      // §五十四：legacy 终态经 Adapter 归一（runBusy 由 phase 派生）
      setRt((s) => legacyRunStatusInto(s, rid, d.status));
      if (d.status === "waiting_approval") {
        // DEV-0061R §34：needs_assistant 语义已废弃（Unified AI）；waiting_approval =
        // ChangeSet 已生成，刷新会话展示提案卡
        void refreshMessages();
      } else if (d.status === "cancelled") {
        setStopped(true);
        void refreshMessages();
      } else if (d.status === "no_changeset") {
        // DEV-0053 §9：requires_change_set=true 但 Run 结束没有生成 ChangeSet
        // → 禁止把模型「完成」文字当成功结果；显示守卫卡（正式数据未变化）
        setGuardMsg(d.message ?? "");
        void refreshMessages();
      } else if (d.status === "completed") {
        void refreshMessages();
      } else if (
        // DEV-0058 §76/§170-173：这些状态后端已落库（澄清提问/冲突/校验失败/超载），
        // 前端必须刷新会话让用户立即看到（此前不刷新=用户看不到要回答的问题）。
        // DEV-0060：planner_cancelled（PART I 取消规划回执）/ handoff_chat（§12 TYPE C
        // 新意图回复）同样已落库，需刷新展示。
        d.status === "clarification" ||
        d.status === "goal_conflict" ||
        d.status === "plan_validation_failed" ||
        d.status === "plan_too_large" ||
        d.status === "needs_assistant" ||
        d.status === "planner_cancelled" ||
        d.status === "handoff_chat"
      ) {
        void refreshMessages();
      } else if (
        // DEV-0077.2 §九（BUG-2 修复）：waiting_user / adaptation 问询挂起与
        // adaptation failed——assistant 消息已在 run 返回前落库（agent.rs 收口
        // ⑦ 先持久化后 emit），此前这两个状态无分支 → 消息落库却不可见，
        // 直到下一轮 run 才被顺带刷出（问题 A「下一轮才出现」的主因之一）。
        d.status === "needs_user_input" ||
        d.status === "waiting_user" ||
        d.status === "failed"
      ) {
        void refreshMessages();
      }
      // provider 级 Err：由 ai://error 展示（已完成路径的 failed 也走上面 refresh
      // 让落库的失败说明可见）
    });
    // DEV-0058 §144-152：Apply 后全系统同步——广播事件触发全局数据刷新
    // （Planning/Today/Calendar/Knowledge 同源重查；非 run 事件，不过滤 runId）
    void listen<{ change_set_id: number; profile_id: number }>("ai://applied", () => {
      triggerRefresh();
    }).then((u) => {
      if (alive) unsubs.push(u);
      else u();
    });
    reg<{ error: string }>("ai://error", (d, rid) => {
      // §五十二/§五十四：错误展示保留；终态化由后续 terminal（canonical/legacy）完成
      setRt((s) => legacyErrorInto(s, rid, String(d.error)));
      setStreamError(humanizeError(String(d.error)));
    });
    // DEV-0076 §八：收口产生的 Memory Proposal → AI 认知卡片（确认保存/修改/忽略）
    reg<{ proposals: MemoryProposalCardData[] }>("ai://memory_proposals", (d) => {
      setMemoryProposals((prev) => [...prev, ...(d.proposals ?? [])]);
    });
    // DEV-0077 Phase U1 §七/§十四：Adaptation Proposal Card（结构化事件，不解析 final_text）
    reg<AdaptationProposalEvent>("ai://adaptation_proposal", (d) => {
      setAdaptProposal(d);
      setAdaptDetail(false);
      setAdaptResult(null);
      setAdaptBusy(null);
    });

    return () => {
      alive = false;
      unsubs.forEach((u) => u());
    };
  }, [refreshMessages]);

  /**
   * DEV-0057 PART N：scope chips 与实际发送字段一致。
   * aiStartRun 只有 knowledgePath / sessionTitle 两个业务上下文参数：
   * - 当前页面（page）→ pageLabel + 页面自带 knowledgePath
   * - 当前知识（knowledge）→ knowledgePath（需 learningItemId）
   * - 本次学习（session）→ sessionTitle（需 sessionId）
   * 「当前规划」「整个档案」在 aiStartRun 无对应字段（无真实作用）→ 隐藏。
   */
  const scopeChips = useMemo(() => {
    const chips: { key: AiScope; label: string }[] = [{ key: "page", label: "当前页面" }];
    if (pageContext?.learningItemId != null)
      chips.push({ key: "knowledge", label: "当前知识" });
    if (pageContext?.sessionId != null)
      chips.push({ key: "session", label: "本次学习" });
    return chips;
  }, [pageContext]);

  const currentScopeLabel =
    scopeChips.find((c) => c.key === scope)?.label ?? "当前页面";

  // ---------------- 发送 / 停止 / 模式 ----------------

  // ---- DEV-0077 Phase U1 §十五/§十七：建议卡按钮（不得发自然语言让模型重新理解） ----
  async function onApplyAdaptProposal() {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (!adaptProposal || adaptBusy != null || pid == null || cid == null) return;
    setAdaptBusy("apply");
    try {
      const out = await applyAdaptationProposal({
        profileId: pid,
        conversationId: cid,
        proposalRunId: adaptProposal.run_id,
      });
      setAdaptProposal((p) => (p ? { ...p, state: "applied" } : p));
      setAdaptResult(`已应用。${out.summary}`);
      triggerRefresh();
    } catch (e) {
      setAdaptResult(`应用未生效：${humanizeError(String(e))}`);
    } finally {
      setAdaptBusy(null);
    }
  }

  async function onDismissAdaptProposal() {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (!adaptProposal || adaptBusy != null || pid == null || cid == null) return;
    setAdaptBusy("dismiss");
    try {
      await dismissAdaptationProposal({
        profileId: pid,
        conversationId: cid,
        proposalRunId: adaptProposal.run_id,
      });
      setAdaptProposal((p) => (p ? { ...p, state: "dismissed" } : p));
      setAdaptResult("已暂不调整（数据未变化）。之后仍可重新发起复盘，生成新的建议。");
    } catch (e) {
      setAdaptResult(humanizeError(String(e)));
    } finally {
      setAdaptBusy(null);
    }
  }

  /** §16：本地 push 用户消息 → aiStartRun（立即返回 runId）→ busy，按钮变停止 */
  async function send(text: string) {
    const pid = profileIdRef.current;
    if (pid == null || !text.trim()) return;
    let convId = conversationIdRef.current;
    if (convId == null) {
      try {
        const conv = await createAiConversation(pid, "assistant");
        convId = conv.id;
        conversationIdRef.current = conv.id;
        setConversationId(conv.id);
      } catch (e) {
        setStreamError(humanizeError(String(e)));
        return;
      }
    }
    lastUserTextRef.current = text;
    setStopped(false);
    setPendingChangeSet(null);
    setShowChangeSet(false);
    setGuardMsg(null);
    setStreamError(null);
    setStreamSources([]);
    // DEV-0077.3 §十四/§二十（UI-TC001）：invoke 之前创建 client_turn_id 并
    // 立即进入 starting —— 点击 Send 零空白等待（「正在处理…」即刻可见）。
    const clientTurnId = createClientTurnId();
    setRt((s) => startTurn(s, clientTurnId));
    setConvoMsgs((m) => [
      ...m,
      {
        id: -Date.now(),
        conversation_id: convId!,
        profile_id: pid,
        role: "user",
        content: text,
        run_id: null,
        created_at: new Date().toISOString(),
      },
    ]);
    setRunBusy(true);
    try {
      // DEV-0057 PART N：scope → aiStartRun 的 knowledgePath / sessionTitle 真实映射
      const ctx = pageContext;
      let knowledgePath = ctx?.knowledgePath ?? null;
      let sessionTitle: string | null = null;
      if (scope === "knowledge" && ctx?.learningItemId != null) {
        knowledgePath = ctx.knowledgePath ?? ctx.learningName ?? null;
      } else if (scope === "session" && ctx?.sessionId != null) {
        knowledgePath = ctx.knowledgePath ?? null;
        sessionTitle = ctx.sessionTitle ?? ctx.learningName ?? ctx.pageLabel ?? null;
      }
      const rid = await aiStartRun({
        profileId: pid,
        conversationId: convId,
        userMessage: text,
        pageLabel: ctx?.pageLabel ?? "Higher",
        knowledgePath,
        sessionTitle,
        // DEV-0060.1 PART A：Runtime Time Truth——每次发送都带 WebView 本地时钟，
        // 后端 AiRuntimeEnvelope 校验后注入 Prompt（TODAY/星期/时区由 Higher 提供）
        localDate: localIsoDate(),
        localDatetime: localIsoDatetime(),
        timezoneOffsetMinutes: -new Date().getTimezoneOffset(),
        // DEV-0077.3 §十六：Correlation ID 透传（invoke 前已知 → 事件可先于
        // run_id 返回到达，前端凭它匹配，UI-TC002）
        clientTurnId,
      });
      runIdRef.current = rid;
      setRunId(rid);
      // §十七：run_id 返回后绑定（此后同 turn 错误 run_id 拒绝）
      setRt((s) => bindRunId(s, rid));
    } catch (e) {
      setRunBusy(false);
      runIdRef.current = null;
      setRunId(null);
      // invoke 失败：本轮 runtime 收口 failed（无 run 可 reconcile，错误即时可见）
      setRt((s) => ({ ...s, phase: "failed", terminalStatus: "failed" }));
      setStreamError(humanizeError(String(e)));
    }
  }

  function submit() {
    const text = input;
    if (!text.trim() || busy) return;
    setInput("");
    void send(text.trim());
  }

  /** DEV-0058 §51-53：页面统一 Planner 入口（AI 生成计划/AI安排）——Context 投递
   *  pendingSend + 事件 → 本 Panel 以主输入同路径 send()（conversation+流式+ChangeSet 卡全套） */
  useEffect(() => {
    const handler = () => {
      const text = pendingSendRef.current;
      if (!text) return;
      pendingSendRef.current = null;
      if (runBusy) return; // 上一 run 进行中：忽略（用户可稍后手发）
      void send(text);
    };
    window.addEventListener("higher:aipanel-pending-send", handler);
    return () => window.removeEventListener("higher:aipanel-pending-send", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runBusy, pageContext, conversationId]);

  /** §18：停止当前 run（保留已产出内容） */
  async function stopRun() {
    const rid = runIdRef.current;
    if (rid == null) return;
    try {
      await aiCancelRun(rid);
    } catch {
      setRunBusy(false);
    }
  }

  // ---------------- DEV-0076 §八：AI 认知卡片操作 ----------------

  /** 确认保存（pending → confirmed）/ 忽略（pending → rejected）；成功后移除卡片 */
  async function resolveMemoryProposal(memoryId: number, action: "confirm" | "reject") {
    const pid = profileIdRef.current;
    if (pid == null) return;
    setMemoryBusy(memoryId);
    try {
      if (action === "confirm") await confirmAiMemory(pid, memoryId);
      else await rejectAiMemory(pid, memoryId);
      setMemoryProposals((prev) => prev.filter((p) => p.memory_id !== memoryId));
      setMemEdit(null);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    } finally {
      setMemoryBusy(null);
    }
  }

  /** 修改：拉取 pending 完整条目 → 内联编辑内容（保存后卡片显示新值，仍待确认） */
  async function openMemoryEdit(card: MemoryProposalCardData) {
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      const { pending } = await listAiMemories(pid);
      const m = pending.find((x) => x.id === card.memory_id);
      setMemEdit({
        id: card.memory_id,
        memory_type: m?.memory_type ?? card.memory_type,
        category: m?.category ?? "",
        memory_key: m?.memory_key ?? "",
        memory_value: m?.memory_value ?? card.question,
        source_excerpt: m?.source_excerpt ?? "",
      });
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  /** 保存修改（update_ai_memory，source_kind → user_edit）；卡片 question 同步为修改后内容 */
  async function saveMemoryEdit() {
    const pid = profileIdRef.current;
    if (pid == null || !memEdit) return;
    setMemoryBusy(memEdit.id);
    try {
      await updateAiMemory(pid, memEdit.id, {
        memory_type: memEdit.memory_type,
        category: memEdit.category,
        memory_key: memEdit.memory_key,
        memory_value: memEdit.memory_value,
        source_excerpt: memEdit.source_excerpt,
      });
      const value = memEdit.memory_value;
      setMemoryProposals((prev) =>
        prev.map((p) => (p.memory_id === memEdit.id ? { ...p, question: value } : p)),
      );
      setMemEdit(null);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    } finally {
      setMemoryBusy(null);
    }
  }

  // DEV-0061R §33：模式切换 / 旧续跑入口已删除（Unified Higher AI，
  // 无双模式；写入恒走 ChangeSet Approval Boundary）

  // ---------------- 历史 / 新对话 ----------------

  async function openHistory() {
    setHistoryOpen(true);
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      setConversations(await listAiConversations(pid, 20));
    } catch {
      setConversations([]);
    }
  }

  async function pickConversation(c: AiConversation) {
    const pid = profileIdRef.current;
    if (pid == null) return;
    setHistoryOpen(false);
    conversationIdRef.current = c.id;
    setConversationId(c.id);
    // §四十九：切会话 = 更换 client_turn scope + 清 transient（旧会话事件被过滤）
    setRt(initialRuntimeState());
    setStreamSources([]);
    setStreamError(null);
    setPendingChangeSet(null);
    setGuardMsg(null);
    setStopped(false);
    setMemoryProposals([]);
    setMemEdit(null);
    try {
      await loadConversation(pid, c.id);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  async function archiveConversation(id: number) {
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      await archiveAiConversation(pid, id);
      setConversations((prev) => prev.filter((c) => c.id !== id));
      if (conversationIdRef.current === id) {
        // 当前会话被归档 → 新建
        await startNewConversation();
      }
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  async function startNewConversation() {
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      const conv = await createAiConversation(pid, "assistant");
      conversationIdRef.current = conv.id;
      setConversationId(conv.id);
      setConvoMsgs([]);
      setMsgOffset(0);
      setHasMoreMsgs(false);
      setRt(initialRuntimeState());
      setStreamSources([]);
      setStreamError(null);
      setPendingChangeSet(null);
      setShowChangeSet(false);
      setGuardMsg(null);
      setStopped(false);
      setMemoryProposals([]);
      setMemEdit(null);
      setHistoryOpen(false);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  /** 「加载更早」（offset 分页，向前拼接） */
  async function loadEarlier() {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (pid == null || cid == null || runBusy) return;
    try {
      const older = await listAiMessages(pid, cid, MSG_PAGE, msgOffset);
      if (older.length > 0) {
        setConvoMsgs((m) => [...older, ...m]);
        setMsgOffset((o) => o + older.length);
      }
      setHasMoreMsgs(older.length >= MSG_PAGE);
    } catch {
      /* 忽略 */
    }
  }

  // ---------------- 渲染 ----------------

  /** [[Sx]] → Markdown 上标引用解析（§47；仅流式消息带本轮 sources 可点击，历史消息纯上标） */
  const citeOf = useCallback(
    (sid: string): MdCitation | null => {
      if (!activeProfile || !runId) return null;
      const src = streamSources.find((s) => s.sid === sid);
      if (!src) return null;
      const pid = activeProfile.id;
      const rid = runId;
      return { title: src.title, onClick: () => void openExternalUrl(pid, rid, sid) };
    },
    [activeProfile, runId, streamSources]
  );

  // DEV-0065.1 §7/§30：无 Closed 态——Panel 恒驻，仅 Expanded / Collapsed 两态
  // DEV-MOBILE-001 §77：mobile 无 collapsed rail（全屏或隐藏由 MobileLayout 控制）
  if (collapsed && !isMobile) {
    // §34：整条 46px rail = 单个全尺寸按钮（点击/Enter/Space 原生语义展开；无嵌套按钮）
    return (
      <aside className="aipanel aipanel--rail" aria-label="Higher AI（已收起）">
        <button
          className="aipanel__rail-hit"
          title="展开 Higher AI"
          onClick={() => setMode(false)}
        >
          <span className="aipanel__rail-icon" aria-hidden="true">✦</span>
          <span className="aipanel__rail-label" aria-hidden="true">
            HIGHER&nbsp;AI
          </span>
        </button>
      </aside>
    );
  }

  return (
    <aside className={`aipanel${isMobile ? " aipanel--mobile" : ""}`}>
      {/* DEV-MOBILE-002 F1-B：AI 为一级页面——无“‹ 返回”主退出
          （BottomNav 即导航；History 等二级层自带关闭）。
          保留 subtitle：当前上下文（F1-C 来源页上下文）。 */}
      {isMobile && pageContext && (
        <div className="aipanel__mobile-subtitle">{pageContext.pageLabel}</div>
      )}
      {/* Header：Higher AI（DEV-0061R §33：单一模式；修改经审查后写入）
          DEV-0065.1 §37：三动作 = 历史 / 新对话 / 收起为侧栏（无关闭） */}
      <div className="aipanel__header">
        <div className="aipanel__header-main">
          <span className="aipanel__title">Higher AI</span>
        </div>
        <div className="aipanel__header-actions">
          <button
            className="aipanel__icon-btn"
            title="历史对话"
            onClick={() => void openHistory()}
          >
            ⏱
          </button>
          <button
            className="aipanel__icon-btn"
            title="新对话"
            onClick={() => {
              if (hasPendingProposal) {
                setConfirmNew(true);
                return;
              }
              newConversation();
              setShowProposal(false);
              void startNewConversation();
            }}
          >
            ＋
          </button>
          {/* DEV-MOBILE-001 §77：mobile 无“收起为侧栏”（返回键/底部导航负责离开 /ai） */}
          {!isMobile && (
            <button
              className="aipanel__icon-btn"
              title="收起为侧栏"
              onClick={() => setMode(true)}
            >
              ⇥
            </button>
          )}
        </div>
      </div>

      {/* 新建对话确认（未应用 Proposal） */}
      {confirmNew && (
        <div className="aipanel__confirm">
          <p>当前还有未处理的 AI 修改建议。</p>
          <div className="btn-row">
            <button
              className="btn btn--small btn--primary"
              onClick={() => {
                newConversation();
                setShowProposal(false);
                setConfirmNew(false);
                void startNewConversation();
              }}
            >
              继续新建
            </button>
            <button className="btn btn--small" onClick={() => setConfirmNew(false)}>
              取消
            </button>
          </div>
        </div>
      )}

      {/* 历史抽屉 */}
      {historyOpen && (
        <div className="aipanel__history">
          <div className="aipanel__history-head">
            <span>历史对话</span>
            <button className="aipanel__icon-btn" onClick={() => setHistoryOpen(false)}>
              ✕
            </button>
          </div>
          <div className="aipanel__history-list">
            {conversations.length === 0 && (
              <p className="muted" style={{ fontSize: 12, padding: "8px 4px" }}>
                还没有历史对话。
              </p>
            )}
            {conversations.map((c) => (
              <div
                key={c.id}
                className={
                  "aipanel__history-item" +
                  (c.id === conversationId ? " aipanel__history-item--active" : "")
                }
              >
                <button className="aipanel__history-pick" onClick={() => void pickConversation(c)}>
                  <span className="aipanel__history-title">{c.title}</span>
                  <span className="aipanel__history-meta">
                    {(c.updated_at || "").slice(0, 16).replace("T", " ")}
                  </span>
                </button>
                <button
                  className="aipanel__history-archive"
                  title="归档此对话"
                  onClick={() => void archiveConversation(c.id)}
                >
                  归档
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Context 一行（§58：轻量替代旧四行 Context Card） */}
      <div className="aipanel__ctxline">
        <span className="aipanel__ctxline-label" title={pageContext?.knowledgePath ?? ""}>
          {pageContext?.knowledgePath
            ? `知识 · ${pageContext.knowledgePath.split(" > ").join(" › ")}`
            : pageContext?.pageLabel ?? "Higher"}
        </span>
        <button className="aipanel__settings-link" onClick={() => navigate("/settings")}>
          AI 设置 ›
        </button>
      </div>

      {/* 消息流 */}
      <div className="aipanel__messages" ref={messagesRef}>
        {hasMoreMsgs && !runBusy && (
          <button className="aipanel__load-earlier" onClick={() => void loadEarlier()}>
            加载更早的消息
          </button>
        )}

        {convoMsgs.length === 0 && messages.length === 0 && !runBusy && !streamError && (
          <div className="aipanel__empty">
            {apiKeyMissing ? (
              <>
                <p>还没有配置 AI。</p>
                <button className="btn btn--small btn--primary" onClick={() => navigate("/settings")}>
                  前往设置
                </button>
              </>
            ) : (
              <>
                <p className="aipanel__empty-title">可以问我关于当前学习的问题</p>
                <div className="aipanel__quick">
                  {pageContext?.knowledgePath && (
                    <button
                      className="aipanel__quick-btn"
                      disabled={busy}
                      onClick={() => void send("帮我分析当前知识点，看看哪里可能不完整")}
                    >
                      分析当前知识
                    </button>
                  )}
                  <button
                    className="aipanel__quick-btn"
                    disabled={busy}
                    onClick={() => void send("看看我最近学了什么")}
                  >
                    看看我最近学了什么
                  </button>
                </div>
              </>
            )}
          </div>
        )}

        {/* DB 对话消息（含后端落库的错误 / 历史遗留标记 / 已停止） */}
        {convoMsgs.map((m) => {
          const isError = m.role === "assistant" && m.content.startsWith("[出错]");
          const isSystem = m.role === "system";
          const cls =
            "aipanel__msg aipanel__msg--" +
            (m.role === "user" ? "user" : isSystem ? "system" : "assistant") +
            (isError ? " aipanel__msg--error" : "");
          return (
            <div key={m.id} className={cls}>
              <div className="aipanel__msg-content">
                {m.role === "user" || isError || isSystem ? (
                  m.content
                ) : (
                  <Markdown text={m.content} />
                )}
              </div>
            </div>
          );
        })}

        {/* 流式 assistant 占位（delta 逐字 + ▌光标；终态刷新前保持可见） */}
        {(runBusy || streamText !== "") && streamError == null && (
          <div
            className={
              "aipanel__msg aipanel__msg--assistant" +
              (runBusy ? " aipanel__msg--streaming" : "")
            }
          >
            <div className="aipanel__msg-content">
              {streamText === "" ? (
                /* §二十/§二十一：无 delta 时只显示当前 stage（不堆叠成消息）；
                 * starting 态 = 立即反馈「正在处理…」 */
                stageLabel(rt.phase === "starting" ? "starting" : rt.stage)
              ) : (
                <Markdown text={streamText} citeOf={citeOf} />
              )}
              {runBusy && <span className="aipanel__stream-cursor" aria-hidden />}
            </div>
            {streamSources.length > 0 && activeProfile && (
              <SourcesArea
                sources={streamSources}
                profileId={activeProfile.id}
                runId={runId}
              />
            )}
          </div>
        )}

        {/* 运行错误 */}
        {streamError != null && (
          <div className="aipanel__msg aipanel__msg--assistant aipanel__msg--error">
            <div className="aipanel__msg-content">{streamError}</div>
          </div>
        )}

        {/* 已停止标记（§19：保留已产出） */}
        {stopped && !runBusy && (
          <div className="aipanel__stopped">■ 已停止</div>
        )}

        {/* DEV-0076 §八：AI 认知卡片（Memory Proposal：确认保存 / 修改 / 忽略） */}
        {memoryProposals.map((p) => (
          <div key={p.memory_id} className="aipanel__memcard">
            <div className="aipanel__memcard-title">我发现一个可能有帮助的信息：</div>
            <p className="aipanel__memcard-q">“{p.question}”</p>
            {p.kind === "derived" && (
              <p className="aipanel__memcard-note">（AI 根据对话推断，非你的原话）</p>
            )}
            <p className="aipanel__memcard-ask">是否保存到我的长期记忆？</p>
            {memEdit?.id === p.memory_id ? (
              <div className="aipanel__memcard-edit">
                <textarea
                  className="aipanel__memcard-input"
                  rows={2}
                  value={memEdit.memory_value}
                  onChange={(e) =>
                    setMemEdit((m) => (m ? { ...m, memory_value: e.target.value } : m))
                  }
                />
                <div className="btn-row">
                  <button
                    className="btn btn--small btn--primary"
                    disabled={memoryBusy === p.memory_id || !memEdit.memory_value.trim()}
                    onClick={() => void saveMemoryEdit()}
                  >
                    保存修改
                  </button>
                  <button className="btn btn--small" onClick={() => setMemEdit(null)}>
                    取消
                  </button>
                </div>
              </div>
            ) : (
              <div className="btn-row">
                <button
                  className="btn btn--small btn--primary"
                  disabled={memoryBusy === p.memory_id}
                  onClick={() => void resolveMemoryProposal(p.memory_id, "confirm")}
                >
                  确认保存
                </button>
                <button
                  className="btn btn--small"
                  disabled={memoryBusy === p.memory_id}
                  onClick={() => void openMemoryEdit(p)}
                >
                  修改
                </button>
                <button
                  className="btn btn--small"
                  disabled={memoryBusy === p.memory_id}
                  onClick={() => void resolveMemoryProposal(p.memory_id, "reject")}
                >
                  忽略
                </button>
              </div>
            )}
          </div>
        ))}

        {/* 快捷 Action 结果（旧流程：runAction / Review 页 sendChat） */}
        {messages.map((m, i) => (
          <div key={i} className={"aipanel__msg aipanel__msg--" + m.role + (m.error ? " aipanel__msg--error" : "")}>
            <div className="aipanel__msg-content">
              {m.role === "assistant" && !m.error ? <Markdown text={m.content} /> : m.content}
            </div>
            {/* 聊天触发的 Proposal：显示查看变更入口（与快捷 Action 相同组件） */}
            {m.chatType === "knowledge_proposal" && proposal && !showProposal && (
              <button
                className="btn btn--small btn--primary"
                style={{ marginTop: 8 }}
                onClick={() => setShowProposal(true)}
              >
                查看变更（{proposal.operations.length} 项）
              </button>
            )}
            {!m.error && (m.toolTrace?.length || m.contextProvided?.length) && (
              <details className="aipanel__trace">
                <summary>本次使用的上下文</summary>
                {m.contextProvided && m.contextProvided.length > 0 && (
                  <div className="aipanel__trace-ctx">
                    {m.contextProvided.map((c, j) => (
                      <span key={j} className="aipanel__trace-chip">{c}</span>
                    ))}
                    <span className="muted" style={{ fontSize: 10 }}>
                      （由 Higher 直接提供，非工具调用）
                    </span>
                  </div>
                )}
                {m.toolTrace && m.toolTrace.length > 0 && (
                  <ul className="aipanel__trace-list">
                    {m.toolTrace.map((t, j) => (
                      <ToolTraceRow key={j} t={t} />
                    ))}
                  </ul>
                )}
              </details>
            )}
            {/* 本次请求详情（DEV-0023 §58：Model/Scope/Tool 数/Tokens/耗时；无 API Key） */}
            {!m.error && m.diag && (
              <details className="aipanel__diag">
                <summary>本次请求详情</summary>
                <div className="aipanel__diag-body">
                  <span>AI：{m.diag.providerProfileName ?? "旧版本未记录"}</span>
                  <span>Model：{m.diag.providerModel ?? "旧版本未记录"}</span>
                  <span>Scope：{m.diag.action}</span>
                  <span>工具调用：{m.diag.toolCount} 次（{m.diag.toolRounds} 轮，上限 6）</span>
                  {m.diag.promptTokens != null && <span>prompt_tokens：{m.diag.promptTokens}</span>}
                  {m.diag.completionTokens != null && <span>completion_tokens：{m.diag.completionTokens}</span>}
                  {m.diag.totalTokens != null && <span>total_tokens：{m.diag.totalTokens}</span>}
                  {m.diag.durationMs != null && <span>耗时：{m.diag.durationMs} ms</span>}
                </div>
              </details>
            )}
            {m.error && m.errorDetail && (
              <details className="aipanel__diag">
                <summary>技术详情</summary>
                <div className="aipanel__diag-body">{m.errorDetail}</div>
              </details>
            )}
            {!m.error && m.tokens != null && (
              <div className="aipanel__usage">本次使用 {m.tokens} tokens</div>
            )}
          </div>
        ))}

        {actionBusy && !runBusy && (
          <div className="aipanel__msg aipanel__msg--assistant">正在分析…</div>
        )}

        {/* DEV-0061R §33：旧双模式入口卡片已删除（Unified Higher AI） */}

        {/* DEV-0077 Phase U1 §十四：AI 调整建议卡（应用/查看详情/暂不调整；
            前端零业务直写——Apply 走后端 Stored Proposal → ONE ChangeSet 管线） */}
        {adaptProposal && !runBusy && (
          <div className="aipanel__msg aipanel__msg--assistant">
            <div className="aipanel__msg-content">
              <div className="aipanel__adapt-card">
                <div className="aipanel__adapt-title">AI 调整建议</div>
                <div className="aipanel__adapt-evidence">
                  最近 {adaptProposal.evidence.window_days} 天：
                  计划 {fmtAdaptMinutes(adaptProposal.evidence.planned_minutes)}，
                  实际 {fmtAdaptMinutes(adaptProposal.evidence.actual_minutes)}，
                  未完成任务 {adaptProposal.evidence.unfinished_task_count}
                  {adaptProposal.evidence.overdue_task_count > 0
                    ? `（其中逾期 ${adaptProposal.evidence.overdue_task_count}）`
                    : ""}
                </div>
                {adaptProposal.reason && (
                  <div className="aipanel__adapt-reason">发现：{adaptProposal.reason}</div>
                )}
                <div className="aipanel__adapt-label">建议：</div>
                <ol className="aipanel__adapt-list">
                  {adaptProposal.adjustments.map((a, i) => (
                    <li key={i}>{a.summary}</li>
                  ))}
                </ol>
                {adaptDetail && (
                  <details open className="aipanel__adapt-detail">
                    <summary>详情（证据 / 偏差 / 调整项）</summary>
                    <div className="aipanel__adapt-detail-body">
                      <div>
                        窗口：{adaptProposal.evidence.window_days} 天 · 已完成{" "}
                        {adaptProposal.evidence.completed_task_count} 项 · 完成率{" "}
                        {(() => {
                          const done = adaptProposal.evidence.completed_task_count;
                          const total = done + adaptProposal.evidence.unfinished_task_count;
                          return total > 0 ? `${Math.round((done / total) * 100)}%` : "—";
                        })()}
                        （backlog {adaptProposal.evidence.unfinished_task_count} · 逾期{" "}
                        {adaptProposal.evidence.overdue_task_count}）
                      </div>
                      {adaptProposal.deviations.map((d, i) => (
                        <div key={i}>
                          · {d.type}：{d.explanation}
                        </div>
                      ))}
                      {adaptProposal.adjustments.map((a, i) => (
                        <div key={`k${i}`}>
                          · [{a.kind}] {a.summary}
                        </div>
                      ))}
                    </div>
                  </details>
                )}
                {adaptResult && <p className="aipanel__adapt-result">{adaptResult}</p>}
                {adaptProposal.state === "pending" ? (
                  <div className="btn-row">
                    <button
                      className="btn btn--small btn--primary"
                      disabled={adaptBusy != null}
                      onClick={() => void onApplyAdaptProposal()}
                    >
                      {adaptBusy === "apply" ? "正在应用..." : "应用调整"}
                    </button>
                    <button
                      className="btn btn--small"
                      disabled={adaptBusy != null}
                      onClick={() => setAdaptDetail((v) => !v)}
                    >
                      查看详情
                    </button>
                    <button
                      className="btn btn--small"
                      disabled={adaptBusy != null}
                      onClick={() => void onDismissAdaptProposal()}
                    >
                      {adaptBusy === "dismiss" ? "处理中..." : "暂不调整"}
                    </button>
                  </div>
                ) : (
                  <div className="muted" style={{ fontSize: 11 }}>
                    {adaptProposal.state === "applied"
                      ? "已应用（可在修改记录中撤销）"
                      : "已暂不调整"}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* DEV-0053 §9：AI Guard（no_changeset）——正式数据没有发生变化 */}
        {guardMsg != null && !runBusy && (
          <div className="aipanel__guard">
            <div className="aipanel__guard-title">
              Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。
            </div>
            {guardMsg && <p className="aipanel__guard-msg">{guardMsg}</p>}
            <div className="btn-row">
              <button
                className="btn btn--small btn--primary"
                onClick={() => {
                  // 用同一条原始用户消息重新 aiStartRun
                  const text = lastUserTextRef.current;
                  setGuardMsg(null);
                  if (text) void send(text);
                }}
              >
                重新生成修改方案
              </button>
              <button className="btn btn--small" onClick={() => setGuardMsg(null)}>
                知道了
              </button>
            </div>
          </div>
        )}

        {/* §128：ChangeSet 待审阅入口（DEV-0058 §105：文案与后端一致「查看计划」） */}
        {pendingChangeSet && !showChangeSet && (
          <div className="aipanel__msg aipanel__msg--assistant">
            <div className="aipanel__msg-content">
              本次修改提案：{pendingChangeSet.title || "AI 修改建议"}（{pendingChangeSet.count} 项）
            </div>
            <button
              className="btn btn--small btn--primary"
              style={{ marginTop: 8 }}
              onClick={() => setShowChangeSet(true)}
            >
              查看计划
            </button>
          </div>
        )}

        {/* Proposal：查看变更（旧 knowledge_organize 流程保留） */}
        {proposal && !showProposal && (
          <div className="aipanel__msg aipanel__msg--assistant">
            <div className="aipanel__msg-content">
              我建议对知识库进行 {proposal.operations.length} 项修改。
            </div>
            <div className="aipanel__proposal-ops">
              {proposal.operations.map((op, i) => (
                <div key={i} className="aipanel__proposal-op">
                  <span>{op.operation === "update_content" ? op.name || `#${op.learning_item_id}` : op.name}</span>
                  <span className="prop-op__badge">
                    {op.operation === "update_content" ? "修改" : "新增"}
                  </span>
                </div>
              ))}
            </div>
            <button className="btn btn--small btn--primary" onClick={() => setShowProposal(true)}>
              查看变更
            </button>
          </div>
        )}
      </div>

      {/* ChangeSet Review（§128-133 / DEV-0053 §106-109） */}
      {showChangeSet && pendingChangeSet && activeProfile && (
        <ChangeSetReview
          profileId={activeProfile.id}
          changeSetId={pendingChangeSet.change_set_id}
          onClose={() => setShowChangeSet(false)}
          onApplied={(lines) => {
            /* DEV-0053 §11：把真实后端结果行作为 system 消息插入当前会话显示 */
            const pid = profileIdRef.current;
            const cid = conversationIdRef.current;
            if (pid == null || cid == null || !lines || lines.length === 0) return;
            const now = new Date().toISOString();
            setConvoMsgs((m) => [
              ...m,
              ...lines.map((text, i) => ({
                id: -(Date.now() + i),
                conversation_id: cid,
                profile_id: pid,
                role: "system",
                content: text,
                run_id: null,
                created_at: now,
              })),
            ]);
          }}
        />
      )}

      {/* Proposal Review（内嵌现有组件；应用后清空待处理 Proposal） */}
      {showProposal && proposal && activeProfile && (
        <AiProposalReview
          profileId={activeProfile.id}
          action="knowledge_organize"
          learningItemId={pageContext?.learningItemId ?? null}
          sessionId={pageContext?.sessionId ?? null}
          items={proposalItems}
          initialProposal={proposal}
          onClose={() => setShowProposal(false)}
          onApplied={() => {
            setProposal(null);
            setShowProposal(false);
          }}
        />
      )}

      {/* 输入栏（§62 Compact Composer；busy 时按钮=停止） */}
      <div className="aipanel__inputbar">
        <button
          className="aipanel__ctx-toggle"
          onClick={() => setCtxOpen((v) => !v)}
          title="展开 / 收起上下文范围"
        >
          上下文 {ctxOpen ? "▴" : "▾"}
          <span className="aipanel__ctx-current">@{currentScopeLabel}</span>
        </button>
        {ctxOpen && (
          <div className="aipanel__scopes">
            {scopeChips.map((c) => (
              <button
                key={c.key}
                className={
                  "aipanel__scope-chip" + (scope === c.key ? " aipanel__scope-chip--active" : "")
                }
                onClick={() => setScope(c.key)}
              >
                @{c.label}
              </button>
            ))}
          </div>
        )}
        <textarea
          className="aipanel__input aipanel__input--multi"
          rows={2}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              if (!input.trim() || busy) return;
              e.preventDefault();
              submit();
            }
          }}
          /* DEV-MOBILE-002 §37：Android 无桌面键盘提示 */
          placeholder={isMobile ? "问点什么…" : "问点什么…（Enter 发送，Shift+Enter 换行）"}
          disabled={busy}
        />
        <div className="aipanel__sendrow">
          {runBusy ? (
            <button
              className="btn btn--small aipanel__stop-btn"
              title="停止本次回答（保留已生成内容）"
              onClick={() => void stopRun()}
            >
              ■ 停止
            </button>
          ) : (
            <button
              className="btn btn--small btn--primary"
              disabled={actionBusy || !input.trim()}
              onClick={submit}
            >
              发送
            </button>
          )}
          <span className="aipanel__model-inline">
            <span className="muted">AI</span>
            <select
              className="aipanel__model-select"
              value={activePrimaryId ?? ""}
              disabled={runBusy}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (v) void switchPrimary(v);
              }}
            >
              {conns.length === 0 && <option value="">（未配置）</option>}
              {conns.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.display_name}
                  {c.compatibility_status === "limited" ? "（有限兼容）" : ""}
                </option>
              ))}
            </select>
          </span>
        </div>
      </div>
    </aside>
  );
}

/** 消息底部来源区：编号 + 标题 + 域名 + [打开网页]（§111-112 系统浏览器打开） */
function SourcesArea({
  sources,
  profileId,
  runId,
}: {
  sources: WebSource[];
  profileId: number;
  runId: string | null;
}) {
  return (
    <div className="aipanel__sources">
      <div className="aipanel__sources-title">来源</div>
      {sources.map((s, i) => (
        <div key={s.sid} className="aipanel__source">
          <span className="aipanel__source-num">[{i + 1}]</span>
          <span className="aipanel__source-main">
            <span className="aipanel__source-title" title={s.url}>
              {s.title || s.url}
            </span>
            <span className="aipanel__source-domain">{domainOf(s.url)}</span>
          </span>
          <button
            className="aipanel__source-open"
            onClick={() => void openExternalUrl(profileId, runId, s.sid)}
          >
            打开网页
          </button>
        </div>
      ))}
    </div>
  );
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function ToolTraceRow({ t }: { t: ToolTraceEntry }) {
  return (
    <li className={"aipanel__trace-item aipanel__trace-item--" + t.status}>
      <span className="aipanel__trace-status">
        {t.status === "success" ? "✓" : "✕"}
      </span>
      <span>{t.label}</span>
      <span className="muted aipanel__trace-name">{t.tool}</span>
    </li>
  );
}

export { humanizeError };
