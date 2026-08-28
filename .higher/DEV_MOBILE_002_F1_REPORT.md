# DEV-MOBILE-002 F1 · AI Root Navigation + Android Safe Area 报告

日期：2026-08-28 · android/dev · Higher-Windows 零修改

## A · AI 一级页面化
- [MobileLayout.tsx](file:///c:/Users/37653/Desktop/Higher/Higher-Android/src/mobile/MobileLayout.tsx)：移除 fixed `.mobile-ai-host` Overlay；AiPanel 常驻渲染于 `.mobile-main` 内 `.ai-slot`（非 /ai 隐藏但 mounted——Runtime/pendingSend 事件链不丢）；/ai 时 `mobile-main--ai`（overflow hidden + flex column）
- BottomNav 在 AI 页常驻可见；AI↔今日/规划/知识/我的 直接互切

## B · 移除一级返回箭头
- [AiPanel.tsx](file:///c:/Users/37653/Desktop/Higher/Higher-Android/src/components/ai/AiPanel.tsx)：删除 `aipanel__mobile-backrow`；Header 保留 历史/新对话/收起（mobile 无收起）+ subtitle=当前上下文；二级层（History）自带 ✕ 关闭

## C · 来源上下文保留
- MobileLayout 上下文 effect：`/ai` 路由不重置 pageContext（今日→AI 仍 @今日任务；规划→AI 保留 @学习规划）

## D/E · Safe Area 根统一
- [index.html](file:///c:/Users/37653/Desktop/Higher/Higher-Android/index.html)：`viewport-fit=cover` + theme-color
- `.mobile-layout` 唯一承担 `padding-top: env(safe-area-inset-top)`
- `.mobile-bottomnav { padding-bottom: max(env(safe-area-inset-bottom), 8px) }`（手势条与导航本体分离）
- 清理 styles.css 三处重复 env（topbar 高度/padding、bottomnav 旧 padding、composer safe-bottom）——无 double padding

## F · 无 Native Bridge
未触碰 MainActivity.kt（真机若证明 OPPO WebView inset 错误再议）

## G · AI 高度链
`mobile-main--ai(flex)` → `ai-slot--visible(flex:1; min-height:0)` → `aipanel--mobile(height:100%)`：Header/Messages(flex:1)/Composer 在 BottomNav 之上；键盘弹起 100dvh 收缩 Composer 保持可见

## 测试
- `test:mobile` 11/0 · `test:ai-runtime` 13/0 · tsc 0 错 · `npm run build` OK
- Rust 契约 33/33：shell 6（ui_tc005/007 重写为 F1 契约：无 fixed host、无 backrow、subtitle、safe 根统一、bottomnav max()）· governance 6 · artifact 5 · startup 5 · tc_contract 11

## Android Build
唯一入口全 Gate 通过（dist=android · compile=android · 无 localhost:1420）
**APK**：`src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk` · 452.0MB

## Windows
0 UI / AI Runtime 行为变化（AiPanel 仅删 isMobile 分支代码；styles.css 仅清理 mobile 专属旧规则）

```
AUTOMATION: PASS
REAL_DEVICE: PENDING
OVERALL: HOLD
```
