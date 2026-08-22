DEV-0062R.1 · Probe Input/Output Truth Repair

Decision-Complete Hotfix Task

本任务由 ChatGPT 完成全部产品、架构、状态语义、探针算法、调用上限、失败分类、UI 行为、测试矩阵和验收决策。

Trae 只负责：

读取真实源码
→ 严格按本任务实现
→ 编译
→ 跑指定自动测试
→ 记录真实结果
→ 自动 Gate 后停止

Trae 不是产品经理，不是架构师，不是 AI 能力策略设计者。

本轮不是 DEV-0063。

本轮不是重新设计 DEV-0062 / DEV-0062R。

本轮只修复 DEV-0062R Human Runtime 新暴露出的：

Probe Input / Output Truth
Basic Chat False Negative
Response Budget / Final Content Truth
Connection Test Semantics

核心原则：

Capability Guard 保留
Provider Truth 保留
No Hidden Fallback 保留
Direct Write = 0
Approval First
Semantic Contract v2 不动
Pending Action 不动
Schema v024 不动
Migration = 0

自动 Gate 通过后必须停止，等待用户 Human Runtime。

0 · 角色与权限

0.1 ChatGPT 已做完的决策

以下内容已决定，Trae 零决策权：

“测试连接”的产品语义

“检测 Higher 兼容性”的产品语义

Basic Chat 的成功条件

final content / reasoning-only / empty content 的含义

finish_reason=length 的处理方式

Probe A-E 的 token budget

Probe A / Probe D 的 retry 条件与次数

Probe B 的 Native / PromptOnly / Repair Once 语义

Probe 是否继续后续阶段

Hard Connection Failure 的定义

Capability 持久化时机

Config Changed During Probe 的处理

Probe Result 安全分类

UI 展示

Automated Tests

Human Runtime

STOP Conditions

Schema / Migration

Trae 不得重新决定。

0.2 Trae 仅允许处理的机械实现

Trae 仅允许自行决定：

局部变量名
private helper 名
Rust borrow / lifetime 写法
TypeScript 类型的等价机械实现
测试 fixture 的局部命名
不改变本任务行为的 CSS 间距细节

只要一个选择可能影响：

用户可见行为
AI 行为
Provider 请求参数
Capability 判定
DB 真值
调用次数
fallback
重试
正式数据
模块职责

就不再属于机械实现。

必须：

STOP
DECISION_REQUIRED

1 · Baseline

施工基线是：

DEV-0062R
AUTOMATED GATE PASSED
HUMAN RUNTIME FAILED AT H02

当前真实 Git HEAD 仍应为：

c19ce78cf85f6e8d1bb4f616576e9f225b897573

DEV-0062 + DEV-0062R 仍处于未提交工作区。

施工前必须运行：

git rev-parse HEAD
git status --short
git diff --stat
Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"

预期：

HEAD = c19ce78...
工作区 DIRTY
DIRTY 内容 = DEV-0062 + DEV-0062R + 当前 TASK 输入

禁止：

git reset --hard
git checkout .
git restore .

如果发现未知用户修改：

STOP
BASELINE_DRIFT

记录 diff 后停止。

2 · Human Runtime 新证据

用户在 DEV-0062R 自动 Gate 通过后重新运行真实 Higher。

2.1 连接测试成功

用户点击：

测试连接

UI：

连接成功，模型：deepseek-v4-flash

说明：

网络可达
认证至少在该请求上成功
模型 endpoint 至少返回了可解析响应 envelope

2.2 第一次 Compatibility 旧结果

旧检测结果曾为：

Basic Chat           ✓
Structured JSON      ✗
Tool Calling         ✓
Temperature 0        ✓
Streaming            ✓

因此 DEV-0062R 重点修了 Structured JSON false negative。

2.3 DEV-0062R Re-Probe 新结果

用户重新点击：

检测 Higher 兼容性

新的真实结果：

不兼容：DeepSeek 无法完成基础对话（empty_content）

Basic Chat           ✗
Structured JSON      未检测
Tool Calling         未检测
Temperature 0        未检测
Streaming            未检测

因此：

Probe A 现在成为新的前置 false negative

Action 继续被 Capability Guard 安全阻止。

2.4 当前结论

不能得出：

