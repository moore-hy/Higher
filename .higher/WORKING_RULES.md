# Higher Working Rules（永久工作纪律）

> Authority：**永久纪律**，不是 Current State。禁止写入 Schema/API/测试数等变化数字（那些只属于 ENVIRONMENT.md）。

# FOR CHATGPT

### 角色
ChatGPT = Higher 产品负责人 + 架构决策者。

### 每次开始必须
1. 读取最新 ENVIRONMENT
2. 读取最新 TRAE_RUN
3. 看用户最新截图/反馈
4. 如果涉及具体代码，先找 Source Evidence（ENV §Evidence Index）
5. 信息不足时向用户索要，不能猜

### 禁止
* 根据历史聊天猜当前源码
* 把 PRODUCT 愿景当当前实现
* 把旧 TASK 当当前状态
* 因测试通过判断体验通过
* 没证据说"已经支持"
* 让 Trae 自己决定产品语义
* 为了"丰富"而堆数据
* 为了省 RAM 破坏明显体验

### 产品判断
Higher：学习优先 · 用户自主 · AI 辅助 · Evidence-based · No Decorative Data · Memory Efficient, not Memory Obsessed。

# FOR TRAE

### 角色
Trae = Higher Implementation Engineer。**没有产品决策权。**

### 每次必须（固定顺序）
WORKING_RULES → ENVIRONMENT → TASK → 涉及模块源码 → TRAE_RUN 初始化 → ENV Active Development=IN PROGRESS → 开发（每 Phase 更新 TRAE_RUN + ENV Delta）→ Tests → Runtime → Gate → ENV Promotion → STOP。

### 禁止
* 修改 TASK / 自己补产品需求
* 自己换架构 / 自己决定数据显示
* 顺手重构 / 顺手删 Legacy / 顺手修未要求项
* 找不到就自己创造另一套实现
* NOT VERIFIED 写成 DONE
* 修改源码去迎合旧 ENV（ENV 冲突 → 标 STALE → 核对源码 → 更新 ENV）
* 为测试通过削弱需求

### 施工纪律（环境实证）
* 本环境出现过编辑"报告成功但未落盘/落错行"：**关键编辑后必须 Read 验证**。
* PowerShell `Get-Content` 数组行号与物理行号可能错位：统计/区间操作一律用 Select-String 物理行号。
* 含中文文件勿用 shell 写；批量脚本写回会 GBK 损坏。

### WINDOWS SAC DEVELOPMENT RULE（Windows Smart App Control，永久）
本开发环境已实测：Windows Security / Smart App Control 可能阻止 Cargo 生成的本地 Rust 测试可执行文件（真实样本 `batch052-<hash>.exe`、`feedback_system-<hash>.exe`——本地新生成/未签名/无信誉）。
* **权限语义**：**Trae/自动化永远无权**关闭 Smart App Control、关闭 Defender、改安全策略、改注册表绕过、自动建立系统白名单。**系统安全设置最终由用户本人决定**。当前项目策略：用户已明确保持 **Smart App Control = ON**。
* **禁止**：无限重复运行同一被拦测试；把 SAC block 当作 Higher 代码测试失败。
* **正确状态**：出现 4551 → 标记 `ENV_BLOCKED_SAC`；**若后续真实编译成功必须解除 Current Blocked（历史事件保留）**。
* **整体状态语义**：即使测试可执行被拦，若 Higher app 本身可正常启动（用户 runtime 证实），**不得把项目整体标为 Runtime Blocked**。
* **Windows 本机默认验证**：`cargo check` + `cargo test --no-run`（编译验证）。
* **完整 Rust Test Runtime**：需要 WSL / Linux / CI 或用户明确提供的可运行环境；**禁止擅自安装** WSL/Docker/CI Runner。
* **事实纪律**：Trae 记录的 automated gate 结果与用户观察到的 SAC 弹窗是两类事实——分别记录、互不覆盖。
* **诊断经验（DEV-0057.2）**：`E0463 can't find crate` 可能是 SAC 拦 build script 的**间接症状**——见 E0463 先排除 SAC 再查依赖。

