# DEV-0077.3 · Runtime Baseline（§六 全链审计）

审计日期：2026-08-27（三路并行全文核对，行号为当前磁盘实际行号）

---

## 1. 事件通道总表（唯一 Tauri 出口 = [run.rs:62-70](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/run.rs#L62) `pub fn emit(app, event, run_id, payload)`，外层信封 `{"run_id", "data"}`）

### 1.1 Production 路径（可达）

| # | 位置 | 事件 | payload.data | Producer | Consumer（前端） | 协议 |
|---|---|---|---|---|---|---|
| 1 | lib.rs:4866 | `ai://run-status` | `{status}`（completed/needs_user_input/cancelled/failed） | `ai_start_run` spawn 收口（run 返回后一发） | AiPanel L355-398（六分支 → refreshMessages） | 消息协议 A |
| 2 | lib.rs:4877 | `ai://error` | `{error}` | 同上 Err 分支（先 add_message「[出错]」再 emit） | AiPanel L407-410 | 消息协议 A |
| 3 | agent.rs:608 | `ai://delta` | `{text}`（ActionPlan 文案全文一次性） | Tool Loop | AiPanel L345-348（`d.delta ?? d.text`） | 消息协议 B |
| 4 | agent.rs:925 | `ai://delta` | `{text}`（FinalAnswer 全文一次性） | Tool Loop | 同上 | 消息协议 B |
| 5 | agent.rs:1124 | `ai://delta` | `{text}`（挂起问题文本） | 收口前 | 同上 | 消息协议 B |
| 6 | agent.rs:780 | `ai://changeset` | `{change_set_id,title,count}` | plan_draft 提案 | AiPanel L352-354 | 通知 |
| 7 | agent.rs:1153 | `ai://memory_proposals` | `{proposals}` | Memory 收口 | AiPanel L412-414 | 通知 |
| 8 | agent.rs:328/392 (agent_tools.rs) | `ai://source` | WebSource | web 工具 | AiPanel L349-351 | 通知 |
| 9 | commands.rs:89 | `ai://applied` | `{change_set_id,profile_id,source}`（**伪 run_id `cs-{id}`**） | 共享 Apply | AiPanel L401-406（不过滤 runId） | 通知 |
| 10 | adaptation/mod.rs:193 | `ai://adaptation_proposal` | proposal payload | Proactive 收口 | AiPanel L416-421 | 通知 |
| 11 | lib.rs:2183 | `higher:ai-profiles-changed` | `{}` | Profile 切换 | AiPanel L219-222 | 通知 |

### 1.2 Legacy / Dead（不可达）

`run_chat_turn`（lib.rs:4887-6384，~1500 行私有函数）**全仓库 0 调用点**（仅 4 个测试以源码文本断言其存在）。其内部 8 个 emit（lib.rs:5084 `ai://changeset` / 5410 `ai://delta`（legacy 真·SSE 流式唯一实现）/ 5583 / 5809 / 5867 `ai://source` / 5899 / 6197 / 6303 `ai://run-status`）均不可达。判定：**DEPRECATED dead path**（agent.rs:12 注释自认「主入口切换成功后待 Cleanup 移除」）。

### 1.3 文档-实现不符

run.rs:3-5 头注释声明 `ai://run-started`、`ai://step` —— 全仓库零实现（前端永不可收）。trace.rs:34 `fn emit` 是 `ai_run_events` DB 写入，非 Tauri 事件。

## 2. Message Protocol 结论（当前共几套）

**两套并存的用户可见消息协议 + 一套孤岛 error**：
- **协议 A（终态拉取式）**：`ai://run-status` / `ai://error` → 前端 refreshMessages 从 DB 拉。终态依赖「run 返回后才 emit」。
- **协议 B（一次性全文推送）**：`ai://delta {text}` 全文一次性（**无逐 token 流**；legacy 流式实现位于 dead path）。
- **孤岛**：`ai://error` 与 run-status 平行、payload 结构不同、由 lib.rs spawn 层而非 run 内发出。
- 其余（changeset/source/memory_proposals/applied/adaptation）为旁路通知，非消息协议。

## 3. 持久化时序（问题 3/4 确证）

### 3.1 Global Agent 主收口（agent.rs）

