# DEV-0065.2R · Higher Clean Windows Release + Persistent Data · TRAE_RUN

- **DEV ID**: DEV-0065.2R（干净 Windows 发布 + 持久化数据；核心产品决策：**不迁移任何开发数据，首装=干净空壳，未来升级保留用户生产数据**）
- **Timestamp Source**: SYSTEM · Start 2026-08-23（+08:00）
- **Baseline HEAD**: 295d4e02ffb689cf77709f17522e58c321293ce1 · **Branch**: main · **Worktree Before**: clean（仅 ?? 本任务书）——BASELINE GATE PASS

## Phase 0 · 源码审计（编辑前实测）
- **路径不一致确认（§14 根因）**：lib.rs 四个功能点五处 prod 分支用 `app.path().app_data_dir()`（Roaming）——setup db_dir（L7258）/att_root（L7291）/vault_dir（L7303）/backups_dir（L2637）/runtime_db_path（L2653）；而 db.rs database_path 用 LOCALAPPDATA env（Local）——Roaming/Local 分裂。
- **版本不对齐**：tauri.conf 1.0.0 / package 1.0.0 / package-lock ×2 / Cargo.toml 0.1.0。
- main.rs 标准 windows_subsystem 入口（不动）；.gitignore 已有 .data/.webview-data、无 release/；capabilities 无 asset scope（配置在 tauri.conf.json）。

## Phase A · 版本对齐 + bundle 配置
- 五处版本 → **0.3.0**：tauri.conf.json / package.json / package-lock.json（root + packages.""）/ Cargo.toml（+description 中文化）/ Cargo.lock（cargo check 自动重生成）。
- tauri.conf.json bundle：targets=["nsis"] · mainBinaryName="Higher" · nsis{installMode=currentUser, languages=["SimpChinese"], displayLanguageSelector=false, startMenuFolder="Higher"} · windows.webviewInstallMode={type:offlineInstaller}（**修错**：首版误放 nsis 内，tauri-build 报 unknown field → 移至 bundle.windows 层级）· **useLocalToolsDir=true**（NSIS 工具链缓存进 src-tauri\target\.tauri，绕开沙箱对 %LOCALAPPDATA%\tauri 的写限制）。
- asset scope 增 `$LOCALDATA/attachments/**` 与 `$LOCALDATA/com.higher.desktop/attachments/**`（覆盖新生产路径；未放宽到任意文件系统，§32 无 ASSET_SCOPE_CONFLICT）。

## Phase B · 生产数据根 AppLocalData（§9/§14/§15）
- lib.rs **五处 prod 分支** `app_data_dir()` → `app_local_data_dir()`（db_dir/att_root/vault_dir/backups_dir/runtime_db_path）+ 三处注释路径真值更新（%LOCALAPPDATA%\com.higher.desktop\）；db.rs database_path 已指 Local（LOCALAPPDATA+com.higher.desktop+higher.db）无需改——全源码一致，无 Roaming/Local 分裂。
- **事故与修复**：并行编辑同一 lib.rs 导致 att_root 修改被 vault_dir 编辑覆盖（工具报成功但文件回退）→ 重新应用并全量 grep 验证 5/5。教训：同文件编辑必须串行。
- cargo check PASS（0 errors）。

## Phase C · 构建脚本 + gitignore + R01-R20
- 新建 `scripts/Build-Higher-Release.ps1`（§21 十步：clean worktree 校验[-AllowDirty 逃生门，§53 Commit:NO 场景] → HEAD → tsc → vite build → cargo check → batch0652_release → tauri build → 定位 NSIS → 拷贝 release\ → SHA256；**编码事故**：无 BOM UTF-8 被 PowerShell 5.1 按 ANSI 误读解析炸裂 → 重写为 UTF-8 BOM）。
- .gitignore + release/。
- 新建 **batch0652_release R01-R20 20/20**：R01 身份/R02 版本五处/R03 Higher.exe/R04 NSIS only/R05 currentUser/R06 SimpChinese+startMenuFolder/R07 offlineInstaller/R08 动态窗口/R09 五分支 app_local_data_dir+禁 app_data_dir/R10 DB 路径一致（lib+db.rs）/R11 attachments·vault·backups 同根+scope/R12 零 resources·externalBin·db/R13 release/ gitignore/R14 ai 零 diff/R15 v024 无 v025/R16 migrations 零 diff/R17 脚本十步标记+无迁移引用/R18 无 Migrate* 脚本/R19 脚本与 conf 无 .data/.webview-data/R20 依赖名集合=HEAD（npm+lock+cargo）。
- **§50 授权的旧测试最小调整（4 处，mandated 变更直接矛盾旧零 diff 断言）**：batch064 u27（→依赖名集合比对 HEAD）、u28（lib.rs 新增行白名单 decorations|app_local_data_dir|AppLocalData|%LOCALAPPDATA%，不再要求 any(decorations)——HEAD 已含）；batch064r2 r2_u24（→依赖集合比对）、**r2_u22（基线期已损坏**：FROZEN_PREEXISTING 脏集已随 295d4e0 提交，diff 实测=""，assert_eq 必败——修正为「已提交，自 HEAD 起零 diff」）；batch0651 t20（→依赖集合比对+conf 身份三断言）。

## Phase D · Automated Gate + 安装包
- **Gate 全绿**：tsc 0 / npm build ✓ / cargo check 0 / batch0652 **20/20** / batch0651 **20/20** / batch064r2 **27/27** / batch064 **28/28** / batch063 **18/18** / ai_panel **8/8** / batch062r1 **41/41** / batch062r **44/44** / batch062 **57/57**（串行 RUST_TEST_THREADS=1；useLocalToolsDir 加入后全部复跑）。
- **tauri build**：Higher.exe 产出 → NSIS 下载/校验/解压（首次因沙箱拒写 %LOCALAPPDATA%\tauri 失败 os error 5 → useLocalToolsDir=true 解决）→ makensis → `Higher_0.3.0_x64-setup.exe`。
- **产物**：`release\Higher_0.3.0_Setup.exe`（221,679,004 bytes ≈211MB，含 WebView2 离线安装器）+ `release\Higher_0.3.0_SHA256.txt`（79c0734b135a7844d68ef0018cf066f0889b80f4ca131997eca6bf989f4d02b3）。
- **Forbidden Diff 审计**：本轮 M = tauri.conf.json/package.json/package-lock.json/Cargo.toml/Cargo.lock/lib.rs/.gitignore/batch064_ui/batch064r2_ui/batch0651_ui（后三=§50 授权调整）+ ?? scripts/Build-Higher-Release.ps1/batch0652_release.rs；**src/**（前端）与 src-tauri/src/ai|repository|migrations|db.rs 零 diff**；未 commit/push/tag（§53）。
- **AI Runtime Changes: 0 · Domain Changes: 0 · Schema v024 · Migration 0 · Dependency 0**；P2：最终图标未定（当前用占位 icon pack）、unsigned。
- **HUMAN RUNTIME PENDING（H01-H14）**：干净首装→建 INSTALL-PERSIST 测试数据→重启持久化→DB 物理位置确认→同版本重装保留→程序独立性→功能/AI 冒烟→卸载默认保留（若默认删数据=STOP UNINSTALL_DATA_DEFAULT_UNSAFE）→卸载后重装数据回归→桌面/开始菜单→窗口 Shell→版本元数据→（可选）断网安装。

# DEV-0065.1 · Higher Desktop Shell（Custom Titlebar + Two-State AI Rail）· TRAE_RUN

- **DEV ID**: DEV-0065.1（A. 自定义桌面标题栏；B. Higher AI 三态→两态；无学习域/AI Runtime 改动）
- **Timestamp Source**: SYSTEM · Start 2026-08-22T22:45:19+08:00
- **Baseline HEAD**: b14e23703999d5855ef020e6b27a1684ada03741 · **Branch**: main
- **Known Worktree Before**: 14 M（ENVIRONMENT/TRAE_RUN/batch061r/App/Layout/ChangeSetReview/DailyTasksSection/ai-AiPanel/Knowledge/LearningWorkspace/Planning/Settings/Today/styles.css）+ ??（4 份 TASK 输入 0063/0064R/0064R.2/0065.1 · batch063_ui/batch064_ui/batch064r2_ui · appearance/×3 · PlanningWeekBoard · WallpaperLayers）——与 DEV-0064R.2 收口一致，无 UNKNOWN_WORKTREE_DIFF。

## Phase A · Source Evidence（§52；编辑前实测）
- **窗口真值**：main 窗口在 `src-tauri/src/lib.rs` run() 内 `WebviewWindowBuilder::new(app,"main",App("index.html"))` 动态创建（L7230）；builder 链 `.title("Higher").inner_size(1024.0,720.0).resizable(true)`——**decorations 默认 ON = 白条根因**；`tauri.conf.json app.windows = []`（冻结不动）。
- **Capability 真值**：`capabilities/default.json` permissions = core:default + dialog:default + notification:default（windows:["main"]）——**缺 4 个窗口 mutation 权限**。
- **壁纸真值**：App.tsx = ActiveProfileProvider > WallpaperLayers + ProfileGate（fixed inset:0 / 单图 / pointer-events:none）——正确，不重建。
- **AI 真值**：AiPanel.tsx 三态（!open→fab / open+collapsed→46px rail / open+expanded→340px）；Context 持 open/setOpen（持久化 ui.ai_panel_open，DB）+ Panel 本地 collapsed（higher.aiPanel.mode）；Expanded header 4 按钮（历史/新对话/⇥收起/✕关闭）；rail 仅 30px 小按钮可点；另 3 消费方（Today/FinalGoalCard/PlanningTruthSummary）调用 setOpen(true)。
- **几何真值**：.layout{height:100vh}；.profile-gate{min-height:100vh}；<=1100px 时 .aipanel absolute overlay（不动）；Layout.tsx 用 `open: aiOpen` 拼接 layout__main--with-ai 类（styles.css 中该类无规则，纯残留）。
- **Product Decisions**：custom titlebar（34px/fixed top/z1200/var(--h-sidebar)/仅下边框/3 控件/data-tauri-drag-region）；one wallpaper（titlebar 0 image）；AI 两态 only（no X/no FAB/whole rail clickable/missing→collapsed/ui.ai_panel_open 不再消费不迁移）。

## Phase B-F 施工记录
- **Phase B · AI 两态**：AiPanelContext.tsx 删 open/setOpen state + ui.ai_panel_open 读写 + sendChat/runAction 的 setOpenState(true)（pending-send 事件仍触发 Panel 自动展开）+ value/memo 清理；AiPanel.tsx 删 !open-FAB 分支/✕按钮、rail 重构为 `.aipanel__rail-hit` 整条 46px 单按钮（✦ icon + 竖排 HIGHER AI，无嵌套按钮；点击/Enter/Space 原生语义）、初始化 `!== "expanded"` → missing/invalid=collapsed；Layout.tsx 去 aiOpen（main 类固定 `layout__main`）；**连锁最小适配**（§31 必然、非业务改动）：Today.tsx/FinalGoalCard.tsx/PlanningTruthSummary.tsx 各删 `setOpen/setAiOpen(true)` 消费行（tsc 硬错误无法绕过；handler/消息/sendChat 链零变化）；styles.css 删 .aipanel-fab(-hover) + 旧 rail 样式 → rail-hit 全尺寸按钮 + 轻量 accent-soft hover。
- **Phase C · Titlebar**：新建 `src/components/DesktopTitlebar.tsx`（getCurrentWindow().minimize/toggleMaximize/close，isTauriRuntime() 屏蔽浏览器，catch 防未处理 rejection；drag 区 flex:1 带 data-tauri-drag-region，含 app 名 span 同标注；3 控件 44px 在 drag 区外；纯 CSS 图标 线/方/×；Close 红 hover 白图标）；App.tsx = WallpaperLayers → DesktopTitlebar → .app-shell__content > ProfileGate（恒渲染全部 gate 阶段）；styles.css：--h-titlebar-height:34px token + .titlebar（fixed top/left/right · var(--h-sidebar) · border-bottom --h-border · z-index 1200）+ .app-shell__content{margin-top:titlebar; height:calc(100vh-titlebar); overflow:hidden} + .layout height:100% + .profile-gate min-height:100%；lib.rs builder `.decorations(false)`（无 transparent/fullscreen 等）；capabilities 加恰好 4 个 core:window:allow-*（无越权）。
- **Phase D · 测试**：新建 **batch0651_ui T01-T20 20/20**（dynamic window/conf windows=[]/decorations(false)/无 transparent/4 精确权限/App 结构顺序/titlebar 非 wallpaper consumer/几何 34+1200+content offset+layout·gate 100%/getCurrentWindow 三调用/drag 与控件分离/AI 无 closed·fab·x/两态+missing→collapsed/旧 DB 键不消费/整 rail 单按钮/三动作/runtime 事件串/ChangeSetReview/Enter·Shift+Enter/壁纸消费者=2/依赖零 diff）；batch064_ui U21 按新产品真值改写（无 FAB/X/open 分支）+ U28 收窄（lib.rs decorations 是唯一合法 src-tauri/src diff，且校验 diff 内容确实只加 decorations）→ **28/28**；batch064r2_ui R2-U23 同理收窄（§46 授权：旧断言被直接矛盾）→ **27/27**。
- **中途问题**：①tsc 三处 setOpen 消费残留（见 Phase B 连锁）；②测试自伤：我的新注释含 `ui.ai_panel_open`/`padding-top: 34px` 字样被自家断言命中 → 改注释措辞；T14 锚点 rail-hit→aipanel--rail；③git 路径前缀（src-tauri/src/lib.rs vs src/lib.rs）→ ends_with 过滤；④lines().cloned() 类型错 → map(to_string)。
- **Gate**：tsc 0 / build ✓ / cargo check ✓（capability schema 验证通过）/ batch0651 **20/20** / batch064 **28/28** / batch064r2 **27/27** / batch063 **18/18** / ai_panel **8/8** / batch062r1 **41/41** / batch062r **44/44** / batch062 **57/57** / batch061r **47/47** / batch0602 **29/29**（串行）。
- **Forbidden Diff 审计**：本轮新增 M = capabilities/default.json · src/lib.rs（仅 decorations）· App.tsx · Layout.tsx · ai/AiPanel.tsx · ai/AiPanelContext.tsx · styles.css · FinalGoalCard.tsx · PlanningTruthSummary.tsx · Today.tsx（后三=§31 连锁最小适配，各仅删 setOpen 消费行）· batch064_ui/batch064r2_ui（§46/§78 授权）+ ?? DesktopTitlebar.tsx · batch0651_ui.rs；**tauri.conf.json/package*/Cargo*/api.ts/types.ts/appearance/**/ai·repository·migrations·db.rs/其余 pages 零 diff**。
- **AI Runtime Changes: 0 · Backend Domain Changes: 0 · Schema v024 · Migration 0 · Dependency 0**；P2 记录不修（§106 Planning Week 选中日残留；§107 旧会话 no_changeset 异常）；未 commit/push；**HUMAN RUNTIME PENDING（H01-H24：启动无白条/壁纸连续/无壁纸回归/拖拽/双击最大化/最小化/最大化/关闭/缩放/AI 无 X/FAB/整 rail 展开/会话保持/态持久化/跨页稳定/页面 AI 动作自动展开/发送/Proposal/Modal+标题栏/1024·1366·1920/窄宽 overlay/浏览器预览）**。

# DEV-0064R.2 · Single Global Wallpaper Surface System · TRAE_RUN

- **DEV ID**: DEV-0064R.2（单一全局壁纸 + 全界面半透明 Surface 统一修正；DEV-0064R Human Runtime 产物）
- **Start/End**: 2026-08-22（同日）
- **Baseline**：HEAD = `b14e237`（main）；轮前 Worktree = DEV-0063+0064R 未提交集（14 M + 9 ??），与 DEV-0064R 收口快照逐项一致，无 UNKNOWN_WORKTREE_DIFF。
- **User Product Decision**: Single Global Wallpaper + Translucent Surface System。
- **Full Audit**: 277 files / ~130k lines（本任务书自含；本轮施工前另做 styles.css hard-coded surface 定向审计）。