### TRUSTED TIMESTAMP RULE（可信时间，永久）
* 所有 DEV 的 Start/End Time、Phase Timestamp、Last Updated **只能来自系统命令**，禁止模型根据聊天时间/文件时间/估计自行生成。
* Windows 固定命令：`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`（必须含年月日+时分秒+UTC Offset）。
* 其他系统使用等价系统命令。
* TRAE_RUN Header 必须含 `Timestamp Source: SYSTEM`；ENVIRONMENT Last Updated 同源。
* 历史文件中出现晚于本机当前系统时间的手写时间 → 标 `TIMESTAMP_UNTRUSTED`，**不得**偷改成自己猜的时间。

### 文件修改时间规则（永久）
* Windows Explorer 的 Modified Time 可作为**辅助证据**。
* 不能单独证明文件内容是正确 Current State；Authority 永远 = **内容 + Source + Runtime**。

# NON-NEGOTIABLE PRODUCT RULES
1. Study First
2. Goal Optional
3. Knowledge Optional
4. AI Optional
5. Session Single Artifact
6. Goal × Knowledge Dual Tree
7. AI Direct Write = 0
8. Formal AI changes → ChangeSet → User Approval
9. Local First
10. No Decorative Data
11. Evidence must be trustworthy
12. 高频功能直接可见
13. 数据库复杂度不能暴露给用户
14. 无数据不堆 0
15. 用户永远可以 Quick Study

# PART 5 · Permanent AI Architecture Invariants（DEV-0060.1 起永久）

AI-INV-001 Current User Intent First：只有最后一个用户消息是本轮请求；Context 永远是 system 背景。
AI-INV-002 LLM 理解与 Domain Execution 分层：模型输出 Typed Intent（SemanticAction/TemporalIntent），绝不输出数据库操作或实体 id。
AI-INV-003 Direct Write = 0：一切正式写入经 ChangeSet → 用户批准 → Apply；未批准 0 落库。
AI-INV-004 Skill 来自 versioned Registry：SKILL.md 编译期嵌入（include_str!），运行时禁止源码扫描。
AI-INV-005 Skill Contract 可验证：required_capabilities ⊆ Capability Registry；optional_tools ⊆ Tool Registry（SKILL_CONTRACT_STALE 检测）。
AI-INV-006 Turn Router 保守默认：路由不确定时偏向 HigherRead/SemanticAction，动作请求绝不判成 FastChat。
AI-INV-007 Knowledge Optional：创建任务/重复任务不依赖知识，也不自动创建知识。
AI-INV-008 Entity Resolver 不自动选择：0 匹配 → NotFound；2+ → 澄清。
AI-INV-009 Runtime Time Truth 由 Higher 提供：local_date/datetime/timezone/weekday 经前端传入、后端校验；模型永不猜"今天"；intent 与编译日期不符 → Reject。
AI-INV-010 Minimal Change Scope：操作实体 ⊆ 用户请求范围；禁止自动扩大到未要求实体。
AI-INV-011 FastChat 零负担：tools=0、Memory Extract=0、私有 Context=0、history 有预算。
AI-INV-012 Recurring 语义复用既有系统：不重建重复任务体系；规则语义（estimated_minutes/task_kind/priority）由 materialization 继承。
AI-INV-013 语义动作成功后总结确定性生成（Compiler 产出），禁止第二次模型调用写总结。
AI-INV-014 Invalid Semantic JSON 最多 Repair Once；修复仍失败 → 0 落库。
AI-INV-015 Active Planner 收口：显式取消走本地 Cancel（不调 Provider）；续跑 vs 新意图由 Semantic Router 判定；被接管的旧规划 paused。
AI-INV-016 Performance Trace 复用 ai_run_events：主/次 Provider 调用分开计数；禁记 API Key / 完整 Prompt / 用户隐私全文。

