/**
 * DEV-0077.3 · Frontend AI Runtime State Machine（§十八-§二十一 / §四十一-§五十）。
 *
 * 纯函数模块：零 React 依赖（§十九），AiPanel 只负责
 * 「订阅 Event → dispatch Runtime Event → 渲染」。
 *
 * 最高原则（§三）：SQLite / Run State = Truth；Tauri Event = Notification。
 * 即使所有事件全部丢失，watchdog + reconcile 也必须从 DB 恢复消息。
 *
 * 事件接受规则（§十七 / §五十）：
 * - client_turn_id == activeClientTurnId 才接受（UI-TC003 旧 turn 一律忽略）；
 * - run_id 绑定后同 turn 但错误 run_id → 拒绝；
 * - seq 单调递增：seq <= lastSeq 忽略（UI-TC004 防重放/乱序/晚到）。
 *
 * Transient 清除规则（§四十二/§四十三，UI-TC005/006）：
 * - terminal 到达但 committed message 尚未 hydrate → streamText 保留；
 * - 只有 confirmHydrated 确认 persisted message 出现后才清 transient。
 */

/** §十八：Runtime UI 状态机（phase 是唯一渲染事实源） */
export type AiRuntimePhase =
  | "idle"
  | "starting"
  | "running"
  | "streaming"
  | "committing"
  | "needs_user_input"
  | "completed"
  | "failed"
  | "cancelled";

/** §七：canonical `ai://runtime` 事件 payload（v1） */
export interface RuntimeEventPayload {
  version?: number;
  client_turn_id?: string;
  run_id?: string;
  profile_id?: number;
  conversation_id?: number;
  seq?: number;
  kind?: string;
  stage?: string | null;
  delta?: string | null;
  message_id?: number | null;
  status?: string | null;
  error_code?: string | null;
  timestamp_ms?: number;
}

export interface AiRuntimeUiState {
  phase: AiRuntimePhase;
  /** §十四：本轮 active client turn（send 时生成；UI-TC002 run_id 空档匹配凭据） */
  activeClientTurnId: string | null;
  /** invoke 返回后绑定（§十七：绑定后同 turn 错误 run_id 拒绝） */
  runId: string | null;
  lastSeq: number;
  /** §九/§二十一：当前 stage（只显示当前，不堆叠成聊天消息） */
  stage: string | null;
  streamText: string;
  /** §四十二：message_committed 的 message_id（hydrate 确认凭据） */
  committedMessageId: number | null;
  /** terminal 已到但尚未确认 hydrate（§四十三：此间禁止清 streamText） */
  terminalStatus: string | null;
  error: string | null;
  stopped: boolean;
  eventsReceived: number;
}

export function initialRuntimeState(): AiRuntimeUiState {
  return {
    phase: "idle",
    activeClientTurnId: null,
    runId: null,
    lastSeq: 0,
    stage: null,
    streamText: "",
    committedMessageId: null,
    terminalStatus: null,
    error: null,
    stopped: false,
    eventsReceived: 0,
  };
}

