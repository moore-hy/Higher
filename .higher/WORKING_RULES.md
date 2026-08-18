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

# 开发节奏
* **BATCH WHEN CLEAR**：产品语义已定 + 技术可从源码验证 + 风险可自动测试 → 一次性施工。
* **STOP WHEN HUMAN SIGNAL REQUIRED**：仅真实 AI 行为/真实 UI 体验/真实迁移结果/真实异常数据/真实用户路径无法自动确认时才停，标 `HUMAN_RUNTIME_REQUIRED`。
* 事实优先级：Runtime > 源码 > DB Schema > 测试 > 证据文档 > ENV 摘要 > PRODUCT/宪法 > 历史。