```
final_text ready（最晚 L1177-1186）
→ L1140-1144  extract_memories（Memory LLM await）   ← 阻塞点：先 memory 再落库
→ L1147-1149  post_turn_apply
→ L1153-1158  emit ai://memory_proposals
→ L1187-1188  add_message（assistant 正式落库）        ← 主答案至此才持久化
→ L1249/1258/1270 finish_run（同锁块内，先 message 后 run ✓）
→ 返回 lib.rs → L4866 emit ai://run-status（终态唯一通知）
```

**确证问题 3**：FINAL_TEXT_READY → MESSAGE_COMMITTED 之间含 Memory LLM call（违反 §九十）。
**确证**：add_message 先于 finish_run（部分顺序已正确）；但全部事件晚于 memory。

### 3.2 run_id Race（问题 4 确证）

`ai_start_run`：L4843 `runs.register()` 同步生成 run_id → spawn → 后台 emit。前端 AiPanel send：L529 `setRunBusy(true)` → L541 `await aiStartRun` → L554 才 `runIdRef.current = rid`。**invoke 返回前，事件过滤 `run_id !== runIdRef.current` 丢弃一切早到事件**（AiPanel L335）。且 client 侧无任何本地 turn 关联键。

### 3.3 Event 可靠性（问题 5 确证）

`let _ = a.emit(...)`（run.rs:66）失败静默；事件被当作消息事实源（run-status 是唯一终态触发），丢失 → UI 停留在 runBusy 直至用户下次 send 顺带 refresh。**无 watchdog、无 reconcile**。

### 3.4 Streaming 现状（问题 1 确证）

- 生产主链 `ModelResponder::chat`（agent.rs:51-103，Live 分支 L59 `c.chat(...)` → client.rs:169 `stream:false`）——**全链路非流式**。
- `AiClient::chat_stream`（client.rs:211-300）已处理 SSE/`delta.content`/usage/取消，**但不支持 tools、不解析 tool_calls、不读 finish_reason、不解析 reasoning_content**；仅 dead path（lib.rs:5408）与 compatibility.rs:480 使用。
- 前端流式区（AiPanel L971-995）：`streamText===""` 时静态「正在思考…」+ 光标；**无任何 stage/进度机制**。

### 3.5 ai_run_events 写入（非 Tauri）

agent.rs:439-449 `workflow_user_context`、agent.rs:967-971 `workflow_researching`；trace.rs:34-42 统一 INSERT（turn_started/route_decided/provider_request_started|finished/run_finished 等）。

## 4. 前端 Runtime 状态分布（现状）

- 散布：runIdRef(L182)+runId state(L147) 双存、runBusy(L146)、streamText(L148)、streamError(L150)、stopped(L151)；hydrationSeqRef(L193)+commitIfLatest(L194-196)。
- 订阅集中 effect L329-427（`reg` 按 runIdRef 过滤）；`ai://applied` 唯一不过滤（L401）。
- send（L494-562）：无 client turn id；乐观用户消息 `id:-Date.now()`；refreshMessages 仅由 run-status 六分支触发。
- loadEarlier（L739-753）未走 hydration guard（runBusy 时禁用，风险受控）。
- api.ts：aiStartRun L1824-1848（无 clientTurnId 字段）；**无 ai_get_run_snapshot / getAiRunStatus**（全库仅任务书提及）；package.json **无任何 test script**。

## 5. Adaptation 路径（§51 对照）

Proactive：mod.rs:187-201 emit proposal → 收口 mod.rs:210-216（add_message + finish_adaptation_run→agent::finish_run）。它**复用 agent 收口**（零自有 SQL）但 emit 独立、不经 run.rs 之外统一层——需在 F 阶段并入 canonical emitter。

## 6. 修复落点映射（→ 施工）

| 任务书问题 | 修复点 |
|---|---|
| 1 非流式 | client.rs chat_stream 最小扩展（tools/tool_calls/finish_reason）+ ModelResponder::chat_streaming + 主循环接线（planner_ready 轮禁流式，防 JSON 逐字泄漏） |
| 2 无阶段反馈 | runtime_events.rs Stage + agent.rs 关键节点 emit |
| 3 memory 先于落库 | 收口重排：finalize_visible_run（add_message→finish_run→message_committed→terminal）→ memory 后置 |
| 4 run_id race | client_turn_id（前端 UUID → ai_start_run → 透传全部事件） |
| 5 Event=事实源 | DB Truth + reconcile + watchdog（1200ms）+ seq 单调去重 |
| 6 三套协议 | runtime_events.rs 唯一 canonical `ai://runtime` + legacy 兼容由 emitter 内部适配层统一发送 |
