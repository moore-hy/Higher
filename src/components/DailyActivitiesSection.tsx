import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  createFollowupTaskFromSession,
  deleteSession,
  endSession,
  organizeSessionIntoKnowledge,
  setSessionActivityKind,
  setSessionGoal,
  startQuickSession,
  startSession,
  updateSessionTitle,
} from "../api";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "./ActiveSessionConflictModal";
import type { DailyActivityRow, Goal, LearningItem } from "../types";
import { todayDate } from "../utils";

/**
 * 今日活动区（DEV-0053 §24-36 / DEV-0054 §42-52）。
 *
 * Today 与 Calendar Daily Report 共用同一 StudySession View（§72-73 不复制数据）：
 * - DEV-0054 §53-54：移除四个大分组 → 默认时间倒序单列表（最新在上）
 * - §55：Section Header 右侧 Filter chips 全部|核心|常规|积累|计划外
 * - 行（§42-52）：轻量文字 Badge + Title（可点击打开）+ 右 `HH:mm · Xm` / `进行中`
 *   + 按状态直接显示动作（§46-50）：
 *     active→进入学习/结束；ended+已归类→打开/继续学习；ended+未归类→打开/整理进知识；
 *     accumulation→打开/继续
 * - ⋯（§51）：编辑标题 / 修改分类 / 调整目标关联 / 调整知识关联 / 生成后续任务 / 删除
 *   （「打开」为直接动作，不再进 ⋯ §127）
 */

/** §68：UTC SQLite datetime → UTC+8 HH:mm（与后端 date(started_at,'+8 hours') 同一语义） */
export function studyClockHHMM(raw: string): string {
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return "";
  const t = new Date(d.getTime() + 8 * 3600_000);
  return `${String(t.getUTCHours()).padStart(2, "0")}:${String(t.getUTCMinutes()).padStart(2, "0")}`;
}

