# DEV-MOBILE-004-F2 · AI Root Tab Layout 报告

- 日期：2026-08-28
- 工作树：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`）
- 前置：F1 artifact parity 已真机验证通过（本轮不再排查 APK/dist/assets/.so/签名）
- 本轮结论：

```
UI_RUNTIME:      PASS（F2-TC001~007 行为+级联测试全绿；Rust 契约 23 项全绿）
ARTIFACT_PARITY: 10/10 PASS
REAL_DEVICE:     PENDING（等待真机截图验收）
```

按指令 STOP，不自行宣布 REAL_DEVICE PASS。

---

## 一、取证（§一 · 真正命中的生产路径）

点击 BottomNav AI 后的实际渲染链（逐层读取源码，非 marker 检索）：

```
NavLink to="/ai"（MobileLayout.tsx BottomNav）
  → HashRouter 路由 /ai（App.tsx:101，MobileLayout 的子路由，MobileAiPage = null 占位）
  → MobileLayout onAiRoute=true（location.pathname.startsWith("/ai")）
  → main.mobile-main.mobile-main--ai 内常驻 .ai-slot.ai-slot--visible
  → <AiPanel presentation="mobile">（MobileLayout.tsx:165-171）
```

§一逐项回答：

| 检查项 | 结论 |
|---|---|
| Portal / Overlay / Dialog / fixed host | **不存在**（AiPanel.tsx:912 直接 `<aside class="aipanel aipanel--mobile">`，无 createPortal） |
| AI 的 position:fixed / inset:0 / 100vh / 100dvh | **存在，但在 styles.css 桌面响应式规则里命中手机**（见下） |
| activeTab==='ai' 提前 return 独立 AiPanel | 不存在 |
| showAi / openAi / aiOpen 旧桌面状态 | 不存在（AiPanel 仅 `collapsed && !isMobile` 桌面 rail 分支，mobile 不触发，AiPanel.tsx:893） |
| BottomNav 是否仍在 DOM | **在**。被浮层盖住（`.mobile-bottomnav` static / z-index auto < 浮层 z-index） |
| z-index 是否导致 AiPanel 覆盖 BottomNav | **是**（浮层 z-index 80/200 > BottomNav auto；TopBar z300 在其上仍可见） |

### 元凶（CSS 级联，非 JSX 结构）

```css
/* styles.css:5549 —— 桌面「AI overlay Drawer」，max-width 覆盖全部手机宽度 */
@media (max-width: 1279px) {
  .aipanel {
    position: fixed;      /* ← 手机上命中：viewport 全屏浮层 */
    right: 0; top: 0; bottom: 0;
    width: min(360px, 92vw);
    z-index: 80;
    box-shadow: -10px 0 28px rgba(0,0,0,.5);
    transition: transform .15s ease;   /* ← 右侧滑入的"独立抽屉"观感 */
  }
}
/* styles.css:963 —— 第二条（更早声明，被上条覆盖为死规则） */
@media (max-width: 1100px) { .aipanel { position:absolute; top:0; right:0; bottom:0; z-index:200; … } }
```

`mobile.css` 的 `.platform-android .aipanel--mobile` 只覆盖了 `width/min-width/max-width/height/border/background`，**漏掉 `position/inset/z-index/box-shadow`**。`.ai-slot`/`.mobile-main`/`.mobile-layout` 均无 position → `.aipanel` 的 containing block = viewport。

真机现象逐一对应：

| 真机现象 | 机理 |
|---|---|
| AI 变全屏独立页 | `position:fixed; top:0; bottom:0` + width 被 mobile 覆盖为 100% → viewport 全屏，transition 带右侧滑入观感 |
| BottomNav"完全消失" | 仍在 DOM，被 `z-index:80` 浮层盖住（BottomNav static/z auto） |
| 截图"知识体系"侵入状态栏 | AI 内容从 viewport `top:0` 开始，越过 TopBar 下方；`.aipanel__mobile-subtitle` 显示来源 pageContext label——从 Knowledge 进入时恰为「知识体系」（F1-C 保留来源上下文设计），被顶入状态栏区域。非 Knowledge 页面穿透（/ai 时 Outlet 为 null，Knowledge 已卸载） |

## 二、修复（§二/§三/§四 · 最小改动，只动 mobile.css 一处）

[mobile.css](file:///c:/Users/37653/Desktop/Higher/Higher-Android/src/mobile/mobile.css) `.platform-android .aipanel--mobile` 补齐覆盖（特异性 20 > 桌面规则 10）：

```css
position: static;   /* 回到 .ai-slot--visible 的 flex 文档流 */
inset: auto;        /* 清 top/right/bottom（不再锚定 viewport） */
z-index: auto;      /* 取消 80/200 浮层层级 */
box-shadow: none;   /* 去掉浮层投影 */
```

修复后布局链（与 §二/§三 目标结构一致——AI 本就是 MobileLayout main 内的第五个 root tab，仅 CSS 被劫持）：

```
.mobile-layout（flex column · height:100% · padding-top:env(safe-area-inset-top)）
├─ header.mobile-topbar（Higher · AI · 2028考研）
├─ main.mobile-main--ai（flex column · overflow:hidden）
│   └─ .ai-slot--visible（flex:1 · min-height:0）
│       └─ aside.aipanel--mobile（static · 100%×100% · flex column · overflow:hidden）
│           ├─ .aipanel__mobile-subtitle（@知识体系，文档流内，TopBar 之下）
│           ├─ header/消息区（flex:1 · min-height:0 · 自滚）
│           └─ .aipanel__inputbar（composer，main 内部底部）
└─ nav.mobile-bottomnav（五项常驻；padding-bottom:max(env(safe-area-inset-bottom),8px)）
```

- §四 safe-area：零新实现——AI 复用 `.mobile-layout` 根统一 `env(safe-area-inset-top)` 与 BottomNav `max(env(safe-area-inset-bottom),8px)`。
- **桌面 Windows 零改动**：styles.css 两条 @media 原样保留（TC003r 锁定裸 `.aipanel` 窄屏仍 fixed Drawer）。
- 未触碰：AI Agent/Runtime、Database、Planner、Memory、HigherAction、Windows UI、签名、package id、版本。

## 三、测试（§七 · 真实布局行为，非字符串 marker）

新增两层测试 + 既有契约回归，全部 PASS：

**行为层**（[tests/mobile/f2AiRootTabLayout.test.tsx](file:///c:/Users/37653/Desktop/Higher/Higher-Android/tests/mobile/f2AiRootTabLayout.test.tsx)，jsdom + node:test + mock.module + tsx，真实渲染 MobileLayout 路由树，6/6）：

| TC | 断言（真实 DOM 行为） | 结果 |
|---|---|---|
| F2-TC001 | /ai 时 `nav.mobile-bottomnav` 在 DOM；`.aipanel--mobile` 是 `.mobile-main .ai-slot--visible` 子节点（非 body 直挂浮层）；AiPanel 不包含 BottomNav | PASS |
| F2-TC002 | BottomNav 恰五项（今日/规划/知识/AI/我的），恰一个 active 且为 AI | PASS |
| F2-TC003b | AiPanel 祖先链 = mobile-layout → mobile-main → ai-slot（文档流） | PASS |
| F2-TC004 | Knowledge→AI→Planning 点击切换，`.mobile-bottomnav` **同一 DOM 节点**全程不卸载（node identity + isConnected） | PASS |
| F2-TC005 | composer 在 `.mobile-main` 内、BottomNav 是 `.mobile-layout` 末元素且 `compareDocumentPosition` 在 composer 之后（文档流保证 bottom ≤ nav top） | PASS |
| F2-TC007 | /ai 时知识/规划页内容不在 DOM；「知识体系」仅存在于 AI subtitle 且位于 main 文档流内 | PASS |

**级联层**（[tests/mobile/f2AiCascade.test.ts](file:///c:/Users/37653/Desktop/Higher/Higher-Android/tests/mobile/f2AiCascade.test.ts)，解析真实 styles.css+mobile.css，按 @media 匹配→特异性→顺序计算 winning declaration，6 条）：

| TC | 断言（computed style） | 结果 |
|---|---|---|
| F2-TC003 | 360/390/412/430 宽 `.aipanel--mobile` computed `position=static`、`z-index=auto`、`box-shadow=none`、top/right/bottom=auto、width/height=100%、无 100vh/100dvh | PASS |
| F2-TC003r | 裸 `.aipanel`（无 --mobile）在 360-1200 宽仍 `position=fixed` —— 桌面 Drawer 规则原样保留（Windows 行为不变） | PASS |
| 高度链 | `.ai-slot--visible`{flex:1,min-height:0}；`.mobile-main--ai`{flex column,overflow:hidden} | PASS |
| F2-TC005c | BottomNav/composer/subtitle 均无 fixed/absolute/top/bottom 锚点；BottomNav 保留 env(safe-area-inset-bottom) | PASS |
| F2-TC006 | 四分辨率：`.mobile-main` overflow-x=clip、AI width=100%/min-width=0、input/img max-width=100%（不横向溢出契约） | PASS |
| §四 safe-area | `.mobile-layout` padding-top=env(safe-area-inset-top)、padding-bottom=0（根统一） | PASS |

**Rust 契约回归**：`android_mobile_shell_tests` 6 + `android_governance_tests` 6 + `mobile_tc_contract_tests` 11 = 23 全绿（其中 TC001 断言按 F1 meta 多字段扩展同步更新，并新增 sourceFingerprint/mobileShellRevision 契约）。

测试设施新增（devDependencies）：`jsdom` + `tsx` + `@types/jsdom`；命令 `npm run test:mobile`（17/17）与 `npm run test:mobile-f2`（6/6）。

## 四、构建（§八）

`Build-Higher-Android.ps1 -Configuration Release -ArtifactSuffix ai-tab-fix`（新增 `-ArtifactSuffix` 参数，正式包路径不变）：

```
SOURCE_FINGERPRINT       PASS  a658bf93a3026baad3edcdd23657527dceb943b7fe3905431001b79300e591db
ANDROID_PLATFORM         PASS  platform=android + compile target=android
DIST_FRESH_BUILD         PASS  12:41:59 删 dist/ 后重建
DIST_ASSETS_PARITY       PASS  dist → gen assets 逐文件 SHA-256 全等
APK_ASSETS_PARITY        PASS  APK assets vs dist 逐文件 SHA-256 全等
MOBILE_SHELL_META        PASS  APK 内 meta == dist meta（mobile-ai-bottomnav-v1）
PACKAGE_IDENTITY         PASS  com.higher.android / 0.1.0 (1000)
UNIVERSAL_ABI            PASS  arm64-v8a + armeabi-v7a + x86_64
SIGNING                  PASS  v2 · 证书 0c864d48…d33c02（升级链根不变）
LOCALHOST_SCAN           PASS  三 ABI .so 无 devUrl
```

## 五、产物与真机验收

```
release/android/
  Higher-v0.1.0-ai-tab-fix.apk           65.1 MB  ← 本轮验证包
  Higher-v0.1.0-ai-tab-fix.sha256.txt
  Higher-v0.1.0-ai-tab-fix-size-report.txt
  Higher-v0.1.0-parity-test.apk          （F1 验证包，保留）
  Higher-v0.1.0.apk                      （正式包未动，验收通过后替换）
```

验收清单（对应 §五 期望视觉）：

1. BottomNav 点 AI → 页面结构：TopBar（Higher · AI · 2028考研）→ AI 内容（subtitle @知识体系/对话历史）→ composer（[问点什么…][发送] AI DeepSeek）→ **BottomNav 今日/规划/知识/AI/我的常驻**；
2. 进入 AI 后底部五栏仍在、可点；
3. Knowledge → AI → Planning 来回切换，BottomNav 不闪不重建；
4. 「知识体系」等文字不再侵入系统状态栏区域；
5. 360-430 宽度机型无横向滚动。

## 六、遗留

- `REAL_DEVICE: PENDING`——等待用户真机截图验收；通过后再以同链路生成正式 `Higher-v0.1.0.apk`。
- styles.css 1100px 断点的 `.aipanel{position:absolute}` 为死规则（被 1279px fixed 覆盖）——按禁改 Windows UI 未动，仅记录。
