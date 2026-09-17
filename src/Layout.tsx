import { NavLink, Outlet } from "react-router-dom";
import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import {
  Brain,
  Database,
  Map as MapIcon,
  Route as RouteIcon,
  Settings as SettingsIcon,
  Sun,
  TrendingUp,
  Waypoints,
} from "lucide-react";
import { useActiveProfile, canSwitchProfile } from "./contexts/ActiveProfileContext";
import { createStudyProfile, updateStudyProfile } from "./api";
import AiPanel from "./components/ai/AiPanel";
import HigherCommandBar from "./components/cognitive/HigherCommandBar";
import { useAiPanel } from "./components/ai/AiPanelContext";
import { PROFILE_TYPE_LABELS } from "./types";
import type { ProfileType, StudyProfile } from "./types";

/**
 * Higher V2 Shell（DEV-0011）。
 *
 * DEV-0055 PART 19 §69-71：一级导航收敛为 今日 / 规划 / 知识 / 数据，设置置于 footer。
 * 「学习复盘」入口删除（/review 兼容重定向）；「整体进度」并入「学习规划」。
 *
 * COGNITIVE CORE V1.2 §21 修订（W9）：**桌面端**一级导航锁定为产品概念 IA
 *
 *   Today     /           我现在该做什么？
 *   Journey   /journey    我要去哪、计划怎么展开？
 *   Memory    /memory     什么正在被遗忘、该复习什么？
 *   Progress  /progress    能力/数量/质量/适应性发生了什么变化？
 *   Settings  /settings   sidebar footer
 *
 * 图标一律 lucide-react（§21：新桌面侧栏禁止 emoji 图标）。
 * `/knowledge`、`/data`、`/sync` 按 §21 降级为 **preserved internal power-user route**：
 * 路由、页面与全部能力**原样保留**（§37 未删除任何生产能力），
 * 但不再占用一级导航，只在 sidebar 的「进阶」次级分组中以弱化样式可达
 * （避免把生产功能退化成只能靠 deep link 才够得着的死路）。
 *
 * 平台切分：本组件只在桌面挂载（App.tsx：`IS_ANDROID ? <MobileLayout/> : <Layout/>`），
 * 因此这里是天然的 desktop-only 改动点，Android shell IA 零改动（§22）。
 */
const NAV_ITEMS = [
  { to: "/", label: "Today", end: true, zh: "今日 · 我现在该做什么", Icon: Sun },
  { to: "/journey", label: "Journey", end: false, zh: "学习旅程 · 我要去哪", Icon: RouteIcon },
  { to: "/memory", label: "Memory", end: false, zh: "记忆 · 什么正在被遗忘", Icon: Brain },
  { to: "/progress", label: "Progress", end: false, zh: "进展 · 什么发生了变化", Icon: TrendingUp },
];

/** §21 preserved power-user routes（不进一级导航，弱化分组呈现） */
const ADVANCED_NAV_ITEMS = [
  { to: "/knowledge", label: "知识", end: false, Icon: MapIcon },
  { to: "/data", label: "数据", end: false, Icon: Database },
  { to: "/sync", label: "同步", end: false, Icon: Waypoints },
];

