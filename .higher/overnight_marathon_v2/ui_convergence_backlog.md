# UI Convergence Backlog — DESKTOP SHELL

> **状态：只登记，不施工。** 本文件是 P6R/R6 的账本产物，唯一目的是**如实记录**
> 「新认知层语言」与「旧产品表面」当前在同一桌面 Shell 内共存的位置与形状。
>
> **本文件不授权任何重设计。** 任务书明确：*"record the current old/new visual overlap
> without executing a redesign"*。下列每一项都**不是**待办事项，而是**收敛前必须由 owner
> 决策**的事实清单。任何删除/替换/迁移视觉的动作都不得在本 marathon 内发生。
>
> 生成时间：2026-09-19（HIGHER OVERNIGHT MARATHON V2 · P6R/R6）
> 基线 HEAD：`f8e8495`　分支：`main`
> 证据方式：源码行号 + CSS 实际取值（只读 grep/read，未修改任何文件）

---

## 0. 判定轴（术语定义，用于后续统一收敛口径）

| 术语 | 含义 | 判定证据 |
|---|---|---|
| **认知层语言（HC）** | COGNITIVE CORE V1.2 引入的新视觉体系 | class 前缀 `hc-*`；token 前缀 `--hc-*` |
| **旧产品语言（LEGACY）** | COGNITIVE CORE 之前既有的产品表面 | class 无 `hc-` 前缀（`.card` / `.btn` / `.today-*`） |
| **双重表面（DUAL）** | 同一屏内同时渲染两套语言 | 同文件内既出现 `hc-*` 又出现旧 class |
| **几何分歧（GEO）** | 两套语言的同语义控件取值不同 | 见 §4 实测值 |

实测规模：

- `src/styles.css` 中 `--hc-*` token 出现 **179** 次（L1 全量统计）。
- 桌面页面按 `hc-` 使用密度排序：`CognitiveProgress.tsx`(65) > `TrainingExperience.tsx`(38)
  > `Memory.tsx`(29) > `Today.tsx`(17)，其余 16 个页面为 **0**。

---

## 1. Today.tsx —— 同一屏内 3 层表面叠加（最严重）

`src/pages/Today.tsx` 一个文件内同时存在三种代际的表面：

| # | 区域 | 行号锚点 | 语言 | 说明 |
|---|---|---|---|---|
| 1a | `today-head`（页头信息） | L575–L578 | LEGACY | `today-head__info` / `today-head__sub` |
| 1b | **进行中学习横幅** | **L614** `<section className="card today-banner today-hero lw-bar" aria-label="正在学习">` | LEGACY + `.card` + `.lw-bar` | 见 §3，与 1c 直接竞争首屏 |
| 1c | 保存回执条 | L649–L650 | LEGACY | `card today-saved` / `today-saved__text` |
| 1d | **认知层 Today 主体** | **L667** `<div className="hc-today">` | HC | `hc-today__top` / `hc-today__signals` / `hc-today__hint`(role=status) |
| 1e | 认知层起点选择 | L696–L724 | HC | `hc-choice`(role=group) / `hc-choice__row` / `hc-btn`（3 个） |
| 1f | 旧详情折叠区 | L754–L759 | HYBRID | `<details className="hc-legacy">`，summary「今天的细节」/「任务 · 活动 · 伙伴 · 其他入口」；**HC 前缀但装的是全部旧内容** |
| 1g | 第二块 hero | **L766** `<div className="today-hero">` | LEGACY | 与 1b 同类名（`today-hero`）但**不是** section，是第二处 hero 语义 |
| 1h | 主行动区 | L777 `id="primary-next-action"` | LEGACY | 被 `today__secondary` 文案引用 |
| 1i | 次级导航 | L795 `<nav className="today__secondary" aria-label="其他入口">` | LEGACY | 与 §2 侧栏一级导航语义重叠（同类「入口」两处） |
| 1j | 今日任务区 | L826–L828 | LEGACY | `card today__section` / `card__title` |
| 1k | 今日活动区 | L865–L866 | LEGACY | 同上 |
| 1l | 阶段复盘横幅 | L889–L894 | LEGACY | `card today-banner today-banner--quiet today-banner--review` |

**重叠事实**：首屏自上而下顺序为 `LEGACY 横幅(1b) → LEGACY 保存条(1c) → HC 主体(1d/1e)
→ HC 折叠(1f) → LEGACY hero(1g) → LEGACY 导航(1i) → LEGACY 两区(1j/1k) → LEGACY 横幅(1l)`。
即 **HC 主体被夹在两段 LEGACY 之间**，用户滚动一次即跨越 3 个视觉代际。

**收敛时必须保留的能力（不可随视觉一起消失）**：见 §6。

---

## 2. Layout.tsx —— 一级导航已收敛，次级分组仍并列

`src/Layout.tsx`（484 行）是三段式导航：

