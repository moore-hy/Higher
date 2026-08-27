/**
 * DEV-0077.3 §六十-§六十二：Frontend Pure Runtime Tests（UI-TC001~010）。
 *
 * 纯状态机行为测试（node:test 内置 runner，零第三方依赖）：
 * 禁止 read_to_string(AiPanel.tsx) + contains() 式静态断言——那是源码
 * grep，不是 Runtime Evidence。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  bindRunId,
  confirmHydrated,
  createClientTurnId,
  initialRuntimeState,
  isBusyPhase,
  isLatestHydration,
  legacyDeltaInto,
  legacyRunStatusInto,
  nextHydrationSeq,
  reduceAiRuntimeEvent,
  startTurn,
  watchdogResolve,
} from "../../src/components/ai/runtimeState.js";
import type { RuntimeEventPayload } from "../../src/components/ai/runtimeState.js";

const CTID = "ct-1111-2222";
function ev(p: Partial<RuntimeEventPayload>): RuntimeEventPayload {
  return { client_turn_id: CTID, ...p };
}

/** 走完一次 send 起步：startTurn + bindRunId（invoke 已返回 run_id） */
function startedTurn() {
  let s = startTurn(initialRuntimeState(), CTID);
  s = bindRunId(s, "run-A");
  return s;
}

test("UI-TC001: client_turn_id 创建于 aiStartRun invoke 之前（starting 即刻成立）", () => {
  const ctid = createClientTurnId();
  // invoke 之前：仅凭前端本地信息即进入 starting（§二十 立即反馈）
  const s = startTurn(initialRuntimeState(), ctid);
  assert.ok(ctid.length > 0, "client_turn_id 非空");
  assert.equal(s.activeClientTurnId, ctid, "state 绑定本轮凭据");
  assert.equal(s.phase, "starting", "invoke 前已是 starting（零空白等待）");
  assert.equal(s.runId, null, "尚未 invoke：run_id 必为空");
});

test("UI-TC002: run_id 尚为空时，正确 client_turn_id 事件必须被接受（run_id race）", () => {
  let s = startTurn(initialRuntimeState(), CTID); // invoke 返回前
  assert.equal(s.runId, null);
  s = reduceAiRuntimeEvent(s, ev({ kind: "run_started", seq: 1, run_id: "run-A" }));
  assert.equal(s.phase, "running", "run_id 未绑定也不丢首事件");
  s = reduceAiRuntimeEvent(s, ev({ kind: "stage", stage: "loading_context", seq: 2 }));
  assert.equal(s.stage, "loading_context");
});

test("UI-TC003: 旧 client_turn_id 事件一律忽略", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "hello", seq: 3 }));
  const stale = reduceAiRuntimeEvent(
    s,
    ev({ client_turn_id: "ct-OLD", kind: "delta", delta: "XXX", seq: 4 }),
  );
  assert.equal(stale.streamText, "hello", "旧 turn 文本不得混入");
  assert.equal(stale.eventsReceived, s.eventsReceived, "旧 turn 不计入接受事件");
});

test("UI-TC004: seq 单调——收到 5 后再收 4 忽略（防重放/乱序/晚到）", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "a", seq: 5 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "b", seq: 4 }));
  assert.equal(s.streamText, "a", "seq 4 晚到被丢弃");
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "c", seq: 5 }));
  assert.equal(s.streamText, "a", "seq 5 重放同样被丢弃");
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "d", seq: 6 }));
  assert.equal(s.streamText, "ad", "seq 6 正常接受");
});

test("UI-TC005: terminal 到达但 DB message 未 hydrate——streamText 不得清除", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "最终回答", seq: 1 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "message_committed", message_id: 42, seq: 2 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "terminal", status: "completed", seq: 3 }));
  assert.equal(s.phase, "completed");
  assert.equal(s.streamText, "最终回答", "terminal 不清 transient（§四十三）");
  const notYet = confirmHydrated(s, false);
  assert.equal(notYet.streamText, "最终回答", "hydrate 未确认前禁止清（内容不得消失）");
});

test("UI-TC006: message_committed → hydrate message_id 确认后 transient 消失", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "delta", delta: "回答正文", seq: 1 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "message_committed", message_id: 7, seq: 2 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "terminal", status: "completed", seq: 3 }));
  const done = confirmHydrated(s, true);
  assert.equal(done.streamText, "", "persisted message 出现 → transient 清除");
  assert.equal(done.committedMessageId, 7);
});