DeepSeek 无法聊天

因为：

同一 Connection 在 DEV-0061R 曾真实完成 Higher Action
测试连接仍成功
上一轮 Compatibility Basic Chat 曾通过

当前需要修的是：

Probe A 的输入条件
Response Truth
输出预算
empty final content 的 bounded recovery

3 · Source Audit 已确认的问题

以下按本轮已审计事实施工。

3.1 Test Connection 与 Basic Chat 的语义混在一起

当前 legacy test_ai_connection() 的成功判断近似：

content.is_some()
OR
tool_calls.is_some()

因此：

content = Some("")

可能仍显示：

连接成功

这与：

Higher Basic Chat 必须有非空最终文本

不是同一语义。

本轮不再要求它们结果相同。

必须明确拆分产品含义。

3.2 Probe 使用过小输出预算会产生假阴性风险

当前/历史 Probe 请求存在极小 max_tokens 使用。

对可能存在：

reasoning
thinking
内部推理 token

的 OpenAI-compatible / DeepSeek-compatible 响应，

可能出现：

HTTP 200
reasoning 消耗输出预算
final content = ""

然后被误判：

Basic Chat = false

3.3 当前 response DTO 对 Final Content Truth 信息不足

兼容 Provider 可能返回：

content
reasoning_content
finish_reason
tool_calls

Higher 当前主要依赖：

content
tool_calls

缺少：

reasoning_content
finish_reason

导致：

真正空回答
reasoning-only
length 截断

可能全部被压成：

empty_content

本轮必须增加安全分类能力。

3.4 当前 Probe A 一次空内容就判死

这会形成：

偶发 empty final content
↓
basic_chat=false 持久化
↓
Capability Guard
↓
全部 Action 永久阻断

一次瞬时空响应不能拥有这么大的产品后果。

3.5 当前 Probe A 失败后后续能力全部未检测

用户得到：

Basic ✗
JSON 未检测
Tools 未检测
Temp0 未检测
Streaming 未检测

诊断信息过少。

本轮必须区分：

Hard Connection Failure

和：

Soft Capability Failure

只有 Hard Connection Failure 才允许跳过全部后续 Probe。

4 · 固定产品语义

4.1 “测试连接”只测试连接，不测试 Higher 能力

按钮：

测试连接

固定语义：

验证 Base URL / API Key / Model 能否完成一次基础 API 请求，并返回可解析 completion envelope。

它 不代表：

完整兼容 Higher
Basic Chat 已通过
Structured JSON 已通过
Action 可用

成功文案必须改为：

API 连接成功，模型：<model>。Higher 能力请使用「检测 Higher 兼容性」验证。

失败：

API 连接失败：<sanitized category/message>

4.2 “检测 Higher 兼容性”才负责 Capability Truth

按钮：

检测 Higher 兼容性

继续检测：

Basic Chat
Structured JSON
Tool Calling
Temperature 0
Streaming

只允许它更新：

capabilities_json
compatibility_status
last_test_message
last_tested_at

5 · Response Truth Model

必须在：

src-tauri/src/ai/client.rs

补齐兼容 response 的安全结构信息。

5.1 Choice 必须读取 finish_reason

至少：

finish_reason: Option<String>

允许值由 Provider 返回：

stop
length
tool_calls
content_filter
其他兼容字符串

不要做厂商硬编码 enum。

5.2 Message 必须可读取 reasoning_content

至少：

reasoning_content: Option<String>

注意：

reasoning_content 只用于判断响应类别

禁止：

向用户展示完整 reasoning_content
写入 trace
写入 DB
写入 last_test_message
写入 TRAE_RUN

禁止泄漏模型隐藏推理。

5.3 不改变正常聊天输出

用户正常看到的 assistant 文本仍然只能来自：

message.content

本轮禁止：

reasoning_content → final_text

6 · Final Content 分类

新增 provider-agnostic helper。

推荐位置：

ai/compatibility.rs

固定分类：

FinalText
EmptyFinal
ReasoningOnly
LengthTruncated
ToolOnly
RequestError

具体判定：

6.1 FinalText

content exists
AND
content.trim() 非空

6.2 ReasoningOnly

content None/blank
AND
reasoning_content exists
AND
reasoning_content.trim() 非空

