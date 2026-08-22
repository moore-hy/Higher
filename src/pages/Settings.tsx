import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  compilePersonalization,
  confirmPersonalizationProfile,
  createAiProviderProfile,
  deletePersonalizationSource,
  deleteAiProviderProfile,
  editPersonalizationProfile,
  getActiveAiProfiles,
  getNotificationEnabled,
  getPersonalizationProfile,
  getRequirementTemplate,
  getWebSearchSettings,
  importPersonalizationFiles,
  listAiProviderProfiles,
  listBackups,
  listArchivedTasksByProfile,
  listPersonalizationSources,
  setActiveAiProfiles,
  setNotificationEnabled,
  setWebSearchSettings,
  setUiSetting,
  getUiSetting,
  syncNotifications,
  testAiProviderCompatibility,
  testAiProviderConnection,
  unarchiveTask,
  updateAiProviderProfile,
  vaultCreateSnapshot,
  vaultExportEvents,
  vaultListEvents,
  vaultListSnapshots,
  vaultLock,
  vaultStatus,
  vaultUnlock,
} from "../api";
import {
  canSwitchProfile,
  useActiveProfile,
} from "../contexts/ActiveProfileContext";
import { createStudyProfile, executeProfileCleanup, previewProfileCleanup, updateStudyProfile } from "../api";
import { PROFILE_TYPE_LABELS } from "../types";
import type {
  AiProviderProfile,
  CleanupPreview,
  PersonalizationProfile,
  PersonalizationSource,
  ProfileType,
  StudyProfile,
  Task,
  VaultEvent,
  VaultSnapshot,
} from "../types";
import { downloadTextFile, formatDateTime, todayDate } from "../utils";

/**
 * 设置中心（DEV-0016 + DEV-0030 数据管理 + DEV-0042 学习提醒）。
 *
 * 一、学习档案：切换 / 编辑 / 创建 / 退出（复用现有 Profile API，无第二套逻辑）
 * 二、AI 设置：DeepSeek（Provider / Base URL / API Key 明文 / Model / Thinking / 测试连接）
 * 三、学习提醒（DEV-0042）：系统通知开关 + 权限状态（拒绝不阻塞使用）
 * 四、数据管理：当前档案的活动数据清理（clear/keep today/month/year + Full Reset；
 *    预览 → 备份 → 事务执行；所有操作只影响当前 StudyProfile）
 *
 * 权限说明固定展示：AI 只在主动使用时运行；建议不会自动写入知识库。
 */