/** 秒 → "XhYm" / "Xm"；null = 进行中（§33） */
export function durationShort(seconds: number | null | undefined): string {
  if (seconds == null) return "进行中";
  const m = Math.floor(seconds / 60);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h${m % 60}m`;
  return `${m}m`;
}

const KIND_ORDER = ["core", "regular", "accumulation", "unplanned"] as const;
type ActKind = (typeof KIND_ORDER)[number];
type ActFilter = "all" | ActKind;

const KIND_LABELS: Record<ActKind, string> = {
  core: "核心学习",
  regular: "常规学习",
  accumulation: "积累学习",
  unplanned: "计划外学习",
};

/** §44/§55：轻量短标签（Badge / Filter chips 共用；13px 无大色块） */
const KIND_SHORT: Record<ActKind, string> = {
  core: "核心",
  regular: "常规",
  accumulation: "积累",
  unplanned: "计划外",
};

/** started_at → 可比较毫秒（时间倒序排序用） */
function startedMs(raw: string): number {
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  return isNaN(d.getTime()) ? 0 : d.getTime();
}

export default function DailyActivitiesSection({
  profileId,
  activities,
  items,
  goals,
  onChanged,
  emptyNote,
  onEmptyQuickStart,
}: {
  profileId: number;
  activities: DailyActivityRow[];
  items: LearningItem[];
  goals: Goal[];
  onChanged: () => void | Promise<void>;
  /** 空态文案（Today：今天还没有学习记录。/ Calendar：这一天还没有学习记录。） */
  emptyNote?: string;
  /** 空态「开始快速学习」（§110：仅 Today 传） */
  onEmptyQuickStart?: () => void;
}) {
  const navigate = useNavigate();
  const [error, setError] = useState("");
  const [filter, setFilter] = useState<ActFilter>("all");
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const [renameFor, setRenameFor] = useState<DailyActivityRow | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [kindFor, setKindFor] = useState<DailyActivityRow | null>(null);
  const [goalFor, setGoalFor] = useState<DailyActivityRow | null>(null);
  const [goalValue, setGoalValue] = useState<number | "">("");
  const [knowFor, setKnowFor] = useState<DailyActivityRow | null>(null);
  const [knowSearch, setKnowSearch] = useState("");
  const [followupFor, setFollowupFor] = useState<DailyActivityRow | null>(null);
  const [followupDate, setFollowupDate] = useState(todayDate());
  const [followupMinutes, setFollowupMinutes] = useState("");
  const [deleting, setDeleting] = useState<DailyActivityRow | null>(null);
  const [endingId, setEndingId] = useState<number | null>(null);
  const { conflict, guard, close } = useActiveSessionConflict();

  /** §54：时间倒序单列表（最新在上）+ §55 filter */
  const rows = useMemo(() => {
    const sorted = [...activities].sort((a, b) => startedMs(b.started_at) - startedMs(a.started_at));
    if (filter === "all") return sorted;
    return sorted.filter((a) => (a.activity_kind ?? "unplanned") === filter);
  }, [activities, filter]);

  /** 各 filter 的计数（chips 上显示） */
  const kindCount = useMemo(() => {
    const m = new Map<string, number>();
    for (const a of activities) {
      const k = a.activity_kind ?? "unplanned";
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  }, [activities]);

  const filteredItems = useMemo(() => {
    const q = knowSearch.trim().toLowerCase();
    return q ? items.filter((i) => i.name.toLowerCase().includes(q)) : items;
  }, [items, knowSearch]);

  async function run(fn: () => Promise<unknown>, done?: () => void) {
    setError("");
    try {
      await fn();
      done?.();
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  }

  /** §35 继续学习：有知识 → 同知识新 Session；无 → 快速学习（Start Guard 冲突 → 弹窗） */
  async function continueStudy(a: DailyActivityRow) {
    setMenuFor(null);
    setError("");
    try {
      const s =
        a.learning_item_id != null
          ? await startSession(a.learning_item_id)
          : await startQuickSession(profileId);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guard(e)) return;
      setError(String(e));
    }
  }

  /** §48 active 行「结束」：endSession 后刷新 */
  async function endOne(a: DailyActivityRow) {
    setEndingId(a.id);
    setError("");
    try {
      await endSession(a.id);
      await onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setEndingId(null);
    }
  }

  return (
    <div className="today__acts">
      {error && <div className="alert alert--error">{error}</div>}

      {activities.length === 0 ? (
        <div className="today__empty">
          <p className="today__empty-note">{emptyNote ?? "这一天还没有学习记录。"}</p>
          {onEmptyQuickStart && (
            <div className="btn-row today__empty-btns">
              <button className="btn btn--small btn--primary" onClick={onEmptyQuickStart}>
                开始快速学习
              </button>
            </div>
          )}
        </div>
      ) : (
        <>
          {/* §55 Filter chips（默认全部；active chip accent） */}
          <div className="today__acts-filter">
            {(["all", ...KIND_ORDER] as ActFilter[]).map((f) => {
              const label = f === "all" ? "全部" : KIND_SHORT[f];
              const count = f === "all" ? activities.length : kindCount.get(f) ?? 0;
              if (f !== "all" && count === 0) return null;
              return (
                <button
                  key={f}
                  className={"chip today__acts-chip" + (filter === f ? " chip--active" : "")}
                  onClick={() => setFilter(f)}
                >
                  {label}
                  <span className="today__acts-chipcount">{count}</span>
                </button>
              );
            })}
          </div>

          {rows.length === 0 ? (
            <p className="muted today__empty">这个分类下没有学习记录。</p>
          ) : (
            <ul className="actrow-list">
              {rows.map((a) => {
                const kind = (a.activity_kind ?? "unplanned") as ActKind;
                const isActive = a.duration_seconds == null;
                return (
                  <li key={a.id} className="actrow">
                    <span className={"actrow__kind actrow__kind--" + kind}>
                      {KIND_SHORT[kind]}
                    </span>
                    <button
                      className="actrow__title"
                      onClick={() => navigate(`/learn/${a.id}`)}
                      title="打开这条学习记录"
                    >
                      {a.title || `学习记录 #${a.id}`}
                    </button>
                    <div className="actrow__right">
                      <span className="actrow__time">
                        {studyClockHHMM(a.started_at)} · {durationShort(a.duration_seconds)}
                      </span>
                      <div className="actrow__acts">
                        {isActive ? (
                          <>
                            <button
                              className="btn btn--small btn--primary"
                              onClick={() => navigate(`/learn/${a.id}`)}
                            >
                              进入学习
                            </button>
                            <button
                              className="btn btn--small"
                              disabled={endingId != null}
                              onClick={() => void endOne(a)}
                            >
                              {endingId === a.id ? "结束中…" : "结束"}
                            </button>
                          </>
                        ) : (
                          <>
                            <button className="btn btn--small" onClick={() => navigate(`/learn/${a.id}`)}>
                              打开
                            </button>
                            {kind === "accumulation" ? (
                              <button className="btn btn--small" onClick={() => void continueStudy(a)}>
                                继续
                              </button>
                            ) : a.learning_item_id == null ? (
                              <button
                                className="btn btn--small"
                                onClick={() => {
                                  setKnowFor(a);
                                  setKnowSearch("");
                                }}
                              >
                                整理进知识
                              </button>
                            ) : (
                              <button className="btn btn--small" onClick={() => void continueStudy(a)}>
                                继续学习
                              </button>
                            )}
                          </>
                        )}
                      </div>
                      <div className="actrow__menu">
                        <button
                          className="actrow__more"
                          title="更多操作"
                          onClick={(e) => {
                            e.stopPropagation();
                            setMenuFor(menuFor === a.id ? null : a.id);
                          }}
                        >
                          ⋯
                        </button>
                        {menuFor === a.id && (
                          <>
                            <div className="actrow__backdrop" onClick={() => setMenuFor(null)} />
                            <div className="actrow__pop">
                              <button
                                onClick={() => {
                                  setMenuFor(null);
                                  setRenameFor(a);
                                  setRenameValue(a.title ?? "");
                                }}
                              >
                                编辑标题
                              </button>
                              <button
                                onClick={() => {
                                  setMenuFor(null);
                                  setKindFor(a);
                                }}
                              >
                                修改分类
                              </button>
                              <button
                                onClick={() => {
                                  setMenuFor(null);
                                  setGoalFor(a);
                                  setGoalValue("");
                                }}
                              >
                                调整目标关联
                              </button>
                              <button
                                onClick={() => {
                                  setMenuFor(null);
                                  setKnowFor(a);
                                  setKnowSearch("");
                                }}
                              >
                                调整知识关联
                              </button>
                              <button
                                onClick={() => {
                                  setMenuFor(null);
                                  setFollowupFor(a);
                                  setFollowupDate(todayDate());
                                  setFollowupMinutes("");
                                }}
                              >
                                生成后续任务
                              </button>
                              <button
                                className="actrow__danger"
                                onClick={() => {
                                  setMenuFor(null);
                                  setDeleting(a);
                                }}
                              >
                                删除
                              </button>
                            </div>
                          </>
                        )}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={conflict}
        onClose={close}
        onResolved={() => void onChanged()}
      />

      {/* 编辑标题（update_session_title 现有命令） */}
      {renameFor && (
        <div className="modal-overlay" onClick={() => setRenameFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">编辑学习记录标题</div>
            <input
              className="modal__input"
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              autoFocus
            />
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                disabled={!renameValue.trim()}
                onClick={() =>
                  void run(
                    () => updateSessionTitle(renameFor.id, renameValue.trim()),
                    () => setRenameFor(null)
                  )
                }
              >
                保存
              </button>
              <button className="btn" onClick={() => setRenameFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 修改分类（set_session_activity_kind 四选一） */}
      {kindFor && (
        <div className="modal-overlay" onClick={() => setKindFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">「{kindFor.title || `学习记录 #${kindFor.id}`}」属于哪类学习？</div>
            <div className="taskmodal__picker">
              {KIND_ORDER.map((k) => (
                <button
                  key={k}
                  className={
                    "chip" + ((kindFor.activity_kind ?? "unplanned") === k ? " chip--active" : "")
                  }
                  onClick={() =>
                    void run(
                      () => setSessionActivityKind(profileId, kindFor.id, k),
                      () => setKindFor(null)
                    )
                  }
                >
                  {KIND_LABELS[k]}
                </button>
              ))}
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setKindFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 修改目标关联（set_session_goal + goal 选择器） */}
      {goalFor && (
        <div className="modal-overlay" onClick={() => setGoalFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">修改目标关联</div>
            <p className="evmodal__note muted">只更新这条学习记录的目标关联，不改动其他数据。</p>
            <select
              className="modal__input"
              value={goalValue}
              onChange={(e) => setGoalValue(e.target.value ? Number(e.target.value) : "")}
            >
              <option value="">不关联目标</option>
              {goals.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                onClick={() =>
                  void run(
                    () => setSessionGoal(profileId, goalFor.id, goalValue === "" ? null : goalValue),
                    () => setGoalFor(null)
                  )
                }
              >
                保存
              </button>
              <button className="btn" onClick={() => setGoalFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 修改知识关联（organize_session_into_knowledge + knowledge 选择器） */}
      {knowFor && (
        <div className="modal-overlay" onClick={() => setKnowFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">把「{knowFor.title || `学习记录 #${knowFor.id}`}」归入哪个知识？</div>
            <p className="evmodal__note muted">只建立学习记录 → 知识的关联，不修改知识正文。</p>
            <div className="taskmodal__picker">
              <input
                className="modal__input taskmodal__search"
                value={knowSearch}
                onChange={(e) => setKnowSearch(e.target.value)}
                placeholder="搜索知识…"
              />
            </div>
            <div className="taskmodal__list">
              {filteredItems.slice(0, 30).map((i) => (
                <button
                  key={i.id}
                  className="taskmodal__item"
                  onClick={() =>
                    void run(
                      () => organizeSessionIntoKnowledge(profileId, knowFor.id, i.id),
                      () => setKnowFor(null)
                    )
                  }
                >
                  {i.name}
                </button>
              ))}
              {filteredItems.length === 0 && (
                <span className="muted" style={{ fontSize: 12 }}>
                  没有匹配的知识。
                </span>
              )}
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setKnowFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 生成后续任务（create_followup_task_from_session：日期 + 分钟小表单） */}
      {followupFor && (
        <div className="modal-overlay" onClick={() => setFollowupFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">从「{followupFor.title || `学习记录 #${followupFor.id}`}」生成后续任务</div>
            <p className="evmodal__note muted">会创建一个新任务；这条学习记录本身保持不变。</p>
            <div className="taskmodal__daterow">
              <label className="modal__field">
                计划日期
                <input
                  className="modal__input"
                  type="date"
                  value={followupDate}
                  onChange={(e) => setFollowupDate(e.target.value)}
                />
              </label>
              <label className="modal__field">
                预计分钟（可选）
                <input
                  className="modal__input"
                  type="number"
                  min={1}
                  max={1440}
                  value={followupMinutes}
                  onChange={(e) => setFollowupMinutes(e.target.value)}
                  placeholder="如 30"
                />
              </label>
            </div>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                disabled={!followupDate}
                onClick={() => {
                  const minutes = followupMinutes.trim() === "" ? null : Number(followupMinutes);
                  void run(
                    () =>
                      createFollowupTaskFromSession(
                        profileId,
                        followupFor.id,
                        followupDate || null,
                        minutes
                      ),
                    () => setFollowupFor(null)
                  );
                }}
              >
                创建任务
              </button>
              <button className="btn" onClick={() => setFollowupFor(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 删除（delete_session 现有命令；确认由前端负责） */}
      {deleting && (
        <div className="modal-overlay" onClick={() => setDeleting(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除这条学习记录？</div>
            <p className="evmodal__note">
              「{deleting.title || `学习记录 #${deleting.id}`}」的时间与笔记记录将一并删除，无法恢复。
            </p>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                onClick={() => void run(() => deleteSession(deleting.id), () => setDeleting(null))}
              >
                删除
              </button>
              <button className="btn" onClick={() => setDeleting(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
