import { Suspense, lazy } from "react";
import { HashRouter, Navigate, Route, Routes, useSearchParams } from "react-router-dom";
import { ActiveProfileProvider, useActiveProfile } from "./contexts/ActiveProfileContext";
import { AiPanelProvider } from "./components/ai/AiPanelContext";
import WallpaperLayers from "./components/WallpaperLayers";
import DesktopTitlebar from "./components/DesktopTitlebar";
import Layout from "./Layout";
// DEV-MOBILE-001 §62/§64：平台 Shell Split（TAURI_ENV_PLATFORM 注入，浏览器 dev → desktop）
import { IS_ANDROID } from "./platform/runtimePlatform";
import MobileLayout, { MobileAiPage } from "./mobile/MobileLayout";
import MobileSettings from "./mobile/MobileSettings";
import Evaluations from "./pages/Evaluations";
import Goals from "./pages/Goals";
import History from "./pages/History";
import Planning from "./pages/Planning";
import ProfileSelector from "./pages/ProfileSelector";
import ProfileWelcome from "./pages/ProfileWelcome";
import Settings from "./pages/Settings";
import Sync from "./pages/Sync"; // DEV-SYNC-002 §十：/sync 同步工作台（缺此 import → ProfileGate ReferenceError 白屏）
import Tasks from "./pages/Tasks";
import Today from "./pages/Today";
import { todayDate } from "./utils";

/** 旧 /items 链接兼容：重定向到 /knowledge（保留 ?goal= 参数）。 */
function LegacyItemsRedirect() {
  const [searchParams] = useSearchParams();
  const goal = searchParams.get("goal");
  return (
    <Navigate to={goal ? `/knowledge?goal=${goal}` : "/knowledge"} replace />
  );
}

/** DEV-0041：学习复盘并入学习规划。/review → /planning?date=YYYY-MM-DD（无则今天）。 */
function ReviewRedirect() {
  const [searchParams] = useSearchParams();
  const date = searchParams.get("date") || todayDate();
  return <Navigate to={`/planning?date=${date}`} replace />;
}

/** DEV-0055 §125：/data route-level lazy（recharts 不进主 bundle） */
const DataPage = lazy(() => import("./pages/Data"));

/** DEV-0057 PART Z：Knowledge（Tiptap/知识图重页）与 LearningWorkspace（编辑器）route-level lazy */
const KnowledgePage = lazy(() => import("./pages/Knowledge"));
const LearningWorkspacePage = lazy(() => import("./pages/LearningWorkspace"));

/** Profile Gate：根据档案状态决定显示欢迎页/选择页/主应用 */
function ProfileGate() {
  const { gate, retryBoot } = useActiveProfile();

  if (gate.phase === "loading") {
    return (
      <div className="profile-gate">
        <p className="profile-gate__loading">加载中...</p>
      </div>
    );
  }

  // DEV-MOBILE-001 F1 §八：初始化失败（有限重试后）——错误页 + 重新尝试，
  // 不再伪装成 no_profiles，也不允许无限「加载中...」。
  if (gate.phase === "error") {
    return (
      <div className="profile-gate profile-gate--error">
        <p className="profile-gate__loading">Higher 启动时遇到问题</p>
        <button className="btn btn--primary profile-gate__retry" onClick={retryBoot}>
          重新尝试
        </button>
      </div>
    );
  }

  if (gate.phase === "no_profiles") {
    return <ProfileWelcome />;
  }

  if (gate.phase === "select") {
    return <ProfileSelector />;
  }

  // phase === "active"：进入 V2 主应用
  // 一级入口（DEV-0055 最终收敛）：/ 今日 · /planning 规划 · /knowledge 知识 · /data 数据
  // 「学习复盘」并入学习规划（/review → /planning?date=…）；「整体进度」并入学习规划
  // （/progress 兼容重定向）。旧实体页（goals/tasks/evaluations/history/items）
  // 保留为内部兼容路由，不在主导航展示。
  return (
    <HashRouter>
      <AiPanelProvider>
        <Suspense fallback={<div className="profile-gate"><p className="profile-gate__loading">加载中…</p></div>}>
          <Routes>
            {/* DEV-MOBILE-001 §64/§170：按平台只挂载一个 Shell（Windows=Layout，Android=MobileLayout） */}
            <Route element={IS_ANDROID ? <MobileLayout /> : <Layout />}>
              <Route path="/" element={<Today />} />
              <Route path="/planning" element={<Planning />} />
              {/* DEV-0041：学习复盘并入学习规划，兼容重定向 */}
              <Route path="/review" element={<ReviewRedirect />} />
              <Route path="/knowledge" element={<KnowledgePage />} />
              {/* DEV-0055：学习数据一级页面（lazy §125） */}
              <Route path="/data" element={<DataPage />} />
              {/* DEV-SYNC-002 §十：Windows 同步工作台一级入口（Android 走「我的 → 设备同步」） */}
              {!IS_ANDROID && <Route path="/sync" element={<Sync />} />}
              {/* DEV-0301：整体进度并入学习规划，兼容重定向 */}
              <Route path="/progress" element={<Navigate to="/planning" replace />} />
              {/* DEV-MOBILE-001 §68：Android AI 一级导航（内容=恒驻 MobileAiHost 全屏） */}
              {IS_ANDROID && <Route path="/ai" element={<MobileAiPage />} />}
              {/* Learning Workspace（DEV-0017：开始学习进入正式学习工作区；DEV-0057 lazy） */}
              <Route path="/learn/:sessionId" element={<LearningWorkspacePage />} />
              {/* 设置（DEV-0016：非学习业务模块；Android=「我的」列表入口，F1 §十七） */}
              <Route path="/settings" element={IS_ANDROID ? <MobileSettings /> : <Settings />} />
              {/* 内部兼容 / 技术调试路由 */}
              <Route path="/goals" element={<Goals />} />
              <Route path="/tasks" element={<Tasks />} />
              <Route path="/evaluations" element={<Evaluations />} />
              <Route path="/history" element={<History />} />
              <Route path="/items" element={<LegacyItemsRedirect />} />
            </Route>
          </Routes>
        </Suspense>
      </AiPanelProvider>
    </HashRouter>
  );
}

function App() {
  return (
    <ActiveProfileProvider>
      {/* DEV-0064 §10/§40：壁纸/遮罩独立图层（fixed + pointer-events:none）+ 启动恢复 */}
      <WallpaperLayers />
      {/* DEV-0065.1 §10：自定义桌面标题栏（恒渲染于全部 ProfileGate 阶段；
          背景 var(--h-sidebar) 透出同一全局壁纸；无独立 background-image）
          DEV-MOBILE-001 §65：Android 完全不渲染 DesktopTitlebar */}
      {!IS_ANDROID && <DesktopTitlebar />}
      <div className="app-shell__content">
        <ProfileGate />
      </div>
    </ActiveProfileProvider>
  );
}

export default App;