/** §十四（UI-TC001）：invoke 之前创建 client_turn_id（不依赖后端返回） */
export function createClientTurnId(): string {
  // DOM-free 类型（node:test 环境无 DOM lib 也可编译）
  const c = (
    globalThis as { crypto?: { randomUUID?: () => string } }
  ).crypto;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  // 非安全上下文兜底（WebView file:// 等）：时间戳 + 随机段
  return `ct-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

const BUSY_PHASES: ReadonlySet<AiRuntimePhase> = new Set([
  "starting",
  "running",
  "streaming",
  "committing",
]);

export function isBusyPhase(p: AiRuntimePhase): boolean {
  return BUSY_PHASES.has(p);
}

const TERMINAL_PHASES: ReadonlySet<string> = new Set([
  "completed",
  "needs_user_input",
  "failed",
  "cancelled",
]);

export function isTerminalStatus(s: string): boolean {
  return TERMINAL_PHASES.has(s);
}

/** DB waiting_user → 事件语义 needs_user_input（§三十四统一） */
function mapStatus(s: string): AiRuntimePhase | null {
  if (s === "waiting_user") return "needs_user_input";
  if (TERMINAL_PHASES.has(s)) return s as AiRuntimePhase;
  return null;
}

/**
 * §二十（UI-TC001 纯流验证）：Send 点击瞬间（invoke 之前）即进入 starting——
 * 立即反馈不等待 Backend。返回新 state（activeClientTurnId = 本轮凭据）。
 */
export function startTurn(
  state: AiRuntimeUiState,
  clientTurnId: string,
): AiRuntimeUiState {
  return {
    ...initialRuntimeState(),
    phase: "starting",
    activeClientTurnId: clientTurnId,
  };
}

/** §十七：run_id 返回后绑定（仅首次；同 turn 不得改绑） */
export function bindRunId(
  state: AiRuntimeUiState,
  runId: string,
): AiRuntimeUiState {
  if (state.runId != null || state.activeClientTurnId == null) return state;
  return { ...state, runId };
}

/** §五十（UI-TC002/003/004）：canonical 事件接受 + 归约 */
export function reduceAiRuntimeEvent(
  state: AiRuntimeUiState,
  ev: RuntimeEventPayload,
): AiRuntimeUiState {
  // ① active turn 凭据（run_id 未返回时空档也接受，UI-TC002）
  if (state.activeClientTurnId == null) return state;
  if (ev.client_turn_id !== state.activeClientTurnId) return state; // UI-TC003
  // ② run_id 绑定后不得串台（§十七）
  if (state.runId != null && ev.run_id != null && ev.run_id !== state.runId) {
    return state;
  }
  // ③ seq 单调（UI-TC004）
  let lastSeq = state.lastSeq;
  if (typeof ev.seq === "number") {
    if (ev.seq <= lastSeq) return state;
    lastSeq = ev.seq;
  }
  const base: AiRuntimeUiState = { ...state, lastSeq, eventsReceived: state.eventsReceived + 1 };
  switch (ev.kind) {
    case "run_started":
      return base.phase === "starting" ? { ...base, phase: "running" } : base;
    case "stage":
      return { ...base, stage: ev.stage ?? null };
    case "delta": {
      if (base.terminalStatus != null) return base; // terminal 后不再追文本
      const chunk = ev.delta ?? "";
      const phase: AiRuntimePhase = BUSY_PHASES.has(base.phase)
        ? "streaming"
        : base.phase;
      return { ...base, streamText: base.streamText + chunk, phase };
    }
    case "message_committed":
      return {
        ...base,
        committedMessageId: ev.message_id ?? base.committedMessageId,
        phase: BUSY_PHASES.has(base.phase) ? "committing" : base.phase,
      };
    case "terminal": {
      const mapped = ev.status ? mapStatus(ev.status) : null;
      if (!mapped) return base;
      // §四十三（UI-TC005）：streamText 保留——等 confirmHydrated 才清
      return {
        ...base,
        phase: mapped,
        terminalStatus: ev.status ?? null,
        stopped: mapped === "cancelled" ? true : base.stopped,
      };
    }
    case "error":
      // §五十二：kind=error 不是终态；failed 由 terminal 宣告
      return { ...base, error: ev.error_code ?? "error" };
    default:
      return base;
  }
}

/** §五十四：legacy `ai://delta {text}` 适配（非流式轮全文补发通道） */
export function legacyDeltaInto(
  state: AiRuntimeUiState,
  runId: string | null,
  text: string,
): AiRuntimeUiState {
  if (state.activeClientTurnId == null) return state;
  if (runId != null && state.runId != null && runId !== state.runId) return state;
  if (!BUSY_PHASES.has(state.phase)) return state;
  if (state.terminalStatus != null) return state;
  return {
    ...state,
    streamText: state.streamText + text,
    phase: "streaming",
  };
}

/** §五十四：legacy `ai://run-status {status}` 适配（terminal 兼容通道） */
export function legacyRunStatusInto(
  state: AiRuntimeUiState,
  runId: string | null,
  status: string,
): AiRuntimeUiState {
  if (state.activeClientTurnId == null) return state;
  if (runId != null && state.runId != null && runId !== state.runId) return state;
  const mapped = mapStatus(status);
  if (!mapped) return state;
  if (state.terminalStatus != null) return state; // canonical terminal 已收
  return {
    ...state,
    phase: mapped,
    terminalStatus: status,
    stopped: mapped === "cancelled" ? true : state.stopped,
  };
}

/** §五十四：legacy `ai://error {error}` 适配（展示层错误；不终态化） */
export function legacyErrorInto(
  state: AiRuntimeUiState,
  runId: string | null,
  message: string,
): AiRuntimeUiState {
  if (state.activeClientTurnId == null) return state;
  if (runId != null && state.runId != null && runId !== state.runId) return state;
  return { ...state, error: message };
}

/** §四十一（UI-TC005/006）：hydrate 确认——persisted message 出现后才清 transient。
 * terminal 未到（普通刷新，如 legacy waiting_approval）不构成「保留」条件。 */
export function confirmHydrated(
  state: AiRuntimeUiState,
  hasCommittedAssistant: boolean,
): AiRuntimeUiState {
  if (state.terminalStatus != null && !hasCommittedAssistant) return state; // UI-TC005
  return { ...state, streamText: "", stage: null };
}

/** §四十五-§四十六（UI-TC007）：watchdog 以 DB snapshot 收敛（事件全丢兜底） */
export function watchdogResolve(
  state: AiRuntimeUiState,
  snapshotStatus: string,
): AiRuntimeUiState {
  if (state.terminalStatus != null) return state;
  if (!BUSY_PHASES.has(state.phase)) return state;
  const mapped = mapStatus(snapshotStatus);
  if (!mapped) return state; // running → 继续等待
  return {
    ...state,
    phase: mapped,
    terminalStatus: snapshotStatus,
    stopped: mapped === "cancelled" ? true : state.stopped,
  };
}

/** §四十八（UI-TC008）：hydration 序号守卫——旧请求晚返回不得覆盖新会话/新 run */
export function nextHydrationSeq(current: number): number {
  return current + 1;
}
export function isLatestHydration(seq: number, current: number): boolean {
  return seq === current;
}

/** §二十一：stage → 用户可见文案（只显示当前 stage） */
export function stageLabel(stage: string | null): string {
  switch (stage) {
    case "starting":
      return "正在处理…";
    case "loading_context":
      return "正在读取你的 Higher…";
    case "understanding_goal":
      return "正在理解目标…";
    case "checking_information":
      return "正在核对信息…";
    case "waiting_model":
      return "正在思考…";
    case "planning":
      return "正在生成规划…";
    case "executing":
      return "正在写入 Higher…";
    case "verifying":
      return "正在校验结果…";
    case "finalizing":
      return "正在整理回复…";
    case "reviewing":
      return "正在复盘…";
    default:
      return "正在思考…";
  }
}
