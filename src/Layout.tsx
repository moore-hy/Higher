import { NavLink, Outlet } from "react-router-dom";
import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { useActiveProfile, canSwitchProfile } from "./contexts/ActiveProfileContext";
import { createStudyProfile, updateStudyProfile } from "./api";
import AiPanel from "./components/ai/AiPanel";
import { useAiPanel } from "./components/ai/AiPanelContext";
import { PROFILE_TYPE_LABELS } from "./types";
import type { ProfileType, StudyProfile } from "./types";

/**
 * Higher V2 Shell（DEV-0011）。
 *
 * 一级导航最终收敛（DEV-0055 PART 19 §69-71）：
 * 今日 / 规划 / 知识 / 数据；设置保留在 Sidebar footer。
 * 「学习复盘」入口删除（/review 兼容重定向到 /planning?date=…）；
 * 「整体进度」并入「学习规划」（/progress 兼容路由重定向）；
 * 学习数据成为一级页面（/data，DEV-0055 PART 26）。
 */
const NAV_ITEMS = [
  { to: "/", label: "今日", end: true, icon: "📅", group: "学习" },
  { to: "/planning", label: "规划", end: false, icon: "🧭", group: "学习" },
  { to: "/knowledge", label: "知识", end: false, icon: "🗂", group: "学习" },
  { to: "/data", label: "数据", end: false, icon: "📊", group: "洞察" },
  // DEV-SYNC-002 §十：同步一级入口（/sync 工作台；配对管理仍在 设置 → 设备同步）
  { to: "/sync", label: "同步", end: false, icon: "🔄", group: "洞察" },
];

function Layout() {
  const { activeProfile, exitProfile, refreshGate, enterProfile, refreshKey } = useActiveProfile();
  // DEV-0065.1 §32：AI Panel 恒驻（无 open 态）；Main 宽度由 flex 自动跟随
  // 340px expanded / 46px collapsed，无 JS 宽度计算
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
      if (p === "/planning") return "planning" as const;
      if (p === "/knowledge") return "knowledge" as const;
      if (p === "/data") return "data" as const;
      if (p === "/settings") return "settings" as const;
      return "today" as const;
    })();
    const labels = {
      today: "今日任务",
      planning: "学习规划",
      knowledge: "知识体系",
      learning: "学习工作区",
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
    <div className="layout" key={refreshKey}>
      <aside className="layout__sidebar">
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

        <nav className="layout__nav">
          {NAV_ITEMS.map((item, i) => (
            <span key={item.to} className="layout__nav-slot">
              {(i === 0 || NAV_ITEMS[i - 1].group !== item.group) && (
                <span className="layout__nav-group">{item.group}</span>
              )}
              <NavLink
                to={item.to}
                end={item.end}
                title={item.label}
                className={({ isActive }) =>
                  "layout__nav-item" + (isActive ? " layout__nav-item--active" : "")
                }
              >
                <span className="layout__nav-icon">{item.icon}</span>
                <span className="layout__nav-label">{item.label}</span>
              </NavLink>
            </span>
          ))}
        </nav>

        <div className="layout__sidebar-footer">
          {/* 设置（DEV-0016）：非学习业务模块，置于 Sidebar 底部 */}
          <NavLink
            to="/settings"
            title="设置"
            className={({ isActive }) =>
              "layout__nav-item layout__nav-item--settings" +
              (isActive ? " layout__nav-item--active" : "")
            }
          >
            <span className="layout__nav-icon">⚙</span>
            <span className="layout__nav-label">设置</span>
          </NavLink>
        </div>
      </aside>
      <div className="layout__body">
        <main className="layout__main">
          <Outlet />
        </main>
        {/* Higher AI Agent Panel（DEV-0022 → DEV-0065.1：恒驻右栏两态，340px/46px） */}
        <AiPanel />
      </div>

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
