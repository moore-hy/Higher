import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useActiveProfile, canSwitchProfile } from "../contexts/ActiveProfileContext";
import { listStudyProfiles } from "../api";
import type { StudyProfile } from "../types";
import { PROFILE_TYPE_LABELS } from "../types";
import type { ProfileType } from "../types";
import { isTauriRuntime } from "../utils/tauriEnv";
import {
  createSyncRefreshDispatcher,
  type SyncCompletedPayload,
} from "../sync/syncRefresh";

/**
 * 档案选择页面。
 *
 * 显示所有学习档案列表，用户可以进入任意档案或创建新档案。
 * 切换前检查是否有进行中的 Session。
 * DEV-SYNC-002 §九（SYNC2-UI-TC02）：profiles_changed > 0 时重读档案列表，
 * 同步新导入的档案无需退出页面即可出现。
 */
export default function ProfileSelector() {
  const { enterProfile } = useActiveProfile();
  const [profiles, setProfiles] = useState<StudyProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);

  useEffect(() => {
    loadProfiles();
    // §九：远端同步导入/更新档案 → 立即重读列表
    if (!isTauriRuntime()) return;
    const dispatch = createSyncRefreshDispatcher({ profiles: () => void loadProfiles() });
    const un = listen<SyncCompletedPayload>("sync://completed", (e) => dispatch(e.payload));
    return () => {
      void un.then((f) => f());
    };
  }, []);

  async function loadProfiles() {
    try {
      const list = await listStudyProfiles();
      setProfiles(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleEnter(profileId: number) {
    const canSwitch = await canSwitchProfile();
    if (!canSwitch) {
      setError("当前仍有学习正在进行。请先结束当前学习，再切换学习档案。");
      return;
    }
    setError(null);
    await enterProfile(profileId);
  }

  if (loading) {
    return (
      <div className="profile-gate">
        <p className="profile-gate__loading">加载中...</p>
      </div>
    );
  }

  if (showCreate) {
    return <ProfileCreateInline onDone={() => setShowCreate(false)} onCancel={() => setShowCreate(false)} />;
  }

  return (
    <div className="profile-gate">
      <div className="profile-gate__header">
        <h1 className="profile-gate__title">Higher</h1>
        <p className="profile-gate__subtitle">选择学习档案</p>
      </div>
      {error && <div className="profile-gate__error">{error}</div>}
      <div className="profile-selector__list">
        {profiles.map((p) => (
          <div key={p.id} className="profile-card">
            <div className="profile-card__name">{p.name}</div>
            <div className="profile-card__type">
              {p.profile_type ? (PROFILE_TYPE_LABELS[p.profile_type as ProfileType] ?? p.profile_type) : "未分类"}
            </div>
            <div className="profile-card__meta">
              {p.last_opened_at
                ? `最近使用：${formatRelativeTime(p.last_opened_at)}`
                : "尚未使用"}
            </div>
            {p.target_description && (
              <div className="profile-card__desc">{p.target_description}</div>
            )}
            <button
              className="btn btn--primary btn--small"
              onClick={() => handleEnter(p.id)}
            >
              进入
            </button>
          </div>
        ))}
      </div>
      <button
        className="btn btn--primary profile-gate__btn"
        onClick={() => setShowCreate(true)}
      >
        + 创建新档案
      </button>
    </div>
  );
}

function formatRelativeTime(isoStr: string): string {
  const date = new Date(isoStr + "Z");
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (diffDays === 0) return "今天";
  if (diffDays === 1) return "昨天";
  if (diffDays < 7) return `${diffDays} 天前`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)} 周前`;
  return date.toLocaleDateString("zh-CN");
}

/** 内联创建档案表单（在选择页面内展开） */
function ProfileCreateInline({
  onDone,
  onCancel,
}: {
  onDone: () => void;
  onCancel: () => void;
}) {
  const { enterProfile } = useActiveProfile();
  const [name, setName] = useState("");
  const [profileType, setProfileType] = useState<string>("kaoyan");
  const [targetDescription, setTargetDescription] = useState("");
  const [targetDate, setTargetDate] = useState("");
  const [currentSituation, setCurrentSituation] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function handleCreate() {
    if (!name.trim()) {
      setError("档案名称不能为空");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const { createStudyProfile } = await import("../api");
      const profile = await createStudyProfile({
        name: name.trim(),
        profileType: profileType === "custom" ? null : profileType,
        targetDescription: targetDescription.trim() || null,
        targetDate: targetDate || null,
        currentSituation: currentSituation.trim() || null,
      });
      // 创建成功后自动进入该档案
      await enterProfile(profile.id);
      onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="profile-gate">
      <div className="profile-gate__header">
        <h1 className="profile-gate__title">创建学习档案</h1>
      </div>
      {error && <div className="profile-gate__error">{error}</div>}
      <div className="profile-create__form">
        <div className="form-row">
          <label className="form-label">学习类型</label>
          <select
            value={profileType}
            onChange={(e) => setProfileType(e.target.value)}
            className="form-input"
          >
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>
                {PROFILE_TYPE_LABELS[t]}
              </option>
            ))}
          </select>
        </div>
        <div className="form-row">
          <label className="form-label">档案名称 *</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="如：2027 考研"
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">目标描述（可选）</label>
          <input
            type="text"
            value={targetDescription}
            onChange={(e) => setTargetDescription(e.target.value)}
            placeholder="如：目标 XX 大学计算机专业"
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">目标日期（可选）</label>
          <input
            type="date"
            value={targetDate}
            onChange={(e) => setTargetDate(e.target.value)}
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">当前情况 / 当前基础（可选）</label>
          <textarea
            value={currentSituation}
            onChange={(e) => setCurrentSituation(e.target.value)}
            placeholder="如：数学基础较弱，目前在职，每天晚上可以学习约 3 小时"
            rows={3}
            className="form-input"
          />
        </div>
        <div className="btn-row">
          <button
            className="btn btn--primary"
            onClick={handleCreate}
            disabled={saving}
          >
            {saving ? "创建中..." : "创建并进入"}
          </button>
          <button className="btn" onClick={onCancel}>
            返回
          </button>
        </div>
      </div>
    </div>
  );
}