| # | 区域 | 行号锚点 | 语言 | 内容 |
|---|---|---|---|---|
| 2a | 品牌区 | L115–L118 | LEGACY | `layout__brand` / `layout__brand-mark` / `layout__brand-name` |
| 2b | 学习档案区 | L121–L168 | LEGACY | `shell-profile` / `shell-profile__btn|name|sub|arrow|menu-item` |
| 2c | **认知一级导航** | **L171–L190** | HC 容器 + LEGACY item | `<nav className="layout__nav hc-nav" aria-label="主导航">`；item class 为 **`layout__nav-item hc-nav__item`（两者叠加）** |
| 2d | **进阶次级分组** | **L193–L213** | HC | `<nav className="hc-nav hc-nav--advanced" aria-label="进阶功能">`；caption「进阶」；item `layout__nav-item hc-nav__item hc-nav__item--advanced` |
| 2e | 设置 footer | L215–L231 | HC 容器 + LEGACY item | `layout__sidebar-footer` / item `layout__nav-item layout__nav-item--settings hc-nav__item` |
| 2f | 主舞台 | L234–L240 | HC | `layout__body hc-mainstage` / `layout__main` / `<HigherCommandBar/>` |

一级导航定义（L45–L50，§21 锁定 IA，**不得改动**）：
`Today /` · `Journey /journey` · `Memory /memory` · `Progress /progress`，
图标全部 lucide-react（§21 禁止 emoji）。

次级分组定义（L53–L57，preserved power-user routes，**能力原样保留，只弱化呈现**）：
`知识 /knowledge` · `数据 /data` · `同步 /sync`。

**重叠事实**：`nav` 容器已是 HC，但**每个 `NavLink` 的 class 字符串同时挂新旧两套**
（如 L180–L181：`"layout__nav-item hc-nav__item" + (isActive ? " layout__nav-item--active hc-nav__item--active" : "")`）。
即**同一个 DOM 节点被两套 CSS 同时命中**——收敛时的关键风险点：任一侧删除都会改变现有观感，
且 `layout__nav-item--active` 与 `hc-nav__item--active` 的优先级关系是隐式的。

---

## 3. 进行中学习横幅 vs 认知层 Today 英雄区（首屏竞争）

任务书点名的具体冲突，实测确认存在：

- **横幅侧（LEGACY）**：L614 `card today-banner today-hero lw-bar`，`aria-label="正在学习"`。
  含 `today-banner__main` / `today-banner__label` / `today-banner__name`（`activeItemName`）/
  `today-banner__meta`（`已进行 {activeElapsed}`）/ `today-banner__stale` / `today-banner__actions`。
- **英雄区侧（HC）**：L667 `hc-today`，含 `hc-today__top` / `hc-today__hint`(role=status)，
  以及 L766 第二处 `today-hero`。

**冲突形状**：两者都在回答「我现在该做什么？」——这是 Today 页唯一的产品问题（§21）。
横幅的方案是**延续/返回**（`resumeHref` → 有 active run 则 `/train/:id`，否则 `/learn/:id`），
英雄区的方案是**从认知信号重新推荐起点**（`hc-choice` 三按钮，L696–L724）。
两者在同一屏同时可见时，CTA 语义**互相稀释**且方向可能不一致。

**⚠️ owner 级决策未决**：横幅与英雄区谁在收敛后承担「现在该做什么」的唯一回答权，
**属于产品决策，不是工程决策**。本 marathon 不做选择、不做实验、不改任何默认值。

---

## 4. 几何分歧实测值（LEGACY 8px vs HC 12px 语言）

同语义控件的真实 CSS 取值对比（`src/styles.css`，只读）：

| 语义 | LEGACY 定义 | HC 定义 | 分歧 |
|---|---|---|---|
| 按钮圆角 | `.btn` L2481 `border-radius: var(--radius-ctl)` → L91 → L48 `--h-radius-sm: 8px` | `.hc-btn` L1455 `border-radius: 12px`（硬编码） | **8px vs 12px** |
| 按钮高度 | `.btn` L2479 `height: var(--btn-h)` | `.hc-btn` L1453 `min-height: 38px` | 固定高 vs 最小高 |
| 按钮字号 | `.btn` L2482 `font-size: 13px` | `.hc-btn` L1459 `font-size: 14px` + `font-weight: 550` | 13px vs 14px/550 |
| 按钮底色 | `.btn` L2476 `background: var(--bg)`（实底） | `.hc-btn` L1457 `rgba(151,214,255,0.06)`（玻璃） | 实底 vs 半透明玻璃 |
| 卡片圆角 | `.card` 族多处以字面量 `border-radius: 8px`（L704/718/2561/2632/2906/3496/3725/3878/4036/4068/4163/4202/4353/4419/4450/4474/4506… 至少 18 处） | `--h-radius-md: 12px` / `lg: 16px` / `xl: 20px`（L49–L51） | **字面量 8px vs token 12/16/20px** |
| token 体系 | `--radius-card: var(--h-radius-md)` L90 / `--radius-ctl: var(--h-radius-sm)` L91 | `--hc-*` 独立命名空间（179 处） | 两套 token 命名空间并存 |

