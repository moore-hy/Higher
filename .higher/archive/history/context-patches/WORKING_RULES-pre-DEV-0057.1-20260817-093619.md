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
