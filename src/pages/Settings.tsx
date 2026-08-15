import { useEffect, useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import {
  getAiSettings,
  getNotificationEnabled,
  listBackups,
  listArchivedTasksByProfile,
  saveAiSettings,
  setNotificationEnabled,
  syncNotifications,
  testAiConnection,
  unarchiveTask,
} from "../api";
import {
  canSwitchProfile,
  useActiveProfile,
} from "../contexts/ActiveProfileContext";
import { createStudyProfile, executeProfileCleanup, previewProfileCleanup, updateStudyProfile } from "../api";
import { PROFILE_TYPE_LABELS } from "../types";
import type { CleanupPreview, ProfileType, StudyProfile, Task } from "../types";
import { todayDate } from "../utils";

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
  const [tab, setTab] = useState<"profile" | "ai" | "notify" | "data">("profile");

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">设置</h1>
      </header>

      <div className="review-window" style={{ marginBottom: 16 }}>
        <button
          className={
            "review-window__item" + (tab === "profile" ? " review-window__item--active" : "")
          }
          onClick={() => setTab("profile")}
        >
          学习档案
        </button>
        <button className={"review-window__item" + (tab === "ai" ? " review-window__item--active" : "")}
          onClick={() => setTab("ai")}
        >
          AI 设置
        </button>
        <button
          className={"review-window__item" + (tab === "notify" ? " review-window__item--active" : "")}
          onClick={() => setTab("notify")}
        >
          学习提醒
        </button>
        <button
          className={"review-window__item" + (tab === "data" ? " review-window__item--active" : "")}
          onClick={() => setTab("data")}
        >
          数据管理
        </button>
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

const CLEANUP_OPTIONS: { key: CleanupScopeKey; label: string; desc: string }[] = [
  { key: "clear_today", label: "清除今天的活动数据", desc: "删除今天的任务 / 学习记录 / 验证 / 问题 / 调整 / 学习附件" },
  { key: "keep_today", label: "只保留今天的活动数据", desc: "删除今天以外其他日期的活动数据（长期学习系统结构保留）" },
  { key: "clear_month", label: "清除本月的活动数据", desc: "删除本月全部活动数据" },
  { key: "keep_month", label: "只保留本月的活动数据", desc: "删除其他月份的活动数据" },
  { key: "clear_year", label: "清除今年的活动数据", desc: "删除今年全部活动数据" },
  { key: "keep_year", label: "只保留今年的活动数据", desc: "删除其他年份的活动数据" },
  { key: "full_reset", label: "清空当前档案全部数据", desc: "保留档案外壳，删除目标 / 知识 / 阶段 / 计划 / 全部记录与附件" },
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
        {CLEANUP_OPTIONS.map((o) => (
          <li key={o.key} className={"cleanup-item" + (o.key === "full_reset" ? " cleanup-item--danger" : "")}>
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

// ---------------- AI 设置 ----------------

function AiSection() {
  const [baseUrl, setBaseUrl] = useState("https://api.deepseek.com");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("deepseek-v4-flash");
  const [thinking, setThinking] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    (async () => {
      try {
        const s = await getAiSettings();
        setBaseUrl(s.base_url || "https://api.deepseek.com");
        setApiKey(s.api_key ?? "");
        setModel(s.model || "deepseek-v4-flash");
        setThinking(!!s.thinking_enabled);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  async function save() {
    setSaving(true);
    setMessage("");
    setError("");
    try {
      await saveAiSettings({
        baseUrl: baseUrl.trim() || "https://api.deepseek.com",
        apiKey: apiKey.trim(),
        model: model.trim() || "deepseek-v4-flash",
        thinkingEnabled: thinking,
      });
      setMessage("已保存");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function test() {
    setTesting(true);
    setMessage("");
    setError("");
    try {
      // 先保存当前输入再测试（避免测试旧配置）
      await saveAiSettings({
        baseUrl: baseUrl.trim() || "https://api.deepseek.com",
        apiKey: apiKey.trim(),
        model: model.trim() || "deepseek-v4-flash",
        thinkingEnabled: thinking,
      });
      const r = await testAiConnection();
      setMessage(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
    }
  }

  if (loading) return <section className="card"><p className="muted">加载中…</p></section>;

  return (
    <>
      <section className="card">
        <h2 className="card__title">AI 设置（DeepSeek）</h2>
        {error && <div className="alert alert--error">{error}</div>}
        {message && <div className="alert alert--ok">{message}</div>}

        <label className="modal__field">
          Provider
          <select className="modal__input" value="deepseek" disabled>
            <option value="deepseek">DeepSeek</option>
          </select>
        </label>

        <label className="modal__field">
          API Base URL
          <input className="modal__input" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.deepseek.com" />
        </label>

        <label className="modal__field">
          API Key（明文显示与保存，仅保存在本机数据库）
          <input className="modal__input" type="text" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-..." autoComplete="off" />
        </label>

        <label className="modal__field">
          Model
          <select
            className="modal__input"
            value={["deepseek-v4-flash", "deepseek-v4-pro"].includes(model) ? model : "__custom"}
            onChange={(e) => {
              if (e.target.value !== "__custom") setModel(e.target.value);
            }}
          >
            <option value="deepseek-v4-flash">deepseek-v4-flash</option>
            <option value="deepseek-v4-pro">deepseek-v4-pro</option>
            <option value="__custom">{model ? `自定义：${model}` : "自定义模型名"}</option>
          </select>
          <input
            className="modal__input"
            style={{ marginTop: 6 }}
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="或直接输入其他模型名"
          />
        </label>

        <label className="modal__field" style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <input type="checkbox" checked={thinking} onChange={(e) => setThinking(e.target.checked)} />
          Thinking Mode（深度思考；若服务商返回模型错误请关闭）
        </label>

        <div className="btn-row">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
          <button className="btn" onClick={test} disabled={testing}>
            {testing ? "测试中…" : "测试连接"}
          </button>
        </div>
      </section>

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
