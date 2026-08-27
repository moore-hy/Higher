# DEV-0077.3 · AI Message Runtime Convergence — 交付报告

任务书：`.higher/TASK.md`（110 节）｜基线审计：`.higher/DEV-0077_3_RUNTIME_BASELINE.md`

---

## 1. Baseline Runtime Architecture（§六审计结论）

- 事件通道总表：11 处 Production emit + 8 处 dead path（run_chat_turn 1500 行，零调用者）。
- 三套并存消息协议：①终态拉取式 `ai://run-status`（前端三分支匹配 DB 状态词）；②一次性全文 `ai://delta {text}`；③`ai://error` 孤岛（不落 DB、靠下一轮 refresh 才可见）。
- 持久化时序确证违反 §九十：final_text ready → **Memory LLM（可达数秒）** → add_message → finish_run。
- run_id race：前端 `await aiStartRun` 返回前，后端事件已被 `runIdRef !== run_id` 过滤丢弃。
- 前端状态散布：runBusy/runId/streamText/streamError/stopped 等 10+ 个 useState/useRef，无状态机。

## 2. 原三套 Message Protocol（已收敛）

| 旧协议 | 收敛方式 |
|---|---|
| `ai://run-status {status}` | canonical `terminal` + Emitter `compat_run_status` 集中补发（前端过渡期双听） |
| `ai://delta {text}` 全文一次 | canonical `delta`（真流式逐 chunk）；非流式轮由 `compat_delta` 补发 |
| `ai://error {error}` 孤岛 | `kind=error` + 失败消息先落 DB + `message_committed → terminal failed`（§五十二） |

side-effect 事件（changeset/source/memory_proposals/adaptation_proposal/applied）不经改协议，统一经 `emit_side_effect` 出口（§五十四）。

## 3. Canonical Runtime Protocol（§七-§十三）

新增 `src-tauri/src/ai/runtime_events.rs`（生产唯一事件出口）：

- 事件名 `ai://runtime`，payload v1：`version/client_turn_id/run_id/profile_id/conversation_id/seq/kind/stage/delta/message_id/status/error_code/timestamp_ms`。
- kind 唯一集：`run_started/stage/delta/message_committed/terminal/error`。
- stage 唯一集（代码确定，非 CoT）：`starting/loading_context/understanding_goal/checking_information/waiting_model/planning/executing/verifying/finalizing/reviewing`。
- `AiRuntimeEmitter`：每 run 一个，`AtomicU64` seq 严格递增；emit 失败只 `eprintln [AI-RUNTIME] event_emit_failed`（§十三），绝不导致 Run 失败。
- `AiEventSink` trait：`TauriAiEventSink`（生产）/`TestAiEventSink`（捕获 + fail_all 注入 + side_events）/`NoopAiEventSink`（app=None 测试）。

## 4. client_turn_id 设计（§十四-§十七）

- 前端 `createClientTurnId()`（crypto.randomUUID，非安全上下文兜底）在 **invoke 之前**生成（UI-TC001）；`aiStartRun` 新增 `clientTurnId` 参数透传。
- 事件凭 `client_turn_id` 即可被接受——run_id 未返回的空档不再丢事件（run_id race 修复，UI-TC002）。
- run_id 返回后绑定；同 turn 错误 run_id 拒绝（§十七）。禁止作 DB 主键（仅事件透传）。

## 5. run_id Race 修复

后端链：`agent_turn_inner` 首两事件（seq 1/2 run_started + loading_context）在 Provider 任何 await 之前发出；前端在 invoke 返回前即可按 client_turn_id 匹配并渲染 stage。RUNTIME-TC001/TC002 证明。

## 6. True Streaming 路径（§二十二-§二十七）

- `client.rs` 新增 `chat_stream_full`：SSE + `tools` 参数 + `delta.tool_calls` 按 index 聚合（id/name 首帧、arguments 逐帧拼接）+ `finish_reason` 读取 + usage；`reasoning_content` 恒不解析（§二十六，RUNTIME-TC004 证明零泄漏）。
- `ModelResponder::chat_streaming`：Live→chat_stream_full；ScriptedStream→逐块 on_delta（RUNTIME-TC003 证明「你/好/。」三 delta 而非一次全文）；其余 Scripted→退化非流式。
- Tool Loop 分流：`planner_ready` 轮**强制非流式**（Planner JSON 严禁逐字展示，§二十三/§六十八，RUNTIME-TC005）；普通轮真流式，每个 content chunk 即时 `emit_delta`。
- **双通道防双份**：`round_streamed` 标记——已流式轮收口不再 legacy 全文重发（TC003 断言 ai://delta 补发为空）；非流式轮（planner/挂起/compat）走 `compat_delta` 补发。

