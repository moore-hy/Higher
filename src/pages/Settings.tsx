import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  applyAppearance,
  DEFAULT_PREFS,
  LIMITS,
  loadPrefs,
  loadWallpaper,
  removeWallpaper,
  savePrefs,
  setWallpaper,
  THEMES,
  THEME_SWATCH,
  validateWallpaperFile,
} from "../appearance/appearanceHelpers";
import type { AppearancePrefs } from "../appearance/appearanceHelpers";
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
  getUserProfileTemplate,
  type AiMemoryItem,
  type AiProfile,
  confirmAiMemory,
  deleteAiMemory,
  getAiProfile,
  listAiMemories,
  rejectAiMemory,
  saveAiProfile,
  updateAiMemory,
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
import { useNavigate } from "react-router-dom";
import { IS_ANDROID } from "../platform/runtimePlatform";
import {
  syncClientStatus,
  syncClientSyncNow,
  syncPairViaQr,
  syncServerStart,
  syncServerStatus,
  syncServerStop,
  syncUnpair,
  type SyncClientStatus,
  type SyncPairOutcome,
  type SyncQrPairResult,
  type SyncServerStatus,
  type SyncSummary,
} from "../api";
import {
  cancel as cancelBarcodeScan,
  checkPermissions,
  Format,
  openAppSettings,
  requestPermissions,
  scan,
} from "@tauri-apps/plugin-barcode-scanner";

/**
 * 设置中心（DEV-0016 + DEV-0030 数据管理 + DEV-0042 学习提醒）。
 *
 * 一、学习档案：切换 / 编辑 / 创建 / 退出（复用现有 Profile API，无第二套逻辑）
 * 二、外观（DEV-0064）：自定义壁纸（IndexedDB）+ 壁纸效果三滑杆 + 6 套颜色氛围
 * 三、AI 设置：DeepSeek（Provider / Base URL / API Key 明文 / Model / Thinking / 测试连接）
 * 四、学习提醒（DEV-0042）：系统通知开关 + 权限状态（拒绝不阻塞使用）
 * 五、数据管理：当前档案的活动数据清理（clear/keep today/month/year + Full Reset；
 *    预览 → 备份 → 事务执行；所有操作只影响当前 StudyProfile）
 *
 * 权限说明固定展示：AI 只在主动使用时运行；建议不会自动写入知识库。
 */
type SettingsTab =
  | "profile" | "appearance" | "ai" | "notify" | "data" | "personal" | "websearch" | "vault" | "aimemory";

/**
 * DEV-MOBILE-001 F1 §十七：可选 initialTab——Android「我的」列表点入对应 section。
 * 不传（Windows / 既有调用）：默认 "profile"，行为与历史完全一致。
 * DEV-MOBILE-002 §40-41：presentation="mobile-section" 隐藏桌面 page__header 与
 * 9-tab Strip（返回由 MobileSettings 提供）；默认 desktop 完全不变。
 */