function Layout() {
  const { activeProfile, exitProfile, refreshGate, enterProfile, refreshKey } = useActiveProfile();
  /** W9 §22：AI 已成为 420px 覆盖抽屉（关闭零宽度），Shell 只汇报页面上下文 */
  const { setPageContext } = useAiPanel();
  const location = useLocation();
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [creating, setCreating] = useState(false);

  /** 页面 → AI Panel 上下文（DEV-0022 §12；Settings 不提供业务上下文） */
  useEffect(() => {
    const pageKey = (() => {
      const p = location.pathname;
      if (p.startsWith("/learn/")) return "learning" as const;
      // §21：/journey 渲染既有 Planning 页面 → AI 上下文仍是 planning
      if (p === "/planning" || p === "/journey") return "planning" as const;
      if (p === "/knowledge") return "knowledge" as const;
      if (p === "/memory") return "memory" as const;
      if (p === "/progress") return "progress" as const;
      if (p === "/data") return "data" as const;
      if (p === "/settings") return "settings" as const;
      return "today" as const;
    })();
    const labels = {
      today: "今日任务",
      planning: "学习规划",
      knowledge: "知识体系",
      learning: "学习工作区",
      memory: "记忆",
      progress: "进展",
      data: "学习数据",
      settings: "设置",
    } as const;
    if (pageKey === "settings") {
      setPageContext({ page: "settings", pageLabel: labels.settings });
    } else {
      setPageContext({ page: pageKey, pageLabel: labels[pageKey] });
    }
  }, [location.pathname, setPageContext]);

  /** 切换 / 退出都回到档案选择页（退出 = 清除 active；切换语义相同）。 */
  async function handleSwitch() {
    setMenuOpen(false);
    const canSwitch = await canSwitchProfile();
    if (!canSwitch) {
      setError("当前仍有学习正在进行。请先结束当前学习，再切换学习档案。");
      return;
    }
    setError(null);
    await exitProfile();
  }

  return (
    <div className="layout layout--cognitive" key={refreshKey}>
      <aside className="layout__sidebar hc-sidebar">
        <div className="layout__brand">
          <span className="layout__brand-mark">H</span>
          <span className="layout__brand-name">Higher</span>
        </div>

        {/* 学习档案区域：档案名 + 副标题 + ⌄ 菜单 */}
        {activeProfile && (
          <div className="shell-profile">
            <button
              className="shell-profile__btn"
              onClick={() => setMenuOpen(!menuOpen)}
            >
              <span className="shell-profile__name">{activeProfile.name}</span>
              <span className="shell-profile__sub">
                {activeProfile.profile_type
                  ? PROFILE_TYPE_LABELS[activeProfile.profile_type as ProfileType] ??
                    activeProfile.profile_type
                  : "学习档案"}
              </span>
              <span className="shell-profile__arrow">⌄</span>
            </button>
            {menuOpen && (
              <>
                <div className="shell-profile__backdrop" onClick={() => setMenuOpen(false)} />
                <div className="shell-profile__menu">
                  <button
                    className="shell-profile__menu-item"
                    onClick={() => {
                      setMenuOpen(false);
                      setEditing(true);
                    }}
                  >
                    编辑档案
                  </button>
                  <button
                    className="shell-profile__menu-item"
                    onClick={() => {
                      setMenuOpen(false);
                      setCreating(true);
                    }}
                  >
                    创建新档案
                  </button>
                  <button
                    className="shell-profile__menu-item shell-profile__menu-item--danger"
                    onClick={handleSwitch}
                  >
                    切换学习档案 / 退出当前档案
                  </button>
                </div>
              </>
            )}
          </div>
        )}
        {error && <div className="layout__error">{error}</div>}

        <nav className="layout__nav hc-nav" aria-label="主导航">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              title={item.zh}
              aria-label={`${item.label} · ${item.zh}`}
              className={({ isActive }) =>
                "layout__nav-item hc-nav__item" +
                (isActive ? " layout__nav-item--active hc-nav__item--active" : "")
              }
            >
              <span className="layout__nav-icon hc-nav__icon" aria-hidden="true">
                <item.Icon size={17} strokeWidth={1.75} />
              </span>
              <span className="layout__nav-label">{item.label}</span>
            </NavLink>
          ))}
        </nav>

        {/* §21 preserved power-user routes：能力原样保留，只弱化呈现 */}
        <nav className="hc-nav hc-nav--advanced" aria-label="进阶功能">
          <div className="hc-nav__caption">进阶</div>
          {ADVANCED_NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              title={item.label}
              aria-label={item.label}
              className={({ isActive }) =>
                "layout__nav-item hc-nav__item hc-nav__item--advanced" +
                (isActive ? " layout__nav-item--active hc-nav__item--active" : "")
              }
            >
              <span className="layout__nav-icon hc-nav__icon" aria-hidden="true">
                <item.Icon size={16} strokeWidth={1.75} />
              </span>
              <span className="layout__nav-label">{item.label}</span>
            </NavLink>
          ))}
        </nav>

        <div className="layout__sidebar-footer">
          {/* 设置（DEV-0016）：非学习业务模块，置于 Sidebar 底部 */}
          <NavLink
            to="/settings"
            title="设置"
            aria-label="Settings · 设置"
            className={({ isActive }) =>
              "layout__nav-item layout__nav-item--settings hc-nav__item" +
              (isActive ? " layout__nav-item--active hc-nav__item--active" : "")
            }
          >
            <span className="layout__nav-icon hc-nav__icon" aria-hidden="true">
              <SettingsIcon size={17} strokeWidth={1.75} />
            </span>
            <span className="layout__nav-label">Settings</span>
          </NavLink>
        </div>
      </aside>

      <div className="layout__body hc-mainstage">
        <main className="layout__main">
          <Outlet />
        </main>
        {/* §22：底部命令栏是本 Shell 的唯一自由文本入口（桌面） */}
        <HigherCommandBar />
      </div>

      {/* §22：桌面端 AI = 420px 右侧覆盖抽屉（关闭零宽度）。Radix Dialog portal 渲染，
          放在这里只为与 Shell 同生命周期，不参与 flex 布局。 */}
      <AiPanel />

      {editing && activeProfile && (
        <ProfileEditModal
          profile={activeProfile}
          onClose={() => setEditing(false)}
          onSaved={async () => {
            setEditing(false);
            await refreshGate();
          }}
        />
      )}
      {creating && (
        <ProfileCreateModal
          onClose={() => setCreating(false)}
          onCreated={async (p) => {
            setCreating(false);
            await enterProfile(p.id);
          }}
        />
      )}
    </div>
  );
}