对 Basic Chat：

不算成功

但允许触发 bounded retry。

6.3 LengthTruncated

如果：

finish_reason == "length"

且没有非空 final text：

LengthTruncated

允许触发 bounded retry。

6.4 ToolOnly

content blank
AND
tool_calls 非空

Basic Chat：

不算成功

Tool Calling Probe：

按 Tool Probe 自己的 validator 判断

6.5 EmptyFinal

content blank
reasoning blank
finish_reason != length
tool_calls empty

7 · Probe Budget Constants

禁止各 Probe 随便写魔法数字。

在：

ai/compatibility.rs

定义固定常量。

必须使用：

CONNECTIVITY_MAX_TOKENS          = 64

BASIC_CHAT_MAX_TOKENS_1         = 256
BASIC_CHAT_MAX_TOKENS_2         = 1024

STRUCTURED_NATIVE_MAX_TOKENS     = 256
STRUCTURED_PROMPT_MAX_TOKENS     = 256
STRUCTURED_REPAIR_MAX_TOKENS     = 512

TOOL_CALL_MAX_TOKENS             = 256

TEMPERATURE_ZERO_MAX_TOKENS_1    = 256
TEMPERATURE_ZERO_MAX_TOKENS_2    = 1024

STREAMING_MAX_TOKENS             = 256

禁止 Trae自行调整数值。

如果源码 API 不允许其中某值：

STOP
TOKEN_BUDGET_CONTRACT_BLOCKED

8 · Probe A · Basic Chat

8.1 请求内容

Synthetic，无用户 Higher 数据。

固定：

System:
You are a capability probe. Return a short visible final answer.

User:
Reply with HIGHER_OK.

不得包含：

PersonalProfile
Task
Goal
Knowledge
Conversation History
API Key

8.2 Attempt 1

固定：

temperature = 0.0
max_tokens = 256
tools = 0
JSON mode = off

8.3 Attempt 1 成功

只要分类：

FinalText

则：

basic_chat = true

不要求精确等于 HIGHER_OK。

Basic Chat 测的是：

能否产生可见 final answer

不是 instruction-following benchmark。

8.4 Attempt 1 可恢复失败

以下：

EmptyFinal
ReasoningOnly
LengthTruncated

进入 Attempt 2。

8.5 Attempt 2

固定：

temperature = 0.0
max_tokens = 1024
tools = 0
JSON mode = off

Prompt 与 Attempt 1 相同。

8.6 Attempt 2 成功

FinalText：

basic_chat = true

并在安全诊断中记录：

basic: pass_after_retry

8.7 Attempt 2 仍失败

如果：

EmptyFinal
→ basic_chat=false
category=no_final_content

ReasoningOnly
→ basic_chat=false
category=reasoning_only_no_final

LengthTruncated
→ basic_chat=false
category=length_no_final

ToolOnly
→ basic_chat=false
category=unexpected_tool_only

8.8 Request Error

Attempt 1 请求错误需要先分类：

Hard Connection Failure

仅：

authentication failure
authorization failure
endpoint/model not found
DNS/connect failure
invalid Base URL

这种无法继续整个 Probe 的错误。

结果：

basic_chat=false
compatibility=incompatible
后续 B-E = skipped_connection_failure

Soft Request Failure

其他 provider request error：

basic_chat=false

但后续 Probe：

允许继续

以获得完整诊断。

禁止用：

字符串 contains("400")
字符串 contains("422")

决定产品语义。

如果当前 client 层拿不到 HTTP status：

允许最小增加：

sanitized error category / status

但禁止大改 client 架构。

9 · Probe B · Structured JSON

DEV-0062R 已修好的逻辑保持。

不得重写语义。

继续：

Force Native
temp=0
max_tokens=256
↓
真实 parse_turn_decision
↓
失败
↓
Force PromptOnly
temp=0
max_tokens=256
↓
失败但 HTTP success + non-empty
↓
Repair Once
temp=0
max_tokens=512

最大：

3 calls

保持：

Native valid → native
PromptOnly/Repair valid → prompt_only
全部失败 → structured=false / unknown

9.1 Probe B 不得依赖 Probe A “必须 true”才能运行

新规则：

如果：

没有 Hard Connection Failure

即使：

basic_chat=false

