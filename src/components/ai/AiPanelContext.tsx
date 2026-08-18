import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { aiAnalyze, getUiSetting, setUiSetting } from "../../api";
import type { AiActionName } from "../../api";
import type {
  AiKnowledgeProposal,
  AiResult,
  AssistantChatResponse,
  LearningItem,
  ToolTraceEntry,
} from "../../types";
import { useActiveProfile } from "../../contexts/ActiveProfileContext";

/** 页面上下文（由各页面通过 setPageContext 上报；Panel 据此显示与传参） */
export interface AiPageContext {
  page:
    | "today"
    | "planning"
    | "review"
    | "knowledge"
    | "learning"
    | "progress"
    | "data"
    | "settings";
  pageLabel: string;
  goalId?: number | null;
  learningItemId?: number | null;
  sessionId?: number | null;
  /** DEV-0057 PART N：本次学习标题（aiStartRun 的 sessionTitle 参数来源） */
  sessionTitle?: string;
  knowledgePath?: string;
  learningName?: string;
  /** 页面级补充说明（如 Review 的观察窗口），显示在 Context Header */
  pageDetail?: string;
}

/** @ scope（用户点 Chip 选择；决定 assistant_chat 附带的业务上下文） */
export type AiScope =
  | "page"
  | "knowledge"
  | "session"
  | "planning"
  | "profile";

export interface AiChatMessage {
  role: "user" | "assistant";
  content: string;
  /** assistant 消息附带：真实工具调用 + 已提供上下文 + tokens */
  toolTrace?: ToolTraceEntry[];
  contextProvided?: string[];
  tokens?: number | null;
  /** 结构化结果（快捷 Action 时）：按 action 存原始结果 */
  structured?: AiResult;
  /** 本次请求详情（DEV-0023 §58：不含 API Key） */
  diag?: {
    action: string;
    toolCount: number;
    toolRounds: number;
    promptTokens: number | null;
    completionTokens: number | null;
    totalTokens: number | null;
    durationMs: number | null;
  };
  /** 结构化对话响应类型（assistant_chat：message / knowledge_proposal） */
  chatType?: "message" | "knowledge_proposal";
  error?: boolean;
  /** 错误技术详情（安全处理后的原始消息；不含 Key） */
  errorDetail?: string;
}

interface AiPanelState {
  open: boolean;
  setOpen: (v: boolean) => void;
  pageContext: AiPageContext | null;
  setPageContext: (ctx: AiPageContext) => void;
  messages: AiChatMessage[];
  busy: boolean;
  /** 当前正在执行 / 上一次执行的快捷 Action（null = 自由对话模式） */
  activeAction: AiActionName | null;
  /** assistant_chat 的 @ scope */
  scope: AiScope;
  setScope: (s: AiScope) => void;
  /** 触发快捷 Action（页面 AI 按钮入口；自动展开 Panel 并请求）。
   *  extra：附加业务参数（DEV-0046 daily_review 的 date 等），合并进 aiAnalyze 调用。 */
  runAction: (
    action: AiActionName,
    hint?: string,
    extra?: { date?: string }
  ) => Promise<void>;
  /** 自由对话发送 */
  sendChat: (text: string) => Promise<void>;
  newConversation: () => void;
  /** 待审阅 Proposal（knowledge_organize 结果；供 Panel 内嵌 AiProposalReview） */
  proposal: AiKnowledgeProposal | null;
  setProposal: (p: AiKnowledgeProposal | null) => void;
  proposalItems: LearningItem[];
  setProposalItems: (items: LearningItem[]) => void;
  /** 未应用的 Proposal 存在时新建对话需确认（组件内处理） */
  hasPendingProposal: boolean;
  apiKeyMissing: boolean;
  /** DEV-0058：统一 Planner 待发消息（页面按钮 → AiPanel.send 同路径） */
  pendingSendRef: React.RefObject<string | null>;
}

const Ctx = createContext<AiPanelState | null>(null);

/** Conversation Budget（DEV-0023 §16）：最多 8 turn / 约 12000 字符（先到先截断，丢最旧） */
const MAX_TURNS = 8;
const MAX_CHARS = 12000;