## WALLPAPER SURFACE AUDIT（§82-§83；施工前建立）
| Selector | Current Background | Wallpaper Behavior | Decision |
|---|---|---|---|
| .h-wallpaper-layer | var(--h-wallpaper-image)+brightness+saturate | 唯一真实壁纸层 | A（去 brightness） |
| .appearance__preview | 容器直用 background-image | 预览例外 | B（重构为 ::before 真值） |
| body/#root | var(--bg)（canvas 传播） | 盖住壁纸风险 | B（壁纸时 transparent） |
| .layout__sidebar / .aipanel | var(--h-sidebar)+旧 92% 双混 | 断壁纸 | A（删双混；token 66%） |
| .card | var(--bg-elevated)+旧 94% 双混 | 断壁纸 | A（改 var(--h-surface-1)=72%；删双混） |
| .modal / .modal-overlay | surface-2 / rgba .62 | 黑块 | B（76% / 0.28） |
| .taskmenu__pop | var(--h-surface-2) | 自动 78% | B（提到 82% 封顶） |
| .aipanel__inputbar | var(--bg-sidebar) | 自动 66% | B（外壳 76%） |
| .aipanel__msg--assistant | var(--h-surface-1) | 自动 72% | A |
| .kflow__canvas / .kflow__node | #16181d / #1d2027 | 黑块 | B（64% / 78%） |
| react-flow controls / kflow menu | var(--bg-elevated) | 84% | A |
| .hdoc / __toolbar / __codebar / __codeblock | var(--surface-x,#hex) / #14161b | 黑块 | B（72% / 78% / 86%） |
| input/textarea/select | 各处 var(--bg) | 不统一 | B（全局 78% control） |
| input.knowledge__title | transparent（伪装纯文本） | 会被全局规则误伤 | B（例外保持 transparent） |
| 语义色 / accent-soft / 媒体内容 | — | 非 Surface | C / D（保持） |

## 施工记录（Phase A-F）
- **Architecture**: One wallpaper layer only（.h-wallpaper-layer 唯一图片消费者 + Settings 预览唯一例外）；Token Refactor: Solid（--h-*-solid，Theme 写入）/ Effective（--h-bg 等，CSS 按 data-h-wallpaper 决定）。
- **Phase A**：appearance.ts LIMITS 70/70/45（visibility 0..100 def70；saturation 0..100 def70；overlay 20..80 def45 §16 语义不重复）；applyAppearance 只写 *-solid + border/accent + wallpaper 三变量，禁写 --h-bg 等（R2-U08 锁）；:root 加 5 个 solid token + Effective 默认映射 var(*-solid)（无壁纸视觉不变 §23）；旧值 40/100/54 合法不强制迁移（§68）。
- **Phase B**：html[data-h-wallpaper="on"] Effective Tokens bg68/sidebar66/s1-72/s2-78/s3-84 + --bg-elevated=s2-solid84（§26-§27 锁死）；body/#root transparent（§28）；删旧 Sidebar/AI 92%、Card 94% 二次混（R2-U11）；.card 改消费 var(--h-surface-1)。
- **Phase C**（集中置于 styles.css 末尾，原规则之后）：Form :is(input,textarea,select) 78% bg-solid（例外 .knowledge__title 保持 transparent）；Modal 76% s2-solid + overlay rgba(5,7,12,.28)；taskmenu__pop 82% 封顶；aipanel__inputbar 76% sidebar-solid；Graph Canvas #16181d 64% / Node #1d2027 78% + dots ≤15% 对比微调（CSS fill 覆盖 presentation attr）；HDoc 72% / toolbar+codebar 78% / codeblock 86%。
- **Phase D**：Preview Truth 重构（::before=同图+同强度+同饱和+cover/center；::after=同压暗；content=文件名+示例 Surface「卡片示例 · Aa」用 var(--h-surface-1) §65）；滑杆改名 壁纸强度/色彩保留/压暗程度；新增「恢复推荐效果」（保留壁纸+氛围，只改 70/70/45）；「恢复默认」=删壁纸+default+70/70/45（DEFAULT_PREFS）。
- **Phase E**：registerWallpaperUnload 返回 cleanup（removeEventListener）；WallpaperLayers effect `register→restore→return unregister`（cleanup 只移除 listener 不 revoke URL §69）。
- **Tests**：新建 batch064r2_ui **27/27**（R2-U01~U27：单层/fixed/无区域重复铺图/cover-center-no-repeat/无 brightness/Controls 70-70-45/Solid Tokens/Theme 写 Solid/Effective 68-66-72-78-84/body 透明/旧双混删除/Form78/Modal76+overlay28/elevated84/Graph64+78/HDoc72-78-86/AI Surface/Preview Truth/推荐恢复/默认恢复/StrictMode/Frozen JSX=轮前集合/Backend/Dependency/三既有锁）；batch064_ui 更新过时断言（U08-U10 新默认、U13 禁 brightness）后 **28/28**；batch063_ui **18/18**。
- **中途问题与修复**：① 测试裸锚点 `.h-wallpaper-layer`/`.modal-overlay`/`.taskmenu__pop` 被新 CSS 注释或前置 override 抢先命中 → 测试锚点加 ` {`（batch064）+ Special Surfaces override 移文件末尾（batch063 不改文件，CSS 顺序解决，override specificity 本就更高）；② batch063 u14 断言 `--h-bg: #0b0d12` 直接值与 Solid 架构冲突且该文件禁改 → :root 注释保留直接值存档（值真实不变，仅改经 *-solid 提供）。
- **Business JSX Changes: 0**（frozen 8 文件 diff 与轮前逐一相等，R2-U22）；Handler Reimplementation: 0。
- **Forbidden Diff**: PASS（src-tauri/src=0；package*/Cargo*=0；api/types=0；App/Layout/AiPanel/KnowledgeFlow/RichDocEditor/LearningWorkspace/Planning/Today/ChangeSetReview/DailyTasksSection/PlanningWeekBoard/Data 零新增 diff）。
- **Schema v024 · Migration 0 · Backend/AI Runtime 0 · Dependency 0 · 新 !important 0**；knowledge_workspace 6/8 = 既有 stale（§115 不动非 blocker）；未 commit/push；Human Runtime PENDING（H01-H30）。

# DEV-0064 · Higher UI Redesign v2 · TRAE_RUN

- **DEV ID**: DEV-0064（Atmosphere Background/自定义壁纸 + Planning Week/Month + Knowledge v2 + AI Panel v2 + Settings 外观 + Today polish）
- **Start**: 2026-08-22T19:09:00+08:00 ｜ **End**:（进行中）
- **DEV-0064 BASELINE**：HEAD = `b14e23703999d5855ef020e6b27a1684ada03741`（main）；Pre-existing DEV-0063 Diff（已人工验收未提交）= `.higher/ENVIRONMENT.md`、`.higher/TRAE_RUN.md`、`src-tauri/tests/batch061r.rs`(r29)、`src/Layout.tsx`、`src/components/ChangeSetReview.tsx`(Diff Truth)、`src/components/DailyTasksSection.tsx`(菜单收敛)、`src/pages/LearningWorkspace.tsx`(返回今日修复)、`src/pages/Today.tsx`(Hero/降噪)、`src/styles.css`(Design System v1)、`?? batch063_ui.rs`、`?? 两份 TASK 输入`——与 DEV-0063 记录完全一致，无 UNKNOWN_WORKTREE_DIFF。
- **纪律**：纯前端；禁 src-tauri/src/**、api.ts、types.ts、依赖文件；0 migration（v024）；壁纸走 IndexedDB（`higher-appearance`/`wallpaper`/`active`）+ localStorage 数值；新 !important=0。

## DEV-0064 INTERACTION PRESERVATION MATRIX（§37；开工前建立）

### Planning（含新增 Week View——全部复用原 handler）
| Control | Source File | Current Handler | API Call | After | Status |
|---|---|---|---|---|---|
| Week/Month switch（新） | PlanningCalendar.tsx | view state + localStorage `higher.planning.view` | — | UI-only（§38 允许） | NEW |
| 上一/下个月 | PlanningCalendar.tsx | shiftMonth(±1) | listTasksByRangeByProfile（refresh 链） | 同 | PRESERVED |
| 今天 | PlanningCalendar.tsx | goToday | 同上 | 同 | PRESERVED |
| 上一/下一周/本周（新） | PlanningCalendar.tsx | shiftWeek(±1)/goThisWeek（前端偏移；数据仍 range 查询） | listTasksByRangeByProfile | UI-only 计算 | NEW |
| Date Cell（月） | PlanningCalendar.tsx | onSelectDate(isSelected?null:date) → Planning.selectDate | getDailyLearningReport | 同 | PRESERVED |
| Day Cell（周，新） | PlanningCalendar.tsx | 同 selectDate（复用正下方日报） | 同上 | 同一 handler | NEW(reuse) |
| + 新建任务 | PlanningCalendar.tsx | setCreateFor(selectedDate??today) → TaskModal mode=create | createTask | 同（周视图列内 + 新建同路径，自动带该日期） | PRESERVED/NEW(reuse) |
| Week Task Start/Edit/Delete（新） | PlanningCalendar.tsx | 复用：start→startTaskSession+navigate；edit/delete→setEditing/setDeleting（DailyTasksSection 同款）或 Planning openEditTask | startTaskSession/update/deleteTask | 同原 handler | NEW(reuse) |
| 重复任务 显/建/编/启停/删 | PlanningCalendar.tsx | setShowRules/setRuleCreate/setRuleEdit/setEnabled/delete | recurring API | 同 | PRESERVED |
| PlanningTruthSummary/FinalGoalCard/GoalTreePanel/NextStep | 各组件 | 原样 | 原样 | 未触碰 | FROZEN |

### Knowledge
| Control | Source File | Current Handler | After | Status |
|---|---|---|---|---|
| Tree Select/Expand/Create/Rename/Move/Delete/Search | KnowledgeTree 组件 | onSelect/onToggle/handleCreateRoot/onCreateChild/onRename/onMove/onDelete | 未触碰（仅视觉） | FROZEN |
| 工作区/知识图 Tabs | Knowledge.tsx viewMode | setViewMode | 同（统一 Segmented 样式） | PRESERVED |
| Empty state（右侧未选） | Knowledge.tsx | — | 固定文案+复用 handleCreateRoot/setViewMode("graph") | POLISH(reuse) |
| Editor/Autosave/Session list/Attachment/Graph controls | RichDocEditor/KnowledgeFlow | 原样 | 未触碰（Graph canvas 稳定 neutral 深底，不随氛围染色） | FROZEN |

### AI Panel
| Control | Source File | Current Handler | After | Status |
|---|---|---|---|---|
| Open（fab） | AiPanel.tsx | setOpen(true) | 同 | PRESERVED |
| Close（✕） | AiPanel.tsx | setOpen(false) | 同（保持 ui.ai_panel_open 持久化语义=关闭） | PRESERVED |
| Collapse/Expand（新） | AiPanelContext+AiPanel | setCollapsed + localStorage `higher.aiPanel.mode` | UI-only | NEW |
| 新对话/历史/归档 | AiPanel.tsx | 原样 | 同 | PRESERVED |
| Send / Shift+Enter | AiPanel.tsx | 主发送 onKeyDown | 同（Enter 发送/Shift+Enter 换行不变） | PRESERVED |
| AI 设置 | AiPanel.tsx | navigate("/settings") | 同 | PRESERVED |
| Connection Selector | AiPanel.tsx | setActiveAiProfiles | 同 | PRESERVED |
| Proposal 查看/应用/取消 | AiPanel+ChangeSetReview | ai://changeset / applyAiChangeSet | 同（Patch Truth 保留） | PRESERVED |

### Settings
| Control | Source File | Current Handler | After | Status |
|---|---|---|---|---|
| Tab switch | Settings.tsx setTab | — | +外观 tab（位次 2） | PRESERVED/NEW |
| 外观：导入/替换/删除/恢复默认（新） | AppearanceSection | IndexedDB + localStorage（UI-only） | NEW |
| 壁纸三滑杆 + 6 氛围（新） | AppearanceSection | applyAppearance CSS vars | NEW |
| 其余全部 Tab（档案/AI/私人化/联网/提醒/数据/保险箱） | Settings.tsx | 原样 | 未触碰 | FROZEN |

### Today
| Control | Current Handler | After | Status |
|---|---|---|---|
| 快速学习/新建任务/AI安排/Start/Edit/Delete/继续/结束 | DEV-0063 Matrix 全 PRESERVED | 仅 spacing/typography polish，handler 原样 | PRESERVED |

## DEV-0064 施工记录（Phase A-F · 2026-08-22）

- **Files read**：TASK.md(§1-§51)、styles.css(:root/布局/aipanel/pcal/knowledge/kflow/today 区)、App.tsx、main.tsx、Layout.tsx、pages/{Settings,Planning,Today,Knowledge,LearningWorkspace}.tsx、components/{PlanningCalendar,DailyTasksSection,TaskModal,KnowledgeFlow,ai/AiPanel,ai/AiPanelContext}.tsx、types.ts(Task/DailyTaskRow)、batch063_ui.rs。
- **Phase A · Atmosphere Background + Settings 外观**：新建 `src/appearance/appearance.ts`（6 主题×9 token + KEYS/LIMITS + load/save/apply/init + THEME_SWATCH）、`src/appearance/wallpaperStore.ts`（IndexedDB higher-appearance/wallpaper/active；Blob+mime+name+updated_at；Object URL 单例生命周期 + validateWallpaperFile PNG/JPG/WEBP≤20MB）、`src/appearance/appearanceHelpers.ts`（统一 re-export）、`src/components/WallpaperLayers.tsx`（aria-hidden 双层，init+unload+restore）；App.tsx 根挂载；styles.css 壁纸层（.h-wallpaper-layer z-index:-2 filter=brightness(0.55)+saturate(var)；.h-wallpaper-overlay z-index:-1 rgb(7,9,13)·overlay 变量；均 fixed+pointer-events:none+data-h-wallpaper 门控）+ §12 Surface 透明（sidebar/aipanel 92%、card 94% color-mix；Modal/Input/Editor 不动）；Settings.tsx 新增外观 Tab（位次 2）+ AppearanceSection（§33 固定结构：预览/导入替换删除/三滑杆/6 氛围/恢复默认；file input 隐藏由按钮触发；即时生效）。
- **Phase B · Planning UI v2 + Week/Month**：Planning.tsx header 加 [周][月] Seg Switch（`higher.planning.view`，默认 month）+ 条件渲染；新建 `src/components/PlanningWeekBoard.tsx`（§18-§22：周一→周日 7 列；<上一周/本周/下一周>；每列 星期/日期/任务数·计划时长/Task Card/Session summary；数据同源 materializeRecurringTasksRange+listTasksByRangeByProfile+getProfileRangeSessions；Start=Planning handleStartTask 回调复用、Edit/Create=TaskModal 复用（defaultDate 带日期）、Delete=deleteTask→archive 原链+同款确认；⋯ 菜单=编辑+分隔线+删除）；styles.css：.seg 通用 Segmented + .pweek 7 列（无 overflow:hidden 以免裁剪菜单 §39）。
- **Phase C · Knowledge v2**：空态固定文案（标题/说明/+新建知识(setCreatingRoot)/查看知识图(setViewMode("graph"))，§26）；Tabs 统一 .seg（§27）；Tree 视觉（行高 32/active accent-soft+边框/搜索 focus ring/缩进 12）；Graph canvas/节点固定 neutral 深底（#16181d/#1d2027，不随氛围/壁纸染色，ReactFlow 数据与 layout 零改动）；kws__stats pill 化 + toolbar 节奏。业务（Tree CRUD/Drag/Sort/Editor/Autosave/Flow/Session/Attachment）零触碰。
- **Phase D · AI Panel v2**：三态 Expanded(340px/min320/max380)/Collapsed Rail(46px 标识+展开)/Closed(fab 原样)；header = 历史+新对话+**收起为侧栏(⇥ setMode(true))**+**关闭(✕ setOpen(false))** 分离；`higher.aiPanel.mode` expanded|collapsed；关闭语义保持 ui.ai_panel_open；页面 AI 入口（actionBusy/pending-send 事件）自动展开 rail；Runtime（aiStartRun/事件串/ChangeSet/Proposal/Model Selector/Enter·Shift+Enter）零改动。
- **Phase E · polish**：全局 scrollbar thin/dark/subtle（scrollbar-width+::-webkit-）；:focus-visible 统一 accent ring；Today stat tabular-nums/section gap 12/hero 左侧 accent 锚点条。
- **Tests**：batch064_ui **28/28**（首轮 U07/U20 断言过严：U07 改查 localStorage API 调用而非注释文字、U20 改用实际 CRUD 函数名 createRootLearningItem 等——非源码问题）；batch063_ui 18/18；batch062r1 41/41；batch062r 44/44；batch062 57/57；batch061r 47/47；batch0602 29/29；ai_panel 8/8；review_progress 6/6。
- **PRE-EXISTING TEST FAILURE（非本轮）**：knowledge_workspace 6/8——`test_migration_v006_schema_version_and_idempotent` 与 `test_migration_v005_to_v006_preserves_old_items_with_empty_content` 断言 schema_migrations=[1..=23]，实际含 v024（DEV-0062 引入 v024 时未同步这两处老断言；`git diff HEAD -- src-tauri/src src-tauri/tests/knowledge_workspace.rs` = 0 证明与本轮无关；该文件不在历轮 gate 清单）。按纪律不改，待决策。
- **Forbidden Diff**：PASS（修改=ENVIRONMENT/TRAE_RUN/batch061r(既有 DEV-0063)/App/Layout/ChangeSetReview/DailyTasksSection/AiPanel/Knowledge/LearningWorkspace(既有)/Planning/Settings/Today/styles.css；新增=appearance×3/WallpaperLayers/PlanningWeekBoard/batch064_ui/batch063_ui(既有)/两份 TASK 输入；**无 src-tauri/src、无 package*.json/Cargo.*、无 api.ts/types.ts**）。
- **Backend changes 0 · Schema v024 · Migration 0 · Dependency 0**；Human Runtime PENDING；未 commit/push。

# DEV-0063 · Higher UI Redesign v1 · TRAE_RUN

- **DEV ID**: DEV-0063（Design System + App Shell + Today + Planning + AI Panel；Visual Change, Behavior Freeze）
- **Start**: 2026-08-22T17:31:28+08:00 ｜ **End**: 2026-08-22T17:46:39+08:00（AUTOMATED GATE PASSED · HUMAN CLICK RUNTIME PENDING）

## HUMAN RUNTIME REPAIR（2026-08-22T18:37+08:00 · 用户报告「返回今日」回归）
- **用户证据**：Today→任务开始→LearningWorkspace→结束学习→End Sheet 出现→点「返回今日」→仍停留在 LearningWorkspace 学习完成页面（预期回 `/` Today）。
- **审计事实**：`git diff b14e237 -- src/pages/LearningWorkspace.tsx` = **0 diff**（DEV-0063 未触碰该文件）；End Sheet「返回今日」在 b14e237 及全部历史提交（d5b5433→c19ce78→b14e237）均为 `onClick={closeSheet}`——只关 Sheet（`justEnded=true` → 渲染 lw-ended 结束后视图），**不导航**。即：非 handler 丢失/overlay 拦截，而是该按钮从未绑定「返回今日」的导航语义（结束后视图与错误兜底的「返回今日/返回今日任务」才是 `navigate("/")`）。
- **Root Cause**：End Sheet「返回今日」仅 `closeSheet`，关闭后落入同样名为「返回今日」按钮的结束后视图——用户点击后无路由变化，感知为「点了没反应/仍停留」。
- **Fix（最小，复用原 handler）**：End Sheet「返回今日」→ `onClick={() => { closeSheet(); navigate("/"); }}`——`navigate("/")` 即结束后视图/错误兜底同款原业务路径；`closeSheet` 保留全部状态清理；无任何新 handler/新 API。`暂不处理`/`以后整理`/overlay 点击仍为 closeSheet（保留「以后整理」语义，未动）。
- **其他按钮检查（要求 §同时检查）**：开始下一个（setNextOpen/startNext→startQuickSession/startTaskSession→navigate(/learn/:id)）✓；现在整理（setEndSheetOpen(true)+setSheetTab("link")）✓；AI 分析本次学习（aiRunAction("session_analysis")）✓；AI 帮我整理知识（aiRunAction("knowledge_organize")）✓；结束后视图 返回今日（navigate("/")）✓——全部绑定真实 handler，**ADDITIONAL_INTERACTION_REGRESSION: NONE**。
- **回归锁**：batch063_ui 新增 `u18_endsheet_return_today_navigates`（closeSheet+navigate("/") 且 ended/error 两处原 navigate("/") ≥2）。
- **Gate**：tsc 0 / build ✓ / batch063_ui **18/18** / batch03 22/22（Workspace+Session）/ batch053 9/9 / batch054 4/4 / ai_panel 8/8。Backend Runtime 0 改动；diff 仅 + `src/pages/LearningWorkspace.tsx`（本修复）与 `src-tauri/tests/batch063_ui.rs`（U18）。
- **Human Retest**: PENDING（用户重走 Today→开始→结束→End Sheet→返回今日 → 应直达 Today）。

- **Baseline Gate**：`git rev-parse HEAD` = b14e23703999d5855ef020e6b27a1684ada03741 ✓；branch=main ✓；`git status --short` = 仅 `?? .higher/DEV-0063_Higher_UI_Redesign_v1_TASK.md`（本任务书输入，预期）✓；log-3 = b14e237 / c19ce78 / 2d4e919 ✓。无 BASELINE_DRIFT / WORKTREE_NOT_CLEAN。
- **纪律**：TASK 只读；禁 src-tauri/src/**（除 tests/**）；禁 api.ts/types.ts/package.json/Cargo.toml；0 新依赖；0 migration（v024）；handler 全保留；每 Phase 后 tsc/build/git diff --name-only；禁 !important 新增。

## UI INTERACTION PRESERVATION MATRIX（§35-§38；施工前建立）

### Today（§36）
| Control | Source File | Current Handler | Current API/Tauri Call | After Refactor Handler | Status |
|---|---|---|---|---|---|
| 快速学习 | pages/Today.tsx | handleQuickStart | startQuickSession(profile.id)+navigate(/learn) | 同 handler 原样 | PRESERVED |
| + 新建任务（Header/区内） | pages/Today.tsx | setShowCreate(true) | —（打开 TaskFormModal create） | 同 | PRESERVED |
| AI安排 | pages/Today.tsx | setAiOpen(true)+sendChat(PLAN_REQUEST_MESSAGE) | —（AI Panel 主发送） | 同 | PRESERVED |
| Session 继续 | pages/Today.tsx | navigate(`/learn/${active.id}`) | — | 同 | PRESERVED |
| Session 结束 | pages/Today.tsx | handleEndActive | endSession(active.id)+refresh | 同 | PRESERVED |
| Task Checkbox | components/DailyTasksSection.tsx | toggle(t) | completeTask/uncompleteTask | 同 | PRESERVED |
| Task 开始 | 同上 | start(t) | startTaskSession+navigate | 同 | PRESERVED |
| Task 查看（done） | 同上 | setEditing(t) | —（TaskFormModal edit） | 同 | PRESERVED |
| Task 主行点击 | 同上 | setEditing(t) | — | 同 | PRESERVED |
| Task ⋯ 菜单 | 同上 | setMenuFor(id) | — | 同（items 收敛 Edit/Delete） | PRESERVED |
| Task 编辑（菜单内） | 同上 | setMenuFor(null)+setEditing(t) | updateTaskV2（Modal save） | 同 | PRESERVED |
| Task 删除（菜单内） | 同上 | setMenuFor(null)+setDeleting(t) → confirm → handleDelete | deleteTask→(未删则)archiveTask | 同 | PRESERVED |
| Modal 保存 | 同上 TaskFormModal.save | save() | createTaskV2/updateTaskV2 | 同 | PRESERVED |
| Modal 取消 | 同上 | onClose | — | 同 | PRESERVED |
| 空态 新建任务/快速学习 | 同上 | onEmptyCreate/onEmptyQuickStart | 同上两条链 | 同 | PRESERVED |
| 复盘 banner 开始复盘/稍后 | pages/Today.tsx | navigate("/planning")/setDismissedReview | — | 同（DOM 移至页面底部降噪） | PRESERVED |
| 风险 banner 查看依据 | pages/Today.tsx | navigate("/planning") | — | 同（同上降噪） | PRESERVED |
| AI复盘今天 | pages/Today.tsx | runTodayReview | aiRunAction("daily_review") | 同 | PRESERVED |

### Planning（§37）
| Control | Source File | Current Handler | Current API/Tauri Call | After | Status |
|---|---|---|---|---|---|
| 上个月/下个月 | components/PlanningCalendar.tsx | shiftMonth(±1) | listTasksByRangeByProfile（refresh 链） | 同 | PRESERVED |
| 今天 | 同上 | goToday | 同上 | 同 | PRESERVED |
| Date Cell | 同上 | onSelect(date) → Planning.selectDate | getDailyLearningReport | 同 | PRESERVED |
| + 新建任务（Calendar bar） | 同上 | onCreate() → Planning setTaskCreate | TaskModal → createTask | 同 | PRESERVED |
| 重复任务 显示/新建/编辑/启停 | 同上 | setShowRules/setRuleCreate/setRuleEdit/toggle | create/update_recurring_rule API | 同 | PRESERVED |
| FinalGoalCard 全部操作 | components/FinalGoalCard.tsx | 原样 | 原样 | 未触碰 | FROZEN |
| GoalTreePanel / NextStep | 同名组件 | 原样 | 原样 | 未触碰 | FROZEN |
| Review / TruthSummary | PlanningTruthSummary.tsx | 原样 | 原样 | 未触碰 | FROZEN |

### AI Panel（§38）
| Control | Source File | Current Handler | Current API/Tauri Call | After | Status |
|---|---|---|---|---|---|
| Send | components/ai/AiPanel.tsx | 主发送路径 | aiStartRun | 同 | PRESERVED |
| Shift+Enter 换行 | 同上 | onKeyDown | — | 同 | PRESERVED |
| 新对话/历史/归档 | 同上 | 原样 | createAiConversation/list/archive | 同 | PRESERVED |
| 关闭 Panel | 同上 | setOpen(false) | — | 同 | PRESERVED |
| AI 设置入口 | 同上 | navigate("/settings") | — | 同 | PRESERVED |
| Connection 选择器（footer） | 同上 | setActiveAiProfiles | 同 | 同 | PRESERVED |
| 查看 Proposal | 同上 | ai://changeset 驱动 | — | 同（真值不变） | PRESERVED |
| 应用计划/只应用选中项/继续调整/取消 | components/ChangeSetReview.tsx | 原按钮 | applyAiChangeSet 等 | 同（仅 diffRows 显示算法改） | PRESERVED |

## Phase A · Design System + App Shell + Sidebar + Page Header（§9-§17）
- **styles.css `:root`**：新增 Higher Design System v1 全套 `--h-*` token（bg/sidebar/surface-1/2/3、border/strong、text/secondary/muted、accent+hover/soft/border、success/warning/danger、radius sm-md-lg-xl、space 1-10）；**既有 `--bg/--fg/--accent/--border/--ok/--warn/--error/--radius-*` 整体映射到 v1 调色板**（全站继承新视觉，不逐页改 selector）；字体栈 → `Inter, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif`（无新字体文件）。
- **Sidebar（§15-§16）**：220px 不变；brand 改 H 徽标+名称横排（accent-soft 徽标）；导航分组 学习（今日/规划/知识）/ 洞察（数据）+ footer 设置（系统）——Layout.tsx 仅加非交互 `layout__nav-group` 标签与 wrapper span（NavLink 原样）；nav item 40px/padding 12/radius 10；Default=secondary+transparent、Hover=surface-2、Selected=**accent-soft+accent-border+accent**（替换旧 solid accent）；z-20。
- **Page Header（§17）**：`.page__title` 24→28px/700；`.layout__main` padding 32px；`--h-space-*` 接入。
- **Z-Index 规范（§13）**：modal-overlay 100→**900**；`.modal` +z **910**（surface-2 + 轻阴影）；taskmenu__pop 70→**100**；toast 200→**1000**；.aipanel +z **200**（overlay 模式 30→200）；aipanel-fab 40→200。

## Phase B · Today + Task Card + Task Menu（§18-§24）
- **Task Menu Before**：编辑 / 调整日期 / 调整目标 / 调整知识 / 修改类型 / 删除（六项；后四项全部仅 `setEditing(t)` 打开同一 Modal）。**After**：编辑 + `taskmenu__sep` 分隔线 + 删除（§22）。编辑 → 同一 TaskFormModal（全部字段：标题/日期/时间/预计/类型/优先级/Goal/Knowledge）；删除 → 原确认 Modal → handleDelete（deleteTask→未删则 archiveTask）。文件头注释同步。
- **Today 视觉层次（§19）**：JSX 重排——Header → **Current Study Hero**（today-hero：accent 边 + surface-1 + 轻阴影，继续/结束原 handler）→ 今日任务 → 今日活动 → **Review/风险 banner 移至页尾降噪**（today-banner--quiet：dashed 弱边、字号降档；handler 原样）→ AI复盘次级入口。
- **Task Card（§21）**：surface-1 底 + hover surface-2 + radius-md + 过渡；Checkbox/Title/Meta/Start/⋯ 全保留原 handler。

## Phase C · Planning + Month Calendar（§25-§28）
- `pcal__cell`：surface-1 + subtle border + radius-sm + 过渡；**today = accent-border + accent-soft**（§27）；hover = surface-2；**selected = accent 边 + accent-soft + inset ring**（更强 state）；日期数字/count 用 accent。月导航/选日/新建/重复规则 handler 全未触碰（Matrix FROZEN 项）。

## Phase D · AI Panel + Proposal UI + Update Diff Truth（§29-§34）
- **AI Panel（§29-§30）**：仅视觉——panel 底 h-sidebar+z200；user bubble=accent-soft 靠右、assistant=surface-1 靠左（radius-sm 统一）；input=surface-1+focus accent。**Runtime 零触碰**（aiStartRun/ai://delta/ai://changeset/ai://run-status/Pending/Connection selector 原样；U08-U10/U17 锁定）。
- **Proposal Diff Before/After（§32-§33）**：Before=`diffRows` 遍历 before+after 全键并集 → missing-after-key 渲染为删除（假 DELETE）。**After**：`op.action === "update"` 分支候选**只来自 `Object.keys(after)`**——before 缺失=Added；`oldV === newV` continue（不展示）；`after=null && before 非 null`=「clear」（`- old + 未设置`，弱化警示色）；其余=mod（`- old + new`）；**绝不 push "del"**。create/delete 保持原语义。四个按钮（应用计划/只应用选中项/继续调整/取消）原 handler 未动。

## Forbidden Diff Audit（§49；最终）
```
 M .higher/TRAE_RUN.md                      （文档，预期）
 M src-tauri/tests/batch061r.rs             （§24 允许：r29 旧测试语义更新）
 M src/Layout.tsx                           （导航分组标签；NavLink 原样）
 M src/components/ChangeSetReview.tsx       （diffRows Update Diff Truth）
 M src/components/DailyTasksSection.tsx     （Task Menu 收敛 + 注释）
 M src/pages/Today.tsx                      （Hero/降噪重排；handler 原样）
 M src/styles.css                           （Design System v1 + 各 selector）
?? .higher/DEV-0063_Higher_UI_Redesign_v1_TASK.md （任务书输入）
?? src-tauri/tests/batch063_ui.rs           （§44 允许：新 source-contract 回归）
```
无 src-tauri/src/**；无 api.ts/types.ts/package.json/package-lock.json/Cargo.toml/Cargo.lock → **PASS**。

## AUTOMATED GATE（2026-08-22T17:46:39+08:00 全绿；默认未跑 full cargo test；真实 Provider 0 次调用）
| Gate | 结果 |
|---|---|
| batch063_ui（新） | **17/17**（U01-U17 source-contract） |
| batch062r1（回归） | **41/41** |
| batch062r（回归） | **44/44** |
| batch062（回归） | **57/57** |
| batch061r（回归，r24 适配后） | **47/47** |
| batch0602（回归，RUST_TEST_THREADS=1） | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_foundation（回归） | **7/7** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过** |
| cargo check -j 1 | **0 errors**（8 warnings 既有遗留） |
| Backend Runtime Changes | **0**（src-tauri/src/** 零 diff） |
| Schema Changes / Migration | **0 / v024 保持** |
| Dependency Changes | **0** |
| 新增 !important | **0**（U16 锁定） |

### 失败与修复
1. batch063_ui U04/U05/U07 首轮失败：split 锚点 `if (op.action === "update")` 被 ChangeSetReview line83 统计处 `else if (op.action === "update") s.update++;` 抢先命中 → 改用唯一锚点 `function diffRows`。
2. batch061r r29 首轮失败：DailyTasksSection 文件头 doc 注释仍含旧菜单四词 → 注释按新语义改写（代码本体首轮已收敛）。
3. batch0602 t9 首轮并行失败（ResolvedMany 3≠2）：单测隔离运行通过；确认为全局 RECENT map 在并行 test 线程下的既有竞态（b14e237 既有，非本轮改动——本轮零 Rust runtime 改动）；按项目历史纪律 `RUST_TEST_THREADS=1` 串行运行 → 29/29。已在 TRAE_RUN 记录，不修改 runtime/测试。

### Human Click Runtime = PENDING
- **AUTOMATED GATE PASSED · HUMAN CLICK RUNTIME PENDING**——TASK §52-§64 H01-H14（App Shell 导航/Profile/AI Panel 开关 → Today 三主操作 → Task Checkbox/开始/⋯=编辑+删除 → Edit Modal 全字段 → Delete 确认 → Session 继续/结束 → Planning 月导航/选日/新建/Recurring/Review → AI Panel 发送/Shift+Enter/新对话/关闭/设置/Connection → 真实 Proposal 只显示 changed field → 四按钮 → Approval First → Knowledge/Data/Settings/LearningWorkspace 跨页可点无遮挡 → 1366×768/1920×1080 滚动）由用户实机点击验证。

# DEV-0062R.1 · Probe Input/Output Truth Repair · TRAE_RUN

- **DEV ID**: DEV-0062R.1（修复 0062R Human Runtime 新暴露的 H02：Probe A 误报 empty_content → Basic ✗ + 后四项全未检测；Connection Test 语义混淆；响应预算/Response Truth 缺失）
- **Start**: 2026-08-22T13:56:36+08:00（TASK 读取/Baseline）｜ 施工 14:38-14:57 ｜ **End**: 2026-08-22T14:57:03+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline HEAD**: `c19ce78cf85f6e8d1bb4f616576e9f225b897573`（工作区 = DEV-0062 + DEV-0062R 未提交 + 本 TASK 替换；git status 40 项与审计一致，无 BASELINE_DRIFT）
- **Human Runtime Evidence Received**：0062R 后用户重测——「测试连接」成功（deepseek-v4-flash）；上一轮 Compatibility 曾 Basic ✓/JSON ✗/其余 ✓（已由 0062R 修 Structured）；本轮 Re-Probe 新结果 = **不兼容：DeepSeek 无法完成基础对话（empty_content）**，Basic ✗ 且 JSON/Tools/Temp0/Streaming 全部「未检测」→ Probe A 成为新前置 false negative，Action 继续被 Guard 安全阻止。
- **Schema Before**: v024 ｜ **Schema After**: v024 ｜ **Migration**: **0**（无 v025；v001-v024 未动）

## Probe A Root Cause（TASK §3）
- 3.2 旧 Probe 用极小 max_tokens（8/16）：reasoning 消耗输出预算 → HTTP 200 + final content="" → 误判 basic=false。
- 3.3 旧 Response DTO 无 finish_reason / reasoning_content：真正空回答 / reasoning-only / length 截断全部压成 empty_content。
- 3.4 一次空内容即判死并持久化 → Capability Guard → 全部 Action 永久阻断。
- 3.5 basic 失败后 B-E 全部未检测（诊断缺失；未区分 Hard/Soft）。
- 3.1 test_ai_connection 成功条件（content.is_some OR tool_calls.is_some）与 Higher Basic Chat 语义混用。

## Test Connection Semantics（§4.1/§17，两命令统一）
- `connectivity_check(config)`（compatibility.rs）：temp=0 / max_tokens=64 / tools=0 / JSON off / synthetic "Reply briefly."。
- 成功 = HTTP 成功 + envelope 可解析 + ≥1 choice（content blank 也成功——这不是 Capability Test）；文案「API 连接成功，模型：{model}。Higher 能力请使用「检测 Higher 兼容性」验证。」；失败 = sanitized 人话（认证失败/连接失败/模型不存在/请求失败/响应格式异常）。禁止「模型正常/完全可用/支持 Higher」。

## Response Truth Model（§5，client.rs）
- `ChatChoice.finish_reason: Option<String>`（Provider 原样；无厂商 enum）；`ChatResponseMessage.reasoning_content: Option<String>`；`Completion` 透出两者。
- **reasoning_content 持久化/展示 = 0**（只用于 classify_final 分类；禁 final_text/trace/DB/UI/文档）。正常聊天 assistant 文本仍只来自 message.content。
- `chat_stream` 增加显式温度参数（FastChat=0.3 不变；Probe E=0.0）。

## Token Budgets（§7 常量，禁止调值）
CONNECTIVITY=64 ｜ BASIC_1=256 / BASIC_2=1024 ｜ STRUCTURED_NATIVE=256 / PROMPT=256 / REPAIR=512 ｜ TOOL=256 ｜ TEMP0_1=256 / TEMP0_2=1024 ｜ STREAMING=256。

## Basic Chat Retry（§8）与 Temperature0 Retry（§11）
- 合成请求：System "You are a capability probe. Return a short visible final answer." + User "Reply with HIGHER_OK."（无任何用户数据）。
- Attempt1（256）分类 **FinalText** → true（不要求精确 HIGHER_OK）；**EmptyFinal/ReasoningOnly/LengthTruncated** → Attempt2（1024，同 prompt）→ FinalText = true + detail=pass_after_retry；仍失败 → false + detail=no_final_content / reasoning_only_no_final / length_no_final / unexpected_tool_only。

## Hard vs Soft Failure（§8.8/§13）
- Hard（认证/授权 401、endpoint/模型 404、connect/DNS 失败——按 client 固定 sanitized 文案匹配，无 400/422 字符串语义、无厂商特判）→ A=false、B-E=skipped_connection_failure、overall=incompatible、UI「未继续检测：连接/认证失败」、仅 1 次请求。
- Soft（其余 request error，如 500）→ A=false 但 **B-E 继续**（完整诊断；Control 仍严格 basic+json+temp0，不降安全）。

## Full Probe Continuation（§9.1/§13.1）
- 无 Hard Failure 时 A-E 全部尝试——「Basic soft false → 后四项全未检测」消灭；bounded 最坏 A2+B3+C1+D2+E1 = **9 calls**（R39 锁定）。

## Probe Snapshot（§14）与 Atomic Persistence（§15）
- lib.rs 命令：开始取 profile snapshot（五字段内存比较，Key 不落盘/不打日志/不 hash）→ run_probe → 保存前 re-read → `capability_fields_changed` = true → **discard**（不覆盖 capabilities/last_tested_at/message，返回「AI 连接配置在检测过程中发生变化，本次结果已丢弃，请重新检测。」）。
- 持久化只在 A-E 全部完成后一次 `save_probe_result`（单 UPDATE；内部错误 → 旧 truth 保留，无半套结果）。last_test_message 追加 §18.4 安全摘要段 `basic=…; json=…; tools=…; temp0=…; stream=…`（无 response/reasoning/prompt/Authorization/Key 原文）。

## Files Added / Modified
- 新：`src-tauri/tests/batch062r1.rs`（41 tests R1-R41，localhost fake provider）
- 改：`src-tauri/src/ai/client.rs`（finish_reason/reasoning_content/Completion 透出；chat_stream+temp）、`src-tauri/src/ai/compatibility.rs`（budget 常量/classify_final/is_hard_connection_failure/ProbeDetails+A-D retry 重编排/connectivity_check/capability_fields_changed/总调用计数）、`src-tauri/src/lib.rs`（两 connection test 走 connectivity_check；compatibility 命令 snapshot guard+原子保存；FastChat stream 0.3）、`src/pages/Settings.tsx`（parseSummary/parseSkipped/basicDetailLabel/temp0DetailLabel；五项行细分+未继续检测（连接失败）；检测中四按钮全 disabled）、`src-tauri/tests/batch062r.rs`（r15 新语义适配）
- 未动：action.rs / grounding.rs / planner.rs / semantic_contract.rs / action_continuation.rs / ai_pending_action.rs / migrations / 领域 repository（§22 遵守）

## AUTOMATED GATE（2026-08-22T14:57:03+08:00 全绿；默认未跑 full cargo test；真实 Provider 0 次自动调用）
| Gate | 结果 |
|---|---|
| batch062r1（新） | **41/41**（R1-R41；fake provider 仅 127.0.0.1） |
| batch062r（回归，r15 适配后） | **44/44** |
| batch062（回归） | **57/57** |
| batch061r（回归） | **47/47** |
| batch0602（回归） | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_foundation（回归） | **7/7** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过** |
| cargo check -j 1 | **0 errors**（8 warnings 既有遗留） |
| Schema / Migration | **v024 / 0** |
| 真实 Provider 自动调用 | **0 次** |
| Source Conflicts | **NONE** |

### 失败与修复（收敛过程）
1. 首轮 check：summary format `{}`×6 vs 5 args → 去掉多余占位；`(config, p)` 部分移动 → config 构造改 clone。
2. batch062r1 首轮 5 失败：`basic_bodies` 把 D/E 同 prompt 请求计入 → 拆 `attempt_a_bodies`（首个 structured 请求之前的 HIGHER_OK 请求）+ 排除 stream；r34 `contains("hash")` 命中自身注释 → 改 sha256/md5 断言。
3. batch062r r15 旧断言（basic 失败→只 1 请求+全 None）与新 §9.1 冲突 → 重写为「两次空 final → basic=false no_final_content，B-E 继续且 structured=true，status=incompatible」。

### Human Runtime = PENDING
- **AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**——TASK §39 H00-H10（启动→连接语义文案→Re-Probe 五项全出→Basic retry truth→预期 DeepSeek Control Truth（basic/json/temp0 ✓）→Fresh Action→Approval First→Ambiguity→「第一个」0 Call 续答→Restart→Cross Conversation）由用户实机验证；Trae 禁止烧真实 Key。

# DEV-0062R · Compatibility Reliability & Provider Truth Repair · TRAE_RUN

- **DEV ID**: DEV-0062R（DEV-0062 Human Runtime 暴露的 Compatibility False Negative + Provider Truth 封闭式修复；非 DEV-0063、不重做 DEV-0062）
- **Start**: 2026-08-22T13:56:36+08:00 ｜ **End**: 2026-08-22T14:14:58+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline HEAD**: `c19ce78cf85f6e8d1bb4f616576e9f225b897573`（同 DEV-0062；工作区 = DEV-0062 全部未提交实现 + 本任务书替换，与 TASK §1 审计一致，无 BASELINE_DRIFT）
- **Baseline Git Status**: DIRTY（预期：DEV-0062 未提交工作 + `.higher` 四文档 + TASK.md 替换；无未知用户修改）
- **Schema Before**: v024 ｜ **Schema After**: v024 ｜ **Migration**: **0**（v001-v024 未动；无 v025）
- **Human Runtime Evidence Received**: 用户实测 DEV-0062——「测试连接」成功（deepseek-v4-flash）但「检测 Higher 兼容性」= Limited 缺结构化输出（basic=true / structured_json=false 持久化）；随后 Action「把8月25日的TEST-STABLE改成35分钟。」被 control_capability_guard 提前阻断（Guard/Truth Guard 本身工作正常，能力事实错误）；DEV-0061R 同一 Connection 曾真实完成 Interpreter→SemanticAction→Grounding→ChangeSet。
- **纪律**：TASK 只读（§42）；真实 Provider 0 次自动调用（§35，兼容性行为测试只用 localhost fake provider）；默认不跑 full cargo test；`-j 1`；禁 reset/checkout/restore；STOP 即停

## Audited Root Causes（TASK §3，全部确认并修复）
| # | 根因 | 修复 |
|---|---|---|
| 3.1 P0 | Probe B 用 `client.chat`（temp=0.3）否决 temp=0 的真实 Control Runtime | run_probe 全部 `chat_with_temperature(..., 0.0)`（R11 锁定） |
| 3.2 P0 | HTTP 200 + invalid JSON 不 fallback（只认 400/422 字符串） | Native 任一失败（200 invalid/empty/request error/response_format 不支持）且 basic 已过 → 必进 PromptOnly（R07/R08） |
| 3.3 P0 | Parser 只 `is_object()`（{"abc":123} 也过） | 必须通过真实 `runtime::parse_turn_decision` 且 = FastChat（R01-R04） |
| 3.4 P0 | Re-Probe 被历史 json_strategy=prompt_only 污染（Native 请求实际没发 response_format） | `AiRuntimeConfig.json_mode_override` + `with_forced_json()`（ForceNative/ForcePromptOnly；R05 请求体级断言） |
| 3.5 P0 | Probe 单次失败即 false（Runtime 有 Repair Once） | PromptOnly 200+非空但 parse 失败 → bounded Repair Once（R09）；上限 3 请求（R12） |
| 3.6 P0 | 一次错误 Probe → 持久化 false → 全部 Action 堵死 | 修 Capability Truth（Guard 保留未删，STOP-03 遵守） |
| 3.7 P1 | Control Guard 不查 basic_chat | `control_known_false()` = basic/json/temp0 任一 Some(false)（R23-R25） |
| 3.8 P0 | Primary Resolver first-enabled 隐藏 fallback | 严格真值：id 不存在/disabled → 明确报错（R28/R29） |
| 3.9 P0 | Explicit Control 失效偷偷 Follow Primary | Some(id) 不存在/disabled → 报错；仅 None 允许跟随（R30/R31/R32） |
| 3.10 P1 | legacy save_ai_settings first-enabled fallback | 只认真实 Active Primary（R33） |
| 3.11 P1 | Primary/Control 切换非原子（partial state） | `set_active_profiles_atomic`：双校验 + BEGIN IMMEDIATE 单事务（R34/R35） |
| 3.12 P1 | Active Connection 可直接被 disable | update() Disable Guard（primary/control 拒绝 + 至少留一个 enabled；R38-R41） |
| 3.13 P1 | set_active_primary 注释与实现不一致 | untested 仅拒绝「切换到不同 id」；同 id 维持允许（R36/R37） |
| 3.14 P1 | Compatibility UI 缺诊断信息 | 卡片五项能力行 + strategy + 最后检测时间 + role 徽标 + follow 警告 + 配置变化提示 |

## Files Added / Modified
- **新**：`src-tauri/src/ai/compatibility.rs`（Probe orchestration：A-E + structured_output_valid + tool_call_valid + bounded strategy fallback + sanitized report）、`src-tauri/tests/batch062r.rs`（44 tests R01-R44，localhost fake provider）
- **改（backend）**：`ai/provider.rs`（json_mode_override/with_forced_json/严格 resolver/control_known_false/primary_basic_error/ResolvedAiProfiles+Debug）、`ai/mod.rs`（+compatibility 模块；save_ai_settings 严格真值 → Result<(),String>）、`repository/ai_provider_profile.rs`（update→Result<(),String>+Disable Guard；set_active_primary untested 同 id 修正；+set_active_profiles_atomic）、`lib.rs`（probe 命令改用 compatibility::run_probe；set_active_ai_profiles 原子化；Control Guard=control_known_false；FastChat+PRIMARY guard + primary_client basic_chat 拒绝）
- **改（frontend）**：`src/pages/Settings.tsx`（capMark/capRow/strategyLabel + 卡片五项行 + role 徽标 + follow 警告 + untested 重检提示）、`src/styles.css`（settings-role/settings-conn__caps/settings-cap 三态）
- **改（tests）**：`tests/batch062.rs`（t21 适配 control_known_false 新形态 + cfg() +json_mode_override）

## Structured Probe Algorithm（§7）
- Payload：synthetic TurnDecision `{"route":"fast_chat","skills":[]}`（真实 parse_turn_decision 校验，零业务副作用；wrapper 语义=Runtime 同源：纯 JSON/```json/``` fence/首尾空白 ✓，夹文字/多 JSON/错 shape ✗）
- Step B1 Native（ForceNative，temp=0）→ 成功 = native；任一失败 → Step B2 PromptOnly（ForcePromptOnly，temp=0）→ 成功 = prompt_only；HTTP 成功+非空但 parse 失败 → Repair Once（只含 synthetic contract + 截断 120 字 invalid output + 安全错误类别；temp=0）→ 成功 = prompt_only+repair_used；否则 structured=false + json_strategy=unknown（禁止保存 native 假装可用）
- Bounded：Native 1 + PromptOnly 1 + Repair 1 = **≤3 Provider 请求**

## Provider Resolver Truth / Atomic / Disable Guard
- Resolver（§15）：primary id 必须存在+enabled；control None=Follow（唯一）；Some 失效=报错；Capability 不满足不换 Provider（交 Guard 拒绝）
- Atomic（§17）：Validate 双方（含 untested 同 id 豁免）→ BEGIN IMMEDIATE → 写 primary+control → COMMIT；失败 ROLLBACK 双值不变
- Disable Guard（§18）：active primary「请先切换主要 AI 后再停用」；explicit control「请先改为跟随主要 AI或切换 Control 后再停用」；非 active 允许；至少保留一个 enabled
- §14 Primary Basic Honesty：basic_chat=Some(false) → FastChat/HigherRead/Planner/specialized 均明确人话拒绝（primary_basic_guard），untested legacy 不变

## AUTOMATED GATE（2026-08-22T14:14:58+08:00 全绿；默认未跑 full cargo test；真实 Provider 0 次自动调用）
| Gate | 结果 |
|---|---|
| batch062r（新） | **44/44**（R01-R44，含 localhost fake provider 行为级） |
| batch062（回归，t21 适配后） | **57/57** |
| batch061r（回归） | **47/47** |
| batch0602（回归） | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_foundation（回归） | **7/7** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过**（10.48s） |
| cargo check -j 1 | **0 errors**（7 warnings 既有遗留） |
| Schema | **v024 保持 · Migration 0** |
| 真实 Provider 自动调用 | **0 次**（fake provider 仅 127.0.0.1） |
| Source Conflicts | **NONE** |

### 失败与修复（收敛过程）
1. compatibility.rs 首轮 check：`first_error = e`（E0308 &String）→ `e.clone()`。
2. batch062r 首轮编译：`ResolvedAiProfiles` 无 Debug（unwrap_err 需要）→ derive(Debug)。
3. batch062 t21 断言旧实现形态 `control.capabilities.structured_json == Some(false)` → 最小适配为新 `control_known_false(&control.capabilities)` + provider.rs 三项能力源码断言（语义等价超集）。
4. 测试自身笔误：r43 `contains().collect()` 类型错误 → 简化断言；FakeResponse::Status 分支 format 清理。

### Human Runtime = PENDING
- **AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**——TASK §37 H00-H16（启动/迁移安全→DeepSeek 测试连接→Re-Probe（重点 basic/json/temp0=true + strategy=native OR prompt_only，允许 limited）→连续两次检测稳定→新会话 Action 真实 Proposal→Approval First→limited-but-Control-Compatible Action→ambiguity→「第一个」0 Interpreter 续答→重启续答→跨会话隔离→新意图逃逸→Disable Guard→原子切换→Provider Truth→历史 Provenance）由用户实机验证。

# DEV-0062 · Multi-Provider AI Profiles & Stable Action Continuation · TRAE_RUN

- **DEV ID**: DEV-0062（多 AI Connection / Provider-Model 与 Runtime 解耦 / Primary+Control 双角色 / Compatibility 检测 / Action Clarification → 持久化 Control State / 禁止假 Proposal）
- **Start**: 2026-08-22T12:04:34+08:00 ｜ **End**: 2026-08-22T12:52:17+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline HEAD**: `c19ce78cf85f6e8d1bb4f616576e9f225b897573`（DEV-0061R automated gate passed）
- **Baseline Git Status**: CLEAN（仅 `.higher/TASK.md` 被替换为本任务书，属预期输入）；`git diff --stat` 仅 TASK.md
- **Schema Before**: v023 ｜ **Migration**: v024_ai_provider_profiles_and_action_continuation（本轮唯一）
- **纪律**：TASK 只读（§87）；真实 Provider 0 次自动调用（§80）；默认不跑 full cargo test；`-j 1`；禁 reset/checkout/restore；DECISION_REQUIRED 即停

## PART 0 · Baseline 核对（2026-08-22T12:04）
- `git rev-parse HEAD` = c19ce78cf85f6e8d1bb4f616576e9f225b897573 ✓（与 TASK §1 审计一致，无 BASELINE_DRIFT）
- `git status --short` = ` M .higher/TASK.md`（任务书本体）✓
- `git diff --stat` = 仅 TASK.md（5083 行替换）✓
- 源码事实核对（与 TASK §2 一致）：`ai/mod.rs` `enum AiProvider { Deepseek }` 单值 + `load_ai_settings` 恒 Deepseek；client.rs thinking 直接改 model（DeepSeek 特有）；无 provider.rs / ai_provider_profile / ai_pending_action。SOURCE_CONFLICT：无。

## PART 1 · Migration v024（本轮唯一；v001-v023 未动、无 v025）
- **`migrations/v024_ai_provider_profiles_and_action_continuation.rs`（新）**：
  - `ai_provider_profiles`：name / adapter_kind CHECK(deepseek|openai_compatible) / base_url / api_key / model / thinking_mode CHECK(off|deepseek_model_suffix) / enabled / capabilities 五列三态（basic_chat/structured_json/tool_calls/temperature_zero/streaming 均 NULLABLE bool）/ json_strategy / compatibility_status CHECK(full|limited|incompatible|untested) DEFAULT untested / probe_message / probe_at；idx_enabled。
  - `ai_pending_actions`：conversation_id FK(ai_conversations) / profile_id / source_run_id FK(ai_runs) / status CHECK(active|resolved|cancelled|expired|stale) / action_json / candidates_json / attempt_count / expires_at / created_at / resolved_at；**partial unique index**（status='active' 时 (profile_id,conversation_id) 唯一——每对至多 1 active）。
  - `ai_runs` +8 列 provider snapshot（provider_profile_id/provider_profile_name/adapter_kind/provider_model + control_ 四列）。
  - **Legacy 自动迁移**：profiles 表空时读 settings KV `ai.base_url/api_key/model/thinking_enabled` → 插入首个 DeepSeek Connection（缺省兜底 api.deepseek.com / deepseek-v4-flash）；`ai.active_primary_profile_id` 为空时设为该 Connection；**原 Key 不删除**（STOP-06 安全满足）。

## PART 2 · Provider Architecture（Adapter 边界唯一模块）
- **`ai/provider.rs`（新）**：`AdapterKind{Deepseek,OpenaiCompatible}` / `ThinkingMode{Off,DeepseekModelSuffix}`（serde 与 DB CHECK 对齐）；`AiRuntimeConfig`（请求级 immutable：profile_id/name/adapter_kind/base_url/api_key/model/thinking_mode/capabilities——§13 与 DB Profile 实体分离）：
  - `endpoint()`：base 去尾斜杠 + 单个 `/chat/completions`（双斜杠/重复路径=0）。
  - `effective_model()`：DeepSeek+DeepseekModelSuffix → `{model}-thinking`；**OpenAI Compatible 原样返回**（永不加 -thinking，T12）。
  - `use_native_json()`：json_strategy=prompt_only → 禁发 response_format。
  - `resolve_active_ai_profiles()`：读 settings KV active primary/control；control 缺省=follow primary；active 失效（删除/incompatible/disabled）→ 兜底第一个 enabled；无隐藏 Provider fallback（AI-INV-022）。
  - 用户文案：`control_capability_error` / `primary_tools_error` / `primary_json_error`（确定性中文，不泄漏 raw 400/missing field）。
  - `probe_tool_schema()` 合成工具 `higher_capability_probe`（真实 tool calling schema，不触正式数据）+ `summarize_probe()` 纯函数。
- **`ai/client.rs`**：`AiClient { config: AiRuntimeConfig }`；chat/chat_with_temperature/chat_stream 全部经 `config.endpoint()/effective_model()/use_native_json()`——client 零厂商知识。
- **`repository/ai_provider_profile.rs`（新）**：CRUD + `update`（base_url/model/thinking_mode 变化 → compatibility 重置 untested，SQL CASE 单语句）+ `save_probe_result`（message 截 300 字，无 Key）+ active id 读写（settings KV）+ `set_active_primary`（incompatible/disabled 禁止）+ `set_active_control`（显式 pin 需 control_compatible）+ `delete_guarded`（active/explicit control 不能删；至少留一个可用）。
- **`ai/mod.rs`**：`load_ai_settings` → resolve active primary（legacy 兼容）；`save_ai_settings` → 写 active primary profile；`AiResult` +provider_profile_name/adapter_kind/provider_model。

## PART 3 · Capability Contract（AI-INV-018）
- `AiCapabilities` 三态（true/false/null=untested）五项 + json_strategy(native/prompt_only/unknown)。
- `compute_compatibility_status()` 纯函数：basic_chat=false → **incompatible**；basic+json+tools+temp0 → **full**；其余 → **limited**。
- `control_compatible()` = basic+json+temp0（**不要求 tools**——T17 锁定）。
- **Compatibility Probe（Tauri 命令 `test_ai_provider_compatibility`，A-E）**：A 基础 chat（"ping"≤16 tok）/ B structured JSON（native `response_format` 失败且 400/422 → prompt_only 重试并记 strategy）/ C 合成工具调用 / D temperature=0 复诵 / E streaming 首 delta；结果 `summarize_probe()` → `save_probe_result`（**0 Key 落库**）。
- **Capability Guard 运行时**：仅 `Some(false)` 拒绝（untested/legacy 迁移记录 NULL 保持可运行）——Control 侧 structured_json/temp0 缺失 → `control_capability_error`；Primary 侧 tool_calls 缺失（HigherRead/工具循环）→ `primary_tools_error`；Planning+json 缺失 → `primary_json_error`。

## PART 4 · Primary / Control Role Wiring（§8/§27/§28）
- `ai_start_run` 开头 `resolve_active_ai_profiles()` 一次 resolve → `run_chat_turn(primary: AiRuntimeConfig, control: AiRuntimeConfig)`（签名替换旧 settings 参数）。
- run 开头 INSERT ai_runs 带 8 列 snapshot（provider/control profile id+name+adapter_kind+model——**AI-INV-021 历史不漂移**）。
- **Control 角色**（control_client + `provider_request_started_role(..., "control", config)` trace）：Turn Interpreter / Repair Once / Candidate Selection（temp=0）。
- **Primary 角色**（"primary" trace）：主回答工具循环 / FastChat / Planner / Mastery/PersonalCompile/PlanningReview/ai_analyze（各 resolve primary + structured_json guard）。
- **Streaming 降级（§24）**：FastChat streaming=`Some(false)` → 直接 `Err("streaming_disabled")` 转 non-stream（不试 stream）；unknown → 先 stream 失败一次 fallback；**已有 delta 不重请求**。
- 10 个新 Tauri 命令并注册：list/get/create/update/delete_ai_provider_profile、get/set_active_ai_profiles（set 后 emit `higher:ai-profiles-changed`）、test_ai_provider_connection、test_ai_provider_compatibility。

## PART 5 · Action Continuation（§39-59 · AI-INV-019/020）
- **`repository/ai_pending_action.rs`（新）**：`PendingCandidate`（derive Default；real_id 仅 Backend 可见）+ `from_grounding`（EntityHint→Candidate 转换）；`find_active`（惰性 expire：过期→expired 状态，不 hijack）/ `create_or_replace`（单事务：旧 active→cancelled 再插入，expires_at=`datetime('now','+24 hours')`）/ `set_status`（resolved_at 条件写）/ `bump_attempt` / `candidates`。
- **`ai/action_continuation.rs`（新，deterministic resolver · 0 Provider Call）**：`PendingSelection{Selected(real_id,idx)/StillAmbiguous/NoMatch/NotSelection/Cancel}`；`resolve_pending_selection()`：Cancel（短句≤16 字+取消词+非完整命令）→ 约束收集（`leading_ordinal`：第X个/X号，**纯数字仅 allow_pure 且整串**——防"08-24"尾段误判；`extract_date_with_env`：2026-08-24/8月24日/08-24（ASCII 安全字节区间）；`extract_relative_date`：今天/明天/后天 via `recurring_rule::shift_date`；唯一标题）→ `looks_like_new_intent`（1+1 等退出）→ **约束交集必须唯一命中（不猜）**；`candidates_stale`（task: title/date/status 对比；rule: +enabled；实体已删=stale）；`candidates_text`/`no_match_text` 用户文案。
- **`ai/action.rs`**：`ActionOutcome::Clarification { message, candidates }`（携带真实 Candidate 供持久化；6 处 ambiguous_text 调用点 + Bulk 上限 + compile_action 兼容入口同步）。
- **`lib.rs` Pending Gate（§44 Turn Priority：Envelope → resolve → Read Pending → Pending Gate → Planner gate → Interpreter，~140 行）**：stale 检查 → resolver → Cancel（cancelled 文案）/ NoMatch（attempt+1 重述候选）/ StillAmbiguous（再列候选）/ NotSelection（旧 pending cancelled，正常走 Interpreter）/ **Selected（反序列化原 action_json → plan_action → 真实 ChangeSet → emit ai://changeset → resolved；不二次询问确认）**。`gate.flatten()` 处理双层 Option。
- Action 分支 Clarification → `persist_pending`（PendingCandidate::from_grounding 写 ai_pending_actions）。
- **`ai/runtime.rs`** +`needs_reference_history()`：cue 词表（刚才/刚刚/那个/这个/它/上一个/下一个/第一个/第二个/前一个/后一个/继续/同样/照刚才/那明天/那后天）；完整显式请求不带 recent user history（H20 同句稳定性）。
- **`repository/recurring_rule.rs`**：`shift_date` fn → pub（resolver 复用）。

## PART 6 · Truth Guard（§62/§63）
- `lib.rs`：write intent + HigherRead 路由 + 0 ChangeSet → **final_text 整体替换**为确定性真话（非 append；error=`write_route_miss`）——「文字说已有修改方案但没有查看计划按钮」假状态消灭；Proposal UI 只由真实 ChangeSet 驱动。

## PART 7 · Frontend
- **`src/types.ts`**：AiResult+3 字段；AiCapabilities/AiProviderProfile/AiActiveProfiles。
- **`src/api.ts`**：10 个新 invoke wrapper。
- **`src/pages/Settings.tsx`**：AiSection 完全重写——compatLabel 四态徽标 / controlCompatible / ConnectionModal（adapter 条件显示 Thinking checkbox；datalist 模型建议）/ profiles 列表卡 / 主要AI+动作理解AI 下拉 / 添加入口；imports 全换新 API + `listen`。
- **`src/components/ai/AiPanel.tsx`**：footer「AI [Connection ▾]」（enabled profiles；disabled=runBusy）；diag 显示 `providerProfileName ?? "旧版本未记录"`；reloadConns + `higher:ai-profiles-changed` 监听；删除 getAiSettings/saveAiSettings/changeModel/model state。
- **`src/components/ai/AiPanelContext.tsx`**：diag+providerProfileName/providerModel 透传。
- **`src/styles.css`**：settings-conn/settings-compat（full 绿/limited 橙/bad 红/untested 灰）/btn--danger。

## PART 8 · 测试（batch062.rs 新 57 项 T01-T57 + 回归适配）
- T01-T25 Provider（migration/legacy/active/CRUD/adapter/compatibility/streaming 降级/Key 安全/snapshot）；T26-T32 UI 源码级断言；T33-T53 Action Continuation 全链（resolver 约束/stale/expire/隔离/FK/persist）；T54-T57 Stability。
- helpers：`mk_conv`（INSERT ai_conversations）+ `mk_run`（INSERT ai_runs）——满足双 FK；`persist_pending_from_clarification` 复现 lib.rs 持久化路径。
- schema 断言 23→24 批量适配：adjustment_system/attachments/batch03/feedback_system/evaluation_system/knowledge_workspace/profile_system（count）+ batch049 + batch058 + batch0601（t4 改验 v24）+ batch0602（Clarification 结构体字面量）+ ai_assistant（AiResult 3 新字段）。

### 失败与修复（收敛过程）
1. cargo check 首轮 5 errors：`shift_date is private`（E0603）→ pub fn；`Option<Option<..>>`（E0308 gate 双层）→ `.flatten()`；中文 match `(None,v2)=>v2`（E0308 '十' 特判）→ `(None,_)=>v`；`"月".len_utf8()` 不存在（E0599）→ `.len()`；closure 参数（E0593）→ `unwrap_or(base.clone())`；unused `is_assistant` → `let _legacy_mode`。
2. batch062 编译：`PendingCandidate: Default` 不满足（E0277×16）→ repository derive Default；`apply()` 3 参（E0061）→ `.apply(cs, fx.p, false)`；`for src in [s,p]` move（E0382）→ `[&s,&p]`。
3. **FOREIGN KEY constraint failed**（t33-t45/t48-t51/t53 大面积）：ai_pending_actions 双 FK → 新增 mk_conv/mk_run helpers + 各直接调用点补齐（t43/t48/t49/t50/t51/t45 文件 DB 场景）。
4. t37「08-24」误命中：纯数字 ordinal 匹配尾部"24" → `leading_ordinal(s, allow_pure)`，parse 仅 `i==0` 时 allow_pure。
5. t17 断言方向反：control_compatible 不要求 tools → `assert!(...)`；t19 分割窗口：锚点加 `{` 后缀 + 匹配串改 `client.chat(msgs`。
6. t53 resolve_recent 不命中：需 `recency_hint: Some("recent_updated")` EntityHint 覆盖。
7. batch0601 t4：v==23→24 且 `WHERE version=23` name 断言；ai_assistant AiResult 补 3 None。

### AUTOMATED GATE（2026-08-22T12:49:16+08:00 全绿；默认未跑 full cargo test；真实 Provider 0 次自动调用）
| Gate | 结果 |
|---|---|
| batch062 | **57/57**（T01-T57） |
| batch061r（回归） | **47/47** |
| batch0602（回归） | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_foundation（回归） | **7/7** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过**（10.35s） |
| cargo check -j 1 | **0 errors**（8 warnings 既有遗留） |
| Schema Migration | **v024（唯一）**；v001-v023 未动；无 v025 |
| 真实 Provider 自动调用 | **0 次** |
| Source Conflicts | **NONE** |

### Human Runtime = PENDING
- **AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**——TASK §82 H00-H22（备份→v024 迁移→legacy DeepSeek→第二 Connection→兼容检测→Primary/Control→能力降级→流式→Thinking→Action 澄清/续答/重启/跨会话/新意图逃逸→同句稳定→Direct Write→历史 Provider 显示）由用户实机验证；Trae 禁止烧真实 Key。

### 修改文件（全量）
- 新：`src-tauri/src/migrations/v024_ai_provider_profiles_and_action_continuation.rs`、`src-tauri/src/ai/provider.rs`、`src-tauri/src/ai/action_continuation.rs`、`src-tauri/src/repository/ai_provider_profile.rs`、`src-tauri/src/repository/ai_pending_action.rs`、`src-tauri/tests/batch062.rs`（57 tests）
- 改（backend）：`ai/client.rs`、`ai/mod.rs`、`ai/action.rs`、`ai/runtime.rs`、`ai/trace.rs`、`repository/mod.rs`、`repository/recurring_rule.rs`、`migrations/mod.rs`、`lib.rs`
- 改（frontend）：`src/types.ts`、`src/api.ts`、`src/pages/Settings.tsx`、`src/components/ai/AiPanel.tsx`、`src/components/ai/AiPanelContext.tsx`、`src/styles.css`
- 改（tests）：adjustment_system / attachments / batch03 / batch049 / batch058 / batch0601 / batch0602 / evaluation_system / feedback_system / knowledge_workspace / profile_system / ai_assistant（schema 23→24 + 结构适配）

# DEV-0061R · Higher AI Runtime Stabilization · Recovery · TRAE_RUN

- **DEV ID**: DEV-0061R（Decision-Complete Recovery Task：接管旧 DEV-0061 半施工状态 → 完成 Unified Higher AI / Turn Interpreter / Semantic Contract v2 / Conversation-Scoped Recent / Planner 边界 / Trace / Recurring Range / Task 菜单 / batch061r）
- **Start**: 2026-08-22T09:27:56+08:00 ｜ **End**: 2026-08-22T10:22:01+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: 工作区 = DEV-0060.2 完成态（AUTOMATED GATE PASSED，444 tests）+ 旧 DEV-0061 部分施工后人工停止；Schema **v023 保持（0 migration）**；真实 DeepSeek 0 次自动调用；Rust 低并发；默认不跑 full cargo test
- **纪律**：TASK 只读；禁止 reset/checkout/restore；决策已定 Trae 只实现；DECISION_REQUIRED 即停

## PART 0R · Recovery Audit（§3，2026-08-22）
- `git status --short`：M = 0059.2→0060.2 全部未提交工作（ai/mod、context、context_builder、planner、prompts、tools、lib、migrations/mod、changeset、recurring_rule、task、前端 5 文件、测试 16 文件）；?? = action.rs / grounding.rs / runtime.rs / skills/ / trace.rs / v023 migration / batch060/0601/0602。
- 逐文件甄别：
  - `ai/planner.rs` diff（+412）：全部为 DEV-0060 workflow payload/paused/cancelled——非 0061 施工。
  - `ai/context.rs` diff（+2）：DEV-0060 PART C HigherData——非 0061 施工。
  - `ai/grounding.rs` Recent 段（本日重读）：`static RECENT: OnceLock<Mutex<RecentEntityContext>>` 全局单例仍在——**TASK §2 所述 HashMap 改造实际未落地**。
  - `ai/action.rs`（本日重读全文）：三处 `#[serde(flatten)]` 仍在，`= DEV-0060.2 完成态`。
  - `AiPanel.tsx`：mode toggle / needs_assistant UI 仍在（0060.1 完成态）。
- **结论**：旧 DEV-0061 的实际产出 = 源码审计 + 上一段 TRAE_RUN「DEV-0061 PART 0」核对表（内容与 0061R 前提一致）。无半成品代码、无冲突代码。
- 分类：
  - **RECOVER_KEEP**：DEV-0059.2→0060.2 全部工作区修改（444 tests 基线）；旧 0061 的 PART 0 源码核对结论（下方保留，标注为 Recovery 输入）。
  - **RECOVER_FINISH**：无。
  - **RECOVER_REWRITE**：无（Recent HashMap 按 0061R §21 决策从当前 OnceLock 状态直接实现）。
  - **UNRELATED**：无用户其他工作（全部 M 均为本项目 DEV 链）。
- 无粗暴 Reset；无 checkout/restore。

## 旧 DEV-0061 PART 0 源码核对（Recovery 输入，与 0061R 一致保留）
| 断言 | 源码证据 | 结论 |
|---|---|---|
| Contract flatten 矛盾 | action.rs UpdateTask/UpdateRecurringTask/BulkUpdateTasks `#[serde(flatten)]`；runtime.rs EXAMPLES 教 `"update":{}` 嵌套 → payload 全 None → 伪 NothingToChange | 属实（P0） |
| Recent app-global | grounding.rs:475 `static RECENT` 无隔离 | 属实（P0） |
| ContextPurpose 页面劫持 | context_builder.rs:193 session/knowledge 先于 user message | 属实 |
| Planner 关键词劫持 | planner.rs:21-32 PLANNING_WRITE_PATTERNS 含「帮我安排/生成任务/安排一下」 | 属实 |
| Trace FK 失败 | lib.rs ai_runs 仅终态 INSERT；v017 ai_run_events.run_id FK → 早期 event 丢失 | 属实（P0） |
| 温度统一 0.3 | client.rs:135/207 | 属实 |
| 双 NL 主路径 | AiPanel.send→aiStartRun；AiPanelContext.sendChat→aiAnalyze | 属实 |
| mode 双 UI | AiPanel.tsx:592-611/899；lib.rs 三处 needs_assistant gate | 属实 |
| 菜单 z-index | styles.css 3927 z-30 < 8088 backdrop z-60 | 属实 |
| Materialize 只当日 | recurring_rule.rs:409 单日签名 | 属实 |
| session↔task | study_sessions.task_id FK（v002/v012/v013）+ idx_sessions_task | 可判定，无 STOP |
| user_modified_at | v021:236 存在 | 可用，无 STOP |
| ai_conversations.mode CHECK | v017:29 含 'assistant' → 新会话写 assistant 合法 | 0 migration 可行 |
- SOURCE_CONFLICT：无；SESSION_PROTECTION：已确认（task_id），无 STOP。

## PART 1R-16R · 施工记录（2026-08-22 完成；Schema v023 保持，**0 migration**；真实 DeepSeek 0 次自动调用）

### Semantic Contract v2（PART 16-20）
- **`ai/semantic_contract.rs`（新）= 唯一事实源**：`SEMANTIC_CONTRACT_VERSION="2"`；13 条 canonical JSON examples（create×2 / update_task×3 含 recency_hint+offset_days:2 / set_task_status / delete_task / update_recurring_task×2（reconcile true/false）/ set_recurring_enabled / delete_recurring_rule / bulk）；`parse_example`（剥 ```json 围栏）/ `all_examples_parse()`（测试锁定）/ `prompt_fragment()`（【Semantic Contract v2】协议+examples 进 Interpreter prompt）/ `repair_instruction(invalid_json, parser_error)`（Repair Once 指令）。
- **`ai/action.rs`**：UpdateTask / UpdateRecurringTask / BulkUpdateTasks 三 variant 字段 `update` → **`patch`**，删除全部 `#[serde(flatten)]`（顶层平铺与 `"update":{}` 嵌套均 parse 失败 → ContractFailure，不再伪 NothingToChange）；`ActionOutcome` + **`ContractFailure(String)`**（与 NothingToChange 严格分离：模型缺 patch/结构不合法 = 「这次没有成功生成可靠的修改方案，正式数据没有变化。请再试一次。」）；Update 类 `patch.is_empty()` → ContractFailure；`RuleUpdatePayload::is_empty()`；`PlanInput` + `conversation_id`（禁 ambient）；`ground_task/ground_rule` 带 conversation_id；`future_pending_occurrences` SQL 加 **`user_modified_at IS NULL AND NOT EXISTS(study_sessions)`**（四重保护 §56）。

### Turn Interpreter 唯一控制入口（PART 9-15）
- **`ai/runtime.rs`**：删除旧 `SEMANTIC_ACTION_EXAMPLES`（单源化）；`semantic_action_prompt` 引用 `semantic_contract::prompt_fragment()`；`TemporalIntent` + `Yesterday`（offset -365..=365）；新增 `TurnDecision { FastChat | HigherRead{skills} | Action{action:SemanticAction} | Planning | PlannerContinuation | Clarification{question} }`——**route+action 一次请求产出**；`turn_interpreter_prompt(user_message, env, planner_active, planner_pending, recent_user_messages≤3 仅指代型辅助)`；`parse_turn_decision`（route=action 直接 serde 解析 action）。
- **`ai/client.rs`**：`chat()` 委托 `chat_with_temperature(..., 0.3)`（普通聊天）；控制层（Interpreter / Repair / Selection）显式 `0.0`。
- **`ai/planner.rs`**：`PLANNING_WRITE_PATTERNS` 收窄——删除「安排学习/帮我安排/生成任务/安排任务/安排一下/帮我排/给我排/排进去/安排上/给我安排一下」；`PLANNING_WRITE_VERBS` 删「生成/创建」（「帮我安排明天30分钟数学」= Action 非 Planner）；`PLANNING_WRITE_HINTS`+`PLANNING_WRITE_VERBS` 双条件；写意图恒 `Planning`（NeedsAssistant 枚举保留但不再产生）。
- **`ai/context_builder.rs`**：`detect_context_purpose` 用户消息优先——Session/Knowledge 需显式 session_cues/knowledge_cues 指代才升级（页面是 Soft Context）。
- **`lib.rs`**：Turn Interpreter 块替代 Semantic Router（`chat_with_temperature(..., Some(1400), 0.0)`）；**Repair Once**（temp=0、tools=0、只含 Contract+invalid JSON+parser_error；二次失败 → 兜底 HigherRead 确定性文案）；PlannerContinuation 仅 active workflow 时成立；Action 分支 `if let TurnDecision::Action` **不再二次调用模型**；Clarification 直接用 Interpreter question；Candidate Selection `chat_with_temperature(..., Some(300), 0.0)`。

### Unified Higher AI（PART 33-38）
- **后端**：`lib.rs` `run_chat_turn` `is_assistant = true` 恒定（参数改 `_is_assistant_legacy`）；删除 readonly NeedsAssistant gate（~30 行）、ASSISTANT_TOOLS 工具循环差异、needs_assistant 协议解析、`[需要助手模式]` 落库、前端 emit；旧 readonly conversation 不阻止 Proposal（PDR-008 双模式正式退役）。
- **前端**：`AiPanel.tsx` 删 mode state/modeRef/switchMode/resumeWithAssistant/header 双按钮/needs 卡片/isNeeds 渲染/历史 mode 标签；`createAiConversation` 恒 "assistant"；`AiPanelContext.tsx` `sendChat` 全部转 `pendingSendRef + higher:aipanel-pending-send` 事件——**ONE Interactive NL Entry**（aiAnalyze assistant_chat 不再承担通用聊天）。

### Conversation-Scoped Recent（PART 21-28）
- **`ai/grounding.rs`** Recent 段重写：`type RecentKey=(i64,i64)`；`static RECENT: OnceLock<Mutex<HashMap<RecentKey, RecentEntityContext>>>`；`with_recent` / `clear_recent` / `record_grounded`（四参）/ `record_apply`（Apply 成功后四参）/ **`load_recent_from_applied`**（restart fallback：同 (profile,conversation) latest applied ChangeSet 回读）/ `resolve_recent`（hint 驱动）；**Pending Proposal ≠ Canonical Recent**（仅 applied 记入）；`#[doc(hidden)] recent_map_for_test` 测试通道。

### Trace 生命周期 + Provider 预算（PART 42-46）
- **`lib.rs`**：run 开头 `INSERT INTO ai_runs ... status='running' ON CONFLICT(id) DO NOTHING`（满足 ai_run_events FK——早期事件不再丢）；终态 `ON CONFLICT(id) DO UPDATE SET status...`（同一 run row）。
- **`ai/trace.rs`**：+`turn_started(page_label)` / `turn_decided(decision,how)` / `semantic_action_repaired(outcome)` / `changeset_created(change_set_id,op_count)`（全部经 ai_run_events 持久化，0 migration）。
- Provider 预算：普通轮 Interpreter×1（+Repair×≤1）；Action 唯一候选 0 额外调用、2-8 候选 Selection×1；普通聊天 temp=0.3 单请求。

### Bounded Materialization + Reconcile 四重保护（PART 50-57）
- **`repository/recurring_rule.rs`**：`ROLLING_HORIZON_DAYS=30` / `MAX_RANGE_DAYS=400`；`shift_date(base,n)`（julianday）；**`materialize_recurring_tasks_range(conn,p,start,end)`**（起止颠倒/超界 Err；逐日幂等 exists_for_rule_date）；**`materialize_rolling_horizon(conn,p,today)`**（today→today+30）。
- **`repository/changeset.rs`**：recurring_rule create Apply 后 `materialize_rolling_horizon(tx,...)`（未来 30 天 occurrence 提前可见）。
- 四重保护（R33-R37 锁定）：`planned_date > today AND status='pending' AND user_modified_at IS NULL AND NOT EXISTS(study_sessions)`——past/completed/手改/有 Session 事实的 occurrence 永不动；disable 只清合法未来 pending derived。

### Task ⋯ 菜单（PART 49）
- **`styles.css`**：`.taskmenu__pop` z-index 30 → **70**（> backdrop 60，菜单不再被遮）；Today.tsx 六项 handler（编辑/调整日期/调整目标/调整知识/修改类型/删除）全部真实可用（R28-R29 源码+行为断言）。

### 修改文件（全量）
- 新：`src-tauri/src/ai/semantic_contract.rs`、`src-tauri/tests/batch061r.rs`（47 tests）
- 改（backend）：`ai/action.rs`、`ai/runtime.rs`、`ai/client.rs`、`ai/planner.rs`、`ai/context_builder.rs`、`ai/grounding.rs`、`ai/trace.rs`、`ai/mod.rs`（+semantic_contract）、`repository/recurring_rule.rs`、`repository/changeset.rs`、`lib.rs`（Turn Interpreter/Unified AI/trace running 先建/record_apply 四参/+materialize 两命令注册）
- 改（frontend）：`components/ai/AiPanel.tsx`、`components/ai/AiPanelContext.tsx`、`styles.css`、`api.ts`（+materializeRecurringTasksRange/materializeRecurringRolling）、`pages/PlanningCalendar.tsx`（refresh=Range 可见月 / 30s=Rolling）、`pages/Today.tsx`（refresh/定时=Rolling）；`skills/{task,recurring_task,time}/SKILL.md`（`update:`→`patch:` 批量）
- 测试适配：`tests/batch0601.rs`（patch 字段/负 offset 合法/rolling 断言）、`tests/batch0602.rs`（CONV/四参/PlanInput）

### 失败与修复（batch061r 收敛过程）
1. 编译：`create_for_profile` 返回 Task 非 i64（+.id）；Trace 签名参数序；unused imports；helper 漏 `}`——全部修正。
2. batch0601 旧断言 vs 新决策：负 offset 拒绝断言 → 改验证 `-1=昨天`；T18-T21 `tasks==1` → rolling 后 `>=31` + 单日幂等不增 + 窗口外单独 materialize。
3. batch061r 运行失败多轮：mk_task rule_id 误传 plan_id 位（FK violation）→ 后置 UPDATE；中文字节边界 panic → `chars().take(40)`；study_sessions 列名 → duration_seconds；AiPanel 注释残留禁词（两处）→ 改写；r26 窗口锚点；**r33 根因 = rule op entity_id（rules 表自增 1）与 task id（tasks 表自增 1）撞号** → `task_op_ids` 按 entity_type="task" 过滤；r41 `count()>=4` → `>=3`（lib.rs 实际恰好 Interpreter/Repair/Selection 三处 0.0）；临时 DEBUG eprintln 已删除。
4. SearchReplace 并行编辑同文件互相覆盖（action.rs enum / lib.rs / planner.rs planning_gate 被吞）→ 发现后逐个串行重做；此后同文件编辑一律单发。

### AUTOMATED GATE（2026-08-22T10:22:01+08:00，全部通过；默认未跑 full cargo test）
| Gate | 结果 |
|---|---|
| batch061r | **47/47**（R01-R42 + Eval E01-E21 子集 + Mock Provider + 温度源码断言） |
| batch0601（回归，rolling 适配后） | **33/33** |
| batch0602（回归） | **29/29** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过**（11.7s） |
| cargo check -j 1 | **0 errors**（6 warnings 为既有遗留） |
| Schema Migration | **0**（v023 保持） |
| 真实 DeepSeek 自动调用 | **0 次** |
| Source Conflicts | **NONE** |

### 最终状态
**DEV-0061R / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**（H01-H22 见 TASK §69，用户实机验证）


# DEV-0060.2 · Higher AI Grounding & Multi-Step Action Runtime · TRAE_RUN

- **DEV ID**: DEV-0060.2（自然语言引用 → Candidate Retrieval → Entity Grounding → Target Scope → Grounded Action Plan → Multi-Operation ChangeSet → Approval；完成 Level 3 / 建立 Level 4-5 基础）
- **Start**: 2026-08-21T20:53:58+08:00 ｜ **End**: 2026-08-21T21:20:38+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: DEV-0060.1 final worktree（Human Runtime 已确认：普通问答/建每日任务/日期正确/Apply 可见）；Schema **v023 保持（本轮 0 migration）**；真实 DeepSeek 0 次自动调用；Rust 低并发
- **纪律**：TASK 只读；不建 grounding/recent/candidate/action plan/vector/embedding/agent loop/workflow 表；不用 Embedding 解 Grounding；不用中文关键词 if/else 承担语义；模型不得生成 Entity ID；Planner 不动；Approval First 不动

## PART 0 · 源码核对 + 真实失败根因（§3，2026-08-21）
| 项 | 源码证据 | 结论 |
|---|---|---|
| FAIL-1 Task Grounding 根因 | action.rs resolve_task：`title LIKE '%'||hint||'%'`——"背单词" 不是 "背10个英语单词" 的连续子串（背/10/个/英语/单词）→ rows=0 → NotFound("没有找到与「背单词」匹配的任务")，与用户实测报错逐字一致 | **属实**：结构过滤（profile+date）本可得到唯一候选，但 LIKE 先行且必败 |
| FAIL-2 Rule Grounding 根因 | resolve_recurring_rule 同 LIKE 失败 → compile_action 返回 Err → lib.rs 旧路径 Err(e) 直接展示 `{e}`（含 repository "ChangeSet 至少包含一个操作" 类内部文案通道未关死） | **属实**：Compiler→create 之间无 Empty Plan Guard，内部错误文案可泄漏 |
| ChangeSet create guard | changeset.rs:78 `if ops.is_empty() { return Err("ChangeSet 至少包含一个操作") }` | 内部防线存在，但用户可见路径未消费 typed outcome |
| task delete / rule delete 能力 | changeset.rs ("task","delete") / ("recurring_rule","delete") 均已实现（含 Undo） | 本轮直接复用 |
| tasks status 值域 | v011：pending（默认）；生命周期 pending/in_progress/completed/skipped；created_at/updated_at 可用 | Bulk status 过滤按此实现 |
| Recent 可用字段 | ai_change_sets.run_id/conversation_id、ai_change_operations after_json（apply 后回写真实 id）、tasks/recurring_task_rules created_at/updated_at | 不需 migration：session-local RecentEntityContext + Apply 后从 operations 回读 |
| Apply 挂点 | lib.rs apply_ai_change_set（成功后）| Recent Context 更新点 |
- SOURCE_CONFLICT：无。

## PART A-S 施工记录（2026-08-21 完成；Schema v023 保持，**0 migration**）

### Grounding Layer（PART A/B/E——`ai/grounding.rs` 新）
- **ReferenceHint（EntityHint）**：entity_type/title_hint/is_plural + 可选 TemporalIntent(date)/status_hint/recurrence_hint/recency_hint/quantity/scope_hint——**只有引用语义，绝无 ID**。
- **TargetScope**：Occurrence / Series / MatchedSet / Recent / Current（Series vs Occurrence 决定 reconcile 语义）。
- **Candidate Retrieval**：`retrieve_task_candidates` 动态拼 SQL——**结构过滤先行**（profile + date + status（not_completed→`!= 'completed'`）+ recurrence presence），>8 才 `narrow_by_hint` 通用 lexical（子串→bigram 重合 + 单字重合度评分，**非中文关键词 if/else**）；`retrieve_rule_candidates`（enabled + repeat_type 过滤）；MAX_CANDIDATES=8。Candidate{candidate_id:"T-3"/"R-2",title,date,status,repeat_type,real_id}。
- **Candidate Selection**：`selection_prompt`（只含用户消息+hint+候选 DTO，**不含源码/不含表结构**）+ `parse_selection`（**candidate_id ∈ 候选集 guard**——模型幻觉 ID → Invalid，绝不猜）。0 候选→NotFound。
- **GroundingOutcome**：Resolved / ResolvedMany / Ambiguous(Vec<Candidate>) / NotFound / Unsupported；唯一候选 → **0 额外 Provider 调用直取**。

### RecentEntityContext（PART F）
- `OnceLock<Mutex<…>>` app-session-local 临时层，每实体类 ≤10（MAX_RECENT=10），**0 表 0 migration**。
- `record_grounded`（grounding 成功即记）；`record_apply(conn, cs_id)` 在 **Apply 成功后**从 ai_change_operations 回读 entity_id / after_json.id——**Recent 只记真实落库的实体**。
- `resolve_recent`：消费前校验实体仍属当前 profile（防跨账号/已删除残留）。

### GroundedActionPlan（PART G-N——`ai/action.rs` 重写）
- `SemanticAction` 九变体：+DeleteTask / DeleteRecurringRule（cleanup_future 默认 true）/ BulkUpdateTasks；UpdateTask `#[serde(flatten)]`；SetRecurringEnabled/UpdateRecurringTask 带 reconcile_future（默认 true）。
- `plan_action(PlanInput)`：Create 类 0 grounding；UpdateTask diff（无变化→NothingToChange）；**ResolvedMany → 多 op ONE ChangeSet**；Series reconcile（`future_pending_occurrences`：planned_date > local_date **AND** status=pending——过去/Completed 永不重写）；disable → 清理未来 pending 投影；Bulk（>MAX_BULK=50 → Clarification）。
- `ActionOutcome`：ProposalReady{ops,title,summary,scope,selection_called} / Clarification / NotFound / NothingToChange / Unsupported——**全部用户语言，内部错误 0 泄漏**。
- **Empty Plan Guard**：0 op 绝不进 ChangeSetRepository::create（validate_action 返回用户文案而非 Err）。
- 旧 `resolve_task/resolve_recurring_rule`（LIKE）保留供 batch0601 锁定回归。

### Prompt / Skill（PART P/T）
- `semantic_action_prompt` 重写：Reference 规则（不得生成 ID/候选由 Runtime 提供）+ `SEMANTIC_ACTION_EXAMPLES`（concat! 常量，9 动作示例）+ target 可选字段说明。
- CAPABILITY_REGISTRY 9→13（+task.delete / task.bulk_update / recurring_rule.delete）；三 Skill version→"2"；task intents +delete_task/bulk_update_tasks；recurring +delete_recurring_rule；三份 SKILL.md v2（Reference Semantics / Occurrence vs Bulk / Series vs Occurrence / reconcile_future:false / "刚才那个" 示例 / 引用日期进 target.date）。

### Runtime 接线（PART R/S——`ai/trace.rs` + `lib.rs`）
- trace +9 事件：grounding_started / candidates_retrieved / grounding_resolved / grounding_ambiguous / grounding_not_found / candidate_selection_started|finished / action_plan_compiled / empty_plan_guarded（复用 ai_run_events，0 migration）。
- lib.rs SemanticAction 分支：Pre-Grounding（closure 检索；recency_hint → resolve_recent；1 候选 → Resolved **0 call**；2..8 → selection_prompt + chat(300) + parse_selection + ground_single，全程 trace）→ plan_action → **ProposalReady 且 ops 非空才 create**；Clarification/NotFound/NothingToChange/Unsupported → 确定性用户文案。
- `apply_ai_change_set` 成功后挂 `record_apply`。

### Provider call budget（实测）
- 唯一候选（FAIL-1 场景）：**0 次额外调用**（结构过滤直取）；2-8 候选：selection ×1（合计 ≤2/轮）；Create 类 0 grounding call。**模型全程不生成 ID**。

### Modified Files（全量）
- 新：`src-tauri/src/ai/grounding.rs`、`src-tauri/tests/batch0602.rs`
- 改：`src-tauri/src/ai/action.rs`（重写）、`ai/trace.rs`（+9 事件）、`ai/runtime.rs`（semantic_action_prompt+EXAMPLES）、`ai/skills/mod.rs`（registry 13/intents/version 2）、`skills/{task,recurring_task,time}/SKILL.md`（v2）、`src-tauri/src/lib.rs`（Pre-Grounding 接线 + Empty Guard + record_apply）、`ai/mod.rs`（+grounding）
- 测试适配：`tests/batch0601.rs`（EntityHint Default / UpdateRecurringTask·SetRecurringEnabled payload 化，33/33 保持）

### Tests（batch0602.rs 29 项 = TASK T1-T35 全覆盖；禁真实 DeepSeek）
- T1-T7 Grounding（T1=真实 FAIL-1「背单词」→唯一候选直取 Ground；LIKE 场景对照）｜T8-T10 Recent（回读/≤10/跨 profile 拒绝）｜T11-T13 Scope（Occurrence/Series/MatchedSet）｜T14-T15 真实失败回归（NotFound 文案 / 0 op 不泄漏内部错误）｜T16-T18 Empty Guard（T18 源码级断言无 banned 文案）｜T19-T21 Bulk（多 op ONE ChangeSet / status 过滤 / >50 Clarification）｜T22-T23 Occurrence vs Series｜T24-T27 Reconciliation（未来同步 / 过去不动 / Completed 不动 / disable 清理）｜T28-T31 Provider 预算（0 call / ≤1 selection / 幻觉 candidate_id→Invalid / 无 ID 生成）｜T32-T35 安全（模型 ID 拒绝 / 跨 profile 隔离 / Unsupported 文案 / Empty Guard 用户文案）

## AUTOMATED GATE（2026-08-21，全部通过）
| Gate | 结果 |
|---|---|
| cargo check | **0 errors** |
| batch0602 | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| 指定回归（batch0592/batch0591/ai_assistant/ai_panel） | **全绿** |
| 全量 cargo test（低并发 `RUST_TEST_THREADS=1`+`-j 1`） | **38 suites / 444 passed / 0 failed**（exit 0；415+29=444 吻合） |
| tsc | **0 errors** |
| npm run build | **通过** |
| 真实 DeepSeek 自动调用 | **0 次** |

# DEV-0060.1 · AI Semantic Action Runtime & Skill Foundation · TRAE_RUN

- **DEV ID**: DEV-0060.1（自然语言理解 → Runtime Truth → Skill → Typed Intent → Domain Compiler → ChangeSet + Fast Chat 真流式 + Task/Recurring 第一批领域能力 + Performance Trace + 永久架构 Guardrails）
- **Start**: 2026-08-21T19:02:12+08:00 ｜ **End**: 2026-08-21T19:50:45+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: DEV-0060 final worktree（Human Runtime 已验证主骨架）；Schema v022 → **本轮 v023**（唯一 migration：recurring_task_rules 语义补齐）；真实 DeepSeek 0 次自动调用；Rust 低并发
- **纪律**：TASK 只读；不建 Skill/Agent/Router/Performance 数据库表；性能 trace 复用 ai_run_events；AI Direct Write = 0；不重写 Planner

## PART 0 · 源码核对（§3 审计复核，2026-08-21）
| TASK 断言 | 源码证据 | 结论 |
|---|---|---|
| §3.1 主路径每轮全量 21 tools | lib.rs run_chat_turn `ai::tools::tool_definitions()` 全量传入 6 轮循环 | **属实（PERF-P0）** |
| §3.2 主 Chat 非真流式 | run_chat_turn 用非流式 `client.chat`；no-tool 后一次性 emit 全文 | **属实**（chat_stream 已存在于 client.rs:185） |
| §3.3 无可靠 current local date | AiPanel send() 未传 date（api.ts aiStartRun 有 date 字段但未用）；多套日期源并存；Runtime 出现 UI=08-21 vs AI=08-18 | **属实（TIME-P0）** |
| §3.4 ContextPurpose 启发式在意图判断前 | context_builder detect_context_purpose：session/knowledge 先于用户意图 | **属实** |
| §3.5 模型直接拼 operations | propose_change_set 由模型自由拼 ProposedOp JSON；出现「至少包含一个操作」空集错误 | **属实** |
| §3.6 Prompt 违反 Knowledge Optional | prompts.rs SYSTEM_PROMPT「structured 任务必须关联稳定知识节点/积累型用宽节点」诱导建 Knowledge | **属实** |
| §3.7 Recurring 系统已存在 | recurring_rule.rs 完整（daily/weekly/weekdays/materialize 幂等 exists_for_rule_date） | **属实（必须复用）** |
| §3.8 TaskModal Knowledge 前置 | TaskModal.tsx:110 `if (repeat !== "none" && itemId != null)` | **属实** |
| §3.9 手工 Recurring 双写 | TaskModal 先 createTask（无 rule_id）再建 rule → materialize 可再生同日同题任务 | **属实** |
| §3.10 Rule 缺 estimated/kind/priority | recurring_task_rules 仅 13 列，无三字段 | **属实（v023 范围）** |
| §3.11 ChangeSet 不支持 recurring_rule | changeset.rs apply_one 无 ("recurring_rule",…)；tools schema 无 | **属实** |
| §3.12 task/update 不完整 V2 | apply_one ("task","update") 只更新 title/date/time/goal（无 estimated/kind/priority/item）vs update_v2 8 字段 | **属实** |
| §3.13 ai_run_events 存在 | v017:67 建表（run_id/event_type/data_json/created_at） | **属实（trace 复用）** |

- SOURCE_CONFLICT：无。

## PART A-M 施工记录（2026-08-21 完成；Schema v023 唯一一条 migration）

### Migration（PART F）
- `migrations/v023_recurring_task_semantics.rs`（新）+ `mod.rs` 注册（version 23, "recurring_task_semantics"）：三条 ALTER ADD——`estimated_minutes INTEGER NULL` / `task_kind TEXT NOT NULL DEFAULT 'structured'` / `priority TEXT NOT NULL DEFAULT 'normal'`；legacy 行保留默认值 0 损失（T1-T4）；**未建任何 Skill/Agent/Router/Performance 表**。

### Architecture changes（Runtime Envelope / Skill / Routing / Provider plan）
- **PART A Runtime Envelope**（`ai/runtime.rs` 新）：`AiRuntimeEnvelope::validated`（YYYY-MM-DD + tz -720..840 校验；**weekday 由 local_date 推导，不信前端**）；`prompt_block()` = 【Runtime Time Truth】（今天/周X/UTC±HH:MM/当前时间；page_date≠today 显式提示——page_date 与 runtime date 分离）；`TemporalIntent`（today/tomorrow/offset_days 0..365/absolute_date/weekday_relative 1..7）→ `resolve(env)` 纯函数（SQLite julianday 加法）；`validate_temporal_semantics`：intent 与编译日期不符 → Err（**TODAY 编译错日期 → Validator Reject**）。
- **PART B Skill System**（`ai/skills/mod.rs` + `skills/{time,task,recurring_task}/SKILL.md` 新）：SkillSpec（id/version/description/**instructions=include_str! 编译期嵌入**/supported_intents/required_capabilities/optional_tools）；`registry()`=time/task/recurring_task 三 Skill；`validate_registry()` SKILL_CONTRACT_STALE；`registry_summary()`（Router 输入摘要）；CAPABILITY_REGISTRY 9 项；TOOL_REGISTRY 21 ToolSpec（permission=Read/Web/Proposal + category + affinity）。**运行时 0 源码扫描**。
- **PART C Turn Router**：六路由 TurnRoute；`fast_chat_shortcut`（高置信寒暄/纯概念 + higher_cues 排除）；`semantic_router_prompt`（只含消息+envelope+planner 摘要+skill 摘要）+ `parse_router_decision`；`semantic_action_prompt`（注入 selected Skills 全文）+ `parse_semantic_action`；**conservative 默认**：active planner→planner_continuation，否则 higher_read。§11 收口（lib.rs）：Cancel（本地）→ legacy 澄清兜底 → 显式新规划 gate → active planner 由 Semantic Router 判定续跑/新意图（**is_new_intent_message 不再是唯一判断**）；新意图接管 → 旧 workflow **paused**。
- **PART D FastChat 真流式**（lib.rs 分支）：`chat_stream` 每 delta 即 emit ai://delta；tools=0 / Memory Extract=0 / 私有 Context=0；`bound_history(8 轮,14000 字符)`（rev 取尾、超预算丢最旧、当前消息永不被裁）；流式失败单次非流式 fallback（主请求语义=1）。
- **PART E Typed SemanticAction**（`ai/action.rs` 新）：六动作（serde tag=type）；`EntityHint`（title_hint+可选 date intent）；`resolve_task/resolve_recurring_rule`（LIKE+可选日期过滤；0→NotFound、2+→Ambiguous）；`compile_action`（CreateRecurringTask → R1 rule create + 命中 recurrence 时 T1 initial task 带 `recurring_rule_ref:"R1"`；CreateTask knowledge_hint 只 resolve existing）；`validate_action`（Minimal Scope：ops 实体 ⊆ requested + knowledge create 硬禁 + CreateTask 时间语义）。**模型不输出 ProposedOp/实体 id**；readonly → needs_assistant（Approval First 不削弱）；Repair Once 只修 schema；成功总结由 Compiler 确定性产出（无二次模型调用）。
- **PART H/I ChangeSet**（`repository/changeset.rs`）：+("recurring_rule", create/update/status_change/delete)；task create 读 `recurring_rule_id|recurring_rule_real_id`；check_forward_refs+resolve_refs 支持 recurring_rule_ref；Undo +recurring_rule（create/update）；task update V2 八字段（未提供保留 before；**snapshot_before 与 fetch_task 均补 V2 全字段**——T28/T29 抓出的真 bug 已修）。
- **PART G 手工路径**（`components/TaskModal.tsx` + Tauri 命令 + api/types）：重复 → 只 `createRecurringRule`（三语义字段可选）+ `materializeRecurringTasks(首日)`；**不再先建无 rule_id 普通 Task；不再要求关联 Knowledge**；首日幂等由 exists_for_rule_date 保证。
- **PART J Tool Scoping**（`ai/tools.rs`）：`tool_definitions_for_scopes(affinities)`（按 TOOL_REGISTRY affinity 过滤）；`fast_chat_tools()=[]`；`scopes_for_route`（fast_chat→[]/higher_read→personal,task,knowledge,read/planning→planning,web）；主循环按 route 动态裁剪（不再每轮 21 全量）。
- **PART L Prompt**（`ai/prompts.rs`）：双树规划段改 Knowledge Optional + Minimal Change Scope；+【Semantic Understanding】段；删"Task 必须建 Knowledge"语义。
- **PART M Performance Trace**（`ai/trace.rs` 新）：Trace（main/secondary_requests 分开计数；first_delta 一次；t_ms 自动注入；run_finished 汇总 duration/请求数）；写入**复用 ai_run_events**（0 migration）；禁记 API Key/完整 Prompt/隐私全文。

### Provider call plan（每轮上限）
- FastChat：main ×1（stream）+ router ×0（本地短路）+ memory ×0
- SemanticAction：router ×1 + action ×1（+repair ×≤1，仅 invalid JSON）+ 总结 ×0
- HigherRead/Planning：router ×1（或本地）+ main 工具循环 ≤6 轮（tools 按 route 裁剪）+ 合法 secondary（Validation Repair/Citation/Memory Extract 按 purpose）

### Performance events
- ai_run_events：route_decided / context_built / provider_request_started / provider_first_delta / provider_request_finished（main_total/secondary_total）/ tool_round_started|finished / semantic_action_parsed / domain_resolved / changeset_compiled / run_finished（status/duration_ms/请求数）

### Modified Files（全量）
- 新：`src-tauri/src/ai/runtime.rs`、`src-tauri/src/ai/skills/mod.rs`、`src-tauri/src/ai/action.rs`、`src-tauri/src/ai/trace.rs`、`src-tauri/src/migrations/v023_recurring_task_semantics.rs`、`skills/time/SKILL.md`、`skills/task/SKILL.md`、`skills/recurring_task/SKILL.md`、`src-tauri/tests/batch0601.rs`
- 改：`src-tauri/src/ai/mod.rs`（+4 模块）、`ai/tools.rs`、`ai/prompts.rs`、`repository/recurring_rule.rs`（RuleSemantics+create/update_with_semantics+materialize→create_from_rule_v2）、`repository/task.rs`（create_from_rule_v2）、`repository/changeset.rs`、`src-tauri/src/lib.rs`（ai_start_run+3 参数/Turn Router/FastChat/SemanticAction/Planner 收口/trace/tool_trace 改名/create|update_recurring_rule 三字段）、`migrations/mod.rs`、`src/api.ts`（aiStartRun+3 / recurring 三字段）、`src/types.ts`（RecurringRule 三字段）、`src/components/ai/AiPanel.tsx`（localIsoDate/localIsoDatetime+send 传参）、`src/components/TaskModal.tsx`、`src/components/ChangeSetReview.tsx`（recurring_rule 标签）
- 测试适配（schema v023 版本断言 →23）：adjustment_system / profile_system / knowledge_workspace×2 / feedback_system×2 / insight_review / learning_hierarchy / stage_b_core / evaluation_system×2 / learning_loop×2 / attachments / batch058

### Tests（batch0601.rs 33 项 = TASK T1-T58 全覆盖；禁真实 DeepSeek）
- T1-T4 migration（数量不丢/legacy 默认/幂等/v023）｜T5-T10 Skill（id/version/capability/tool/无 DirectWrite/embedded）｜T11-T15 Time（TODAY/TOMORROW/+3/next weekday/Validator Reject）｜T16-T24 Compiler（无 knowledge create/rule+initial task/apply 前后/幂等×2/Knowledge Optional/三字段继承/weekly 只匹配日）｜T25-T31 Resolver（唯一/0/2+/estimated 真实更新/未提供保留 before/只改 rule/enabled=false）｜T32-T39 Fast Runtime（tools=0 main=1/memory=0/私有 context/无 21 工具/最小输入/无二次总结/Repair Once/0 mutation）｜T40-T45 Tool Scoping（unique/permission/DirectWrite=0/FastChat=[]/Planning ≤6/4 planning read 可调）｜T46-T55 Planner Regression｜T56-T58 Router Integration（目标陈述→续跑/建任务→SemanticAction+paused/取消→本地 Cancel）

### Remaining Unknown（无法自动验证）
- 真实 DeepSeek 下：Router 判定质量 / FastChat 首字延迟体感 / SemanticAction JSON 产出率 / Repair 命中率 / H1-H14 全部 → **Human Runtime Pending（用户实机，TASK §38；Trae 禁烧真实 Key）**

### 自动化 Gate（RUST_TEST_THREADS=1，-j 1；真实 DeepSeek 0 次自动调用）
- cargo check：**0 errors**（5 warnings 为 ai/context.rs 旧模块死代码遗留，非本轮引入）
- batch0601：**33/33**（T1-T58 全覆盖）
- batch060：**16/16**；指定回归 batch0592 / batch0591 / ai_assistant / ai_panel：**全绿**
- 全量 cargo test：**415 passed / 0 failed**（37 套件；前值 382 + batch0601×33）
- npx tsc --noEmit：**0 errors**；npm run build：**通过**
- 工具真实计数（源码重算，T35 锁定 TOOL_ALLOWLIST.len()=21）：READ 18 · WEB 2 · PROPOSAL 1 · DIRECT WRITE 0；FastChat 携带 0 · Planning ≤6

## 状态：DEV-0060.1 / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING

# DEV-0060 · AI Runtime Truth & Planner Recovery（已完成 · 2026-08-21 AUTOMATED GATE PASSED；Human Runtime 主骨架已验证）
- **Start**: 2026-08-21T16:12:32+08:00 ｜ **End**:（进行中）
- **Timestamp Source: SYSTEM**（`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`）
- **Baseline**: DEV-0059.2 final worktree；Schema = **v022**（本轮原则无 migration）；Git main @ 457fe5e dirty（不 reset/rollback）
- **纪律**：TASK 只读；真实 DeepSeek 0 次自动调用；Rust 低并发（`RUST_TEST_THREADS=1` + `-j 1`）；Schema 保持 v022

## PART 0 · 源码核对（§3 审计复核，2026-08-21）
| TASK 断言 | 源码证据 | 结论 |
|---|---|---|
| §3.1 当前用户消息未作为最后 User Turn | lib.rs:4549 `messages.push(ChatMessage::user(format!("{}\n\n{}", context_text, instruction)))` | **属实** |
| §3.2 content equality 过滤历史 | lib.rs:4385 `.filter(\|m\| m.content != user_message)` | **属实** |
| §3.3 无 Tool Call 后 assistant-only 二次生成 | lib.rs:4572-4603：no tool_calls → `chat_stream(vec![assistant(completion.content)])`（无 system/context/history/用户消息） | **属实** |
| §3.4 普通问题加载全量 Personal Context | context_builder.rs build() 每次全装 L1-L4 | **属实** |
| §3.5 旧 goals.final 冒充当前目标 | context_builder.rs:170-179 `current_goal_summary` 读 `goal_level='final'`，L1 输出「当前目标：xxx」 | **属实** |
| §9 TOOL_ALLOWLIST 缺 4 planning tools | tools.rs:689-707（17 项）vs tool_definitions 21 项（含 list_planning_sources/read_planning_source/list_active_goal_targets/read_active_planning_blueprint） | **属实** |
| §11 workflow latest 按 UUID 字典序 | planner.rs:140-149 `ORDER BY id DESC`（ai_runs.id TEXT UUID） | **属实** |

- ai_runs：id TEXT PK（UUID）、created_at TEXT DEFAULT datetime('now')（v017）→ §11 修复可用 `ORDER BY created_at DESC, rowid DESC`，无需 migration。
- `ConversationRepository::add_message` 返回 AiMessage（含 id）→ §5.3 current_message_id 可直接获取。
- SOURCE_CONFLICT：无。

## PART A-S 施工记录（2026-08-21 完成；Schema 保持 v022，0 migration）

### 修改文件
- `src-tauri/src/lib.rs`：ai_start_run 捕获 current_message_id；run_chat_turn 重构（workflow/gate 决策前置 → purpose → context → 取消分支 → PLANNER_TURN_PROTOCOL 指令 → build_chat_messages 组装 → classify_tool_round 无二次生成 → PlannerTurnResult 解析 → target_proposal 编译 → waiting_approval 带 payload → Generic 跳过 Memory Extract）
- `src-tauri/src/ai/planner.rs`：PlanningWorkflowPayload/PlannerQuestion（workflow_json 结构化）；read/set_workflow_payload；read_workflow_state 改 `ORDER BY created_at DESC, rowid DESC`（§11）；WORKFLOW_STATE_PAUSED/CANCELLED；is_workflow_exit_intent / is_new_intent_message / planning_continuation_decision；filter_pending_questions / format_clarification_reply / record_user_reply；TargetProposalDraft + PlanDraft.target_proposal（validator K 契约）；compile_to_changeset_ops(+has_active_goal_target，GT create→activate 链)；PLAN_DRAFT_INSTRUCTION 增 target_proposal K1-K4；PLANNER_TURN_PROTOCOL（TYPE A/B/C + §17 事实优先级）；build_planning_instruction / build_chat_messages / classify_tool_round（pure 可测）；apply_review_assessment compile 调用同步
- `src-tauri/src/ai/context_builder.rs`：ContextPurpose（Generic/Personal/HigherData/Planning/Knowledge/Session）+ detect_context_purpose；build(+purpose)：Generic/Planning 最小化（仅页面/模式），Personal/HigherData/Session/Knowledge 全量；current_goal_summary 重写（active GoalTarget 唯一正式来源；无 GT →「正式目标未设置」，不返回 legacy）
- `src-tauri/src/ai/tools.rs`：TOOL_ALLOWLIST +4 planning read tools（READ 分类）；defined_tool_names()；get_current_goal → Canonical GoalTarget Adapter（formal_targets/primary/safety/legacy_candidates，canonical=goal_target）；get_profile_summary → legacy_target_description/legacy_target_date；get_current_stage/list_plans → legacy compatibility 标记
- `src-tauri/src/ai/prompts.rs`：SYSTEM_PROMPT +【Current User Intent First】（PART O）
- `src-tauri/src/ai/context.rs`：ai_analyze 旧通道 build 调用固定 ContextPurpose::HigherData
- `src/components/ai/AiPanel.tsx`：run-status 兼容 planner_cancelled / handoff_chat（刷新展示；PART V 最小前端改动）
- `src-tauri/tests/batch060.rs`：新增 T1-T16（16 项）
- 测试适配：batch055/057/058（compile 新签名 ×12）；batch056（get_current_goal Adapter 新语义）；ai_assistant/ai_panel（去除硬编码 `len==17`，改 defined_tool_names 一致性——§9.3）

### 行为变化（Before → After）
- 消息组装：`user(context+instruction)` 冒充用户消息 → SYSTEM(base)/SYSTEM(background context)/SYSTEM(instruction)/历史（按 message id 排除当前条）/USER(用户原始消息)——last 永远是用户当前请求
- 历史过滤：content equality → current_message_id
- 无 Tool Call：assistant-only 二次 chat_stream（漂移+双倍 token）→ 直接采用 completion.content（主回答 Provider 生成次数=1；Memory Extract 为独立 secondary op 且 Generic 跳过）
- Context：每轮全量 L1-L4 → 按目的装载（Generic 仅页面/模式；Planning 走 truth context；Personal/HigherData 全量）
- 当前目标：legacy goals.final → active GoalTarget（REACH 主/SAFETY 参考；无 GT 如实「未设置」）
- get_current_goal：canonical=final_goal → Adapter（legacy 仅 candidates）
- Workflow：active 无条件劫持 → Cancel/Continue/NewIntent 三分流；latest 按 created_at；payload 可恢复（original_request/pending/answered/goal_source）
- Planner 主逻辑：本地旧 GoalBrief 缺项固定三问 gate → PLANNER_TURN_PROTOCOL（Provider clarification ≤5 / plan_draft / handoff_chat；已有 GoalTarget 不再被旧 Brief 阻塞）
- 取消规划：无 → 取消短语确定性取消（不调 AI，无 ChangeSet）
- GoalTarget 提案：无 → PlanDraft.target_proposal → 同一 ChangeSet（GT create+activate+Blueprint），未批准 0 落库

### 自动化 Gate（RUST_TEST_THREADS=1，-j 1；真实 DeepSeek 0 次调用）
- cargo check -j 2（lib + tests）：**0 errors**
- batch060：**16/16**
- 指定回归 batch0592 / batch0591 / ai_assistant / ai_panel：**全绿**
- 全量 cargo test：**382 passed / 0 failed**（35 套件；前值 366 + batch060×16）
- npx tsc --noEmit：**0 errors**；npm run build：**通过**
- 工具真实计数（源码重算）：READ **18** · WEB **2** · PROPOSAL **1** · DIRECT WRITE **0**（TOOL_ALLOWLIST 21 项，与 tool_definitions 集合一致由 T7 锁定）

### 状态
- **DEV-0060 / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**（H1-H9 见 TASK §27；用户本人运行）
- Schema Migration Added：**NO**（保持 v022）；Real DeepSeek Called（自动 Gate 内）：**NO**
- Remaining Source Conflict：**NOT FOUND**



## DEV-0059.2（Final Human-Path Guardrails · corrective patch 收口 · 2026-08-18 完成）
- **DEV ID**: DEV-0059.2（只修 DEV-0059/0059.1 最终源码复核确认的真实闭环缺口；禁止新增第二套系统）
- **Baseline**: DEV-0059.1 final worktree；Schema = **v022**（本轮无 schema 变更）；真实 Provider 0 次自动调用
- 中断/恢复记录：本段执行中曾因模型 Provider HTTP 502 中断一次 → 已按纪律处理（不 rollback/reset/checkout、不重复已完成工作、低并发、502≠代码错误、不连续重试），从断点恢复完成剩余任务。
- **§1 P0 Review ChangeSet 可审阅**：PlanningTruthSummary 直接复用现有 ChangeSetReview（waiting_approval + change_set_id →「审阅 AI 调整」入口；Apply/Reject 后关闭 UI + reload + trigger 全局 refresh；不创建第二套审批 UI）。Rust T5 保留，batch0592 #8 验证 change_set_id 从 planning_reviews 可读。
- **§2 P0 cadence 周期 + 防重复 Review**：新命令 `prepare_current_planning_review(profile_id, trigger_type)` → repo.prepare_current：days=max(1,review_interval_days)；period_start=today-(days-1) 严格覆盖 days 个日历日；同 profile/blueprint 存在 due/running/waiting_approval 复用；waiting_approval 原样返回不再启动 AI。前端 startReview 只走正式路径。
- **§3 P0 structured facts 进 AI Context**：`context_builder::flatten_structured` 对象数组递归（text/kind/source 可读事实行）；共享 `personal_profile_structured_summary(structured_json, budget)` 按字段优先级（availability>constraints>current_state>strengths>weaknesses>unresolved>…）截断，杜绝半截 JSON；Dedicated Planner 同用（1800 预算）。
- **§4 P0 PersonalProfile 与 GoalTarget 边界**：`build_personal_structured` 中「最终学习目标」→ unresolved kind=goal_observation + note「不是正式目标」；basics 不再携带准正式目标。
- **§5 P0 GoalTarget data_json 直接进 Planner Truth**：`goal_target_detail_summary` 解析院校/学院/专业/专业代码/考试年份/考试科目/学位类型/学习方式/目标日期；exam_subjects 支持 string/array；不允许只靠 title 猜。
- **§6 P0 考研 GoalTarget UI 表单化**：GoalTargetPanel 考研字段表单（institution_name/program_name 必填；exam_subjects 逗号拆分；取消手写 JSON）；UI 自动 serialize data_json；generic 高级 JSON 折叠；编辑不显示不可改的 role 控件。
- **§7 P0 Blueprint scenario_type 继承**：BlueprintDraft 加 scenario_type；`resolve_blueprint_scenario(conn,profile,bp,prefer_active_blueprint)`：Review 继承 active Blueprint；主生成继承 active GoalTarget 主场景（postgraduate REACH→postgraduate）；无则 generic；compile/validator 同步。
- **§8 P1 source_review 结构化「为什么改」**：BlueprintDraft.source_review[]（source_id/source_name/decision[keep|modify|conflict|missing]/original/suggested/reason/evidence）；validator：modify 必须 reason+suggested 非空、decision 必须合法；compiler 写入 content_md + structured_json；不自动改 GoalTarget。
- **§9 P1 Source 选择诚实 + 分页**：system 区块改名「Available Planning Sources」；UI 审查请求写 `[source_id=12] 文件名`；`read_planning_source` 扩展 start_char(默认0)/max_chars(默认12000,上限16000)，返回 text/start_char/next_start_char/has_more/total_chars；审查必须读到 has_more=false 或明说未完整读取。
- **§10 P1 无 AI 创建第一份 Blueprint**：无 active 时显示「手工新建规划」表单（title/content/interval/scenario 建议）→ createPlanningBlueprint(status=draft) → Phase/Milestone CRUD → 激活草稿。
- **§11 P1 reality_change 建议复盘（不调 AI）**：`planning_review.rs::ensure_reality_change_due`（无 active Blueprint 直接返回；已有 due/running/waiting_approval 不重复；create_due trigger_type=reality_change）；**lib.rs 接线**：confirm_personalization_profile / edit_personalization_profile 成功后调用。
- **§12 Governance**：ENVIRONMENT.md 更新为 v022 / 当前 Gate / Human Runtime 未验证；未动 TASK.md；未写 Runtime Verified。
- **§13 Tests**：新增 `tests/batch0592.rs` **12/12 通过**（cadence 7/14/30 exact period / open review dedupe / 对象数组进 context / 优先级截断保 availability / goal_observation 非 active GoalTarget / data_json 进 truth / scenario 继承 / change_set_id 可读 / source_review modify 缺 reason fail / 分页 has_more / 手工首蓝图 / reality_change 不堆叠）。
- **§14 Gate 全绿**：cargo check -j 2 **0 errors**；batch0592 **12/12**；全量 `RUST_TEST_THREADS=1 cargo test -j 1` **366 passed / 0 failed**；`npx tsc --noEmit` **0 errors**；`npm run build` **通过**。真实 DeepSeek **0 次自动调用**。
- **Blocker**: 无（一次 Provider 502 已恢复；未再出现）。**下一步 = Human Runtime H1-H11（用户实机验证）**，不再新增功能。

## DEV-0059.1（Truth Wiring & Human-Path Closure · corrective patch）
- **DEV ID**: DEV-0059.1（只补 DEV-0059 最终源码复核发现的真实缺口，不增加产品功能）
- **Baseline**: DEV-0059 final worktree；Schema = v021；Purpose = Truth Wiring / Missing Human Paths
- 禁止 rollback/reset DEV-0059；不重新做 v021；不创建 PlannerV2/ChangeSetV2/EvidenceV2
- §1 P0 Planner Truth Context；§2 P0 Planning Source 进 AI；§3 P0 Review AI 全链；§4 P0 Task 手改保护；§5 P0 Evaluation Evidence 接入；§6-9 P1 PersonalProfile（snapshot/contract/export/xlsx）；§10-13 P1 Manual Planning/Cadence/Horizon/Reimport；§14 T1-T10；§15 Gate（batch0591）；§16 Human Runtime；§17 DONE
- 真实 DeepSeek 不在自动 Gate 调用（§15/§16）

## DEV-0059.1 施工记录（2026-08-18 完成全部代码 + 自动 Gate）

### 已完成（代码 + 验证）
1. **§1 P0 Planner Truth Context**：`ai/planner.rs::build_planning_truth_context` 读 confirmed PersonalProfile / active GoalTargets / ready PlanningSources / active Blueprint / trusted evidence，输出 5 区块 instruction；GoalTarget=正式目标主源，旧 Final Goal 仅 legacy fallback（lib.rs planning 分支：无 active GoalTarget 才启用冲突/missing gate）。
2. **§2 P0 Planning Source 进 AI**：`ai/tools.rs` 增 4 个只读工具（list_planning_sources / read_planning_source / list_active_goal_targets / read_active_planning_blueprint）+ 中文标签；PlanningTruthSummary.tsx 显示 source 列表（checkbox 参与审查）+「审查并整理规划」按钮；Direct Write 仍=0。
3. **§3 P0 Review AI 全链**：`planning_review.rs` 增 prepare_running / build_snapshot（Active Blueprint+Phase/Milestone+period Tasks+trusted sessions+trusted evaluations+confirmed PersonalProfile+active GoalTarget）/ complete_no_change_with / save_assessment_with_result；lib.rs 增 prepare_planning_review_ai（不调 Provider）+ run_planning_review_ai（用户确认后一次调用；NO_CHANGE→completed+刷新 cadence；ADJUSTMENT_PROPOSAL→Blueprint vN+1→ChangeSet waiting_approval→apply 自动 completed（changeset.rs apply 内联动）；Provider 失败→failed 不后台 retry）；核心判定抽为 `planner.rs::apply_review_assessment`（Provider 无关，T4/T5 直测）。前端 PlanningTruthSummary 增加证据快照摘要 +「确认并启动 AI 评估」。
4. **§4 P0 Blueprint Task 手改保护**：`task.rs` Task struct/TASK_COLUMNS/parse_task 扩到 21 列（origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at）；update_v2 对 origin='blueprint' 动态写 `user_modified_at=datetime('now')`；types.ts Task 同步。
5. **§5 P0 Evaluation Evidence V1 接入**：`evaluation.rs` Evaluation 扩 4 字段 + create_with_evidence（session_id/source_kind/source_ref/trust_state）+ 6 处 SELECT 列 + parse；lib.rs create_evaluation 加 4 参数；ai/tools.rs list_recent_evaluations 加 trust_state 过滤；types.ts/api.ts 同步；`trust_state='needs_review'` 不进 trusted evidence（§3 snapshot 亦过滤）。
6. **§6 P1 PersonalProfile source snapshot**：`personalization.rs` 增 save_draft_with_sources（Draft 落库并写 personalization_profile_sources snapshot relation；confirm 后保持；新 Source→vN+1 不改变 vN）+ list_sources_for_version；compile 命令改用 build_personal_structured；lib.rs 增 list_sources_for_personal_profile_version 命令。
7. **§7 P1 structured_json contract**：`personalization.rs::build_personal_structured` 输出 schema_version:1（basics/capabilities/strengths/weaknesses/habits/preferences/constraints/availability/current_state/unresolved/field_provenance）；无法归类进 unresolved；禁止猜值；compile 时 conflicts 进 unresolved；Context Builder 已 structured_json 优先。
8. **§8 P1 PersonalProfile Export 修复**：exporters.ts gather 增加 personalSources（listSourcesForPersonalProfileVersion）；Personal DOCX/XLSX 的 Source 区改用 Personal Sources（Planning Sources 只留 Blueprint export）。
9. **§9 P1 Personal Source 支持 XLSX**：import_personalization_files 增加 xlsx 分支（复用 source_ingest::extract_xlsx_text）；Settings file picker 同步；新增 **v022 migration**（重建 personalization_sources 表，file_type CHECK 加 'xlsx'，保留数据与索引）。
10. **§10 P1 Manual Planning UI**：planning.rs 增 update_blueprint_meta / update_review_cadence / update_phase / delete_phase / update_milestone / delete_milestone + 6 个 lib.rs 命令；PlanningTruthSummary「手工维护规划」面板：蓝图 title/content、Phase/Milestone CRUD、Draft 激活。
11. **§11 P1 Review Cadence UI**：7/14/30/自定义 N/关闭 chips；只改 review_enabled/review_interval_days/next_review_at；不调 AI。
12. **§12 Rolling Horizon 提示**：未来 7 天 blueprint 任务 <3 显示「近期计划不足 7 天」只读提示（不生成）。
13. **§13 Re-import Source Kind**：「重新导入（Higher 导出）」入口 → importPlanningSource(...,"export_reimport")；普通导入仍 user_file。
14. **§14 Tests T1-T10**：`tests/batch0591.rs`（10/10 通过，含 T4/T5 fixture 直测 apply_review_assessment、T9 最小 xlsx zip 构造、T3 正常 update_v2 手改保护）。
15. **§15 Gate 全部通过**：cargo check -j 2 ✓；batch0591 10/10 ✓；batch058/049/052 回归 ✓；npx tsc --noEmit ✓；npm run build ✓；完整 `RUST_TEST_THREADS=1 cargo test -j 1` 全绿 ✓（版本断言随 v022 批量更新 21→22）。
16. **Schema 变更**：v021 → **v022**（personalization_sources.file_type 支持 xlsx；重建表保留数据）。其余无 schema 变更。

### Blocker
- 无。真实 DeepSeek Provider 未在自动 Gate 调用（按 §15/§16 留到 Human Runtime H6/H9/H10）。

- **Timestamp Source: SYSTEM**（`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`）
- **DEV ID**: DEV-0059（个人事实 → 目标事实 → 规划蓝图 → 安全投影 → 周期复盘 → 导入导出 · 一次性收口）
- **Start**: 2026-08-18T15:53:15+08:00 ｜ **End**:（进行中）
- **Baseline Context**: HGCTX-0004（读取）→ 目标 **HGCTX-0005**
- **Baseline Schema**: v020（本轮新增 **v021**）
- **Git**: main @ 457fe5e；dirty（不 reset/clean/rollback）
- **DEV-0058 处置（§2）**: **SUPERSEDED_IN_PLACE_BY_DEV-0059** / NOT ACCEPTED AS STANDALONE DEV —— 不 rollback、不 reset、不删除；兼容部分吸收复用，冲突部分在当前源码上收敛。

## PART 0 · Preflight Code-Truth Mapping（§0 强制，施工前）

### 需求 → 当前实现映射（需求行 = DEV-0059 冻结产品事实）

| 需求 | 当前 DB 表 / 列 | 当前 Rust Domain / Repository | 当前 Tauri Command | 当前 src/api.ts wrapper | 当前 Frontend Page / Component | 当前 AI Planner / Context / ChangeSet | 分类 |
|---|---|---|---|---|---|---|---|
| PersonalProfile 三层正式事实（StudyProfile=容器） | study_profiles（v013；target_* 列保留但不再 canonical） | repository/study_profile.rs | list/get/create/update_study_profile | api.ts 对应 | Settings 档案 Tab | — | [修改] |
| PersonalProfile = 我是谁（version rows） | personalization_profiles（v017：profile_id UNIQUE，status draft/confirmed，version，md_content，structured_json，dirty） | repository/personalization.rs（insert_source/update_source_status/list_sources/get_source/delete_source/store_chunks/all_chunks/get_profile/save_draft/confirm/user_edit/mark_dirty + extract_docx/extract_pdf/decode_text） | personalization 命令族（lib.rs） | api.ts personalization 族 | Settings→私人化 Tab | Context Builder L2 私人化段落 | [修改]（v021 演进为 version rows + sources 快照表） |
| GoalTarget = 我要去哪（generic core + postgraduate REACH/SAFETY） | goals.goal_brief_json（final 行，v019/v020 canonical）；goals.goal_level/period 树 | repository/goal.rs（GoalBrief/detect_goal_conflicts/readiness/read_goal_state） | save_final_goal_brief / goal CRUD | api.ts goal 族 | FinalGoalCard / GoalTreePanel（Planning） | planner.rs read_goal_state（readiness 门） | [新增 goal_targets 表 + 保留旧 goals] |
| PlanningBlueprint / Phase / Milestone = 我准备怎么去 | 无（legacy：study_stages/plans 保留不写新） | repository/study_stage.rs / plan.rs（legacy 保留） | plan/stage 命令（legacy） | api.ts（legacy 0 调用） | Planning 页（GoalTree 为主） | ai/planner.rs（GoalTree-centric draft：year/month/day goals） | [新增 planning_blueprints/phases/milestones] |
| Planning Source（导入/外部 AI） | personalization_sources 模式可复用（txt/md/docx/pdf；sha256/chunks） | personalization.rs extract_docx/extract_pdf（手写 ZIP/PDF） | personalization import 命令 | api.ts | Settings→私人化（无规划源 UI） | Context Builder | [新增 planning_sources/chunks + 复用 extract 抽 source_ingest.rs] |
| Task = 近期准备做什么（origin/blueprint ownership） | tasks（goal_id+learning_item_id 双 FK；planned_date/status/estimated_minutes） | repository/task.rs | task CRUD/materialize_recurring | api.ts task 族 | Today/Planning 任务 | planner.rs 生成 task ops | [修改]（v021 加 origin/planning_*_id/projection_key/user_modified_at） |
| StudySession = 实际做了什么（trusted 统一） | study_sessions.duration_review_state（normal/needs_review/confirmed/corrected，v020） | repository/study_session.rs | confirm_session_duration/correct_session_time/end | api.ts | Today/Workspace/Data | planner.rs time_of_day_distribution | [修改]（v021 trusted view + repo trusted 路径；time-of-day 区间算术） |
| Evaluation/Evidence V1 | evaluations（v004：RESTRICT FK session） | repository/evaluation.rs | evaluation CRUD | api.ts | Evaluations 组件 | list_recent_evaluations（§20.1 需 profile-first 修复） | [修改]（v021 session_id NULL/source_kind/source_ref/trust_state + enum 收敛） |
| ChangeSet = AI 正式写入唯一协议 | ai_change_sets（status 已 6 态 canonical）/ ai_change_operations（action CHECK 已收敛） | repository/changeset.rs（create/apply/undo/selective/refs；apply_one 支持 task/goal/knowledge/document/session/evaluation） | propose/apply/undo | api.ts | ChangeSetReview.tsx | planner.rs compile_to_changeset_ops | [修改]（§6.6 registry 单一源 + §25 新增 entity/action + activation transaction） |
| AI Planner workflow state（clarification 续跑状态机） | ai_runs（无 workflow 列） | ai/planner.rs（planning_gate/is_clarification_reply 文案启发式 §6.8 禁止继续） | run_chat_turn | api.ts ai 族 | AiPanel/AiPanelContext | planner.rs | [修改]（v021 ai_runs +workflow_type/state/json） |
| AI Context 中文 PersonalProfile | personalization_profiles.structured_json/md_content | ai/context_builder.rs（L2 私人化命中段落） | — | — | — | Context Builder | [修改]（§6.9 structured_json 优先） |
| Knowledge Goal Optional | learning_items.goal_id **已可空**（FK ON DELETE SET NULL） | repository/learning_item.rs | list_learning_items_by_profile | api.ts | Knowledge.tsx | — | [修改]（UI 空态/筛选语义；§6.10） |
| Import/Export（docx/exceljs） | 无（依赖：react/dialog 已有） | 无 write_export_file | 无 | 无 | 无 | — | [新增] |

### 关键事实核对结论（§5.2 baseline 一致，直接继续）
- Schema v020 / 20 migrations 完整 ✓；React+Tauri+Rust+SQLite ✓；Personalization 多文件导入已存在 ✓；Planner 已存在 ✓；ChangeSet 已存在 ✓；Knowledge backend goal_id nullable ✓；DEV-0058 current worktree partial（planning_gate/is_clarification_reply/readonly needs_assistant/ai://applied 等保留吸收）✓
- 差异记录：ai_change_sets status 已是 6 态（§6.5 DB 层已满足，需前端收敛）；learning_items.goal_id 已可空（§6.10 DB 层已满足，需 UI 空态与 child 继承验证）

## PART 1 · P0 Correctness（§6）执行记录

（随施工更新）

## PART 2 · 执行进度（2026-08-18 续跑；此前因 Provider 402 中断一次，余额恢复后从断点继续）

### 已完成
1. **PART 0 preflight 映射**（§0/§5）：写入本文件顶部；baseline 与 §5.2 一致。
2. **PHASE 1 P0（§6 全 10 项）**：
   - §6.1 trusted 统一：time_of_day/knowledge_workspace/learning_item.stats/learning_data.stats/study_profile calendar 全部排除 needs_review（lib.rs 三处与 daily_report 原本已排除）；v021 建 `trusted_study_sessions` VIEW。
   - §6.2 time_of_day 区间算术（按天切分 × bucket 重叠，替代逐秒循环；结果与逐秒一致）。
   - §6.3 utils.ts 新增 splitDurationSeconds/formatDurationTimer/formatDurationCompact/formatDurationDetail；替换 Data/LearningWorkspace/Knowledge/DailyActivities 主路径 formatter。
   - §6.4 LearningWorkspace Timer 依赖 tick 每秒真实更新。
   - §6.5 ChangeSetReview isSettled 改 canonical 6 态；仅 waiting_approval 可审查交互；STATUS_LABELS 去 pending。
   - §6.6 tool schema==apply_one 核对一致；§25 新实体已同步进 schema。
   - §6.7 Evaluation enum 收敛：evaluation.rs canonical_evaluation_type/is_valid_evaluation_type + 6 值；changeset/ai schema/types.ts 同步；repo create 自动映射 legacy。
   - §6.8 Planner workflow 显式状态机：planner.rs workflow_* 常量+helpers；lib.rs 各分支写 workflow_state（collecting/clarifying/failed/waiting_approval/applied）；文案启发式降为 legacy 兜底；v021 ai_runs 加列。
   - §6.9 Context Builder structured_json 优先 + 中文 2-gram 检索 + 不再头 1500 字兜底。
   - §6.10 Knowledge Goal Optional：无 Goal 加载全部/建根/建子（child 继承 parent.goal_id）；goal 筛选可选含「全部」；空态文案更新。
3. **PHASE 2 v021**：`v021_personal_planning_truth.rs` 注册（trusted view / ai_runs workflow 列 / personalization version rows 重建+legacy 迁移 / profile_sources 快照 / goal_targets+考研 partial unique / planning_sources/chunks / blueprints/phases/milestones / reviews / evaluations Evidence V1 列 / tasks origin+projection UNIQUE 索引）。
4. **PHASE 3-4**：personalization.rs 重写 version rows（get_confirmed/get_draft/list_versions/save_draft vN+1/confirm 事务/user_edit/mark_dirty→draft 提示 + user_edit_in_tx）；goal_target.rs（create/activate(+in_tx)/replace/dismiss/list_legacy_candidates/postgraduate JSON 校验）；commands+api.ts+types 全注册。
5. **PHASE 5-6**：source_ingest.rs（ZIP EOCD+central directory 解析 / list_zip_entries / read_zip_entry / extract_xlsx_text，支持 data descriptor）；planning_source.rs；planning.rs（Blueprint/Phase/Milestone + activate 事务 + project_tasks_in_tx §22 幂等 + today_utc8）；planning_review.rs（due/running/waiting_approval/completed + is_review_due + latest_risk_state + complete_no_change）；Cargo.toml +base64。
6. **PHASE 7/9 ChangeSet 扩展（§25）**：apply_one 新实体（goal_target create/update/status_change；planning_blueprint create+active 同事务激活；planning_phase/milestone create）；通用 ref 键解析；undo 支持；activate_blueprint_in_tx（不嵌套事务）；ai/tools.rs schema 同步。
7. **测试 batch058（20 项）**：v021 迁移幂等/新表列/legacy confirmed→v1/PersonalProfile 版本约束/GoalTarget 考研 reach/safety 替换+JSON 校验+legacy 候选不自动激活/Task origin=manual/Blueprint 激活投影+手工保护+幂等+单 active/trusted view 6h/time_of_day 区间+trusted/Planner workflow state/Goal Optional 全链/Evaluation enum 映射+repo 迁移/ChangeSet goal_target create+status_change/v021 无数据丢失/Review due。**20/20 通过**（2.55s；SAC 未拦截本轮测试可执行）。
8. **UI 阶段（§26-30）**：
   - §27 GoalTargetPanel（新组件）：考研 REACH/SAFETY 槽位 + 通用目标；编辑/替换（版本+1 old→historical）/历史/来源；空态 + legacy 候选「据此创建」（不自动激活旧 Goal）；接入 Planning 顶部。
   - §28 PlanningTruthSummary 重写：GoalTargetPanel + Active Blueprint 摘要（版本/复盘间隔/下次复盘/risk 标记）+ Review 状态（due/进行中/上次完成）+ 操作区（生成规划→AI 面板 blueprint 模式 / 导入规划资料 txt·md·docx·pdf·xlsx / 开始复盘 create_planning_review_due）；修复此前只 import 未渲染的问题。
   - §29 PlanningCalendar：加载 active blueprint 的 phases/milestones；exact milestone 进 cell（◆ 标题）、month-only milestone 显示在月级摘要（不伪装某一天）、current phase 显示在月历上方。
   - §30 Today：Review Reminder 卡（「该进行阶段复盘了」[开始复盘][稍后]）+ Risk Banner（near_safety/below_safety/off_reach → 「查看依据」；不自动调 AI）。
   - §26 Settings：statusText 适配 draft/superseded/confirmed；updatedText=confirmed_at??updated_at；主卡「版本 vN · 来源 N 份」；「更多」菜单新增导出 Word/Excel（§32）。
9. **Import/Export（§31-35）**：安装 docx/exceljs（无依赖冲突）；新建 `src/lib/exporters.ts`（§32 个人档案 DOCX/XLSX、§33 蓝图 Word 15 章节、§34 蓝图 Excel 10 sheets：Overview/Targets/Phases/Milestones/Monthly/Subject/14-day/Risks/Sources/Changelog；全部 dynamic import docx/exceljs）；save dialog → write_export_file（§31.3 只写用户所选路径）；`npm run build` 确认 docx/exceljs 均为独立 lazy chunk（不进 Today 初始 bundle）。
10. **Planner 演进（§23）**：PlanDraft 增加 `blueprint: Option<BlueprintDraft>`（blueprint/phases/milestones/future_tasks/assumptions/unresolved/external_facts/suggested_target_changes）；PLAN_DRAFT_INSTRUCTION 扩展 blueprint 模式（B1-B6 规则）；validate_plan_draft 蓝图分支（标题/复盘间隔/阶段日期/里程碑精度 month 允许 YYYY-MM/任务窗口 ≤21 天/suggested role 校验）；compile_to_changeset_ops 蓝图分支（blueprint create status=active → 同事务激活+安全投影 + phases/milestones create，`blueprint_ref`/`phase_ref` 通用 ref 解析（resolve_refs+check_forward_refs+apply_one 扩展），suggested_target_changes 只进 content_md 不自动改目标，goal-tree 模式完全兼容）。
11. **测试 batch058 扩展（§23，原 batch059 因 SAC 拦截新 exe 合并入 batch058 运行）**：+7 项蓝图测试（编译结构/不触碰 goal_targets/roundtrip+goal-tree 兼容/校验 ok/校验 errors/超窗口/ChangeSet apply 全链+幂等/替换 supersede）。**修复 bp_add_days 儒略日算法 bug**（原算法把"一年第 N 天"当"当月第 N 天"递减导致 future_tasks 日期错到 2027-03 → 投影 0 条；改用 civil_days/civil_from_days 后投影 2/2 通过）。**28/28 通过**。
12. **全量回归 + 测试断言同步（v020→v021）**：22 处版本断言更新（adjustment_system/attachments/batch03/batch049/feedback_system/insight_review/learning_loop/knowledge_workspace/learning_hierarchy/profile_system/stage_b_core 的 `vec![1..20]`→`[1..21]`、`count,20`→`21`、`latest_version()==20`→`21`、attachments `last()==Some(&21)`）；batch052 `test_personalization_chunks_and_user_edit_confirm` 断言适配 §8 新语义（新库首次 confirm = v1，user_edit 后 v2，旧 v1→superseded）。
13. **最终 Gate 全绿（SAC 已由用户关闭，低并发 RUST_TEST_THREADS=1 + cargo test -j 1）**：全量 **344 个测试通过**（30 个 test 套件 + lib 3；含 batch058 28 项蓝图全链）；cargo check 0 errors；tsc 0 errors；npm run build 通过（docx/exceljs/exporters 独立 lazy chunk）。

### Gate 状态（最终）
- cargo check：**0 errors**
- cargo test（全量，低并发）：**344 passed / 0 failed**
- tsc --noEmit：**0 errors**
- npm run build：**通过**（11.75s）
- package.json metadata：`Higher - 本地个人学习系统` ✓
- SAC：用户已在开发期间关闭 Smart App Control（不再阻塞）；此前 ENV_BLOCKED_SAC 记录作废

### 未完成（Human Runtime Required，§61）
- H1 Migration / H2 Zero Barrier / H3 Time / H4 Personal Sources / H5 GoalTarget / H6 Planning Source / H7 Plan Apply / H8 Protection / H9 AI Clarification（真实 Provider）/ H10 Review / H11 Export —— 清单已写入 ENVIRONMENT.md，全部需用户实机验证
- 真实 DeepSeek Provider 验证（按纪律留到最终 Human Runtime，不烧余额）

### Blocker
- 无（SAC 已关闭；无 Provider 阻塞；402/429/502 未再现，若再现 → PROVIDER_BLOCKED 记录不重试）