仍允许执行 Probe B。

原因：

获得完整诊断
不让一个 Soft Basic false 把所有能力都变成“未检测”

最终 Control compatibility 仍严格要求：

basic=true
json=true
temp0=true

所以不会降低安全性。

10 · Probe C · Tool Calling

保持 DEV-0062R validator：

function.name == higher_capability_probe
arguments parse success
arguments.ok == true

请求：

temperature = 0.0
max_tokens = 256

即使 Probe A soft fail：

仍允许执行

只有：

Hard Connection Failure

才 skip。

11 · Probe D · Temperature Zero

本轮修复重点之一。

11.1 Attempt 1

固定：

temperature=0.0
max_tokens=256
tools=0
JSON mode=off

Synthetic prompt 同 Basic Chat。

11.2 成功

必须：

FinalText

11.3 Retry Once

如果：

EmptyFinal
ReasoningOnly
LengthTruncated

Retry：

temperature=0.0
max_tokens=1024

11.4 最终

Retry FinalText：

temperature_zero=true

否则：

temperature_zero=false

12 · Probe E · Streaming

固定：

temperature=0.0
max_tokens=256

成功条件：

stream request success
AND
至少一个非空 visible delta
OR
最终 assembled final text 非空

以下：

HTTP success
但所有 delta 空
最终 text 空

必须：

streaming=false
category=stream_empty

本轮不增加 streaming retry。

Streaming 仍：

不参与 Full formal capability 判定

保持 DEV-0062 语义。

13 · Probe Orchestration

13.1 默认运行所有 A-E

除：

Hard Connection Failure

外，

Compatibility Probe 必须尝试：

A
B
C
D
E

不能再出现：

Basic soft false
→ B-E 全部未检测

13.2 Hard Connection Failure

出现后：

A = false
B-E = skipped
overall = incompatible

UI 明确：

未继续检测：连接/认证失败

13.3 Provider Call 总上限

没有 Hard Connection Failure 时，单次完整 Probe 最多：

A 2
B 3
C 1
D 2
E 1
=
9 Provider Calls

禁止超过：

9

如果未来实现需要超过：

STOP
PROBE_CALL_BUDGET_EXCEEDED

14 · Probe Snapshot / Stale Result Guard

这是本轮必须增加的可靠性保护。

14.1 Probe 开始时冻结 Connection Snapshot

内存中复制：

profile id
adapter_kind
base_url
api_key
model
thinking_mode
enabled

禁止：

运行到一半重新读 settings 改请求

14.2 Probe 结束前重新读取 Connection

保存结果前：

re-read profile

比较影响 Capability 的字段：

adapter_kind
base_url
api_key
model
thinking_mode

如果任何字段变化：

DISCARD Probe Result
DO NOT overwrite existing capabilities
DO NOT overwrite last_tested_at

返回：

AI 连接配置在检测过程中发生变化，本次结果已丢弃，请重新检测。

API Key 比较只允许内存比较。

禁止：

log key
persist key hash for this purpose

15 · Probe Persistence Must Be Atomic

Probe A-E 全部完成后，才允许一次性保存：

capabilities_json
compatibility_status
last_test_message
last_tested_at

禁止：

A 完成就先写 basic=false
然后 B 崩溃
数据库留下半套新结果

如果 Probe 内部发生：

unexpected internal error
parser panic equivalent
repository error

必须：

保留旧 capability truth
返回错误

不能写半成品。

16 · Capability Guard 保持不变

继续：

Control Required:
basic_chat
structured_json
temperature_zero

任一：

Some(false)

Action：

deterministic reject before Provider

保持：

Direct Write = 0

本轮禁止通过放宽 Guard 修问题。

17 · Connection Test Implementation

测试连接 不允许继续使用模糊文案。

17.1 请求

固定：

temperature=0.0
max_tokens=64
tools=0
JSON mode=off

Synthetic：

Reply briefly.

17.2 Success

成功只代表：

HTTP/provider request succeeded
response envelope successfully parsed
至少有一个 choice/message

即使：

content blank

也可以叫：

API 连接成功

因为这不是 Capability Test。

但是文案必须明确：

API 连接成功，模型：<model>。Higher 能力请使用「检测 Higher 兼容性」验证。

禁止：

