# DEV-MOBILE-002 · Mature Mobile Experience 完成报告

## 1. Baseline
- Android worktree：`android/dev` @ 733e8ba（施工产物未提交，保持工作树）
- Windows worktree：`main` @ 733e8ba，**End = 仅原有 `.higher/TASK.md` dirty，零源码新增修改**（与 Baseline 一致）

## 2. 修改文件（shared，Android Guard 分支，Windows 语义零回归）
- `src/pages/Today.tsx`：Android 顶部只留 [快速学习][+ 新建任务]；AI安排 降为底部次级；Card header 小 ＋ icon
- `src/pages/Planning.tsx`：Presentation ViewModel 节点常量化（原 JSX 机械搬移）+ `IS_ANDROID` 早返回 MobilePlanningView；桌面 JSX 改引同一节点（内容/handler/class 顺序一致）
- `src/pages/Knowledge.tsx`：`selectNode` Android 分支自动关 Drawer（§27）
- `src/components/ai/AiPanel.tsx`：mobile backrow（‹ 返回，§33-34 优先级实现）+ placeholder 去 Shift+Enter 提示（§37）
- `src/pages/Settings.tsx`：可选 `presentation="mobile-section"` 隐藏桌面 header/9-tab（默认 desktop 不变）
- `src/main.tsx`：引入 mobile.css
- `package.json` / `vite.config.ts`（无新改动；上一阶段产物）

## 3. 新增文件（Android-only）
- `src/mobile/mobile.css`（Design Tokens §5 + 全局硬规则 §6 + 六页域 CSS）
- `src/mobile/components/MobileUI.tsx`（PageHeader/Segmented/BottomSheet/ActionSheet/Empty/IconButton，纯 Presentation）
- `src/mobile/pages/MobilePlanningView.tsx`（计划/日历/目标 3 Tab + ⋯ Sheet 内嵌 PlanningTruthSummary）
- `src/mobile/pages/mobilePlanningState.ts`（MOB-TC001-004 纯状态机）
- `src/mobile/mobileBack.ts`（§49 Back 优先级纯函数，MOB-TC005-009）
- `src/mobile/mobileNavigation.ts`（导航唯一数据源，MOB-TC010）
- `tests/mobile/*.test.ts` ×3 + `tsconfig.mobile-test.json`（`npm run test:mobile`）
- `src-tauri/tests/android_governance_tests.rs`（§79-80 Source/CSS Governance）

## 4. 每页策略
| 页 | 策略 |
|---|---|
| Today | controller 单套；Android 条件块（§14 允许路线）；AI安排 不并列 |
| Planning | MobilePlanningView 强制独立（§60）；日期详情=BottomSheet；低频管理入 ⋯ |
| Knowledge | controller 保留 + Android 显式 CSS（tree 默认 none，仅 Drawer）+ 选后自动关 |
| AI | 单一 Runtime；mobile backrow + 全屏 History Drawer（CSS）+ Composer 手机化 |
| Settings | mobile-section presentation；‹ 返回 + section 标题 |
| Learning | CSS 专注态（sticky 结束、editor 全宽）——布局级改造留待真机证据 |

## 5. 测试
- `test:mobile` **11 pass / 0 fail**（MOB-TC001~010）
- Rust：governance 6/6 · shell 6/6 · artifact 5/5 · startup 5/5 · tc_contract 11/11
- Windows Gate：`tsc` 0 错 · `npm run build` OK（meta=desktop）· `cargo check --all-targets` OK · `test:ai-runtime` 13/0
- （`cargo test --no-fail-fast` 全量未在本阶段重跑——改动均经上述定向套件覆盖；建议真机验收前补跑）

## 6. Android Build（唯一入口）
- `scripts\Build-Higher-Android.ps1`：dist=android ✓ · compile target=android（`Dv="android"`）✓ · .so 无 localhost:1420 ✓ · aapt2 `com.higher.android.debug` ✓ · 图标同步 ✓
- APK：`src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk` · 452.0MB

## 7. 未完成 / 已知风险
- Learning Workspace 仅 CSS 级（§43-46 结构级待真机截图证据）
- Android 系统 Back 全局接管未做（§50：先依赖 Web history；真机若直接退出 App 再议 MainActivity bridge）
- visualViewport 键盘适配未加（§38：先 CSS 100dvh，真机证据后再加）
- Planning Week 视图 Android 未入一级（§24 允许：月为默认，桌面周视图未删）

## 8. 真机待验收（§86-87）
18 项截图 + 17 项操作清单（任务书 §86/§87）

```
AUTOMATION: PASS
REAL_DEVICE: PENDING
OVERALL: HOLD
```