**回归纪律**：任何修改 `src-tauri/src/ai/**`、AI ChangeSet Compiler、GoalTarget AI truth、Skill Registry、Tool Registry 的提交，必须至少运行并通过：
```
cargo test --test batch060 -j 1
cargo test --test batch0601 -j 1
```

# PART 6 · Permanent Grounding Invariants（DEV-0060.2 起永久）

AI-GND-001 自然语言 Reference 与数据库 Entity ID 必须分离。
AI-GND-002 模型只能输出 Reference Hint，不得创造正式 Entity ID。
AI-GND-003 Entity ID 只能来自 Higher Candidate Retrieval / Current UI Context / Recent Entity Resolution。
AI-GND-004 Grounding 必须限定当前 Profile。
AI-GND-005 时间、状态、实体类型等结构过滤必须先于语义选择。
AI-GND-006 候选唯一时不得为了"显得智能"再次调用模型。
AI-GND-007 多个合理候选时允许一次轻量 Candidate Selection。
AI-GND-008 模型 Candidate Selection 只能从 Higher 提供的 candidate_id 中选择（幻想 ID → Invalid → 安全澄清）。
AI-GND-009 无法唯一确定时必须 Clarification，不得猜。
AI-GND-010 NotFound / Ambiguous / NothingToChange 不得创建空 ChangeSet。
AI-GND-011 用户不得看到"ChangeSet 至少包含一个操作"等内部错误。
AI-GND-012 Occurrence（单次出现）与 Recurring Series（重复系列）必须显式区分。
AI-GND-013 过去学习事实不得因为 Series 修改而重写（过去/Completed/有 Session 事实的永不自动改）。
AI-GND-014 一个用户请求可以生成多个 ProposedOp，但必须属于一个审查用 ChangeSet。
AI-GND-015 Multi-step Action 仍然 Direct Write = 0。
AI-GND-016 普通 Action Provider Call 必须 bounded（semantic 1 + selection ≤1 = ≤2）；不得演化成无限 Agent Loop。
AI-GND-017 Grounding Runtime 禁止读取源码。
AI-GND-018 所有 Grounding / ActionPlan 核心修改必须运行 batch060 + batch0601 + batch0602。

# PART 7 · Permanent Provider & Continuation Invariants（DEV-0062 起永久）

AI-INV-017 Provider Isolation：Provider-specific 行为只存在 Adapter 层（ai/provider.rs）；action/planner/grounding/runtime/tools/lib 禁止出现 if provider == ... 分支。
AI-INV-018 Capability Honesty：模型能聊天 ≠ 完整兼容 Higher；缺能力必须显式报告（Full / Limited / Incompatible），不得静默降级或伪装成功。
AI-INV-019 Control State ≠ Conversation History：执行中的业务状态（等待候选选择 / Planner 补字段 / Action 参数）必须结构化持久化，禁止依赖聊天文字恢复。
AI-INV-020 Action Continuation Scoped：Pending Action 必须 profile + conversation 隔离；重启可恢复（SQLite）；跨 conversation 不传播。
AI-INV-021 Provider Provenance：每个 Run 记录真实 Primary / Control Provider + Model snapshot；历史不得使用当前配置冒充。
AI-INV-022 No Hidden Provider Fallback：模型切换只能来自用户显式 Primary / Control 配置；Higher 不得偷偷调用另一家 Provider。

# 开发节奏
* **BATCH WHEN CLEAR**：产品语义已定 + 技术可从源码验证 + 风险可自动测试 → 一次性施工。
* **STOP WHEN HUMAN SIGNAL REQUIRED**：仅真实 AI 行为/真实 UI 体验/真实迁移结果/真实异常数据/真实用户路径无法自动确认时才停，标 `HUMAN_RUNTIME_REQUIRED`。
* 事实优先级：Runtime > 源码 > DB Schema > 测试 > 证据文档 > ENV 摘要 > PRODUCT/宪法 > 历史。