“模型正常”
“完全可用”
“支持 Higher”

17.3 Failure

只显示 sanitized：

认证失败
连接失败
模型/接口不存在
请求失败
响应格式异常

禁止 raw body / API Key。

18 · UI · Compatibility Diagnostics

保持轻量，不新增页面。

Connection 卡继续显示：

Basic Chat
Structured JSON + Strategy
Tool Calling
Temperature 0
Streaming
Last Tested
Primary / Control badge

18.1 Basic Chat Detail

检测后至少能显示：

✓
✓（二次尝试成功）
✗ 无最终文本
✗ 仅 reasoning，无最终文本
✗ 输出被长度截断且重试仍无最终文本
✗ 请求失败
未检测

不要显示 reasoning 原文。

18.2 Temperature 0 Detail

类似：

✓
✓（二次尝试成功）
✗ 无最终文本
✗ 请求失败

18.3 Skipped

Hard Connection Failure 时：

Structured JSON   未继续检测（连接失败）
Tool Calling      未继续检测（连接失败）
Temperature 0     未继续检测（连接失败）
Streaming         未继续检测（连接失败）

18.4 Last Test Summary

last_test_message 只能保存安全摘要。

例如：

basic=pass_after_retry; json=prompt_only; tools=pass; temp0=pass; stream=pass

或：

basic=reasoning_only_no_final; json=native; tools=pass; temp0=no_final; stream=pass

禁止：

完整 response
完整 reasoning
完整 prompt
Authorization
API Key

19 · UI · Running State

用户点击：

检测 Higher 兼容性

对应 Connection 的：

测试连接
检测 Higher 兼容性
编辑
删除

在 Probe 完成前至少：

检测按钮 disabled
避免重复点击产生并发 Probe

推荐该连接卡显示：

检测中…

无需新增全局锁。

20 · No Provider-Specific Branches

禁止写：

if provider == DeepSeek { ... }
if model.contains("deepseek") { ... }
if provider == GLM { ... }

本轮全部规则：

Provider-agnostic

reasoning_content 只是兼容字段。

不能把：

reasoning models

等同某一家厂商。

21 · Schema / Migration

严格：

Schema Before = v024
Schema After  = v024
Migration     = 0

禁止：

v025
修改 v001-v024

如果认为必须：

STOP
SCHEMA_CHANGE_REQUIRED

22 · 指定修改文件

本轮预期主要修改：

src-tauri/src/ai/client.rs
src-tauri/src/ai/compatibility.rs
src-tauri/src/lib.rs
src/pages/Settings.tsx
src/styles.css

允许：

src/types.ts
src/api.ts

只在 Compatibility result 类型机械需要时最小修改。

新增：

src-tauri/tests/batch062r1.rs

本轮原则上 不应修改：

action.rs
grounding.rs
planner.rs
semantic_contract.rs
action_continuation.rs
ai_pending_action.rs
migration files
Task/Goal/Knowledge/Session domain repository

如果必须修改这些：

STOP
SCOPE_EXPANSION_REQUIRED

说明原因。

23 · Automated Tests · 新增 batch062r1

新增：

src-tauri/tests/batch062r1.rs

必须继续使用：

localhost fake provider
127.0.0.1
std::net::TcpListener / existing runtime

禁止真实公网 Provider。

禁止真实 API Key。

关键网络行为测试不能退化为纯 read_src()。

24 · Response Truth Tests

R1

content="OK"
reasoning=None
finish=stop

分类：

FinalText

R2

content=""
reasoning=""
finish=stop

分类：

EmptyFinal

R3

content=""
reasoning="internal..."
finish=stop

分类：

ReasoningOnly

不得保存/展示 reasoning 原文。

R4

content=""
reasoning=None
finish=length

分类：

LengthTruncated

R5

content=""
tool_calls non-empty
finish=tool_calls

分类：

ToolOnly

25 · Basic Chat Probe Tests

R6

Attempt1：

200 + final text

结果：

basic=true
request_count=1

R7

Attempt1：

200 + empty final

Attempt2：

200 + final text

结果：

basic=true
detail=pass_after_retry
request_count=2

R8

Attempt1：

reasoning-only

Attempt2：

final text

结果：

basic=true

R9

两次：

reasoning-only

结果：