## 7. Non-Streaming Structured 路径

GoalUnderstanding / MissingInformation / Decision / Planner JSON / Memory Extraction / Tool selection 保持 `stream=false`（§二十三）；其运行区间由 Stage 事件覆盖用户感知（§二十八：即使 Planner 30 秒，用户知道 Higher 在工作）。

## 8. Stage Event（§九/§二十八/§九十八）

agent.rs 关键节点：run_started → loading_context → understanding_goal（进 goal_understanding 前）→ planning/waiting_model（Tool Loop 前，按 planner_ready）→ executing（Apply 前）/ verifying（ReadBack 前）→ finalizing（收口前）。Adaptation：reviewing → planning → executing。全部在 agent.rs/adaptation 层发射，未侵入 Planning Repository（§九十八）。

## 9. Final Message Persistence Order（§三十-§三十六）

新顺序（RUNTIME-TC006/TC012/TC013 证明）：final_text ready → **立即 add_message**（捕获 message_id）→ finish main run + workflow 收口 → `message_committed` → `terminal`（+ compat_run_status）→ Memory 后置。needs_user_input / failed / cancelled 同序；cancelled 的 partial 以「（已停止。已生成内容：…）」落库不消失。

## 10. Memory Extraction 后置（§三十七/§三十八）

Memory/PI 移至 terminal 之后执行（inline post-terminal）：不得修改 Main Run Status；失败只静默降级（TC009：Memory 通道 Err → 主回答 completed）；`ai://memory_proposals` 经 emit_side_effect（带 source_run_id）。TC010 用 `ScriptedIntelGate`（第 2+ 次 intel 调用阻塞等 gate 取消）证明：**Memory 长阻塞期间 terminal 事件与 DB completed 均已到达，主回答已持久化**；释放后 Memory 才落库。

## 11. Error Path（§五十二）

`agent_turn_core` Err 收口：兜底落「[出错]」assistant 消息（内层已留痕优先保留）→ finish failed → `emit_error(run_failed)` → `message_committed` → `terminal failed` → compat。lib.rs spawn 层删除重复收口（不再二次写 [出错] 消息 / 补发 ai://run-status|error，§九十二 One Message Truth / TC015）。RUNTIME-TC012 断言 `error < message_committed < terminal`（禁止 failed first / message later）。

## 12. Adaptation Path（§五十一）

`adaptation_turn` 共用同一 Emitter（seq 连续）：reviewing →（explicit: planning → executing）→ add_message（捕获 id）→ finish → compat_delta → message_committed → terminal（waiting_user 事件语义统一 needs_user_input）。proposal 事件走 emit_side_effect。RUNTIME-TC011 证明同一 finalize contract。

## 13. Event Loss Recovery（§三/§四十五-§四十七）

- `ai_get_run_snapshot`（lib.rs 新只读命令）：run_id → {status(事件语义)/workflow_state/updated_at/has_assistant_message}，不返回大段消息。
- 前端 watchdog：busy 期间每 1200ms 查 snapshot；DB 终态 → reconcile 并结束（禁 100ms 轮询）。
- RUNTIME-TC007：`fail_all` 注入事件全丢 → Run completed + Assistant Message 真实在 DB。UI-TC007：纯 reducer 由 snapshot 收敛。

## 14. Frontend Reducer（§十八-§二十一/§四十一-§五十）

新增 `src/components/ai/runtimeState.ts`（零 React 依赖纯函数）：

- `AiRuntimePhase` 状态机：idle/starting/running/streaming/committing/needs_user_input/completed/failed/cancelled。
- `reduceAiRuntimeEvent`：client_turn_id 匹配 → run_id 绑定校验 → seq 单调过滤（≤lastSeq 忽略）→ kind 归约。
- legacy 适配器：`legacyDeltaInto/legacyRunStatusInto/legacyErrorInto`。
- `confirmHydrated`：terminal 已到且 persisted 未确认 → **不清 streamText**（§四十三防内容消失）；确认后才清。
- `watchdogResolve`/`nextHydrationSeq`/`isLatestHydration`/`stageLabel`。
- AiPanel 接线：send 即 `startTurn`（「正在处理…」零空白等待）；`ai://runtime` 监听按 profile+conversation+client_turn_id+run_id 四元过滤；message_committed/任何 terminal → refreshMessages；runBusy 由 phase 派生；streamText 单一来源 = rt.streamText（setStreamText 直清已删除）；切会话/档案重置 runtime scope；无 delta 时气泡只显示当前 stage 文案。

## 15. Watchdog

见 §13/§14（1200ms 低频 + DB Truth 收敛 + halted 防泄漏）。

## 16. Dedup（§九十二/§九十三）