**结论**：LEGACY 侧并存**两种**写法——部分已桥接到 `--h-radius-*` token（L90/L91），
部分仍是散落的 `8px` 字面量（≥18 处）。这意味着后续收敛**不能只改 token**：
散落字面量必须逐处处理，而逐处处理会触及非本次任务拥有的基线文件。
——这正是本 marathon **不施工**的技术理由之一，已登记为基线债务（见 §5）。

---

## 5. 基线债务（只登记，不修）

| ID | 事实 | 为何不在本 marathon 处理 |
|---|---|---|
| UI-DEBT-1 | `styles.css` ≥18 处卡片圆角为 `8px` 字面量，未走 token | 非本任务拥有；批量替换属重设计 |
| UI-DEBT-2 | 训练体验组件仍在用 LEGACY 控件类：`btn` / `btn--ghost` / `btn--primary` / `btn--small` / `btn-row` / `field-label` / `form-label` / `form-row` / `form-stack` / `input`，与 `hc-train__*`（`hc-train__cue|choices|interactions|material|material-text|provenance|steps|step-hidden|tag|unavailable|unavailable-title|effect*|h3|note`）**同文件混用** | 8 个专项体验是 P1 已交付且被 P4/P5 测试锁定的生产面；改控件类会同时改动视觉与测试断言 |
| UI-DEBT-3 | 16/20 页面 `hc-` 使用数为 **0**（`Tasks/Sync/Settings/Review/Progress/ProfileWelcome/ProfileSelector/ProfileCreate/Planning/NotFound/LearningWorkspace/Knowledge/History/Goals/Evaluations/Data`） | 这些是 preserved production routes（§37 未删除任何生产能力）；将其纳入新语言 = 重设计 |
| UI-DEBT-4 | `Today.tsx` 同时存在 `today-hero`（L614 用作 section 附加类）与 `today-hero`（L766 独立 div） | 同名不同用，收敛前需先统一语义，属设计层 |
| UI-DEBT-5 | `Layout.tsx` NavLink class 同时挂新旧两套（L180–181/203–204/222–223） | 见 §2；删除任一侧都会改变观感，需 owner 决策 |

---

## 6. 收敛时必须存活的动作清单（能力保全基线）

若未来对桌面 Today 表面做收敛，**下列动作不得随视觉一起消失**（§37：未删除任何生产能力）。
每一项都已在源码中找到锚点；本清单是**保全清单**，不是施工清单。

| 动作 | 锚点 | 关联真实链路 |
|---|---|---|
| 继续 / 返回进行中的学习 | Today.tsx L614 横幅区；`resumeHref`（active run → `/train/:id`，否则 `/learn/:id`） | `start_training_for_item → TrainingRun`（见 `production_path_map.md`） |
| 从认知信号选择起点（3 个） | L696–L724 `hc-choice` / `hc-btn` | 认知层推荐 → 训练入口 |
| 保存回执可见（`✓ {barSaved.text}`） | L649–L650 `today-saved` | `record_interaction` 成功后的用户回执 |
| 今日任务列表可达 | L826–L828 `today__section` / `card__title`「今日任务」 | `DailyTasksSection` |
| 今日活动列表可达 | L865–L866「今日活动」 | `DailyActivitiesSection` |
| 伙伴 / 其他入口可达 | L754–L757 折叠 summary「任务 · 活动 · 伙伴 · 其他入口」 | 折叠区内容**始终留在 DOM**（L146 注释：桌面专属 `<details>`，默认收起） |
| 阶段复盘入口可达 | L889–L894 `today-banner--review` | 复盘链路 |
| 次级入口导航 | L795 `today__secondary`（`aria-label="其他入口"`） | 与侧栏 `hc-nav--advanced` 语义部分重叠，收敛时需决定保留哪一处**或**两处 |
| 一级导航四入口 + 设置 | Layout.tsx L171–L190 / L215–L231 | §21 锁定 IA，**本身就是约束而非可选项** |
| 进阶三入口（知识/数据/同步） | Layout.tsx L193–L213 | preserved power-user routes，只能弱化不能删除 |
| 桌面唯一自由文本入口 | Layout.tsx L239 `<HigherCommandBar/>` | §22；底部命令栏不能被视觉层覆盖 |
| AI 面板（420px 覆盖抽屉） | Layout.tsx L16–L18 / L62 | W9 §22；关闭零宽度 |

---

## 7. 本文件明确**不做**的事

- ❌ 不删除、不合并、不重命名任何现有 class。
- ❌ 不把任何 LEGACY 表面迁移到 `hc-*`。
- ❌ 不调整 `today-banner` 与 `hc-today` 的先后顺序或显示条件。
- ❌ 不改 `NAV_ITEMS` / `ADVANCED_NAV_ITEMS`（§21 锁定）。
- ❌ 不为「谁回答『我现在该做什么』」做任何取舍（owner 决策）。

以上五项中任何一项一旦执行，都会同时触及视觉契约与既有 UI 测试断言
（`tests/product-ui/` 204 例），属重设计范畴，超出本任务书授权。