basic=false
detail=reasoning_only_no_final

R10

Attempt1：

finish_reason=length
no final

Attempt2：

final

结果：

basic=true

R11

两次 empty：

basic=false
detail=no_final_content

R12

请求体必须验证：

attempt1 max_tokens=256
attempt2 max_tokens=1024
temperature=0
tools absent
response_format absent

26 · Hard / Soft Failure Tests

R13

Fake Provider 认证失败：

Hard Connection Failure
A=false
B-E skipped

R14

Fake Provider connect/request soft error 后恢复/其他 endpoint 阶段可响应：

按实现允许：

A=false
B-E 继续

核心断言：

Soft Basic failure != 全部未检测

27 · Full Probe Continuation Tests

R15

Basic 两次 empty：

basic=false

但：

Structured valid
Tool valid
Temp0 valid
Stream valid

最终 Capability：

basic=false
structured=true
tools=true
temp0=true
stream=true
overall=incompatible

这是本轮非常重要的诊断语义。

28 · Temperature Zero Tests

R16

Attempt1 final：

temp0=true
count=1

R17

Attempt1 empty，Attempt2 final：

temp0=true
count=2

R18

Attempt1 reasoning-only，Attempt2 reasoning-only：

temp0=false

R19

请求预算：

256
1024

且：

temperature=0

29 · Structured Regression Tests

必须继续通过 DEV-0062R 行为。

R20

Native valid：

structured=true
strategy=native

R21

Native 200 invalid：

PromptOnly attempted

R22

PromptOnly invalid：

Repair Once

R23

Structured 总调用：

<=3

R24

旧 json_strategy=prompt_only：

Re-Probe 仍真实 ForceNative

30 · Tool / Streaming Tests

R25

正确 tool：

tools=true

R26

错误 tool / invalid args：

tools=false

R27

非空 stream：

streaming=true

R28

全空 stream：

streaming=false

R29

Streaming request：

max_tokens=256
temperature=0

31 · Connection Test Tests

R30

HTTP success + valid completion envelope + empty content：

test connection = success

但 success message 必须包含：

API 连接成功
Higher 能力请使用「检测 Higher 兼容性」验证

不能声称 Basic Chat。

R31

认证失败：

test connection = failure

R32

malformed provider response：

test connection = failure

32 · Snapshot / Atomic Persistence Tests

R33

Probe 运行中 Connection model 被修改。

Probe 完成：

result discarded
old capability DB untouched

R34

Probe 运行中 API Key 被修改。

只允许内存比较。

结果：

discard
DB capability untouched
logs contain no key

R35

Probe 内部发生 unexpected error。

结果：

旧 capabilities 保留
不得写半套新结果

R36

正常 Probe 全部完成。

结果：

capabilities/status/message/tested_at 一次性落库

33 · Security Tests

R37

Fake Provider reasoning_content：

"HIGHLY_SECRET_REASONING_ABC"

最终：

last_test_message 不包含它
trace 不包含它
UI result 不包含它

R38

Fake API key：

sk-FAKE-SECRET-062R1

不得出现在：

probe result
last_test_message
trace
error

34 · Call Budget Test

R39

最坏可恢复完整 Probe：

A=2
B=3
C=1
D=2
E=1

总 Provider Calls：

<=9

35 · Guard Regression

R40

最终：

basic=false
json=true
temp0=true

Control：

仍 reject

R41

最终：

basic=true
json=true
temp0=true
tools=false
status=limited

Control：

compatible
Action 不被 overall limited 拦截

36 · Existing Regression Gate

按顺序：

npx tsc --noEmit
npm run build
cargo check -j 1

cargo test --test batch062r1 -j 1
cargo test --test batch062r -j 1
cargo test --test batch062 -j 1
cargo test --test batch061r -j 1
cargo test --test batch0602 -j 1
cargo test --test batch0601 -j 1
cargo test --test batch060 -j 1
cargo test --test batch0592 -j 1
cargo test --test ai_foundation -j 1
cargo test --test ai_assistant -j 1
cargo test --test ai_panel -j 1

默认：

不跑 full cargo test

除非 targeted regression 证明必须扩大。

37 · Automated Gate 禁止真实 Provider

Trae 自动阶段：