/** 编辑档案 Modal（名称 / 类型 / 目标描述 / 目标日期 / 当前情况）。 */
function ProfileEditModal({
  profile,
  onClose,
  onSaved,
}: {
  profile: StudyProfile;
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const [name, setName] = useState(profile.name);
  const [profileType, setProfileType] = useState(profile.profile_type ?? "custom");
  const [targetDescription, setTargetDescription] = useState(profile.target_description ?? "");
  const [targetDate, setTargetDate] = useState(profile.target_date ?? "");
  const [currentSituation, setCurrentSituation] = useState(profile.current_situation ?? "");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function save() {
    if (!name.trim()) {
      setError("档案名称不能为空");
      return;
    }
    setSaving(true);
    try {
      await updateStudyProfile({
        id: profile.id,
        name: name.trim(),
        profileType: profileType === "custom" ? null : profileType,
        targetDescription: targetDescription.trim() || null,
        targetDate: targetDate || null,
        currentSituation: currentSituation.trim() || null,
      });
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">编辑档案</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          档案名称
          <input
            className="modal__input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </label>
        <label className="modal__field">
          学习类型
          <select
            className="modal__input"
            value={profileType}
            onChange={(e) => setProfileType(e.target.value)}
          >
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>
                {PROFILE_TYPE_LABELS[t]}
              </option>
            ))}
          </select>
        </label>
        <label className="modal__field">
          目标描述
          <input
            className="modal__input"
            value={targetDescription}
            onChange={(e) => setTargetDescription(e.target.value)}
            placeholder="如：目标 XX 大学计算机专业"
          />
        </label>
        <label className="modal__field">
          目标日期
          <input
            type="date"
            className="modal__input"
            value={targetDate}
            onChange={(e) => setTargetDate(e.target.value)}
          />
        </label>
        <label className="modal__field">
          当前情况
          <textarea
            className="modal__input"
            rows={2}
            value={currentSituation}
            onChange={(e) => setCurrentSituation(e.target.value)}
          />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

/** 创建新档案 Modal（创建成功自动进入）。 */
function ProfileCreateModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (p: StudyProfile) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [profileType, setProfileType] = useState<string>("kaoyan");
  const [targetDescription, setTargetDescription] = useState("");
  const [targetDate, setTargetDate] = useState("");
  const [currentSituation, setCurrentSituation] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function create() {
    if (!name.trim()) {
      setError("档案名称不能为空");
      return;
    }
    setSaving(true);
    try {
      const p = await createStudyProfile({
        name: name.trim(),
        profileType: profileType === "custom" ? null : profileType,
        targetDescription: targetDescription.trim() || null,
        targetDate: targetDate || null,
        currentSituation: currentSituation.trim() || null,
      });
      await onCreated(p);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">创建新档案</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          我要学习什么
          <select
            className="modal__input"
            value={profileType}
            onChange={(e) => setProfileType(e.target.value)}
          >
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>
                {PROFILE_TYPE_LABELS[t]}
              </option>
            ))}
          </select>
        </label>
        <label className="modal__field">
          档案名称
          <input
            className="modal__input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="如：2027 考研"
            autoFocus
          />
        </label>
        <label className="modal__field">
          目标描述
          <input
            className="modal__input"
            value={targetDescription}
            onChange={(e) => setTargetDescription(e.target.value)}
          />
        </label>
        <label className="modal__field">
          目标日期
          <input
            type="date"
            className="modal__input"
            value={targetDate}
            onChange={(e) => setTargetDate(e.target.value)}
          />
        </label>
        <label className="modal__field">
          当前情况
          <textarea
            className="modal__input"
            rows={2}
            value={currentSituation}
            onChange={(e) => setCurrentSituation(e.target.value)}
          />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={create} disabled={saving}>
            {saving ? "创建中…" : "创建并进入"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

export default Layout;