export default function Settings() {
  const { activeProfile, exitProfile, refreshGate, enterProfile } = useActiveProfile();
  const [tab, setTab] = useState<
    "profile" | "ai" | "notify" | "data" | "personal" | "websearch" | "vault"
  >("profile");

  const TABS: { key: typeof tab; label: string }[] = [
    { key: "profile", label: "学习档案" },
    { key: "ai", label: "AI 设置" },
    { key: "personal", label: "私人化部署" },
    { key: "websearch", label: "联网搜索" },
    { key: "notify", label: "学习提醒" },
    { key: "data", label: "数据管理" },
    { key: "vault", label: "审计与备份" },
  ];

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">设置</h1>
      </header>

      <div className="review-window" style={{ marginBottom: 16 }}>
        {TABS.map((t) => (
          <button
            key={t.key}
            className={
              "review-window__item" + (tab === t.key ? " review-window__item--active" : "")
            }
            onClick={() => setTab(t.key)}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === "profile" ? (
        <ProfileSection profile={activeProfile} onSwitch={async () => {
          const ok = await canSwitchProfile();
          if (!ok) {
            window.alert("当前仍有学习正在进行。请先结束当前学习，再切换学习档案。");
            return;
          }
          await exitProfile();
        }} onSaved={refreshGate} onCreated={async (id: number) => enterProfile(id)} />
      ) : tab === "ai" ? (
        <AiSection />
      ) : tab === "notify" ? (
        <NotificationSection />
      ) : tab === "personal" ? (
        <PersonalizationSection profileId={activeProfile?.id ?? null} />
      ) : tab === "websearch" ? (
        <WebSearchSection />
      ) : tab === "vault" ? (
        <VaultSection />
      ) : (
        <DataSection profileId={activeProfile?.id ?? null} profileName={activeProfile?.name ?? ""} />
      )}
    </div>
  );
}

// ---------------- 学习提醒（DEV-0042） ----------------

function NotificationSection() {
  const [enabled, setEnabled] = useState(true);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  /** null = 未请求过权限；boolean = 系统授权结果 */
  const [permission, setPermission] = useState<boolean | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    (async () => {
      try {
        setEnabled(await getNotificationEnabled());
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
      try {
        setPermission(await isPermissionGranted());
      } catch {
        setPermission(null);
      }
    })();
  }, []);

  async function toggle(next: boolean) {
    setSaving(true);
    setMessage("");
    setError("");
    // 启用时请求系统通知权限（拒绝不阻塞，程序继续正常）
    if (next) {
      try {
        const granted = await isPermissionGranted();
        if (!granted) {
          const verdict = await requestPermission();
          setPermission(verdict === "granted");
        } else {
          setPermission(true);
        }
      } catch {
        /* 权限查询失败不阻塞开关 */
      }
    }
    try {
      await setNotificationEnabled(next);
      setEnabled(next);
      setMessage(next ? "已开启学习提醒" : "已关闭学习提醒");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function resync() {
    setSaving(true);
    setMessage("");
    setError("");
    try {
      await syncNotifications();
      setMessage("已按最新任务重新对齐提醒");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const permissionLabel =
    permission === true ? "已授权" : permission === false ? "未授权（系统设置中可开启）" : "未请求";

  if (loading) return <section className="card"><p className="muted">加载中…</p></section>;

  return (
    <section className="card">
      <h2 className="card__title">学习提醒</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      <label className="modal__field" style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <input
          type="checkbox"
          checked={enabled}
          disabled={saving}
          onChange={(e) => void toggle(e.target.checked)}
        />
        开启系统学习提醒（任务时间到点时提醒我）
      </label>

      <p className="muted" style={{ fontSize: 12, margin: "10px 0 0" }}>
        系统通知权限：{permissionLabel}
        {permission === false && "（拒绝也不影响 Higher 其他功能）"}
      </p>
      <p className="muted" style={{ fontSize: 12, margin: "6px 0 0" }}>
        只提醒有具体时间的任务；未来 30 天内的提醒会自动对齐，改期/删除后旧提醒自动取消。
      </p>

      <div className="btn-row" style={{ marginTop: 12 }}>
        <button className="btn" onClick={() => void resync()} disabled={saving}>
          {saving ? "处理中…" : "立即重新对齐提醒"}
        </button>
      </div>
    </section>
  );
}

// ---------------- 数据管理（DEV-0030） ----------------

type CleanupScopeKey =
  | "clear_today"
  | "keep_today"
  | "clear_month"
  | "keep_month"
  | "clear_year"
  | "keep_year"
  | "full_reset";

const CLEANUP_OPTIONS: { key: CleanupScopeKey; label: string; desc: string; danger?: boolean }[] = [
  { key: "clear_today", label: "清除今天的活动数据", desc: "删除今天的任务 / 学习记录 / 验证 / 问题 / 调整 / 学习附件" },
  { key: "keep_today", label: "只保留今天的活动数据", desc: "删除今天以外其他日期的活动数据（长期学习系统结构保留）" },
  { key: "clear_month", label: "清除本月的活动数据", desc: "删除本月全部活动数据" },
  { key: "keep_month", label: "只保留本月的活动数据", desc: "删除其他月份的活动数据" },
  { key: "clear_year", label: "清除今年的活动数据", desc: "删除今年全部活动数据" },
  { key: "keep_year", label: "只保留今年的活动数据", desc: "删除其他年份的活动数据" },
  { key: "full_reset", label: "清空当前档案全部数据", desc: "保留档案外壳，删除目标 / 知识 / 阶段 / 计划 / 全部记录与附件", danger: true },
];

function DataSection({ profileId, profileName }: { profileId: number | null; profileName: string }) {
  const [previewFor, setPreviewFor] = useState<CleanupScopeKey | null>(null);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [doneMsg, setDoneMsg] = useState("");
  // DEV-0036 §23/§108：已归档任务 + 最近备份
  const [showArchived, setShowArchived] = useState(false);
  const [archived, setArchived] = useState<Task[]>([]);
  const [backups, setBackups] = useState<{ name: string; size_bytes: number; path: string }[]>([]);
  const [backupsOpen, setBackupsOpen] = useState(false);

  useEffect(() => {
    if (backupsOpen) {
      listBackups().then(setBackups).catch(() => setBackups([]));
    }
  }, [backupsOpen]);

  async function loadArchived() {
    if (profileId == null) return;
    try {
      setArchived(await listArchivedTasksByProfile(profileId));
    } catch (e) {
      setError(String(e));
    }
  }

  if (profileId == null) {
    return <p className="muted">还没有激活的学习档案。</p>;
  }

  async function openPreview(key: CleanupScopeKey) {
    setError("");
    setDoneMsg("");
    setLoading(true);
    setPreview(null);
    try {
      const p = await previewProfileCleanup(profileId!, key, todayDate());
      setPreview(p);
      setPreviewFor(key);
      setConfirmText("");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function execute(key: CleanupScopeKey) {
    setBusy(true);
    setError("");
    try {
      const p = await executeProfileCleanup(profileId!, key, todayDate());
      setPreviewFor(null);
      setPreview(null);
      setDoneMsg(
        key === "full_reset"
          ? "档案已重置为空白（数据库备份已生成）"
          : `清理完成：任务 ${p.tasks} · 学习记录 ${p.sessions} · 验证 ${p.evaluations} · 问题 ${p.feedbacks} · 调整 ${p.adjustments} · 附件 ${p.session_attachments}（已自动备份）`
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const isFull = previewFor === "full_reset";

  return (
    <section className="card">
      <h2 className="card__title">数据管理</h2>
      <p className="muted" style={{ fontSize: 12 }}>
        当前档案：<b>{profileName}</b>。以下所有操作只影响这个档案；执行前会自动备份数据库（保留最近 10 份）。
        时间清理不会删除目标 / 知识 / 阶段 / 计划 / 重复任务规则等长期结构。
      </p>
      {error && <div className="alert alert--error">{error}</div>}
      {doneMsg && <div className="alert alert--ok">{doneMsg}</div>}

      <ul className="cleanup-list">
        {CLEANUP_OPTIONS.filter((o) => !o.danger).map((o) => (
          <li key={o.key} className="cleanup-item">
            <div className="cleanup-item__main">
              <span className="cleanup-item__label">{o.label}</span>
              <span className="muted cleanup-item__desc">{o.desc}</span>
            </div>
            <button className="btn btn--small" onClick={() => void openPreview(o.key)} disabled={loading}>
              预览
            </button>
          </li>
        ))}
      </ul>

      {/* DEV-0036 §23：已归档任务（查看 / 恢复） */}
      <div className="cleanup-archived">
        <button
          className="taskmodal__more-toggle"
          onClick={() => {
            const next = !showArchived;
            setShowArchived(next);
            if (next) void loadArchived();
          }}
        >
          {showArchived ? "收起已归档任务 ▴" : "已归档任务（查看 / 恢复） ▾"}
        </button>
        {showArchived && (
          <div className="cleanup-archived__body">
            {archived.length === 0 ? (
              <p className="muted" style={{ fontSize: 12 }}>
                当前档案没有已归档的任务。
              </p>
            ) : (
              <ul className="taskrow-list">
                {archived.map((t) => (
                  <li key={t.id} className="taskrow taskrow--done">
                    <div className="taskrow__main">
                      <span className="taskrow__title">{t.title}</span>
                      <span className="taskrow__meta">
                        归档于 {(t.archived_at ?? "").slice(0, 10)}
                        {t.planned_date ? ` · 原计划 ${t.planned_date}` : ""}
                      </span>
                    </div>
                    <div className="taskrow__actions">
                      <button
                        className="btn btn--small"
                        onClick={async () => {
                          await unarchiveTask(t.id);
                          await loadArchived();
                        }}
                      >
                        恢复
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>

      {/* DEV-0036 §108：最近备份（仅展示日期/大小/路径；不做恢复） */}
      <div className="cleanup-archived">
        <button className="taskmodal__more-toggle" onClick={() => setBackupsOpen((v) => !v)}>
          {backupsOpen ? "收起最近备份 ▴" : "最近备份 ▾"}
        </button>
        {backupsOpen && (
          <div className="cleanup-archived__body">
            {backups.length === 0 ? (
              <p className="muted" style={{ fontSize: 12 }}>
                还没有备份。执行任何清理操作前会自动创建（最多保留 10 份）。
              </p>
            ) : (
              <ul className="cleanup-backups">
                {backups.map((b) => (
                  <li key={b.name} title={b.path}>
                    {b.name} · {(b.size_bytes / 1024 / 1024).toFixed(2)} MB
                  </li>
                ))}
              </ul>
            )}
            <p className="muted" style={{ fontSize: 11 }}>
              备份已保存到 Higher 自己的 backups 目录；恢复数据库需手动替换文件（本轮不提供恢复 API）。
            </p>
          </div>
        )}
      </div>

      {/* DEV-0054 §105-106：危险操作独立 Danger Zone（红边框卡片，置于底部） */}
      <div className="danger-zone">
        <div className="danger-zone__title">危险区</div>
        <p className="muted danger-zone__note">
          以下操作影响面大且不可恢复（自动备份除外），请谨慎使用。
        </p>
        <ul className="cleanup-list">
          {CLEANUP_OPTIONS.filter((o) => o.danger).map((o) => (
            <li key={o.key} className="cleanup-item cleanup-item--danger">
              <div className="cleanup-item__main">
                <span className="cleanup-item__label">{o.label}</span>
                <span className="muted cleanup-item__desc">{o.desc}</span>
              </div>
              <button className="btn btn--small" onClick={() => void openPreview(o.key)} disabled={loading}>
                预览
              </button>
            </li>
          ))}
        </ul>
      </div>

      {/* 预览确认 Modal */}
      {previewFor && preview && (
        <div className="modal-overlay" onClick={() => !busy && setPreviewFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">
              {CLEANUP_OPTIONS.find((o) => o.key === previewFor)?.label}
            </div>
            <p className="muted" style={{ fontSize: 12 }}>
              将删除（真实数量，来自当前档案）：
            </p>
            <ul className="cleanup-preview">
              <li>任务：{preview.tasks}</li>
              <li>学习记录：{preview.sessions}</li>
              <li>验证：{preview.evaluations}</li>
              <li>问题：{preview.feedbacks}</li>
              <li>调整：{preview.adjustments}</li>
              <li>学习附件：{preview.session_attachments}</li>
              {isFull && (
                <>
                  <li>目标：{preview.goals}</li>
                  <li>知识：{preview.knowledge}</li>
                  <li>阶段：{preview.stages}</li>
                  <li>计划：{preview.plans}</li>
                  <li>重复任务规则：{preview.recurring_rules}</li>
                  <li>全部附件：{preview.all_attachments}</li>
                </>
              )}
            </ul>
            <p className="muted" style={{ fontSize: 11 }}>
              执行前会自动备份整个数据库；目标 / 知识 / 阶段 / 计划 / 重复任务规则{isFull ? "会随档案清空一起删除" : "不会删除"}。
            </p>

            {isFull ? (
              <>
                <p style={{ fontSize: 12, color: "var(--error)" }}>
                  这是不可恢复的档案清空（备份除外）。请输入「清空」以确认：
                </p>
                <input
                  className="modal__input"
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  placeholder="清空"
                />
                <div className="modal__actions">
                  <button
                    className="btn btn--primary"
                    disabled={busy || confirmText.trim() !== "清空"}
                    onClick={() => void execute(previewFor)}
                  >
                    {busy ? "执行中…" : "确认清空档案"}
                  </button>
                  <button className="btn" onClick={() => setPreviewFor(null)} disabled={busy}>
                    取消
                  </button>
                </div>
              </>
            ) : (
              <div className="modal__actions">
                <button
                  className="btn btn--primary"
                  disabled={busy}
                  onClick={() => void execute(previewFor)}
                >
                  {busy ? "执行中…" : "确认执行（先备份）"}
                </button>
                <button className="btn" onClick={() => setPreviewFor(null)} disabled={busy}>
                  取消
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

// ---------------- 学习档案 ----------------

function ProfileSection({
  profile,
  onSwitch,
  onSaved,
  onCreated,
}: {
  profile: StudyProfile | null;
  onSwitch: () => Promise<void>;
  onSaved: () => Promise<void>;
  onCreated: (id: number) => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");

  return (
    <section className="card">
      <h2 className="card__title">学习档案</h2>
      {error && <div className="alert alert--error">{error}</div>}
      <div className="settings-profile">
        <div className="settings-profile__name">
          当前档案：<strong>{profile?.name ?? "未选择"}</strong>
        </div>
        <div className="btn-row">
          <button className="btn" onClick={() => setEditing(true)} disabled={!profile}>
            编辑当前档案
          </button>
          <button className="btn" onClick={() => setCreating(true)}>
            创建档案
          </button>
          <button className="btn" onClick={() => void onSwitch()} disabled={!profile}>
            切换 / 退出当前档案
          </button>
        </div>
      </div>

      {editing && profile && (
        <ProfileEditInline profile={profile} onClose={() => setEditing(false)} onSaved={onSaved} />
      )}
      {creating && (
        <ProfileCreateInline
          onClose={() => setCreating(false)}
          onCreated={async (p) => {
            setCreating(false);
            await onCreated(p.id);
          }}
        />
        )}
      </section>
    );
  }

function ProfileEditInline({
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
    if (!name.trim()) return setError("档案名称不能为空");
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
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">编辑当前档案</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          档案名称
          <input className="modal__input" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="modal__field">
          学习类型
          <select className="modal__input" value={profileType} onChange={(e) => setProfileType(e.target.value)}>
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>{PROFILE_TYPE_LABELS[t]}</option>
            ))}
          </select>
        </label>
        <label className="modal__field">
          目标描述
          <input className="modal__input" value={targetDescription} onChange={(e) => setTargetDescription(e.target.value)} />
        </label>
        <label className="modal__field">
          目标日期
          <input type="date" className="modal__input" value={targetDate} onChange={(e) => setTargetDate(e.target.value)} />
        </label>
        <label className="modal__field">
          当前情况
          <textarea className="modal__input" rows={2} value={currentSituation} onChange={(e) => setCurrentSituation(e.target.value)} />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
          <button className="btn" onClick={onClose}>取消</button>
        </div>
      </div>
    </div>
  );
}

function ProfileCreateInline({
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
    if (!name.trim()) return setError("档案名称不能为空");
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
        <div className="modal__title">创建档案</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          我要学习什么
          <select className="modal__input" value={profileType} onChange={(e) => setProfileType(e.target.value)}>
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>{PROFILE_TYPE_LABELS[t]}</option>
            ))}
          </select>
        </label>
        <label className="modal__field">
          档案名称
          <input className="modal__input" value={name} onChange={(e) => setName(e.target.value)} placeholder="如：2027 考研" autoFocus />
        </label>
        <label className="modal__field">
          目标描述
          <input className="modal__input" value={targetDescription} onChange={(e) => setTargetDescription(e.target.value)} />
        </label>
        <label className="modal__field">
          目标日期
          <input type="date" className="modal__input" value={targetDate} onChange={(e) => setTargetDate(e.target.value)} />
        </label>
        <label className="modal__field">
          当前情况
          <textarea className="modal__input" rows={2} value={currentSituation} onChange={(e) => setCurrentSituation(e.target.value)} />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={create} disabled={saving}>
            {saving ? "创建中…" : "创建并进入"}
          </button>
          <button className="btn" onClick={onClose}>取消</button>
        </div>
      </div>
    </div>
  );
}

// ---------------- AI 设置（DEV-0062 · 多 AI Connection） ----------------

/** 兼容性徽标文案（§32） */
function compatLabel(s: string): { text: string; cls: string } {
  switch (s) {
    case "full":
      return { text: "完整兼容", cls: "settings-compat settings-compat--full" };
    case "limited":
      return { text: "有限兼容", cls: "settings-compat settings-compat--limited" };
    case "incompatible":
      return { text: "不兼容", cls: "settings-compat settings-compat--bad" };
    default:
      return { text: "未检测", cls: "settings-compat settings-compat--untested" };
  }
}

/** §22 显式 Control 要求（前端过滤；后端同样校验） */
function controlCompatible(p: AiProviderProfile): boolean {
  return (
    p.enabled &&
    p.capabilities.basic_chat === true &&
    p.capabilities.structured_json === true &&
    p.capabilities.temperature_zero === true
  );
}

/** DEV-0062R §20 单项能力显示：✓ / ✗ / 未检测（Structured JSON 附策略） */
function capMark(v: boolean | null | undefined): string {
  if (v === true) return "✓";
  if (v === false) return "✗";
  return "未检测";
}

function capRow(label: string, v: boolean | null | undefined, extra?: string) {
  const cls = v === true ? "settings-cap--ok" : v === false ? "settings-cap--bad" : "settings-cap--untested";
  return (
    <span className={`settings-cap ${cls}`}>
      {label} {capMark(v)}
      {extra ? `（${extra}）` : ""}
    </span>
  );
}

/** §20 Structured JSON 策略文案 */
function strategyLabel(p: AiProviderProfile): string | undefined {
  if (p.capabilities.structured_json !== true) return undefined;
  switch (p.capabilities.json_strategy) {
    case "native":
      return "Native";
    case "prompt_only":
      return "Prompt Only";
    default:
      return undefined;
  }
}

/** DEV-0062R.1 §18.1/§18.2：从 last_test_message 安全摘要解析 basic=/temp0= 细分值 */
function parseSummary(p: AiProviderProfile, key: "basic" | "temp0"): string {
  const m = p.last_test_message.match(new RegExp(`(?:^|[;｜])\\s*${key}=([^;｜]+)`));
  return m ? m[1].trim() : "";
}

function parseSkipped(p: AiProviderProfile): boolean {
  return p.last_test_message.includes("skipped=skipped_connection_failure");
}

/** Basic Chat 细分文案（§18.1；不显示 reasoning 原文） */
function basicDetailLabel(v: string): string {
  switch (v) {
    case "pass_after_retry":
      return "（二次尝试成功）";
    case "no_final_content":
      return "（✗ 无最终文本）";
    case "reasoning_only_no_final":
      return "（✗ 仅 reasoning，无最终文本）";
    case "length_no_final":
      return "（✗ 输出被长度截断且重试仍无最终文本）";
    case "unexpected_tool_only":
      return "（✗ 意外只返回工具调用）";
    case "request_error":
      return "（✗ 请求失败）";
    default:
      return "";
  }
}

/** Temperature 0 细分文案（§18.2） */
function temp0DetailLabel(v: string): string {
  switch (v) {
    case "pass_after_retry":
      return "（二次尝试成功）";
    case "no_final_content":
    case "reasoning_only_no_final":
    case "length_no_final":
      return "（✗ 无最终文本）";
    case "request_error":
      return "（✗ 请求失败）";
    default:
      return "";
  }
}

/** Connection 编辑 Modal（新增 / 编辑共用；§33/§34） */
function ConnectionModal({
  initial,
  onClose,
  onSaved,
}: {
  initial: AiProviderProfile | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(initial?.display_name ?? "");
  const [adapter, setAdapter] = useState(initial?.adapter_kind ?? "deepseek");
  const [baseUrl, setBaseUrl] = useState(initial?.base_url ?? "https://api.deepseek.com");
  const [apiKey, setApiKey] = useState(initial?.api_key ?? "");
  const [model, setModel] = useState(initial?.model ?? "deepseek-v4-flash");
  const [thinking, setThinking] = useState(
    initial?.thinking_mode === "deepseek_model_suffix",
  );
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function save() {
    setSaving(true);
    setError("");
    try {
      const args = {
        displayName: name.trim(),
        adapterKind: adapter,
        baseUrl: baseUrl.trim(),
        apiKey: apiKey.trim(),
        model: model.trim(),
        thinkingMode: adapter === "deepseek" && thinking ? "deepseek_model_suffix" : "off",
      };
      if (initial) {
        await updateAiProviderProfile({ profileId: initial.id, ...args, enabled });
      } else {
        await createAiProviderProfile(args);
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" role="dialog">
      <div className="modal">
        <h2 className="modal__title">{initial ? "编辑 AI 连接" : "添加 AI 连接"}</h2>
        {error && <div className="alert alert--error">{error}</div>}

        <label className="modal__field">
          连接名称
          <input
            className="modal__input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如：DeepSeek Flash / GLM"
          />
        </label>

        <label className="modal__field">
          服务商 / 兼容类型
          <select
            className="modal__input"
            value={adapter}
            onChange={(e) => setAdapter(e.target.value)}
          >
            <option value="deepseek">DeepSeek</option>
            <option value="openai_compatible">OpenAI Compatible</option>
          </select>
        </label>

        <label className="modal__field">
          API Base URL
          <input
            className="modal__input"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.deepseek.com"
          />
        </label>

        <label className="modal__field">
          API Key（明文显示与保存，仅保存在本机数据库）
          <input
            className="modal__input"
            type="text"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-..."
            autoComplete="off"
          />
        </label>

        <label className="modal__field">
          Model
          <input
            className="modal__input"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="模型名"
            list="deepseek-model-suggestions"
          />
          <datalist id="deepseek-model-suggestions">
            <option value="deepseek-v4-flash" />
            <option value="deepseek-v4-pro" />
          </datalist>
        </label>

        {adapter === "deepseek" ? (
          <label className="modal__field" style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input type="checkbox" checked={thinking} onChange={(e) => setThinking(e.target.checked)} />
            Thinking Mode（深度思考；若服务商返回模型错误请关闭）
          </label>
        ) : (
          <p className="muted" style={{ fontSize: 12 }}>
            如服务商提供独立推理模型，请直接填写对应模型名称。Higher 不会修改模型名。
          </p>
        )}

        {initial && (
          <label className="modal__field" style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
            启用该连接
          </label>
        )}

        <div className="btn-row">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
          <button className="btn" onClick={onClose} disabled={saving}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

function AiSection() {
  const [profiles, setProfiles] = useState<AiProviderProfile[]>([]);
  const [active, setActive] = useState<{ primary_id: number | null; control_id: number | null }>({
    primary_id: null,
    control_id: null,
  });
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<AiProviderProfile | "new" | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  async function reload() {
    try {
      const [list, act] = await Promise.all([listAiProviderProfiles(), getActiveAiProfiles()]);
      setProfiles(list);
      setActive(act);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
    // §37.2 Settings / Panel 同步：任一侧切换 active 即广播刷新
    const un = listen<void>("higher:ai-profiles-changed", () => void reload());
    return () => {
      void un.then((f) => f());
    };
  }, []);

  async function applyActive(primaryId: number, controlId: number | null) {
    setError("");
    setMessage("");
    try {
      await setActiveAiProfiles(primaryId, controlId);
      setMessage("已切换 AI。");
      await reload();
    } catch (e) {
      setError(String(e));
      await reload();
    }
  }

  async function onTest(p: AiProviderProfile) {
    setBusyId(p.id);
    setError("");
    setMessage("");
    try {
      const r = await testAiProviderConnection(p.id);
      setMessage(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function onProbe(p: AiProviderProfile) {
    setBusyId(p.id);
    setError("");
    setMessage("");
    try {
      const r = await testAiProviderCompatibility(p.id);
      setMessage(r.message);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function onDelete(p: AiProviderProfile) {
    setError("");
    setMessage("");
    try {
      await deleteAiProviderProfile(p.id);
      setMessage("已删除该 AI 连接。");
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  if (loading) return <section className="card"><p className="muted">加载中…</p></section>;

  const enabled = profiles.filter((p) => p.enabled);
  const controlCandidates = profiles.filter(controlCompatible);

  return (
    <>
      <section className="card">
        <h2 className="card__title">AI 设置</h2>
        {error && <div className="alert alert--error">{error}</div>}
        {message && <div className="alert alert--ok">{message}</div>}

        <label className="modal__field">
          主要 AI
          <select
            className="modal__input"
            value={active.primary_id ?? ""}
            onChange={(e) => {
              const id = Number(e.target.value);
              if (id) void applyActive(id, active.control_id);
            }}
          >
            {enabled.length === 0 && <option value="">（请先添加 AI 连接）</option>}
            {enabled.map((p) => (
              <option key={p.id} value={p.id}>
                {p.display_name}
                {p.compatibility_status === "limited" ? "（有限兼容）" : ""}
                {p.compatibility_status === "untested" ? "（未检测）" : ""}
              </option>
            ))}
          </select>
        </label>

        <label className="modal__field">
          动作理解 AI（高级；默认跟随主要 AI）
          <select
            className="modal__input"
            value={active.control_id ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              void applyActive(active.primary_id ?? enabled[0]?.id ?? 0, v ? Number(v) : null);
            }}
          >
            <option value="">跟随主要 AI</option>
            {controlCandidates.map((p) => (
              <option key={p.id} value={p.id}>
                {p.display_name}
              </option>
            ))}
          </select>
        </label>
      </section>

      <section className="card">
        <h2 className="card__title">AI 连接</h2>
        {(() => {
          // §20.1：Control = Follow Primary 且当前 Primary 不满足 Control 要求 → 提前警告
          const primary = profiles.find((p) => p.id === active.primary_id);
          const followWarning =
            active.control_id == null &&
            primary != null &&
            primary.capabilities.basic_chat === true &&
            primary.capabilities.structured_json === false;
          return followWarning ? (
            <div className="alert alert--error" style={{ marginBottom: 12 }}>
              当前动作理解跟随主要 AI，但此连接不满足 Higher Action 要求；修改类指令会被安全拒绝。请检测兼容性或单独选择动作理解 AI。
            </div>
          ) : null;
        })()}
        {profiles.map((p) => {
          const badge = compatLabel(p.compatibility_status);
          const isPrimary = p.id === active.primary_id;
          const isControl = p.id === active.control_id;
          const skipped = parseSkipped(p);
          const busy = busyId === p.id;
          // §18.3：Hard Connection Failure 时 B-E 显示「未继续检测（连接失败）」
          const skippedMark = skipped ? "未继续检测（连接失败）" : undefined;
          return (
            <div key={p.id} className="settings-conn">
              <div className="settings-conn__main">
                <span className="settings-conn__name">
                  {p.display_name}
                  {!p.enabled && <span className="muted">（已停用）</span>}
                  {isPrimary && <span className="settings-role">主要 AI</span>}
                  {isControl && <span className="settings-role">动作理解 AI</span>}
                </span>
                <span className="muted settings-conn__meta">
                  {p.adapter_kind === "deepseek" ? "DeepSeek" : "OpenAI Compatible"} · {p.model}
                </span>
                <span className={badge.cls}>{badge.text}</span>
                {/* DEV-0062R §20 + 0062R.1 §18：五项能力 + 策略 + 细分 + 最后检测时间 */}
                <span className="settings-conn__caps">
                  {capRow("基础对话", p.capabilities.basic_chat, basicDetailLabel(parseSummary(p, "basic")))}
                  {capRow("结构化输出", p.capabilities.structured_json, strategyLabel(p) ?? skippedMark)}
                  {capRow("工具调用", p.capabilities.tool_calls, skippedMark)}
                  {capRow("温度 0", p.capabilities.temperature_zero, temp0DetailLabel(parseSummary(p, "temp0")))}
                  {capRow("流式", p.capabilities.streaming, skippedMark)}
                </span>
                <span className="muted settings-conn__meta">
                  {p.last_tested_at
                    ? `最后检测：${formatDateTime(p.last_tested_at)}`
                    : "尚未检测兼容性"}
                </span>
                {p.compatibility_status === "untested" && (
                  <span className="muted settings-conn__meta">
                    连接配置已变化或从未检测，Higher 兼容性需要重新检测。
                  </span>
                )}
              </div>
              <div className="btn-row">
                {/* §19：Probe 完成前该连接全部按钮 disabled（防并发 Probe） */}
                <button className="btn btn--small" onClick={() => setEditing(p)} disabled={busy}>
                  编辑
                </button>
                <button className="btn btn--small" onClick={() => void onTest(p)} disabled={busy}>
                  测试连接
                </button>
                <button className="btn btn--small" onClick={() => void onProbe(p)} disabled={busy}>
                  {busy ? "检测中…" : "检测 Higher 兼容性"}
                </button>
                <button
                  className="btn btn--small btn--danger"
                  onClick={() => void onDelete(p)}
                  disabled={busy}
                >
                  删除
                </button>
              </div>
            </div>
          );
        })}
        <div className="btn-row">
          <button className="btn" onClick={() => setEditing("new")}>
            + 添加 AI 连接
          </button>
        </div>
      </section>

      {editing && (
         <ConnectionModal
           initial={editing === "new" ? null : editing}
           onClose={() => setEditing(null)}
           onSaved={() => void reload()}
         />
       )}

      <section className="card">
        <h2 className="card__title">AI 权限说明</h2>
        <ul className="settings-ai-note">
          <li>AI 只在你主动使用 AI 功能时运行。</li>
          <li>AI 可以读取当前功能需要的学习数据。</li>
          <li>AI 的修改建议不会自动写入知识库。</li>
          <li>只有你确认后 Higher 才会修改正式数据。</li>
        </ul>
      </section>
    </>
  );
}

// ---------------- 私人化部署（DEV-0052 §57-91） ----------------

const AUTO_PZ_KEY = "ui.auto_personalization";

function PersonalizationSection({ profileId }: { profileId: number | null }) {
  const [profile, setProfile] = useState<PersonalizationProfile | null>(null);
  const [sources, setSources] = useState<PersonalizationSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [compiling, setCompiling] = useState(false);
  const [importing, setImporting] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [viewing, setViewing] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editText, setEditText] = useState("");
  const [editSaving, setEditSaving] = useState(false);
  const [showDraft, setShowDraft] = useState(true);
  const [autoMaintain, setAutoMaintain] = useState(false);
  /** DEV-0054 §108：查看/编辑/下载/模板 收进「更多 ▾」下拉 */
  const [moreOpen, setMoreOpen] = useState(false);

  async function load() {
    if (profileId == null) return;
    try {
      const [p, s] = await Promise.all([
        getPersonalizationProfile(profileId),
        listPersonalizationSources(profileId),
      ]);
      setProfile(p);
      setSources(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    setShowDraft(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId]);

  useEffect(() => {
    getUiSetting(AUTO_PZ_KEY)
      .then((v) => setAutoMaintain(v === "true"))
      .catch(() => {});
  }, []);

  async function toggleAutoMaintain(next: boolean) {
    setAutoMaintain(next);
    try {
      await setUiSetting(AUTO_PZ_KEY, next ? "true" : "false");
    } catch {
      /* 失败保持本地态 */
    }
  }

  async function addFiles() {
    if (profileId == null) return;
    const picked = await openDialog({
      multiple: true,
      filters: [
        { name: "支持的资料（txt / md / docx / pdf / xlsx）", extensions: ["txt", "md", "docx", "pdf", "xlsx"] },
      ],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked.map(String) : [String(picked)];
    setImporting(true);
    setError("");
    setMessage("");
    try {
      const created = await importPersonalizationFiles(profileId, paths);
      setMessage(`已导入 ${created.length} 个资料文件。导入后可点「重新分析」生成档案。`);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  }

  async function recompile() {
    if (profileId == null) return;
    setCompiling(true);
    setError("");
    setMessage("");
    try {
      const p = await compilePersonalization(profileId);
      setProfile(p);
      setShowDraft(true);
      setMessage("重新分析完成，已生成新的草稿，请查看后确认。");
    } catch (e) {
      setError(String(e));
    } finally {
      setCompiling(false);
    }
  }

  async function downloadMd() {
    if (!profile) return;
    // 项目未安装 fs 插件：统一走纯前端 Blob 下载（浏览器下载目录）
    downloadTextFile(
      `higher-personalization-v${profile.version}.md`,
      profile.md_content,
      "text/markdown"
    );
    setMessage("已开始下载 .md 文件（保存到浏览器下载目录）。");
  }

  /** §32：导出个人档案 Word/Excel（docx/exceljs 点击时懒加载；§31.3 只写用户所选路径） */
  async function exportProfile(kind: "docx" | "xlsx") {
    if (profileId == null) return;
    setMessage("");
    setError("");
    try {
      const mod = await import("../lib/exporters");
      const { bytes, fileName } =
        kind === "docx"
          ? await mod.exportPersonalProfileDocx(profileId)
          : await mod.exportPersonalProfileXlsx(profileId);
      const path = await saveDialog({
        defaultPath: fileName,
        filters: [
          {
            name: kind === "docx" ? "Word 文档" : "Excel 工作簿",
            extensions: [kind],
          },
        ],
      });
      if (!path) return;
      const { writeExportFile } = await import("../api");
      await writeExportFile(path, mod.base64FromBytes(bytes));
      setMessage(`已导出：${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportTemplate() {
    try {
      const tpl = await getRequirementTemplate();
      downloadTextFile("higher-requirements-template.md", tpl, "text/markdown");
      setMessage("需求采集模板已导出。");
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveEdit() {
    if (profileId == null) return;
    setEditSaving(true);
    setError("");
    try {
      await editPersonalizationProfile(profileId, editText);
      setEditing(false);
      setMessage("已保存修改（档案变为待重新确认的草稿）。");
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setEditSaving(false);
    }
  }

  async function confirmDraft() {
    if (profileId == null) return;
    setError("");
    try {
      await confirmPersonalizationProfile(profileId);
      setMessage("已确认并保存私人化档案。");
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeSource(id: number) {
    if (profileId == null) return;
    setError("");
    try {
      await deletePersonalizationSource(profileId, id);
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  if (profileId == null) {
    return <p className="muted">还没有激活的学习档案。</p>;
  }

  if (loading) return <section className="card"><p className="muted">加载中…</p></section>;

  const statusText =
    profile == null
      ? "未生成"
      : profile.status === "draft"
        ? "草稿待确认"
        : profile.status === "superseded"
          ? "已过期版本"
          : `已生成 v${profile.version}（正式）`;
  const updatedText = profile?.confirmed_at ?? profile?.updated_at ?? null;

  return (
    <>
      <section className="card">
        <h2 className="card__title">私人化部署（完全可选）</h2>
        <p className="muted pz__intro">
          把你的学习背景、习惯与偏好整理成一份本地档案，AI 回答时会更懂你。
          全部数据只保存在本机，不会上传；不配置也完全不影响 Higher 的其他功能。
        </p>
        {error && <div className="alert alert--error">{error}</div>}
        {message && <div className="alert alert--ok">{message}</div>}

        {/* 主卡（§59） */}
        <div className="pz__main">
          <div className="pz__main-info">
            <span className={"pz__status pz__status--" + (profile?.status ?? "none")}>
              {statusText}
            </span>
            <span className="muted pz__main-time">
              {updatedText ? `最后更新：${formatDateTime(updatedText)}` : "还没有生成过档案"}
              {profile != null && ` · 版本 v${profile.version} · 来源 ${sources.length} 份`}
            </span>
          </div>
          <div className="btn-row pz__main-actions">
            <button className="btn btn--small" onClick={() => void addFiles()} disabled={importing}>
              {importing ? "导入中…" : "添加资料"}
            </button>
            <button className="btn btn--small" onClick={() => void recompile()} disabled={compiling}>
              {compiling ? "分析中（可能需要几分钟）…" : "重新分析"}
            </button>
            <div className="taskmenu">
              <button className="taskmenu__btn" onClick={() => setMoreOpen((v) => !v)}>
                更多 ▾
              </button>
              {moreOpen && (
                <>
                  <div className="actrow__backdrop" onClick={() => setMoreOpen(false)} />
                  <div className="taskmenu__pop">
                    <button
                      disabled={!profile}
                      onClick={() => {
                        setMoreOpen(false);
                        setViewing(true);
                      }}
                    >
                      查看
                    </button>
                    <button
                      disabled={!profile}
                      onClick={() => {
                        setMoreOpen(false);
                        setEditText(profile?.md_content ?? "");
                        setEditing(true);
                      }}
                    >
                      编辑
                    </button>
                    <button
                      disabled={!profile}
                      onClick={() => {
                        setMoreOpen(false);
                        void downloadMd();
                      }}
                    >
                      下载 .md
                    </button>
                    <button
                      disabled={!profile}
                      onClick={() => {
                        setMoreOpen(false);
                        void exportProfile("docx");
                      }}
                    >
                      导出 Word
                    </button>
                    <button
                      disabled={!profile}
                      onClick={() => {
                        setMoreOpen(false);
                        void exportProfile("xlsx");
                      }}
                    >
                      导出 Excel
                    </button>
                    <button
                      onClick={() => {
                        setMoreOpen(false);
                        void exportTemplate();
                      }}
                    >
                      导出需求采集模板
                    </button>
                  </div>
                </>
              )}
            </div>
          </div>
          <p className="muted pz__hint">
            支持导入 txt / md / docx / pdf（旧版 .doc 请先另存为 .docx）。导入的文件会被复制到 Higher 自己的资料目录。
          </p>
        </div>

        {/* 草稿待确认（§59） */}
        {profile?.status === "draft" && showDraft && (
          <div className="pz__draft">
            <p>已生成草稿，确认后 AI 才会使用这份档案。</p>
            <div className="btn-row">
              <button className="btn btn--small btn--primary" onClick={() => void confirmDraft()}>
                确认并保存
              </button>
              <button className="btn btn--small" onClick={() => setShowDraft(false)}>
                继续补充
              </button>
            </div>
          </div>
        )}

        {/* 资料列表 */}
        <div className="pz__sources">
          <div className="pz__sources-title">已导入资料（{sources.length}）</div>
          {sources.length === 0 ? (
            <p className="muted" style={{ fontSize: 12 }}>
              还没有导入资料。可导入个人说明、简历、学习计划、错题总结等文件。
            </p>
          ) : (
            <ul className="pz__source-list">
              {sources.map((s) => (
                <li key={s.id} className="pz__source">
                  <div className="pz__source-main">
                    <span className="pz__source-name" title={s.relative_path}>{s.file_name}</span>
                    <span className="pz__source-meta">
                      {s.file_type.toUpperCase()} · 导入于 {formatDateTime(s.created_at)}
                    </span>
                  </div>
                  <button className="btn btn--small" onClick={() => void removeSource(s.id)}>
                    删除
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {/* 自动维护（§91） */}
        <div className="pz__auto">
          <label className="pz__auto-toggle">
            <input
              type="checkbox"
              checked={autoMaintain}
              onChange={(e) => void toggleAutoMaintain(e.target.checked)}
            />
            自动维护私人化档案（AI 在对话中发现的长期信息自动并入草稿）
          </label>
          <p className="muted pz__auto-note">
            自动维护可能产生少量 AI API 调用。关闭后仅在你手动「重新分析」时更新档案。
          </p>
        </div>
      </section>

      {/* 查看档案（只读） */}
      {viewing && profile && (
        <div className="modal-overlay" onClick={() => setViewing(false)}>
          <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">私人化档案（只读）v{profile.version}</div>
            <pre className="pz__view-pre">{profile.md_content}</pre>
            <div className="modal__actions">
              <button className="btn" onClick={() => setViewing(false)}>关闭</button>
            </div>
          </div>
        </div>
      )}

      {/* 编辑档案 */}
      {editing && (
        <div className="modal-overlay" onClick={() => !editSaving && setEditing(false)}>
          <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">编辑私人化档案（Markdown）</div>
            <textarea
              className="pz__edit-area"
              value={editText}
              onChange={(e) => setEditText(e.target.value)}
              rows={20}
              spellCheck={false}
            />
            <p className="muted" style={{ fontSize: 11 }}>
              保存后档案会回到「草稿待确认」，需要再次确认才会被 AI 使用。
            </p>
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void saveEdit()} disabled={editSaving}>
                {editSaving ? "保存中…" : "保存"}
              </button>
              <button className="btn" onClick={() => setEditing(false)} disabled={editSaving}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

// ---------------- 联网搜索（DEV-0052 §95-97） ----------------

function WebSearchSection() {
  const [enabled, setEnabled] = useState(false);
  const [hasKey, setHasKey] = useState(false);
  const [braveKey, setBraveKey] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    (async () => {
      try {
        const [en, hk] = await getWebSearchSettings();
        setEnabled(en);
        setHasKey(hk);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  async function save(nextEnabled: boolean, keyInput?: string) {
    setSaving(true);
    setMessage("");
    setError("");
    try {
      // 只有输入了新 Key 才更新（空输入 = 保留已保存的 Key）
      const key = keyInput != null && keyInput.trim() ? keyInput.trim() : undefined;
      await setWebSearchSettings(nextEnabled, key);
      setEnabled(nextEnabled);
      if (key) setHasKey(true);
      setMessage("已保存。");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <section className="card"><p className="muted">加载中…</p></section>;

  return (
    <section className="card">
      <h2 className="card__title">联网搜索</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      <label className="modal__field ws__toggle">
        <input
          type="checkbox"
          checked={enabled}
          disabled={saving}
          onChange={(e) => void save(e.target.checked)}
        />
        启用联网搜索（允许 AI 在需要时检索互联网并给出带来源的回答）
      </label>

      <label className="modal__field">
        Brave Search API Key{hasKey ? "（已保存；留空则不修改）" : ""}
        <input
          className="modal__input"
          type="password"
          value={braveKey}
          onChange={(e) => setBraveKey(e.target.value)}
          placeholder={hasKey ? "••••••••" : "BSA…"}
          autoComplete="off"
        />
      </label>
      <p className="muted ws__note" style={{ margin: "0 0 8px" }}>
        用于 Higher AI 联网搜索。
      </p>

      <div className="btn-row">
        <button
          className="btn btn--primary"
          disabled={saving || (!braveKey.trim() && !hasKey)}
          onClick={() => void save(enabled, braveKey)}
        >
          {saving ? "保存中…" : "保存"}
        </button>
      </div>

      <p className="muted ws__note">
        Key 仅保存在本机数据库，不会上传。AI 回答中的每个联网结论都会标注来源编号，可点击用系统浏览器打开原文。
      </p>
    </section>
  );
}

// ---------------- 保险箱（DEV-0052 §159-163 / §230 Flow N） ----------------

function VaultSection() {
  const [locked, setLocked] = useState(true);
  const [hint, setHint] = useState("");
  const [stats, setStats] = useState<[number, number, number] | null>(null);
  const [password, setPassword] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const [events, setEvents] = useState<VaultEvent[]>([]);
  const [snapshots, setSnapshots] = useState<VaultSnapshot[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  async function refreshUnlocked() {
    const [st, ev, sn] = await Promise.all([
      vaultStatus(),
      vaultListEvents(50).catch(() => [] as VaultEvent[]),
      vaultListSnapshots().catch(() => [] as VaultSnapshot[]),
    ]);
    setLocked(st.locked);
    setHint(st.hint);
    setStats(st.stats);
    setEvents(ev);
    setSnapshots(sn);
  }

  useEffect(() => {
    void refreshUnlocked().catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function unlock() {
    if (!password) return;
    setUnlocking(true);
    setError("");
    setMessage("");
    try {
      await vaultUnlock(password);
      setPassword("");
      await refreshUnlocked();
      setMessage("保险箱已解锁。");
    } catch (e) {
      setError("密码错误，解锁被拒绝。");
    } finally {
      setUnlocking(false);
    }
  }

  async function lock() {
    setBusy(true);
    try {
      await vaultLock();
      await refreshUnlocked();
      setMessage("保险箱已锁定。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function snapshot() {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await vaultCreateSnapshot();
      await refreshUnlocked();
      setMessage("已创建数据库快照。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportAudit() {
    setBusy(true);
    setError("");
    try {
      const json = await vaultExportEvents();
      downloadTextFile("higher-vault-audit.json", json, "application/json");
      setMessage("审计日志已导出。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card">
      <h2 className="card__title">审计与备份</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      {locked ? (
        <div className="vault__locked">
          <div className="vault__lock-icon">🔒</div>
          <div className="vault__lock-title">已锁定</div>
          <p className="muted vault__lock-hint">{hint || "测试版密码为 root"}</p>
          <div className="vault__unlock-row">
            <input
              className="modal__input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void unlock();
              }}
              placeholder="输入密码"
              autoComplete="off"
            />
            <button className="btn btn--primary" onClick={() => void unlock()} disabled={unlocking || !password}>
              {unlocking ? "解锁中…" : "解锁"}
            </button>
          </div>
          <p className="muted vault__lock-note">
            测试版访问锁，不代表数据加密。审计与备份记录所有写入操作（用户与 AI）并保存数据库快照；锁定时不影响 Higher 的正常使用。
          </p>
        </div>
      ) : (
        <>
          <div className="vault__stats">
            <div className="vault__stat">
              <span className="vault__stat-num">{stats?.[0] ?? events.length}</span>
              <span className="vault__stat-label">审计事件</span>
            </div>
            <div className="vault__stat">
              <span className="vault__stat-num">{stats?.[1] ?? 0}</span>
              <span className="vault__stat-label">Blob 记录</span>
            </div>
            <div className="vault__stat">
              <span className="vault__stat-num">{stats?.[2] ?? snapshots.length}</span>
              <span className="vault__stat-label">快照</span>
            </div>
          </div>

          <div className="btn-row">
            <button className="btn" onClick={() => void lock()} disabled={busy}>
              立即锁定
            </button>
            <button className="btn" onClick={() => void snapshot()} disabled={busy}>
              {busy ? "处理中…" : "创建快照"}
            </button>
            <button className="btn" onClick={() => void exportAudit()} disabled={busy}>
              导出审计 JSON
            </button>
          </div>

          <div className="vault__list">
            <div className="vault__list-title">审计事件（最近 {events.length}）</div>
            {events.length === 0 ? (
              <p className="muted" style={{ fontSize: 12 }}>还没有审计事件。</p>
            ) : (
              <ul className="vault__events">
                {events.map((ev) => (
                  <li key={ev.seq} className="vault__event">
                    <span className={"vault__actor vault__actor--" + ev.actor_type.toLowerCase()}>
                      {ev.actor_type === "USER" ? "USER" : ev.actor_type === "AI" ? "AI" : "SYSTEM"}
                    </span>
                    <span className="vault__event-main">
                      <span className="vault__event-action">{ev.action}</span>
                      <span className="vault__event-entity">{ev.entity_type}{ev.entity_id != null ? ` #${ev.entity_id}` : ""}</span>
                    </span>
                    <span className="vault__event-time" title={ev.run_id ? `run ${ev.run_id.slice(0, 8)}` : ""}>
                      {formatDateTime(ev.timestamp)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="vault__list">
            <div className="vault__list-title">快照（最近 {snapshots.length}）</div>
            {snapshots.length === 0 ? (
              <p className="muted" style={{ fontSize: 12 }}>还没有快照。应用 AI 修改提案时会自动创建。</p>
            ) : (
              <ul className="vault__snaps">
                {snapshots.map((s) => (
                  <li key={s[0]} className="vault__snap">
                    <span>#{s[0]} · {s[1]}</span>
                    <span className="muted">{(s[2] / 1024 / 1024).toFixed(2)} MB · {formatDateTime(s[3])}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </section>
  );
}