test("UI-TC007: 事件全丢——watchdog 由 DB snapshot 兜底收敛（DB=Truth）", () => {
  let s = startedTurn();
  // 全程零事件：busy 状态持续
  assert.ok(isBusyPhase(s.phase));
  s = watchdogResolve(s, "running");
  assert.ok(isBusyPhase(s.phase), "running → 继续等待");
  s = watchdogResolve(s, "completed");
  assert.equal(s.phase, "completed", "watchdog 发现终态 → reconcile 收敛");
  // 随后 hydrate 确认（refreshMessages 读到 message）→ transient 清除、Assistant 可见
  const done = confirmHydrated(s, true);
  assert.equal(done.streamText, "", "hydrate 后 Assistant 以 DB 消息可见");
});

test("UI-TC008: hydration 序号守卫——旧请求 A 晚返回不得覆盖新会话 B", () => {
  const seqA = nextHydrationSeq(0); // loadConversation(A) → 1
  const seqB = nextHydrationSeq(seqA); // 切换后 loadConversation(B) → 2
  assert.equal(seqA, 1);
  assert.equal(seqB, 2);
  assert.equal(
    isLatestHydration(seqA, seqB),
    false,
    "A 的慢响应已过期：禁止提交（不得覆盖 B）",
  );
  assert.equal(isLatestHydration(seqB, seqB), true, "B 是最新：允许提交");
});

test("UI-TC009: failed terminal 同样 reconcile（错误不是事件孤岛）", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "error", error_code: "run_failed", seq: 1 }));
  assert.ok(isBusyPhase(s.phase), "kind=error 非终态（§五十二）");
  s = reduceAiRuntimeEvent(s, ev({ kind: "message_committed", message_id: 9, seq: 2 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "terminal", status: "failed", seq: 3 }));
  assert.equal(s.phase, "failed", "terminal failed → phase failed");
  // 错误消息已 DB commit → hydrate 后可见（不靠下一轮 refresh）
  const done = confirmHydrated(s, true);
  assert.equal(done.streamText, "", "[出错] 消息以 DB 消息呈现");
  // watchdog 兜底同径
  let w = startedTurn();
  w = watchdogResolve(w, "failed");
  assert.equal(w.phase, "failed");
});

test("UI-TC010: needs_user_input——问题文本当轮立即可见（不等待下一轮）", () => {
  let s = startedTurn();
  // 挂起轮：问题文本经 legacy 补发通道（compat_delta）推给前端
  s = legacyDeltaInto(s, "run-A", "还需要你确认 2 项信息：");
  assert.equal(s.streamText, "还需要你确认 2 项信息：", "问题文本当轮可见");
  s = reduceAiRuntimeEvent(s, ev({ kind: "terminal", status: "needs_user_input", seq: 3 }));
  assert.equal(s.phase, "needs_user_input");
  const done = confirmHydrated(s, true);
  assert.equal(done.streamText, "", "问题落库 hydrate 后转正式消息");
});

test("§十七: run_id 绑定后，同 client_turn_id 但错误 run_id → 拒绝", () => {
  let s = startedTurn(); // bound run-A
  const wrong = reduceAiRuntimeEvent(
    s,
    ev({ kind: "delta", delta: "x", run_id: "run-OTHER", seq: 5 }),
  );
  assert.equal(wrong.streamText, "", "串台 run 的事件被拒绝");
});

test("§三十四: DB waiting_user 语义在事件层统一为 needs_user_input", () => {
  let s = startedTurn();
  s = legacyRunStatusInto(s, "run-A", "waiting_user");
  assert.equal(s.phase, "needs_user_input");
  let w = startedTurn();
  w = watchdogResolve(w, "waiting_user");
  assert.equal(w.phase, "needs_user_input", "snapshot 通道同样归一");
});

test("§二十八: stage 只更新当前值（不堆叠历史）", () => {
  let s = startedTurn();
  s = reduceAiRuntimeEvent(s, ev({ kind: "stage", stage: "understanding_goal", seq: 1 }));
  s = reduceAiRuntimeEvent(s, ev({ kind: "stage", stage: "planning", seq: 2 }));
  assert.equal(s.stage, "planning", "仅保留当前 stage");
});