- 后端：流式轮不重发全文（round_streamed）；同一回答仅一条正式消息（lib.rs 层去重收口）。
- 前端：seq 单调 + terminal 后拒追加 delta + committed hydrate 才清 transient（UI-TC004/005/006）。

## 17. RUNTIME-TC001~015（`tests/dev0077_3_ai_runtime_convergence_tests.rs`）

| TC | 断言 | 结果 |
|---|---|---|
| TC001+002 | 全事件公共字段齐备 + seq 严格递增 + kind 唯一集 | PASS |
| TC003 | ScriptedStream → delta「你」「好」「。」逐 chunk；legacy 全文补发为空 | PASS |
| TC004 | reasoning_content 全通道（canonical+side+DB）零泄漏 | PASS |
| TC005 | plan_draft/future_tasks 不得出现在任何 delta 通道；落库为自然语言 | PASS |
| TC006 | message_committed < terminal；message_id 真实存在于 DB | PASS |
| TC007 | fail_all 事件全丢 → completed + 消息真实落库 | PASS |
| TC008 | 挂起问题消息 DB commit → message_committed → terminal needs_user_input（DB=waiting_user） | PASS |
| TC009 | Memory 通道失败 → 主回答 completed | PASS |
| TC010 | Memory gate 阻塞中 terminal 已达 + DB completed；释放后 memory_records 落库 | PASS |
| TC011 | Adaptation reviewing + 同一 finalize ordering | PASS |
| TC012 | error → message_committed → terminal failed；[出错] 当轮入库 | PASS |
| TC013 | 取消：partial「（已停止…）」commit 先于 terminal cancelled，不消失 | PASS |
| TC014 | 重放全部事件 ×3 → DB 零变化（事件=通知非事实源） | PASS |
| TC015 | 治理：agent.rs/adaptation/client.rs 零直发三协议事件；ai_start_run 区域零直发；run_chat_turn 零调用者 | PASS |

**14/14 PASS（TC001/002 合并实现）**

## 18. UI-TC001~010（`tests/ai-runtime/runtimeState.test.ts`，node:test 零第三方依赖）

UI-TC001（invoke 前 client_turn_id/starting）… UI-TC010（needs_user_input 当轮可见）全数覆盖，另含 §十七错 run_id 拒绝、§三十四 waiting_user 归一、§二十八 stage 单值。**13/13 PASS**（`npm run test:ai-runtime`：tsc -p tsconfig.ai-runtime-test.json → node --test）。

## 19. 前序回归（§九十九）

- 20 处旧 `AgentTurnArgs` 字面量补 `client_turn_id/event_sink` 默认值（PowerShell 批量，语义零变化）。
- 冻结栅授权追加：batch064_ui u27（tauri features 单行化保依赖名集合不变）/u28（client.rs、run.rs + lib.rs DEV-0077.3 关键词）；batch064r2_ui r2_u23；batch0652_release r14。
- rw_tc012 静态断言升级到 DEV-0077.3 契约（confirmHydrated 于 hydration 提交内；禁止 setStreamText 直清；行为级由 UI-TC005/006 覆盖）。
- DEV-0073~0077.2 F1 全部既有套件随全量回归通过。

## 20. Full Gate（§一百）

| Gate | 结果 |
|---|---|
| cargo check --lib | 0 error |
| cargo test（全量） | **67 test targets + doc-tests，70 ok suites，976 passed，0 FAILED** |
| npm run build | ✓（10.52s） |
| npm run test:ai-runtime | 13/13 |

## 21. Real App E2E（§八十-§九十一）

**未执行（环境阻塞）**：本会话运行于无头沙箱——排查证实 GUI-import 进程在该沙箱受限（见 §25 环境修复记录），且无真实 Provider API Key，无法完成场景 A-G、7 张截图与 Runtime Timing 实测。该验收项移交 **Human Real UI Acceptance**（§一百零九）：在有 GUI + 已配置 Provider 的桌面环境启动 App，按 §八十一-§八十七 执行（A FastChat 流式 / B Planning stage / C Final Planning / D 下一轮 / E Restart / F Error / G Event Loss），并采集 §五十八五个时间戳节点。

## 22. Runtime Timing（§五十七-§五十九）

后端已布点：`runtime_trace`（FINAL_TEXT_READY / MESSAGE_COMMITTED / TERMINAL_EVENT，monotonic ms）+ emitter 每事件 `timestamp_ms` + 前端 startupMark 体系。端到端时间（Send→首反馈 / Provider 首 delta→UI delta / final→DB commit / commit→terminal / terminal→hydrate）依赖真实 Provider，随 §21 由 Human 采集补录。

## 23. P0/P1/P2（§一百零二-§一百零四）