Real DeepSeek Calls = 0
Real GLM Calls = 0
Real OpenAI Calls = 0
Real Public Provider Calls = 0

只使用 localhost fake provider。

38 · Human Runtime Gate

自动 Gate 通过后：

STOP CODING

状态只能：

DEV-0062R.1
AUTOMATED GATE PASSED
HUMAN RUNTIME PENDING

39 · Human Runtime Checklist

H00 · Startup

真实用户 DB 启动。

要求：

Schema v024
原数据不丢
DeepSeek Connection 不丢

H01 · Test Connection Semantic

点击：

测试连接

预期成功时文案：

API 连接成功，模型：deepseek-v4-flash。
Higher 能力请使用「检测 Higher 兼容性」验证。

不能再让用户误解：

连接成功 = Higher 全能力成功

H02 · DeepSeek Compatibility Re-Probe

点击：

检测 Higher 兼容性

必须得到五项结果。

禁止：

Basic soft failure
→ 后四项全未检测

H03 · Basic Chat Retry Truth

如果第一次真实响应出现：

empty / reasoning-only / length

Higher 必须自动 bounded retry。

如果第二次有 final：

Basic Chat ✓（二次尝试成功）

H04 · Expected DeepSeek Control Truth

当前用户真实 Connection 若实际可用，目标是观察：

Basic Chat          ✓
Structured JSON     ✓ Native or Prompt Only
Temperature 0       ✓

Tool / Streaming 可以影响 overall Full/Limited，

但只要：

basic/json/temp0=true

Control 必须可用。

H05 · Fresh Action

新 Conversation：

把8月25日的TEST-STABLE改成35分钟。

如果唯一匹配：

真实 Proposal / ChangeSet

不能：

control_capability_guard

前提是 H04 Control Requirements 已通过。

H06 · Approval First

Proposal 未 Apply：

正式 Task 不变

Apply：

Task estimated_minutes=35

H07 · Ambiguity

制造两个候选。

发送修改命令。

预期：

真实 candidate clarification
0 ChangeSet

H08 · Deterministic Continuation

回答：

第一个

预期：

0 Interpreter Provider Call
恢复 Pending Action
生成 Proposal

H09 · Restart

Pending 后关闭 Higher。

重启同 Conversation：

第一个

继续成功。

H10 · Cross Conversation

Conversation B：

第一个

不得读取 A Pending。

40 · Documentation

TRAE_RUN.md

必须追加：

DEV-0062R.1
Start Time
Baseline
Human Runtime Evidence
Probe A Root Cause
Test Connection Semantics
Response Truth Model
reasoning_content handling
finish_reason handling
Token Budgets
Basic Retry
Temperature0 Retry
Hard vs Soft Failure
Full Probe Continuation
Probe Snapshot
Atomic Persistence
Automated Tests
Final Gate
Real Provider Calls = 0
Human Runtime = Pending

ENVIRONMENT.md

更新：

Current DEV = DEV-0062R.1
Schema v024
Migration 0
Probe Input/Output Truth repaired
Human Runtime Pending

PRODUCT.md

本轮：

NO CHANGE

除非 TASK 明确要求的“测试连接 ≠ Higher 兼容性”在 PRODUCT 中已有冲突事实。

如存在冲突：

STOP
DOC_PRODUCT_CONFLICT

不要自行改产品规则。

WORKING_RULES.md

本轮：

NO CHANGE

现有 AI-INV-017~022 保持。

41 · TASK 权限

Trae：

只读 .higher/TASK.md

禁止修改。

42 · Scope Exclusions

本轮禁止顺手修改：

Knowledge
Goal
Planning
Today
Session
Data
Export/Import
通知
Mastery
Memory
Skill
Agent
Planner
Grounding
Action Continuation
Semantic Contract
普通视觉重构

43 · STOP Conditions

STOP-01

想删除：

Capability Guard

STOP-02

想把：

empty content

直接当 Basic Chat 成功。

STOP-03

想把：

reasoning_content

直接作为用户 final answer。

STOP-04

想把完整 reasoning 保存/显示。

STOP-05

想对：

DeepSeek / GLM

写厂商特判。

STOP-06

想新增：

v025
migration

STOP-07

需要修改：

Semantic Contract v2
Pending Action
Planner
Grounding