export default function Settings({
  initialTab,
  presentation = "desktop",
}: {
  initialTab?: SettingsTab;
  presentation?: "desktop" | "mobile-section";
} = {}) {
  const { activeProfile, exitProfile, refreshGate, enterProfile } = useActiveProfile();
  const [tab, setTab] = useState<SettingsTab>(initialTab ?? "profile");

  const TABS: { key: typeof tab; label: string }[] = [
    { key: "profile", label: "学习档案" },
    { key: "appearance", label: "外观" },
    { key: "ai", label: "AI 设置" },
    { key: "personal", label: "私人化部署" },
    { key: "aimemory", label: "AI 记忆" },
    { key: "websearch", label: "联网搜索" },
    { key: "notify", label: "学习提醒" },
    { key: "data", label: "数据管理" },
    { key: "devicesync", label: "设备同步" },
    { key: "vault", label: "审计与备份" },
  ];

  return (
    <div className="page">
      {/* §41：mobile-section 隐藏桌面标题（MobileSettings 提供 ‹ 返回 + section 名） */}
      {presentation === "desktop" && (
        <header className="page__header">
          <h1 className="page__title">设置</h1>
        </header>
      )}

      {/* §41：mobile-section 隐藏桌面 9-tab Strip */}
      {presentation === "desktop" && (
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
      )}

      {tab === "profile" ? (
        <ProfileSection profile={activeProfile} onSwitch={async () => {
          const ok = await canSwitchProfile();
          if (!ok) {
            window.alert("当前仍有学习正在进行。请先结束当前学习，再切换学习档案。");
            return;
          }
          await exitProfile();
        }} onSaved={refreshGate} onCreated={async (id: number) => enterProfile(id)} />
      ) : tab === "appearance" ? (
        <AppearanceSection />
      ) : tab === "ai" ? (
        <AiSection />
      ) : tab === "notify" ? (
        <NotificationSection />
      ) : tab === "personal" ? (
        <PersonalizationSection profileId={activeProfile?.id ?? null} />
      ) : tab === "aimemory" ? (
        <AiMemorySection profileId={activeProfile?.id ?? null} />
      ) : tab === "websearch" ? (
        <WebSearchSection />
      ) : tab === "devicesync" ? (
        <DeviceSyncSection />
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

// ---------------- AI 记忆中心（DEV-0076 §九） ----------------

/** §九.1：我的 AI 画像——七字段编辑（文本 + 逗号分隔数组） */
function AiProfileEditor({ profileId }: { profileId: number }) {
  const [profile, setProfile] = useState<AiProfile | null>(null);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState("");

  useEffect(() => {
    (async () => {
      try {
        setProfile(await getAiProfile(profileId));
      } catch {
        setProfile(null);
      }
    })();
  }, [profileId]);

  if (!profile) return null;
  const set = (patch: Partial<AiProfile>) => setProfile({ ...profile, ...patch });
  const arrToText = (a: string[]) => a.join("，");
  const textToArr = (t: string) => t.split(/[，,]/).map((s) => s.trim()).filter(Boolean);

  return (
    <section className="card">
      <h2 className="card__title">我的 AI 画像</h2>
      <p className="muted" style={{ marginTop: 0 }}>
        AI 对你的长期理解。修改保存后立即生效（等同你亲自确认）。
      </p>
      <label className="field">
        <span>基本信息（年龄/职业/专业）</span>
        <input
          value={profile.basic_information ?? ""}
          onChange={(e) => set({ basic_information: e.target.value })}
        />
      </label>
      <label className="field">
        <span>当前状态</span>
        <input
          value={profile.current_status ?? ""}
          onChange={(e) => set({ current_status: e.target.value })}
        />
      </label>
      <label className="field">
        <span>能力基础（逗号分隔）</span>
        <input
          value={arrToText(profile.abilities)}
          onChange={(e) => set({ abilities: textToArr(e.target.value) })}
        />
      </label>
      <label className="field">
        <span>时间资源（逗号分隔）</span>
        <input
          value={arrToText(profile.resources)}
          onChange={(e) => set({ resources: textToArr(e.target.value) })}
        />
      </label>
      <label className="field">
        <span>限制条件（逗号分隔）</span>
        <input
          value={arrToText(profile.constraints)}
          onChange={(e) => set({ constraints: textToArr(e.target.value) })}
        />
      </label>
      <label className="field">
        <span>偏好（逗号分隔）</span>
        <input
          value={arrToText(profile.preferences)}
          onChange={(e) => set({ preferences: textToArr(e.target.value) })}
        />
      </label>
      <label className="field">
        <span>长期目标（逗号分隔）</span>
        <input
          value={arrToText(profile.long_term_goals)}
          onChange={(e) => set({ long_term_goals: textToArr(e.target.value) })}
        />
      </label>
      <button
        className="btn"
        disabled={saving}
        onClick={async () => {
          setSaving(true);
          setMsg("");
          try {
            await saveAiProfile(profileId, profile);
            setMsg("已保存。");
          } catch (e) {
            setMsg(`保存失败：${e}`);
          } finally {
            setSaving(false);
          }
        }}
      >
        保存画像
      </button>
      {msg ? <p className="muted">{msg}</p> : null}
    </section>
  );
}

/** §九.2：已确认记忆列表（修改/删除） */
function ConfirmedMemoryList({ profileId }: { profileId: number }) {
  const [items, setItems] = useState<AiMemoryItem[]>([]);
  const [editing, setEditing] = useState<AiMemoryItem | null>(null);

  const reload = () => listAiMemories(profileId).then((r) => setItems(r.confirmed)).catch(() => {});
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId]);

  return (
    <section className="card">
      <h2 className="card__title">我的长期记忆（{items.length}）</h2>
      {items.length === 0 ? (
        <p className="muted" style={{ marginTop: 0 }}>
          暂无已确认记忆。对话中确认的 AI 认知会出现在这里。
        </p>
      ) : (
        <ul style={{ paddingLeft: 0, listStyle: "none" }}>
          {items.map((m) => (
            <li key={m.id} style={{ marginBottom: 8 }}>
              <b>{m.memory_key || m.memory_type}</b>：{m.memory_value || m.source_excerpt}
              <span className="muted">（{m.memory_type}）</span>
              <button className="btn btn--ghost" style={{ marginLeft: 8 }} onClick={() => setEditing(m)}>
                修改
              </button>
              <button
                className="btn btn--ghost"
                style={{ marginLeft: 4 }}
                onClick={async () => {
                  if (!window.confirm("删除这条记忆？")) return;
                  await deleteAiMemory(profileId, m.id).catch(() => {});
                  reload();
                }}
              >
                删除
              </button>
            </li>
          ))}
        </ul>
      )}
      {editing ? (
        <MemoryEditModal
          profileId={profileId}
          item={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
          }}
        />
      ) : null}
    </section>
  );
}

/** §九.3：待确认区（确认/拒绝/编辑） */
function PendingMemoryList({ profileId }: { profileId: number }) {
  const [items, setItems] = useState<AiMemoryItem[]>([]);
  const [editing, setEditing] = useState<AiMemoryItem | null>(null);

  const reload = () => listAiMemories(profileId).then((r) => setItems(r.pending)).catch(() => {});
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId]);

  const act = async (fn: (id: number) => Promise<void>, id: number) => {
    await fn(id).catch(() => {});
    reload();
  };

  return (
    <section className="card">
      <h2 className="card__title">待确认信息（{items.length}）</h2>
      {items.length === 0 ? (
        <p className="muted" style={{ marginTop: 0 }}>
          没有等待确认的 AI 认知。
        </p>
      ) : (
        <ul style={{ paddingLeft: 0, listStyle: "none" }}>
          {items.map((m) => (
            <li key={m.id} style={{ marginBottom: 8 }}>
              <div>
                <b>{m.memory_value || m.memory_key}</b>
                <span className="muted">
                  （{m.memory_type === "ai_inference" ? "AI 推断" : "你的陈述"}
                  {m.source_excerpt ? `：「${m.source_excerpt}」` : ""}）
                </span>
              </div>
              <button className="btn" onClick={() => act((id) => confirmAiMemory(profileId, id), m.id)}>
                确认保存
              </button>
              <button
                className="btn btn--ghost"
                style={{ marginLeft: 4 }}
                onClick={() => setEditing(m)}
              >
                修改
              </button>
              <button
                className="btn btn--ghost"
                style={{ marginLeft: 4 }}
                onClick={() => act((id) => rejectAiMemory(profileId, id), m.id)}
              >
                忽略
              </button>
            </li>
          ))}
        </ul>
      )}
      {editing ? (
        <MemoryEditModal
          profileId={profileId}
          item={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
          }}
        />
      ) : null}
    </section>
  );
}