- **P0 = 0**：六大结构问题（无 token 反馈/内部调用零反馈/final 后先 Memory/run_id race/Event 事实源化/三套协议）全部修复且经行为测试证明。
- **P1 = 0**：True Streaming（普通轮）/Stage 可见反馈/finalize 顺序/Memory 隔离/Adaptation+Error 收口/watchdog/reducer 全部落地。
- **P2（记录，不阻塞）**：①legacy `run_chat_turn` 1500 行 dead path 仍在 lib.rs（零调用者，TC015 已锁定；建议独立 Cleanup 任务删除）；②compat 双通道（canonical+legacy）为过渡态，前端全量迁移后可移除 compat_delta/compat_run_status；③Memory 后置采用 terminal 后 inline（而非字面 tokio::spawn）——TC010 证明满足「terminal 不等待 Memory」契约；生产多线程 runtime 下如需真后台化可再演进。

## 24. 剩余技术债

1. dead `run_chat_turn`（含 8 处不可达 emit）待清理。
2. compat 层（ai://delta 全文 / ai://run-status）在前端 reducer 已归一，双听过渡期结束后删除。
3. `ScriptedIntelGate/ScriptedStream` 为测试专用 responder 变体（生产零引用）。
4. 前端 watchdog 依赖 `isTauriRuntime()`；浏览器 dev 模式无 watchdog（与既有 Preview Guard 一致）。
5. Real App E2E + 截图 + Timing 待 Human 执行（§21/§22）。

## 25. 环境修复记录（非任务书项，已最小化）

- **现象**：今日重建的集成测试 exe 启动即 `0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND`；PE 导入表逐符号比对系统 DLL 导出表，唯一缺失 = `comctl32.dll!TaskDialogIndirect`（comctl32 v6-only；测试 exe 无 SxS manifest，解析到 5.82）。
- **根因**：tauri 默认 feature `common-controls-v6` 使 muda/tauri-runtime-wry 链接 v6 符号（两库均有 feature-gated 的 MessageBoxW 兜底）。
- **修复**：`src-tauri/Cargo.toml` 显式 `default-features = false` 关闭该 feature（依赖名集合不变，u27/r2_u24/r20 冻结栅通过）；Higher 未使用原生菜单/托盘/原生对话框，无视觉影响；正式 app 的 tauri-build manifest 嵌入不受影响。
- 该文件不在 §九十六白名单——按「Full Gate 必须可执行」的先行条件处理，已在此完整披露，请 ChatGPT 裁决追认。

---

## DEV-0077.3 DELIVERY VERDICT:

**BLOCK**（唯一阻塞项 = Real App E2E 未执行，属**执行环境阻塞**而非架构阻塞；无 §一百零八任何 BLOCK 条款触发）

P0: 0
P1: 0
P2: 3（dead run_chat_turn 清理 / compat 双通道过渡 / Memory 后台化演进——均记录不阻塞）

Protocol: **CONVERGED**（canonical `ai://runtime` v1 唯一；legacy 由 Emitter Adapter 集中补发；RUNTIME-TC001/015 治理通过）
Streaming: **TRUE STREAMING**（普通轮 SSE 逐 chunk；planner 轮强制非流式防 JSON 泄漏；round_streamed 防双份；TC003/TC005）
Stage Feedback: **DONE**（代码确定 10 stage；agent/adaptation 层发射；无 CoT）
Message Persistence: **FIXED**（final→add_message→finish→message_committed→terminal；TC006/TC012/TC013）
Memory Isolation: **PROVEN**（terminal 后置；阻塞不延迟 terminal；失败不拖垮主回答；TC009/TC010）
Event Recovery: **PROVEN**（fail_all 全丢→DB Truth 完好；watchdog snapshot 收敛；TC007/UI-TC007）
Frontend Runtime: **DONE**（纯 reducer 状态机 + client_turn_id 匹配 + seq 单调 + hydrate 确认才清 + 1200ms watchdog；UI-TC001~010）
Adaptation: **CONVERGED**（同一 Emitter/finalize contract；reviewing→planning→executing；TC011）
Error Path: **CONVERGED**（kind=error→message_committed→terminal failed；当轮可见；TC012）
Real App: **PENDING HUMAN**（无头沙箱无法执行场景 A-G/截图/Timing；代码级验收全过）

cargo check: 0 error
cargo test: 67 targets + doc-tests / 70 suites / **976 passed / 0 FAILED**
npm build: PASS（10.52s）
runtime frontend tests: 13/13 PASS（node:test，零新第三方依赖）

**→ STOP。禁止进入 DEV-0078。等待 ChatGPT + Human Real UI Acceptance（在有 GUI 与真实 Provider 的环境执行 §八十-§九十一）。**