STOP-08

单次 Probe Provider Calls 需要：

> 9

STOP-09

想做无限 retry。

STOP-10

自动测试必须烧真实 Provider Key。

STOP-11

无法原子保存完整 Probe Result。

STOP-12

无法实现 Config Changed During Probe 的 stale result discard。

STOP-13

当前工作区出现未知非 DEV-0062/0062R 修改。

停止格式：

STOP
<CODE>

Current Fact:
...

Why:
...

Decision Needed:
...

44 · Definition of Done

全部满足才算自动 Gate 通过：

Schema v024
Migration 0

Test Connection 语义 = API connectivity only
Test Connection 文案不冒充 Higher capability

Response DTO 支持 finish_reason
Response DTO 可安全识别 reasoning_content
reasoning_content 不展示/不持久化

Basic Probe:
temp=0
256 tokens
empty/reasoning/length → retry once
retry=1024
final text only = success

Temperature0 Probe:
256 → bounded retry 1024

Structured Probe:
保持 DEV-0062R Native/PromptOnly/Repair Once
总 <=3

Tool Probe:
256
严格 tool validator

Streaming:
256
empty stream = false

No Hard Connection Failure:
A-E 都尝试

Hard Connection Failure:
B-E skipped with clear reason

Total Probe Calls <=9

Probe uses immutable config snapshot
Config change during probe → discard
Partial/internal failure → old capability truth retained
Full result persistence atomic

Capability Guard unchanged
Limited-but-control-compatible remains usable
No Hidden Provider Fallback unchanged
Direct Write=0
Approval First
Semantic Contract v2 unchanged
Pending Action unchanged

batch062r1 all green
batch062r all green
batch062 all green
batch061r all green
batch0602 all green
batch0601 all green
batch060 all green
batch0592 all green
ai_foundation all green
ai_assistant all green
ai_panel all green

tsc=0 errors
npm build=PASS
cargo check=0 errors

Real Provider Automated Calls=0

最终：

AUTOMATED GATE PASSED
HUMAN RUNTIME PENDING

45 · Trae 最终回复格式

只允许：

DEV-0062R.1 AUTOMATED GATE RESULT

Status:
AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING
/ STOP <CODE>

Baseline:
HEAD:
Git Status Before:
Git Status After:

Schema Before:
v024

Schema After:
v024

Migration:
0

Human Runtime Evidence Addressed:
...

Root Cause:
Basic Probe False Negative:
Connection Test Semantic Gap:
Output Budget Risk:
Final Content Truth Gap:
Early Probe Abort Gap:

Connection Test:
Semantics:
Request:
Success Message:

Response Truth:
finish_reason:
reasoning_content:
reasoning persistence:
0

Probe Budgets:
Connectivity:
Basic Attempt1:
Basic Attempt2:
Structured Native:
Structured Prompt:
Structured Repair:
Tool:
Temp0 Attempt1:
Temp0 Attempt2:
Streaming:

Basic Chat Retry:
...

Hard vs Soft Failure:
...

Probe Continuation:
...

Probe Snapshot:
...

Atomic Persistence:
...

Capability Guard:
UNCHANGED

Provider Truth:
UNCHANGED

Direct Write:
0

Pre-Approval Formal Mutation:
0

Semantic Contract:
v2 unchanged

Pending Action:
unchanged

Tests:
batch062r1: X/X
batch062r: X/X
batch062: X/X
batch061r: X/X
batch0602: X/X
batch0601: X/X
batch060: X/X
batch0592: X/X
ai_foundation: X/X
ai_assistant: X/X
ai_panel: X/X

Frontend:
tsc:
build:

Cargo:
check:

Real Provider Automated Calls:
0

Human Runtime:
PENDING

Source Conflicts:
NONE / ...

Decision Required:
NONE / ...

Files Added:
...

Files Modified:
...

TRAE_RUN Updated:
YES

ENVIRONMENT Updated:
YES

PRODUCT Updated:
NO

WORKING_RULES Updated:
NO

TASK Modified:
NO

Git Commit:
NO

Git Push:
NO

禁止附带：

DEV-0063 建议
新产品功能
git commit
git push

下一步由 ChatGPT 根据用户 Human Runtime 和 TRAE_RUN 决定。