export function trimHistory(messages: AiChatMessage[]): [string, string][] {
  let turns = messages.filter((m) => !m.error).slice(0, MAX_TURNS * 2);
  let chars = turns.reduce((a, m) => a + m.content.length, 0);
  while (turns.length > 2 && chars > MAX_CHARS) {
    const dropped = turns.shift();
    chars -= dropped ? dropped.content.length : 0;
  }
  return turns.map((m) => [m.role, m.content] as [string, string]);
}

export function AiPanelProvider({ children }: { children: React.ReactNode }) {
  const { activeProfile, refreshKey } = useActiveProfile();
  const [open, setOpenState] = useState(false);
  const [pageContext, setPageContext] = useState<AiPageContext | null>(null);
  const [messages, setMessages] = useState<AiChatMessage[]>([]);
  const [busy, setBusy] = useState(false);
  const [activeAction, setActiveAction] = useState<AiActionName | null>(null);
  const [scope, setScope] = useState<AiScope>("page");
  const [proposal, setProposal] = useState<AiKnowledgeProposal | null>(null);
  const [proposalItems, setProposalItems] = useState<LearningItem[]>([]);
  const [apiKeyMissing, setApiKeyMissing] = useState(false);
  const profileIdRef = useRef<number | null>(null);
  /** DEV-0058 §51-53：页面入口（AI 生成计划/AI安排）→ 统一 Planner 的待发消息 */
  const pendingSendRef = useRef<string | null>(null);
  /** DEV-0058：规划写意图识别（与后端 planning_write_intent 口径对齐的常用子集） */
  const PLAN_WRITE_INTENT_RE =
    /(安排|排个|排一下|排进|加入\s*higher|加入higher|做个.{0,6}计划|生成.{0,6}计划|规划)/i;

  // 持久化展开状态（ui.ai_panel_open；缺省 false）
  const setOpen = useCallback((v: boolean) => {
    setOpenState(v);
    void setUiSetting("ui.ai_panel_open", v ? "true" : "false").catch(() => {});
  }, []);

  useEffect(() => {
    getUiSetting("ui.ai_panel_open")
      .then((v) => setOpenState(v === "true"))
      .catch(() => {});
  }, []);

  // Profile 切换：立即清空对话 / Trace / Proposal（绝不跨档案携带）
  useEffect(() => {
    const prev = profileIdRef.current;
    if (prev != null && activeProfile && prev !== activeProfile.id) {
      setMessages([]);
      setProposal(null);
      setActiveAction(null);
    }
    profileIdRef.current = activeProfile?.id ?? null;
  }, [activeProfile, refreshKey]);

  const hasPendingProposal = proposal != null;

  /** 组装 assistant_chat 的 scope → 实际 action 与附加参数
   *（旧 aiAnalyze 通道：DEV-0057 PART N 后主对话走 aiStartRun，此逻辑保留兼容） */
  const resolveChatRequest = useCallback(
    (text: string) => {
      const ctx = pageContext;
      const pid = activeProfile?.id;
      if (pid == null) throw new Error("还没有可用的学习档案");
      switch (scope) {
        case "session":
          if (ctx?.sessionId == null) throw new Error("当前没有进行中的学习会话");
          return {
            profileId: pid,
            action: "assistant_chat" as AiActionName,
            sessionId: ctx.sessionId,
            learningItemId: ctx.learningItemId ?? null,
            userInstruction: `（用户关注：本次学习）${text}`,
          };
        case "knowledge":
          if (ctx?.learningItemId == null) throw new Error("当前页面没有选中的知识节点");
          return {
            profileId: pid,
            action: "knowledge_analysis" as AiActionName,
            learningItemId: ctx.learningItemId,
            userInstruction: `（用户自由提问）${text}`,
          };
        case "planning":
          return {
            profileId: pid,
            action: "planning_analysis" as AiActionName,
            userInstruction: `（用户自由提问）${text}`,
          };
        case "profile":
          return {
            profileId: pid,
            action: "profile_analysis" as AiActionName,
            userInstruction: `（用户自由提问）${text}`,
          };
        case "page":
        default:
          // 页面上下文决定附带参数（有 session 用 session；有 item 用 item）
          return {
            profileId: pid,
            action: "assistant_chat" as AiActionName,
            sessionId: ctx?.sessionId ?? null,
            learningItemId: ctx?.learningItemId ?? null,
            userInstruction: text,
          };
      }
    },
    [scope, pageContext, activeProfile]
  );

/** 自由对话发送（assistant_chat 结构化协议 + 只读工具 + 上下文重建）。
   *  DEV-0058 §51-53：三入口统一 Planner——页面按钮（Planning「AI 生成计划」/ Today「AI安排」）
   *  经本函数发送时，若命中规划写意图则交由 AiPanel 主输入的 send()（aiStartRun 新管线：
   *  intent→conflict→readiness→clarification→draft→validation→retry→compiler→ChangeSet→审批，
   *  含 conversation 管理 + 流式事件），不再落入旧 aiAnalyze 通道；其余消息维持旧协议。 */
  const sendChat = useCallback(
    async (text: string) => {
      if (!text.trim()) return;
      setOpenState(true);
      if (PLAN_WRITE_INTENT_RE.test(text.trim())) {
        // 写意图 → 新管线（AiPanel 监听 pendingSendRef 后自动 send）
        pendingSendRef.current = text.trim();
        window.dispatchEvent(new CustomEvent("higher:aipanel-pending-send"));
        return;
      }
      if (busy) return;
      setBusy(true);
      setApiKeyMissing(false);
      const userMsg: AiChatMessage = { role: "user", content: text.trim() };
      setMessages((m) => [...m, userMsg]);
      try {
        const req = resolveChatRequest(text.trim());
        const history = trimHistory(messages);
        const r = await aiAnalyze({ ...req, history });
        // 结构化协议（DEV-0023 §47）：message / knowledge_proposal 两类
        let parsed: AssistantChatResponse;
        try {
          parsed = JSON.parse(r.content) as AssistantChatResponse;
        } catch {
          // 极端情况：后端已修复过一次仍非标准结构 → 按纯文本展示
          parsed = { type: "message", message: r.content };
        }
        const proposal =
          parsed.type === "knowledge_proposal" && parsed.proposal
            ? parsed.proposal
            : null;
        if (proposal && Array.isArray(proposal.operations)) {
          setProposal(proposal);
        }
        setMessages((m) => [
          ...m,
          {
            role: "assistant",
            content: parsed.message || "（AI 没有返回内容，请重试）",
            toolTrace: r.tool_trace,
            contextProvided: r.context_provided,
            tokens: r.total_tokens,
            chatType: parsed.type,
            structured: r,
            diag: {
              action: r.action,
              toolCount: r.tool_trace?.length ?? 0,
              toolRounds: r.tool_rounds ?? 0,
              promptTokens: r.prompt_tokens ?? null,
              completionTokens: r.completion_tokens ?? null,
              totalTokens: r.total_tokens ?? null,
              durationMs: r.duration_ms ?? null,
            },
          },
        ]);
      } catch (e) {
        const msg = String(e);
        if (msg.includes("尚未配置 API Key")) setApiKeyMissing(true);
        setMessages((m) => [
          ...m,
          {
            role: "assistant",
            content: humanizeError(msg),
            error: true,
            // 技术详情：仅安全处理后的原始消息（后端错误不含 API Key / Authorization）
            errorDetail: sanitizeDetail(msg),
          },
        ]);
      } finally {
        setBusy(false);
      }
    },
    [busy, messages, resolveChatRequest]
  );

  /** 快捷 Action（页面按钮触发；结果同样显示在 Panel） */
  const runAction = useCallback(
    async (action: AiActionName, hint?: string, extra?: { date?: string }) => {
      if (busy) return;
      setOpenState(true);
      setActiveAction(action);
      setBusy(true);
      setApiKeyMissing(false);
      const label = hint ?? actionLabel(action);
      setMessages((m) => [
        ...m,
        { role: "user", content: `✨ ${label}` },
      ]);
      try {
        const pid = activeProfile?.id;
        if (pid == null) throw new Error("还没有可用的学习档案");
        const ctx = pageContext;
        const r = await aiAnalyze({
          profileId: pid,
          action,
          sessionId: ctx?.sessionId ?? null,
          learningItemId: ctx?.learningItemId ?? null,
          date: extra?.date ?? null,
        });
        // knowledge_organize → Proposal（待用户审阅）
        if (action === "knowledge_organize") {
          let parsed: AiKnowledgeProposal | null = null;
          try {
            parsed = JSON.parse(r.content) as AiKnowledgeProposal;
          } catch {
            parsed = null;
          }
          setProposal(parsed && Array.isArray(parsed.operations) ? parsed : null);
          setMessages((m) => [
            ...m,
            {
              role: "assistant",
              content: parsed
                ? parsed.summary || "我整理出了以下修改建议。"
                : "AI 未能给出可解析的整理建议，请重试。",
              toolTrace: r.tool_trace,
              contextProvided: r.context_provided,
              tokens: r.total_tokens,
              structured: r,
              diag: buildDiag(r),
            },
          ]);
          return;
        }
        setMessages((m) => [
          ...m,
          {
            role: "assistant",
            content: renderStructured(action, r.content),
            toolTrace: r.tool_trace,
            contextProvided: r.context_provided,
            tokens: r.total_tokens,
            structured: r,
            diag: buildDiag(r),
          },
        ]);
      } catch (e) {
        const msg = String(e);
        if (msg.includes("尚未配置 API Key")) setApiKeyMissing(true);
        setMessages((m) => [
          ...m,
          {
            role: "assistant",
            content: humanizeError(msg),
            error: true,
            errorDetail: sanitizeDetail(msg),
          },
        ]);
      } finally {
        setBusy(false);
      }
    },
    [busy, activeProfile, pageContext]
  );

  const newConversation = useCallback(() => {
    setMessages([]);
    setProposal(null);
    setActiveAction(null);
    setBusy(false);
  }, []);

  const value = useMemo(
    () => ({
      open,
      setOpen,
      pageContext,
      setPageContext,
      messages,
      busy,
      activeAction,
      scope,
      setScope,
      runAction,
      sendChat,
      newConversation,
      proposal,
      setProposal,
      proposalItems,
      setProposalItems,
      hasPendingProposal,
      apiKeyMissing,
      /** DEV-0058：统一 Planner 待发消息（AiPanel 消费后清空） */
      pendingSendRef,
    }),
    [
      open, setOpen, pageContext, messages, busy, activeAction, scope,
      runAction, sendChat, newConversation, proposal, proposalItems,
      hasPendingProposal, apiKeyMissing,
    ]
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useAiPanel(): AiPanelState {
  const v = useContext(Ctx);
  if (!v) throw new Error("useAiPanel 必须在 AiPanelProvider 内使用");
  return v;
}

function actionLabel(a: AiActionName): string {
  switch (a) {
    case "today_suggestion":
      return "AI 今日建议";
    case "session_analysis":
      return "AI 分析本次学习";
    case "knowledge_analysis":
      return "AI 检查当前知识";
    case "knowledge_organize":
      return "AI 帮我整理知识";
    case "planning_analysis":
      return "AI 检查规划";
    case "profile_analysis":
      return "AI 分析当前状态";
    case "daily_review":
      return "AI 复盘这一天";
    default:
      return "AI";
  }
}

/** 「本次请求详情」（DEV-0023 §58）：仅含 Model 侧安全信息，绝不含 API Key / Authorization。 */
function buildDiag(r: AiResult): AiChatMessage["diag"] {
  return {
    action: r.action,
    toolCount: r.tool_trace?.length ?? 0,
    toolRounds: r.tool_rounds ?? 0,
    promptTokens: r.prompt_tokens ?? null,
    completionTokens: r.completion_tokens ?? null,
    totalTokens: r.total_tokens ?? null,
    durationMs: r.duration_ms ?? null,
  };
}

/** 错误技术详情清洗：截断 + 双保险剔除疑似密钥片段（后端本就不输出 Key）。 */
function sanitizeDetail(msg: string): string {
  let s = msg;
  // 防御性：绝不显示 Bearer / sk- 开头的疑似密钥
  s = s.replace(/Bearer\s+[\w\-.]+/gi, "Bearer ***");
  s = s.replace(/sk-[\w\-]{8,}/g, "sk-***");
  return s.length > 600 ? s.slice(0, 600) + "…" : s;
}

/** 把结构化 JSON 渲染为可读文本（Panel 统一展示，不复制页面级 AI 结果 UI） */
function renderStructured(action: AiActionName, content: string): string {
  try {
    const v = JSON.parse(content) as Record<string, unknown>;
    const lines: string[] = [];
    if (typeof v.summary === "string" && v.summary) lines.push(v.summary);
    const listKeys = [
      "suggestions", "covered_topics", "possible_gaps", "questions_to_think_about",
      "next_suggestions", "covered", "possible_missing", "structure_issues",
      "unclear_parts", "suggested_next", "strengths", "possible_issues",
      "blank_areas", "focus_directions", "next_stage_suggestions",
      "learned_today", "complete_records", "need_supplement",
      "worth_verifying", "worth_organizing", "next_steps",
    ];
    for (const key of listKeys) {
      const arr = v[key];
      if (!Array.isArray(arr) || arr.length === 0) continue;
      lines.push("", `【${zhKey(key)}】`);
      arr.forEach((item, i) => {
        if (typeof item === "string") {
          lines.push(`${i + 1}. ${item}`);
        } else if (item && typeof item === "object") {
          const o = item as Record<string, unknown>;
          const title = o.title ?? o.name ?? "";
          const reason = typeof o.reason === "string" ? o.reason : "";
          const mins = typeof o.suggested_minutes === "number" ? `（建议 ${o.suggested_minutes} 分钟）` : "";
          lines.push(`${i + 1}. ${title}${mins}`);
          if (reason) lines.push(`   ${reason}`);
        }
      });
    }
    if (typeof v.recent_progress === "string" && v.recent_progress) {
      lines.push("", `最近推进：${v.recent_progress}`);
    }
    if (lines.length === 0) return "AI 没有返回可展示的内容，请重试。";
    return lines.join("\n");
  } catch {
    return content;
  }
}

function zhKey(k: string): string {
  const map: Record<string, string> = {
    suggestions: "建议",
    covered_topics: "覆盖的知识点",
    possible_gaps: "可能的薄弱或遗漏",
    questions_to_think_about: "值得思考的问题",
    next_suggestions: "下一步建议",
    covered: "已覆盖",
    possible_missing: "可能遗漏",
    structure_issues: "结构问题",
    unclear_parts: "表达不清之处",
    suggested_next: "建议继续学习",
    strengths: "规划的优点",
    possible_issues: "可能的问题",
    blank_areas: "明显的空白区域",
    focus_directions: "值得继续关注的方向",
    next_stage_suggestions: "下一阶段建议",
    learned_today: "今天主要学了什么",
    complete_records: "哪些记录完整",
    need_supplement: "可能需要补充",
    worth_verifying: "值得继续验证",
    worth_organizing: "值得整理进 Knowledge",
    next_steps: "下一步可以考虑",
  };
  return map[k] ?? k;
}

/** 人话错误（不透出 Rust debug / HTTP 原文） */
export function humanizeError(msg: string): string {
  if (msg.includes("尚未配置 API Key")) return "还没有配置 AI。请先在设置中填写 API Key。";
  if (msg.includes("API Key 无效")) return "API Key 无效，请检查设置。";
  if (msg.includes("额度")) return "账户额度不足，请前往服务商充值。";
  if (msg.includes("网络连接")) return "网络连接失败，请检查网络后重试。";
  if (msg.includes("超时")) return "请求超时，请重试。";
  if (msg.includes("模型不存在") || msg.includes("404")) return "模型或接口地址不存在，请检查模型名与 Base URL。";
  if (msg.includes("响应格式")) return "AI 响应格式异常，请重试。";
  if (msg.includes("该知识节点不存在") || msg.includes("不属于当前档案")) return "上下文不存在（知识可能已删除或切换了档案），请刷新页面后重试。";
  if (msg.includes("该学习会话不存在")) return "上下文不存在（学习会话可能已结束或切换了档案）。";
  if (msg.includes("还没有可用的学习档案")) return "还没有可用的学习档案。";
  // 兜底：截断，避免长堆栈
  return msg.length > 160 ? msg.slice(0, 160) + "…" : msg;
}
