import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useActiveProfile, canSwitchProfile } from "../contexts/ActiveProfileContext";
import { deleteStudyProfile, listStudyProfiles } from "../api";
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
 * 显示所有学习档案列表，用户可以进入任意档案、创建新档案，或永久删除档案。
 * 切换前检查是否有进行中的 Session。
 * DEV-SYNC-002 §九（SYNC2-UI-TC02）：profiles_changed > 0 时重读档案列表，
 * 同步新导入的档案无需退出页面即可出现。
 *
 * PROFILE DELETE HOTFIX V1 §3：删除是两步动作（首次点击不删除），
 * 后端单事务保证原子性；后端拒绝时保留卡片并显示错误，前端不做乐观更新。
 */
export default function ProfileSelector() {
  const { enterProfile, refreshGate } = useActiveProfile();
  const [profiles, setProfiles] = useState<StudyProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  /** 待确认删除的档案（非 null = 确认弹窗打开；首次点击不删除） */
  const [confirmTarget, setConfirmTarget] = useState<StudyProfile | null>(null);
  /** 每个目标的独立进行中状态：删除在途时禁用该目标的删除与重复提交 */
  const [deletingProfileId, setDeletingProfileId] = useState<number | null>(null);
  /** 删除失败错误（在确认弹窗内可见，卡片保持不动） */
  const [deleteError, setDeleteError] = useState<string | null>(null);

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

  function requestDelete(p: StudyProfile) {
    setError(null);
    setDeleteError(null);
    setConfirmTarget(p);
  }

  function cancelDelete() {
    setConfirmTarget(null);
    setDeleteError(null);
  }

  async function confirmDelete() {
    const target = confirmTarget;
    if (!target) return;
    // 防重复提交：同一目标在途时不再发第二次请求
    if (deletingProfileId !== null) return;
    setDeletingProfileId(target.id);
    setDeleteError(null);
    try {
      await deleteStudyProfile(target.id);
      // 成功：关闭确认、重载列表、失效 active 缓存（被删档案若是 active 则一并清空）
      setConfirmTarget(null);
      await loadProfiles();
      await refreshGate();
    } catch (e) {
      // 失败：不乐观更新，卡片保留，错误可见
      setDeleteError(String(e));
    } finally {
      setDeletingProfileId(null);
    }
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
    // PROFILE SELECTOR SCROLL HOTFIX V2：仅普通选择页加 --selector 修饰类，
    // 令其自身成为纵向滚动容器；loading / 创建表单仍用通用 .profile-gate。
    <div className="profile-gate profile-gate--selector">
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
            <div className="profile-card__actions">
              <button
                className="btn btn--primary btn--small"
                onClick={() => handleEnter(p.id)}
              >
                进入
              </button>
              <button
                className="btn btn--small btn--danger"
                disabled={deletingProfileId === p.id}
                onClick={() => requestDelete(p)}
              >
                {deletingProfileId === p.id ? "删除中…" : "删除"}
              </button>
            </div>
          </div>
        ))}
      </div>
      <button
        className="btn btn--primary profile-gate__btn"
        onClick={() => setShowCreate(true)}
      >
        + 创建新档案
      </button>
      {confirmTarget && (
        <ProfileDeleteConfirm
          profileName={confirmTarget.name}
          busy={deletingProfileId !== null}
          error={deleteError}
          onCancel={cancelDelete}
          onConfirm={confirmDelete}
        />
      )}
    </div>
  );
}

/**
 * 永久删除确认弹窗（§3）。复用既有 `.modal-overlay` / `.modal` / `.danger-zone`
 * / `.modal__actions` 结构与类名，不新增交互组件体系。
 */
function ProfileDeleteConfirm({
  profileName,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  profileName: string;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: () => void | Promise<void>;
}) {
  return (
    <div className="modal-overlay" onClick={() => !busy && onCancel()}>
      <div className="modal modal--quick" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">确定永久删除学习档案「{profileName}」吗？</div>
        <div className="danger-zone">
          <p className="danger-zone__note">
            该档案中的学习记录、任务、知识、规划、评估及相关数据将被永久删除，此操作无法撤销。
          </p>
        </div>
        {error && <div className="modal__error">{error}</div>}
        <div className="modal__actions">
          <button className="btn btn--ghost" disabled={busy} onClick={onCancel}>
            取消
          </button>
          <button
            className="btn btn--danger"
            disabled={busy}
            onClick={() => void onConfirm()}
          >
            {busy ? "删除中…" : "永久删除"}
          </button>
        </div>
      </div>
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