/** 记忆编辑弹窗（§五.4 修改：类型/键/内容/原话） */
function MemoryEditModal({
  profileId,
  item,
  onClose,
  onSaved,
}: {
  profileId: number;
  item: AiMemoryItem;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [memoryType, setMemoryType] = useState(item.memory_type);
  const [key, setKey] = useState(item.memory_key);
  const [value, setValue] = useState(item.memory_value);
  const [excerpt, setExcerpt] = useState(item.source_excerpt);
  const [err, setErr] = useState("");

  return (
    <div className="modal__overlay">
      <div className="modal">
        <div className="modal__title">编辑记忆</div>
        <label className="field">
          <span>类型</span>
          <select value={memoryType} onChange={(e) => setMemoryType(e.target.value)}>
            {[
              "user_fact",
              "user_opinion",
              "user_preference",
              "user_constraint",
              "goal_context",
              "ai_inference",
            ].map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>标签（键）</span>
          <input value={key} onChange={(e) => setKey(e.target.value)} />
        </label>
        <label className="field">
          <span>内容</span>
          <input value={value} onChange={(e) => setValue(e.target.value)} />
        </label>
        <label className="field">
          <span>来源原话（可追溯依据）</span>
          <input value={excerpt} onChange={(e) => setExcerpt(e.target.value)} />
        </label>
        {err ? <p className="form-error">{err}</p> : null}
        <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
          <button
            className="btn"
            onClick={async () => {
              try {
                await updateAiMemory(profileId, item.id, {
                  memory_type: memoryType,
                  category: item.category,
                  memory_key: key,
                  memory_value: value,
                  source_excerpt: excerpt,
                });
                onSaved();
              } catch (e) {
                setErr(String(e));
              }
            }}
          >
            保存
          </button>
          <button className="btn btn--ghost" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

/** DEV-0076 §九：AI 记忆中心（三区） */
function AiMemorySection({ profileId }: { profileId: number | null }) {
  if (profileId == null) {
    return (
      <section className="card">
        <h2 className="card__title">AI 记忆</h2>
        <p className="muted">请先选择学习档案。</p>
      </section>
    );
  }
  return (
    <>
      <AiProfileEditor profileId={profileId} />
      <PendingMemoryList profileId={profileId} />
      <ConfirmedMemoryList profileId={profileId} />
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

  /** DEV-0070 Phase F v2.0 §18：下载用户档案模板（填写后经「添加资料」上传） */
  async function downloadProfileTemplate() {
    try {
      const tpl = await getUserProfileTemplate();
      downloadTextFile("Higher_User_Profile_Template.md", tpl, "text/markdown");
      setMessage("用户档案模板已开始下载；填写个人资料后上传，Higher AI将建立你的个人理解模型。");
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

        {/* DEV-0070 Phase F v2.0 §18：Higher 用户档案（下载模板 → 填写 → 上传建立理解模型） */}
        <div className="pz__main" style={{ marginBottom: 12 }}>
          <div className="pz__main-info">
            <span className="pz__status pz__status--none">Higher 用户档案</span>
            <span className="muted pz__main-time">
              填写个人资料后上传，Higher AI将建立你的个人理解模型。
            </span>
          </div>
          <div className="btn-row pz__main-actions">
            <button className="btn btn--small" onClick={() => void downloadProfileTemplate()}>
              下载用户档案模板
            </button>
          </div>
        </div>

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

// ---------------- 外观（DEV-0064 §6/§33 · 壁纸 + 氛围） ----------------

function AppearanceSection() {
  const [prefs, setPrefs] = useState<AppearancePrefs>(DEFAULT_PREFS);
  /** 只需文件名做展示；图片本体（Blob）不进入 React state（§8）。 */
  const [wallpaperName, setWallpaperName] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setPrefs(loadPrefs());
    void loadWallpaper().then((rec) => setWallpaperName(rec?.name ?? null));
  }, []);

  /** §33 即时生效：保存 + 应用（不 reload / 不 navigate）。 */
  function updatePrefs(next: AppearancePrefs) {
    setPrefs(next);
    savePrefs(next);
    applyAppearance(next);
  }

  /** §7/§13：MIME + 大小校验 → IndexedDB 保存 → 立即预览；失败保留原壁纸。 */
  async function onPickFile(file: File | undefined) {
    if (!file) return;
    setError("");
    setMessage("");
    const invalid = validateWallpaperFile(file);
    if (invalid) {
      setError(invalid);
      return;
    }
    try {
      const rec = await setWallpaper(file, file.name);
      setWallpaperName(rec.name);
      setMessage("壁纸已应用。");
    } catch (e) {
      setError(`壁纸保存失败：${String(e)}`);
    }
  }

  /** §13 删除：清 IndexedDB + revoke + 回颜色氛围背景。 */
  async function onDeleteWallpaper() {
    setError("");
    setMessage("");
    try {
      await removeWallpaper();
      setWallpaperName(null);
      setMessage("已删除壁纸，回到颜色氛围背景。");
    } catch (e) {
      setError(`删除壁纸失败：${String(e)}`);
    }
  }

  /** DEV-0064R.2 §66：恢复推荐效果——保留壁纸与颜色氛围，只把三值调回 70/70/45。 */
  function onRecommended() {
    setError("");
    setMessage("");
    updatePrefs({
      ...prefs,
      visibility: LIMITS.visibility.def,
      saturation: LIMITS.saturation.def,
      overlay: LIMITS.overlay.def,
    });
    setMessage("已恢复推荐效果（70 / 70 / 45）。");
  }

  /** §67 恢复默认：删除壁纸 + default 主题 + 70 / 70 / 45。 */
  async function onReset() {
    setError("");
    setMessage("");
    try {
      await removeWallpaper();
      setWallpaperName(null);
      updatePrefs(DEFAULT_PREFS);
      setMessage("已恢复默认外观。");
    } catch (e) {
      setError(`恢复默认失败：${String(e)}`);
    }
  }

  return (
    <section className="card">
      <h2 className="card__title">外观</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      {/* A. 壁纸 */}
      <div className="appearance__group" style={{ marginTop: 0 }}>
        <div className="appearance__group-title">壁纸</div>
        <div className="appearance__preview">
          {wallpaperName ? (
            <>
              <span className="appearance__preview-name" title={wallpaperName}>
                {wallpaperName}
              </span>
              {/* §65：示例 Surface——预览 壁纸+半透明 Card+文字 的真实效果 */}
              <span className="appearance__preview-sample">卡片示例 · Aa</span>
            </>
          ) : (
            <span className="appearance__preview-empty">未设置自定义壁纸</span>
          )}
        </div>
        {/* §41：隐藏原生 file 控件，由 Higher Button 触发 */}
        <input
          ref={fileRef}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          style={{ display: "none" }}
          onChange={(e) => {
            void onPickFile(e.target.files?.[0]);
            e.target.value = "";
          }}
        />
        <div className="btn-row">
          <button className="btn" onClick={() => fileRef.current?.click()}>
            {wallpaperName ? "替换图片" : "导入图片"}
          </button>
          {wallpaperName && (
            <button className="btn btn--danger" onClick={() => void onDeleteWallpaper()}>
              删除壁纸
            </button>
          )}
        </div>
        <p className="muted" style={{ fontSize: 12, margin: "8px 0 0" }}>
          支持 PNG / JPG / WEBP，单张最大 20 MB。壁纸只保存在本机，不会上传、不会发送给 AI。
        </p>
      </div>

      {/* B. 壁纸效果（R.2 §12-§15：壁纸强度 / 色彩保留 / 压暗程度；推荐 70/70/45） */}
      <div className="appearance__group">
        <div className="appearance__group-title">壁纸效果</div>
        <div className="appearance__slider-row">
          <label htmlFor="ap-visibility">壁纸强度</label>
          <input
            id="ap-visibility"
            type="range"
            min={LIMITS.visibility.min}
            max={LIMITS.visibility.max}
            value={prefs.visibility}
            onChange={(e) => updatePrefs({ ...prefs, visibility: Number(e.target.value) })}
          />
          <output>{prefs.visibility}</output>
        </div>
        <div className="appearance__slider-row">
          <label htmlFor="ap-saturation">色彩保留</label>
          <input
            id="ap-saturation"
            type="range"
            min={LIMITS.saturation.min}
            max={LIMITS.saturation.max}
            value={prefs.saturation}
            onChange={(e) => updatePrefs({ ...prefs, saturation: Number(e.target.value) })}
          />
          <output>{prefs.saturation}</output>
        </div>
        <div className="appearance__slider-row">
          <label htmlFor="ap-overlay">压暗程度</label>
          <input
            id="ap-overlay"
            type="range"
            min={LIMITS.overlay.min}
            max={LIMITS.overlay.max}
            value={prefs.overlay}
            onChange={(e) => updatePrefs({ ...prefs, overlay: Number(e.target.value) })}
          />
          <output>{prefs.overlay}</output>
        </div>
        <p className="muted" style={{ fontSize: 12, margin: "8px 0 0" }}>
          推荐效果 70 / 70 / 45：无需拉满滑杆即可看清原图构图与主要颜色，同时 Higher 保持深色 UI、正文清晰。
        </p>
      </div>

      {/* C. 颜色氛围 */}
      <div className="appearance__group">
        <div className="appearance__group-title">颜色氛围</div>
        <div className="appearance__themes">
          {THEMES.map((t) => (
            <button
              key={t.id}
              className={
                "appearance__theme-btn" +
                (prefs.theme === t.id ? " appearance__theme-btn--active" : "")
              }
              onClick={() => updatePrefs({ ...prefs, theme: t.id })}
            >
              <span
                className="appearance__theme-swatch"
                style={{ background: THEME_SWATCH[t.id] }}
              />
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {/* D. 恢复推荐效果 / 恢复默认（§66-§67） */}
      <div className="appearance__group">
        <div className="btn-row">
          <button className="btn" onClick={onRecommended}>
            恢复推荐效果
          </button>
          <button className="btn" onClick={() => void onReset()}>
            恢复默认
          </button>
        </div>
        <p className="muted" style={{ fontSize: 11, margin: "8px 0 0" }}>
          恢复推荐效果只调整三值为 70 / 70 / 45（保留壁纸与氛围）；恢复默认会删除壁纸并回到默认深色。
        </p>
      </div>
    </section>
  );
}

// ---------------- 设备同步（DEV-SYNC-001 Local-First LAN Sync MVP） ----------------

/** 相对时间展示（最后同步） */
function syncTimeLabel(iso: string | null): string {
  if (!iso) return "从未";
  return formatDateTime(iso);
}

/**
 * 设备同步（DEV-SYNC-003 · QR Pairing）：
 * Windows = 服务器（启动 / 停止 / 已配对设备；配对入口 = /sync 二维码，§十）；
 * Android = 客户端（扫描电脑二维码 → 自动连接 + 配对 + 自动首次双向同步，§十一）。
 * 仅同步 study_profiles / goals / learning_items / tasks；仅建议在可信局域网中使用。
 */
function DeviceSyncSection() {
  return IS_ANDROID ? <DeviceSyncClient /> : <DeviceSyncServer />;
}

/** Windows：同步服务器（启停 / 已配对设备；配对二维码入口在 /sync 工作台，§十） */
function DeviceSyncServer() {
  const navigate = useNavigate();
  const [status, setStatus] = useState<SyncServerStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  /** DEV-SYNC-001-F1 §4：try/catch/finally —— IPC 失败必须退出 loading 并显示真实错误 */
  async function refresh() {
    try {
      setStatus(await syncServerStatus());
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, []);

  async function start() {
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const s = await syncServerStart();
      setStatus(s);
      setMessage("同步服务器已启动，等待手机连接……");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function stop() {
    setBusy(true);
    setMessage("");
    setError("");
    try {
      setStatus(await syncServerStop());
      setMessage("同步服务器已停止。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** Windows 侧「立即同步」：服务器被动等待手机发起 → 展示待同步状态 */
  async function syncNow() {
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const s = await syncServerStatus();
      setStatus(s);
      if (s.pending_outbox > 0) {
        setMessage(`有 ${s.pending_outbox} 条变更待同步：请在手机的「设备同步」中点击「立即同步」发起连接。`);
      } else {
        setMessage("本地暂无待同步变更。双向同步由手机端「立即同步」发起。");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // DEV-SYNC-001-F1 §4：IPC 失败 → 退出 loading 并显示「初始化失败 + 真实错误 + 重试」，
  // 禁止吞错导致永久「加载中…」
  if (!status && error) {
    return (
      <section className="card">
        <h2 className="card__title">设备同步</h2>
        <div className="alert alert--error">
          <p style={{ margin: "0 0 4px", fontWeight: 600 }}>设备同步初始化失败</p>
          <p style={{ margin: 0, fontSize: 12, wordBreak: "break-all" }}>{error}</p>
        </div>
        <div className="btn-row">
          <button
            className="btn btn--primary"
            disabled={busy}
            onClick={() => {
              setError("");
              setLoading(true);
              void refresh();
            }}
          >
            重试
          </button>
        </div>
        <p className="muted" style={{ fontSize: 11, margin: "12px 0 0" }}>
          当前仅建议在可信局域网中使用。重试仍失败时，请重启 Higher（npm run tauri dev）后再试。
        </p>
      </section>
    );
  }

  if (loading || !status) return <section className="card"><p className="muted">加载中…</p></section>;

  return (
    <section className="card">
      <h2 className="card__title">设备同步</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      {!status.running ? (
        <>
          <p className="muted" style={{ margin: "0 0 12px" }}>
            同步服务尚未启动。在同一可信 Wi-Fi 下，点击「启动同步」把电脑上的学习档案
            （Profile / 目标树 / 学习项 / 任务）同步到手机。服务器默认关闭，只有你主动启动时才会接受连接。
          </p>
          <div className="btn-row">
            <button className="btn btn--primary" disabled={busy} onClick={() => void start()}>
              {busy ? "启动中…" : "启动同步"}
            </button>
          </div>
        </>
      ) : (
        <>
          {/* DEV-SYNC-003 §十：IP / 端口 / 配对码退出主界面；
              配对入口 = /sync「添加手机」二维码（候选 IP 自动尝试，无需手工输入） */}
          <div className="sync-panel">
            <div className="sync-panel__title">
              {status.device_name || "Higher Windows"}
              <span className="sync-panel__dot" aria-hidden="true" />
              同步服务运行中
            </div>
            <div className="sync-panel__row"><span>端口</span><b>{status.port}</b></div>
            <div className="sync-panel__row">
              <span>已配对设备</span>
              <b>{status.peers.length > 0 ? `${status.peers.length} 台` : "尚未配对"}</b>
            </div>
          </div>
          {status.peers.length === 0 && (
            <p className="muted" style={{ margin: "8px 0 12px" }}>
              等待手机连接……在「同步」页点击「添加手机」生成二维码，手机 Higher 扫码即可自动配对——
              无需输入 IP、端口或配对码。
            </p>
          )}
        </>
      )}

      {status.peers.length > 0 && (
        <div className="sync-peers">
          <div className="sync-peers__title">我的设备</div>
          {status.peers.map((p) => (
            <div key={p.peer_device_id} className="sync-peers__item">
              <span>{p.peer_name || "Higher 设备"}（{p.peer_platform || "unknown"}）</span>
              <span className="muted">最后同步：{syncTimeLabel(p.last_sync_at)}</span>
            </div>
          ))}
        </div>
      )}

      {status.pending_conflicts > 0 && (
        <div className="alert alert--error">有 {status.pending_conflicts} 条同步冲突，暂未覆盖。</div>
      )}
      {status.pending_outbox > 0 && (
        <p className="muted" style={{ margin: "8px 0" }}>待同步变更：{status.pending_outbox} 条</p>
      )}

      {status.running && (
        <div className="btn-row">
          <button className="btn btn--primary" disabled={busy} onClick={() => void syncNow()}>
            {busy ? "检查中…" : "立即同步"}
          </button>
          <button className="btn" disabled={busy} onClick={() => void stop()}>
            停止同步
          </button>
        </div>
      )}

      {/* DEV-SYNC-002 §十一 + DEV-SYNC-003 §十：日常同步 / 配对二维码都在同步工作台 */}
      <div className="btn-row" style={{ marginTop: status.running ? 0 : 4 }}>
        <button className="btn" onClick={() => navigate("/sync")}>
          {status.running && status.peers.length === 0 ? "添加手机（二维码配对）" : "打开同步"}
        </button>
      </div>

      <p className="muted" style={{ fontSize: 11, margin: "12px 0 0" }}>
        首版仅同步学习档案 / 目标 / 学习项 / 任务，不同步 API Key、密码、附件与知识正文。
        当前仅建议在可信局域网中使用。
      </p>
    </section>
  );
}

/**
 * Android（移动端）：同步客户端（DEV-SYNC-003 §十一 · 扫码配对）。
 * [扫描电脑二维码] → 相机权限（拒绝可恢复）→ 扫码 → 候选 IP 自动连接 →
 * 一次性 token 配对 → 自动首次双向同步（§十四）；失败分类提示 + [重新扫描]（§十三）。
 *
 * DEV-SYNC-003-F2 §九/§十/§十一：
 * - windowed:true（HTML UI 保留：WebView 透明区见相机）+ 扫描框覆盖层 + [取消]；
 * - 30s 无识别自动 cancel() 恢复页面（禁止永久 pending）；
 * - 组件卸载强制 cancel()（Android Back / 路由离开时拆除相机，禁止卡 scanner）。
 */
const QR_SCAN_TIMEOUT_MS = 30_000;

function DeviceSyncClient() {
  const { enterProfile } = useActiveProfile();
  const [status, setStatus] = useState<SyncClientStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [imported, setImported] = useState<SyncPairOutcome["imported_profiles"] | null>(null);
  const [lastSummary, setLastSummary] = useState<SyncSummary | null>(null);
  // §十一/§十三：扫码配对过程态（正在连接）与失败态（可恢复）
  const [connecting, setConnecting] = useState(false);
  const [scanError, setScanError] = useState("");
  const [permDenied, setPermDenied] = useState(false);
  const [confirmUnpair, setConfirmUnpair] = useState(false);
  // DEV-SYNC-003-F2：扫码进行中（覆盖层）与超时恢复态
  const [scanning, setScanning] = useState(false);
  const [scanTimeout, setScanTimeout] = useState(false);

  async function refresh() {
    try {
      setStatus(await syncClientStatus());
    } catch (e) {
      setError(String(e));
    }
  }

  // §七：已配对时启动本机 listener（电脑可主动反向「立即同步」）；离开页面停止。
  // F2 §十一：卸载时强制拆除扫码相机（Android Back / 路由离开不残留 scanner）。
  useEffect(() => {
    void (async () => {
      try {
        const s = await syncClientStatus();
        setStatus(s);
        if (s.paired) {
          await syncServerStart().catch(() => {});
        }
      } catch (e) {
        setError(String(e));
      }
    })();
    return () => {
      cancelBarcodeScan().catch(() => {});
      syncServerStop().catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // F2.1 §三：扫码期间建立真正透明模式 —— html/body 加 higher-qr-scanning，
  // CSS 仅在该态把 WebView 各层背景置透明（Camera PreviewView 在 WebView 后方可见）。
  // 覆盖全部出口：scanning 置 false（成功/取消/超时/异常 finally）或组件卸载时本 cleanup 必然移除。
  useEffect(() => {
    if (!scanning) return;
    document.documentElement.classList.add("higher-qr-scanning");
    document.body.classList.add("higher-qr-scanning");
    return () => {
      document.documentElement.classList.remove("higher-qr-scanning");
      document.body.classList.remove("higher-qr-scanning");
    };
  }, [scanning]);

  /** §六：扫码内容为 base64（UTF-8 JSON）；解码失败或本身即 JSON 时按原文使用 */
  function decodeScanContent(raw: string): string {
    const t = raw.trim();
    if (t.startsWith("{")) return t;
    try {
      const bin = atob(t);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      return new TextDecoder().decode(bytes);
    } catch {
      return t;
    }
  }

  // F2.1：手动结束扫码等待的句柄（插件 cancel() 的 reject 实测未可靠传回 JS，
  // 取消/卸载时由本地 race 立即收尾，UI 不卡透明态）
  const scanAbortRef = useRef<((reason: string) => void) | null>(null);

  /** §十一：取消扫码（本地 race 立即恢复 + 插件 cancel 拆相机） */
  async function cancelScan() {
    try {
      scanAbortRef.current?.("cancelled");
    } catch {
      /* 已收尾 */
    }
    scanAbortRef.current = null;
    await cancelBarcodeScan().catch(() => {});
  }

  /** §十一：扫码配对全流程（权限 → 相机扫码 → 自动连接 → 配对 + 首次双向同步） */
  async function startPairScan() {
    setBusy(true);
    setError("");
    setMessage("");
    setScanError("");
    setScanTimeout(false);
    setPermDenied(false);
    setLastSummary(null);
    setImported(null);
    try {
      // 相机权限：先查后申请；仍拒绝 → 权限说明 + [去开启]（§十一，QR-TC011）
      let perm = await checkPermissions().catch(() => "prompt");
      if (perm !== "granted") {
        perm = await requestPermissions().catch(() => "denied");
      }
      if (perm !== "granted") {
        setPermDenied(true);
        return;
      }
      // F2 §九：windowed:true —— HTML UI 保留（扫描框覆盖层 + [取消]），WebView 透明区见相机
      setScanning(true);
      // F2 §十：30s 无识别自动取消（禁止永久 pending）；F2.1：本地可中断句柄（取消/卸载立即收尾）
      const scanWait = new Promise<never>((_, reject) => {
        scanAbortRef.current = (reason: string) => {
          scanAbortRef.current = null;
          reject(new Error(reason));
        };
        window.setTimeout(
          () => {
            scanAbortRef.current = null;
            reject(new Error("HIGHER_SCAN_TIMEOUT"));
          },
          QR_SCAN_TIMEOUT_MS
        );
      });
      const scanned = await Promise.race([
        scan({ formats: [Format.QRCode], windowed: true }),
        scanWait,
      ]);
      // 真机验收 hook（QR-F2-TC01：scan resolve 的原始 content）
      (window as unknown as { __higherQrDebug?: { content: string; ts: number } }).__higherQrDebug = {
        content: scanned.content,
        ts: Date.now(),
      };
      const payload = decodeScanContent(scanned.content);
      // 扫码成功 → 自动进入「正在连接 Higher Windows…」（§十一）
      setConnecting(true);
      const r = await syncPairViaQr(payload);
      // §十四：配对成功 + 自动首次双向同步结果（直觉化统计）
      const names = r.pair.imported_profiles.map((p) => `「${p.name}」`).join("、");
      setMessage(
        `配对成功，已连接 ${r.server_name || "Higher Windows"}。` +
          (r.sync
            ? `首次同步完成：电脑 → 手机 ${r.sync.pushed} 条 / 手机 → 电脑 ${r.sync.pulled} 条` +
              (r.sync.conflicts > 0 ? ` / 冲突 ${r.sync.conflicts} 条` : "") +
              "。"
            : "") +
          (names ? `已从电脑导入学习档案：${names}。` : "")
      );
      setImported(r.pair.imported_profiles);
      await syncServerStart().catch(() => {}); // 配对完成即监听，电脑可反向连接
      await refresh();
    } catch (e) {
      const msg = String(e);
      // 用户主动取消扫码：静默返回（页面保持可再次扫码，§十三）
      if (/cancel/i.test(msg)) return;
      if (msg.includes("HIGHER_SCAN_TIMEOUT")) {
        // §十：超时 → 自动 cancel 相机 → 恢复页面 + 指引 + [重新扫描]
        await cancelScan();
        setScanTimeout(true);
        return;
      }
      setScanError(msg);
    } finally {
      scanAbortRef.current = null;
      setScanning(false);
      setBusy(false);
      setConnecting(false);
    }
  }

  /** §九：解除配对（只删 trust/token，业务数据保留；重新扫码即可再连） */
  async function unpairPeer() {
    if (!status?.peer_device_id) return;
    setBusy(true);
    setError("");
    try {
      await syncUnpair(status.peer_device_id);
      setConfirmUnpair(false);
      setMessage("已解除配对。重新扫码即可重新建立连接，学习数据保持不变。");
      setImported(null);
      setLastSummary(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function syncNow() {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const s = await syncClientSyncNow();
      setLastSummary(s);
      setMessage(
        s.pushed === 0 && s.pulled === 0
          ? "同步完成：两台设备数据已是最新。"
          : "同步完成。"
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!status) return <section className="card"><p className="muted">加载中…</p></section>;
  const lastSync = syncTimeLabel(status.last_sync_at);

  return (
    <section className="card">
      <h2 className="card__title">Higher 设备同步</h2>
      {error && <div className="alert alert--error">{error}</div>}
      {message && <div className="alert alert--ok">{message}</div>}

      {!status.paired ? (
        <>
          {/* §十一：未配对 = 单一扫码入口，小字提示同 Wi-Fi；无 IP/端口/配对码输入 */}
          <p className="muted" style={{ margin: "0 0 12px" }}>
            确保手机与电脑连接同一 Wi-Fi。
          </p>
          <div className="btn-row">
            <button className="btn btn--primary" disabled={busy} onClick={() => void startPairScan()}>
              {busy ? (connecting ? "正在连接 Higher Windows…" : "正在打开相机…") : "扫描电脑二维码"}
            </button>
          </div>

          {/* §十一：首次拒绝相机权限 → 权限说明 + [去开启]（QR-TC011 不崩溃、可恢复） */}
          {permDenied && (
            <div className="alert alert--error" style={{ marginTop: 12 }}>
              <p style={{ margin: "0 0 8px", fontWeight: 600 }}>需要相机权限才能扫描二维码</p>
              <p style={{ margin: "0 0 8px", fontSize: 12 }}>
                扫码配对需要使用相机识别电脑 Higher 显示的二维码，相机不会用于其它用途。
                可在系统设置（应用 → Higher → 权限 → 相机）中手动开启。
              </p>
              <div className="btn-row">
                <button className="btn" onClick={() => void openAppSettings()}>
                  去开启
                </button>
                <button className="btn" disabled={busy} onClick={() => void startPairScan()}>
                  重新扫描
                </button>
              </div>
            </div>
          )}

          {/* §十三：扫码/连接失败 → 分类提示 + 恢复路径（页面永不卡死） */}
          {scanError && (
            <div className="alert alert--error" style={{ marginTop: 12 }}>
              <p style={{ margin: "0 0 8px", fontWeight: 600 }}>无法连接 Higher Windows</p>
              <p style={{ margin: 0, fontSize: 12, whiteSpace: "pre-line", wordBreak: "break-all" }}>
                {scanError}
              </p>
              <div className="btn-row" style={{ marginTop: 8 }}>
                <button className="btn btn--primary" disabled={busy} onClick={() => void startPairScan()}>
                  重新扫描
                </button>
              </div>
            </div>
          )}

          {/* F2 §十：30s 超时自动取消 → 指引 + [重新扫描]（禁止永久 pending） */}
          {scanTimeout && (
            <div className="alert alert--error" style={{ marginTop: 12 }}>
              <p style={{ margin: "0 0 8px", fontWeight: 600 }}>暂未识别到二维码</p>
              <p style={{ margin: "0 0 8px", fontSize: 12 }}>
                请保持二维码完整、清晰，并确认摄像头已经对焦。
              </p>
              <div className="btn-row">
                <button className="btn btn--primary" disabled={busy} onClick={() => void startPairScan()}>
                  重新扫描
                </button>
              </div>
            </div>
          )}
        </>
      ) : (
        <>
          <div className="sync-panel">
            <div className="sync-panel__title">
              {status.peer_name || "Higher Windows"}
              <span className="sync-panel__dot" aria-hidden="true" />
              {status.peer_addr ? "已连接" : "等待对端"}
            </div>
            <div className="sync-panel__row"><span>地址</span><b>{status.peer_addr || "—"}</b></div>
            <div className="sync-panel__row"><span>最后同步</span><b>{lastSync}</b></div>
            <div className="sync-panel__row"><span>待发送</span><b>{status.pending_outbox} 条</b></div>
          </div>
          {status.pending_conflicts > 0 && (
            <div className="alert alert--error">有 {status.pending_conflicts} 条同步冲突，暂未覆盖。</div>
          )}
          {lastSummary && (
            <div className="sync-result">
              <div className="sync-result__title">
                {lastSummary.pushed === 0 && lastSummary.pulled === 0
                  ? "已同步 · 两台设备数据已是最新"
                  : "同步完成"}
              </div>
              {lastSummary.pushed > 0 && (
                <div className="sync-result__row">
                  <span>发送到电脑</span>
                  <span>
                    新增 {lastSummary.sent_detail.inserted} · 更新 {lastSummary.sent_detail.updated} · 删除{" "}
                    {lastSummary.sent_detail.deleted}
                  </span>
                </div>
              )}
              {lastSummary.pulled > 0 && (
                <div className="sync-result__row">
                  <span>从电脑接收</span>
                  <span>
                    新增 {lastSummary.received_detail.inserted} · 更新 {lastSummary.received_detail.updated} ·
                    删除 {lastSummary.received_detail.deleted}
                  </span>
                </div>
              )}
              {lastSummary.conflicts > 0 && (
                <div className="sync-result__row">
                  <span>冲突</span>
                  <span>{lastSummary.conflicts} 条（未覆盖）</span>
                </div>
              )}
            </div>
          )}
          <div className="btn-row">
            <button className="btn btn--primary" disabled={busy} onClick={() => void syncNow()}>
              {busy ? "同步中…" : "立即同步"}
            </button>
            {/* §九：解除配对（两步确认；只删 trust/token，业务数据保留） */}
            {confirmUnpair ? (
              <>
                <button className="btn btn--danger" disabled={busy} onClick={() => void unpairPeer()}>
                  确认解除配对
                </button>
                <button className="btn" disabled={busy} onClick={() => setConfirmUnpair(false)}>
                  取消
                </button>
              </>
            ) : (
              <button className="btn" disabled={busy} onClick={() => setConfirmUnpair(true)}>
                解除配对
              </button>
            )}
          </div>
        </>
      )}

      {/* §五：配对导入档案 → 明确去向 + 一键切换（active_profile_id 保持本机状态） */}
      {imported && imported.length > 0 && (
        <div className="sync-imported">
          <div className="sync-imported__title">已从电脑导入的学习档案</div>
          {imported.map((p) => (
            <div key={p.sync_id} className="sync-imported__row">
              <span>{p.name}</span>
              <button className="btn" onClick={() => void enterProfile(p.local_id)}>
                切换到该档案
              </button>
            </div>
          ))}
        </div>
      )}

      <p className="muted" style={{ fontSize: 11, margin: "12px 0 0" }}>
        两台设备为平等的 Higher 终端：任意一端点击「立即同步」都会双向交换
        学习档案 / 目标 / 学习项 / 任务；不同步 API Key、密码、附件与知识正文。
        当前仅建议在可信局域网中使用。
      </p>

      {/* F2 §九：windowed:true 扫码覆盖层 —— WebView 透明区见相机，HTML 提供
          标题 / 扫描框 / 提示 / [取消]（用户可退出，禁止卡 scanner） */}
      {scanning && (
        <div className="qr-scan-overlay">
          <div className="qr-scan-title">扫描二维码</div>
          <div className="qr-scan-frame">
            <span className="qr-scan-hint">请将二维码放入框内</span>
          </div>
          <button className="btn qr-scan-cancel" onClick={() => void cancelScan()}>
            取消
          </button>
        </div>
      )}
    </section>
  );
}
