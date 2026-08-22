import { Suspense, lazy } from "react";
import { HashRouter, Navigate, Route, Routes, useSearchParams } from "react-router-dom";
import { ActiveProfileProvider, useActiveProfile } from "./contexts/ActiveProfileContext";
import { AiPanelProvider } from "./components/ai/AiPanelContext";
import WallpaperLayers from "./components/WallpaperLayers";
import DesktopTitlebar from "./components/DesktopTitlebar";
import Layout from "./Layout";
import Evaluations from "./pages/Evaluations";
import Goals from "./pages/Goals";
import History from "./pages/History";
import Planning from "./pages/Planning";
import ProfileSelector from "./pages/ProfileSelector";
import ProfileWelcome from "./pages/ProfileWelcome";
import Settings from "./pages/Settings";
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
  const { gate } = useActiveProfile();

  if (gate.phase === "loading") {
    return (
      <div className="profile-gate">
        <p className="profile-gate__loading">加载中...</p>
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
            <Route element={<Layout />}>
              <Route path="/" element={<Today />} />
              <Route path="/planning" element={<Planning />} />
              {/* DEV-0041：学习复盘并入学习规划，兼容重定向 */}
              <Route path="/review" element={<ReviewRedirect />} />
              <Route path="/knowledge" element={<KnowledgePage />} />
              {/* DEV-0055：学习数据一级页面（lazy §125） */}
              <Route path="/data" element={<DataPage />} />
              {/* DEV-0301：整体进度并入学习规划，兼容重定向 */}
              <Route path="/progress" element={<Navigate to="/planning" replace />} />
              {/* Learning Workspace（DEV-0017：开始学习进入正式学习工作区；DEV-0057 lazy） */}
              <Route path="/learn/:sessionId" element={<LearningWorkspacePage />} />
              {/* 设置（DEV-0016：非学习业务模块，位于 Sidebar 底部） */}
              <Route path="/settings" element={<Settings />} />
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
          背景 var(--h-sidebar) 透出同一全局壁纸；无独立 background-image） */}
      <DesktopTitlebar />
      <div className="app-shell__content">
        <ProfileGate />
      </div>
    </ActiveProfileProvider>
  );
}

export default App;
