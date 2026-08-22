import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createRecurringRule,
  deleteRecurringRule,
  getActivePlanningBlueprint,
  getProfileRangeSessions,
  listPlanningMilestones,
  listPlanningPhases,
  listRecurringRulesByProfile,
  listTasksByRangeByProfile,
  materializeRecurringTasksRange,
  materializeRecurringRolling,
  setRecurringRuleEnabled,
  syncNotifications,
  updateRecurringRule,
} from "../api";
import TaskModal from "./TaskModal";
import type {
  Goal,
  LearningItem,
  PlanningMilestone,
  PlanningPhase,
  RecurringRule,
  StudySession,
  Task,
} from "../types";
import { formatDuration, monthGrid, studyDayOf, todayDate } from "../utils";

const WEEKDAY_NAMES = ["一", "二", "三", "四", "五", "六", "日"];

/**
 * Planning Calendar（DEV-0053 §60-62 重构；DEV-0059 §29 增加 Milestone / Phase 理解）。
 *
 * - 旧 Date Detail 中央 Modal 已废弃（§60/§165）：点击日期不再打开 Modal，
 *   改为受控 selectedDate → Planning 在 Calendar 正下方展开 Daily Learning Report（§61-62）
 * - 月历 cell：日期 / 任务 完成·总数 / 学习时长 / 精确日期 Milestone（§29：exact milestone）
 * - month-only milestone：不伪装成某一天，显示在月历上方月级摘要（§29）
 * - current phase summary：显示在月历上方（§29）
 * - 保留：月切换 / 今天 / + 新建任务 / 重复任务管理（RecurringModal）
 */
export default function PlanningCalendar({
  profileId,
  items,
  goals,
  selectedDate,
  onSelectDate,
}: {
  profileId: number;
  items: LearningItem[];
  goals: Goal[];
  /** 当前选中的日期（null = 未展开日报） */
  selectedDate: string | null;
  /** 点击日期（再次点击同一日期 = 收起） */
  onSelectDate: (date: string | null) => void;
}) {
  const today = todayDate();
  const [y, setY] = useState(() => new Date().getFullYear());
  const [m, setM] = useState(() => new Date().getMonth() + 1);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [sessions, setSessions] = useState<StudySession[]>([]);
  const [rules, setRules] = useState<RecurringRule[]>([]);
  // DEV-0059 §29：Active Blueprint 的 Milestone / Phase（exact → cell；month-only → 月级摘要）
  const [milestones, setMilestones] = useState<PlanningMilestone[]>([]);
  const [phases, setPhases] = useState<PlanningPhase[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [createFor, setCreateFor] = useState<string | null>(null);
  const [showRules, setShowRules] = useState(false);
  const [ruleEdit, setRuleEdit] = useState<RecurringRule | null>(null);
  const [ruleCreate, setRuleCreate] = useState(false);

  const monthStart = `${y}-${String(m).padStart(2, "0")}-01`;
  const monthEnd = `${y}-${String(m).padStart(2, "0")}-${String(new Date(y, m, 0).getDate()).padStart(2, "0")}`;

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      // DEV-0061R §53：打开某月 → 对**可见月范围**有界 materialize（幂等；
      // 可超 rolling 30 天，按当前显示月份生成，不无限延伸）
      await materializeRecurringTasksRange(profileId, monthStart, monthEnd).catch(() => {});
      const [ts, ss, rs, bp] = await Promise.all([
        listTasksByRangeByProfile(profileId, monthStart, monthEnd),
        // §75：月度学习时长用一次 range 查询前端聚合（避免每格 get_day_detail）
        getProfileRangeSessions(profileId, monthStart, monthEnd).catch(() => [] as StudySession[]),
        listRecurringRulesByProfile(profileId),
        getActivePlanningBlueprint(profileId).catch(() => null),
      ]);
      setTasks(ts);
      setSessions(ss);
      setRules(rs);
      // §29：active blueprint 存在时才加载其 Milestone/Phase
      if (bp) {
        const [ml, ph] = await Promise.all([
          listPlanningMilestones(bp.id).catch(() => [] as PlanningMilestone[]),
          listPlanningPhases(bp.id).catch(() => [] as PlanningPhase[]),
        ]);
        setMilestones(ml);
        setPhases(ph);
      } else {
        setMilestones([]);
        setPhases([]);
      }
      // 任务/规则变化后对齐系统学习提醒（fire-and-forget，失败静默）
      void syncNotifications().catch(() => {});
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId, monthStart, monthEnd, today]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 运行中每 30s rolling horizon materialize（有新任务时刷新）
  useEffect(() => {
    const t = window.setInterval(() => {
      void materializeRecurringRolling(profileId, today)
        .then((n) => (n > 0 ? refresh() : undefined))
        .catch(() => {});
    }, 30000);
    return () => window.clearInterval(t);
  }, [profileId, today, refresh]);

  const cells = useMemo(() => monthGrid(y, m), [y, m]);
  const byDate = useMemo(() => {
    const map = new Map<string, Task[]>();
    for (const t of tasks) {
      if (!t.planned_date) continue;
      const arr = map.get(t.planned_date) ?? [];
      arr.push(t);
      map.set(t.planned_date, arr);
    }
    return map;
  }, [tasks]);

  /** §29：精确日期 Milestone（date_precision=day 或 range 覆盖到当天）→ 日期 → milestone[] */
  const milestoneByDate = useMemo(() => {
    const map = new Map<string, PlanningMilestone[]>();
    for (const ms of milestones) {
      if (ms.date_precision === "month" || ms.date_precision === "unknown") continue;
      const start = ms.start_date;
      const end = ms.end_date ?? ms.start_date;
      if (!start || !end) continue;
      const s = start < end ? start : end;
      const e = end < start ? start : end;
      // 仅放入当月可见范围内（day 精度单日；range 精度整段覆盖）
      const from = s > monthStart ? s : monthStart;
      const to = e < monthEnd ? e : monthEnd;
      if (from > to) continue;
      let cur = from;
      while (cur <= to) {
        const arr = map.get(cur) ?? [];
        arr.push(ms);
        map.set(cur, arr);
        // 推进一天
        const d = new Date(cur + "T00:00:00");
        d.setDate(d.getDate() + 1);
        cur = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      }
    }
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [milestones, monthStart, monthEnd]);

  /** §29：month-only milestone（不伪装成某一天）→ 月级摘要 */
  const monthMilestones = useMemo(
    () =>
      milestones.filter((ms) => {
        if (ms.date_precision !== "month") return false;
        const ym = ms.start_date ? ms.start_date.slice(0, 7) : null;
        if (!ym) return false;
        const curYm = `${y}-${String(m).padStart(2, "0")}`;
        if (ym === curYm) return true;
        // range 且跨月
        if (ms.end_date && ms.start_date && ms.start_date.slice(0, 7) <= curYm && ms.end_date.slice(0, 7) >= curYm)
          return true;
        return false;
      }),
    [milestones, y, m]
  );

  /** §29：current phase（start_date<=today<=end_date 或覆盖今天的 range） */
  const currentPhase = useMemo(
    () =>
      phases.find((p) => {
        if (p.start_date && p.start_date > today) return false;
        if (p.end_date && p.end_date < today) return false;
        return true;
      }) ?? null,
    [phases, today]
  );

  /** 当日学习秒数（按 started_at 的 UTC+8 学习日归属，与后端 date(started_at,'+8 hours') 一致）。 */
  const secondsByDate = useMemo(() => {
    const map = new Map<string, number>();
    for (const s of sessions) {
      const date = studyDayOf(s.started_at);
      if (!date) continue;
      const secs = s.duration_seconds ?? 0;
      if (!secs) continue;
      map.set(date, (map.get(date) ?? 0) + secs);
    }
    return map;
  }, [sessions]);

  const itemOf = (id: number) => items.find((i) => i.id === id);

  function shiftMonth(delta: number) {
    let ny = y, nm = m + delta;
    if (nm < 1) { ny -= 1; nm = 12; }
    if (nm > 12) { ny += 1; nm = 1; }
    setY(ny); setM(nm);
  }

  function goToday() {
    const n = new Date();
    setY(n.getFullYear());
    setM(n.getMonth() + 1);
    onSelectDate(today);
  }

  return (
    <div className="pcal">
      {error && <div className="alert alert--error">{error}</div>}

      {/* 顶部（§74）：← 月 → [今天] | [+ 新建任务] [重复任务] */}
      <div className="pcal__bar">
        <div className="pcal__nav">
          <button className="btn btn--small" onClick={() => shiftMonth(-1)}>←</button>
          <span className="pcal__title">{y}年{m}月</span>
          <button className="btn btn--small" onClick={() => shiftMonth(1)}>→</button>
          <button className="btn btn--small" onClick={goToday}>今天</button>
        </div>
        <div className="pcal__bar-actions">
          <button
            className="btn btn--small btn--primary"
            onClick={() => setCreateFor(selectedDate ?? today)}
          >
            + 新建任务
          </button>
          <button className="btn btn--small" onClick={() => setShowRules((v) => !v)}>
            重复任务（{rules.filter((r) => r.enabled).length}）
          </button>
        </div>
      </div>

      {/* 重复任务管理 */}
      {showRules && (
        <section className="card pcal__rules">
          <div className="lw-att__head">
            <h3 className="card__title">重复任务规则</h3>
            <button className="btn btn--small btn--primary" onClick={() => setRuleCreate(true)}>
              + 重复任务
            </button>
          </div>
          {rules.length === 0 ? (
            <p className="muted" style={{ fontSize: 12 }}>
              还没有重复任务。可创建「每天背单词」「每周一三五学高数」这类固定安排。
            </p>
          ) : (
            <ul className="pcal__rule-list">
              {rules.map((r) => {
                const weekdays: number[] = (() => {
                  try { return JSON.parse(r.weekdays_json) as number[]; } catch { return []; }
                })();
                return (
                  <li key={r.id} className={"pcal__rule" + (r.enabled ? "" : " pcal__rule--off")}>
                    <div className="pcal__rule-main">
                      <span className="pcal__rule-title">{r.title}</span>
                      <span className="muted">
                        {r.repeat_type === "daily"
                          ? "每天"
                          : `每周${weekdays.map((w) => WEEKDAY_NAMES[w - 1]).join("·")}`}
                        {r.time_of_day ? ` · ${r.time_of_day}` : ""}
                        {` · ${r.start_date} 起`}
                        {r.end_date ? ` 至 ${r.end_date}` : ""}
                        {r.learning_item_id != null ? ` · ${itemOf(r.learning_item_id)?.name ?? ""}` : ""}
                        {!r.enabled && " · 已停用"}
                      </span>
                    </div>
                    <div className="pcal__rule-actions">
                      <button
                        className="btn btn--small"
                        onClick={async () => {
                          await setRecurringRuleEnabled(r.id, !r.enabled);
                          await refresh();
                        }}
                      >
                        {r.enabled ? "停用" : "启用"}
                      </button>
                      <button className="btn btn--small" onClick={() => setRuleEdit(r)}>
                        编辑
                      </button>
                      <button
                        className="btn btn--small"
                        onClick={async () => {
                          if (!window.confirm(`删除重复任务「${r.title}」？已生成的历史任务会保留。`)) return;
                          await deleteRecurringRule(r.id);
                          await refresh();
                        }}
                      >
                        删除
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          <p className="muted" style={{ fontSize: 11 }}>
            修改规则只影响之后生成的任务，已生成的任务不变。
          </p>
        </section>
      )}

      {/* §29：月级规划摘要（current phase + month-only milestone，不伪装成某一天） */}
      {(currentPhase || monthMilestones.length > 0) && (
        <div className="pcal__plan">
          {currentPhase && (
            <span className="pcal__phase">
              当前阶段：<b>{currentPhase.title}</b>
              {currentPhase.start_date ? `（${currentPhase.start_date} 起` : "（"}
              {currentPhase.end_date ? ` 至 ${currentPhase.end_date}` : "至今"}）
            </span>
          )}
          {monthMilestones.map((ms) => (
            <span key={ms.id} className="pcal__month-ms" title={`${ms.title}（${ms.start_date ?? ""}${ms.end_date ? " ~ " + ms.end_date : ""}）`}>
              {ms.title}
            </span>
          ))}
        </div>
      )}

      {/* 月历（§75：日期 / 任务 x/y / 学习时长 / 最多 2 条任务名；§29：Milestone；点击选中 → 正下方日报） */}
      <div className="pcal__grid">
        {WEEKDAY_NAMES.map((w) => (
          <div key={w} className="pcal__weekday">{w}</div>
        ))}
        {cells.map((date, i) => {
          if (!date) return <div key={i} className="pcal__cell pcal__cell--empty" />;
          const dayTasks = byDate.get(date) ?? [];
          const done = dayTasks.filter((t) => t.status === "completed").length;
          const studySecs = secondsByDate.get(date) ?? 0;
          const dayMilestones = milestoneByDate.get(date) ?? [];
          const isToday = date === today;
          const isSelected = date === selectedDate;
          const hasContent = dayTasks.length > 0 || studySecs > 0 || dayMilestones.length > 0;
          return (
            <button
              key={i}
              className={
                "pcal__cell" +
                (isToday ? " pcal__cell--today" : "") +
                (hasContent ? " pcal__cell--has" : "") +
                (isSelected ? " pcal__cell--selected" : "")
              }
              onClick={() => onSelectDate(isSelected ? null : date)}
              title={isSelected ? "收起日报" : "查看这一天的学习日报"}
            >
              <span className="pcal__date">{Number(date.slice(8))}</span>
              {(dayTasks.length > 0 || studySecs > 0) && (
                <span className="pcal__count">
                  {dayTasks.length > 0 && `任务 ${done}/${dayTasks.length}`}
                  {dayTasks.length > 0 && studySecs > 0 ? " · " : ""}
                  {studySecs > 0 && `学习 ${formatDuration(studySecs)}`}
                </span>
              )}
              {dayMilestones.length > 0 && (
                <span className="pcal__ms">
                  {dayMilestones.slice(0, 1).map((ms) => (
                    <span key={ms.id} className="pcal__ms-item" title={ms.title}>
                      ◆ {ms.title}
                    </span>
                  ))}
                  {dayMilestones.length > 1 && (
                    <span className="pcal__more">+{dayMilestones.length - 1}</span>
                  )}
                </span>
              )}
              {dayTasks.length > 0 && (
                <span className="pcal__tasks">
                  {dayTasks.slice(0, 2).map((t) => (
                    <span key={t.id} className={"pcal__task" + (t.status === "completed" ? " pcal__task--done" : "")}>
                      {t.status === "completed" ? "✓" : "○"} {t.title}
                    </span>
                  ))}
                  {dayTasks.length > 2 && <span className="pcal__more">+{dayTasks.length - 2}</span>}
                </span>
              )}
            </button>
          );
        })}
      </div>
      {loading && <p className="muted">加载中…</p>}

      {/* 新建任务（日期预置选中日期；?date= 深链时即该日） */}
      {createFor && (
        <TaskModal
          mode="create"
          profileId={profileId}
          items={items}
          goals={goals}
          defaultDate={createFor}
          onClose={() => setCreateFor(null)}
          onSaved={async () => {
            setCreateFor(null);
            await refresh();
          }}
        />
      )}

      {/* 重复任务 创建/编辑 */}
      {(ruleCreate || ruleEdit) && (
        <RecurringModal
          rule={ruleEdit}
          profileId={profileId}
          items={items}
          goals={goals}
          onClose={() => {
            setRuleCreate(false);
            setRuleEdit(null);
          }}
          onSaved={async () => {
            setRuleCreate(false);
            setRuleEdit(null);
            await refresh();
          }}
          api={{ createRecurringRule, updateRecurringRule }}
        />
      )}
    </div>
  );
}

/** 重复任务规则 Modal（daily / weekly + 星期多选 + 时间 + 起止 + 知识）。
 *  DEV-0042：导出供复用；prefill 用于从任务预填。 */
export function RecurringModal({
  rule,
  profileId,
  items,
  goals,
  prefill,
  onClose,
  onSaved,
  api,
}: {
  rule: RecurringRule | null;
  profileId: number;
  items: LearningItem[];
  goals: Goal[];
  prefill?: { title?: string; itemId?: number | null };
  onClose: () => void;
  onSaved: () => Promise<void>;
  api: { createRecurringRule: typeof createRecurringRule; updateRecurringRule: typeof updateRecurringRule };
}) {
  const weekdaysInit: number[] = rule
    ? (() => { try { return JSON.parse(rule.weekdays_json) as number[]; } catch { return []; } })()
    : [1, 3, 5];
  const [title, setTitle] = useState(rule?.title ?? prefill?.title ?? "");
  const [repeatType, setRepeatType] = useState<"daily" | "weekly">(
    (rule?.repeat_type as "daily" | "weekly") ?? "daily"
  );
  const [weekdays, setWeekdays] = useState<number[]>(weekdaysInit);
  const [timeOfDay, setTimeOfDay] = useState(rule?.time_of_day ?? "");
  const [startDate, setStartDate] = useState(rule?.start_date ?? todayDate());
  const [endDate, setEndDate] = useState(rule?.end_date ?? "");
  const [itemId, setItemId] = useState<number | "">(rule?.learning_item_id ?? prefill?.itemId ?? "");
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const activeGoal = goals.find((g) => g.status === "active") ?? goals[0];
  const filtered = search.trim()
    ? items.filter((i) => i.name.toLowerCase().includes(search.trim().toLowerCase()))
    : items;

  async function save() {
    if (!title.trim()) return setError("请填写名称");
    if (!itemId) return setError("请选择关联知识");
    if (repeatType === "weekly" && weekdays.length === 0) return setError("每周重复需选择至少一个星期");
    setSaving(true);
    setError("");
    try {
      const payload = {
        title: title.trim(),
        repeatType,
        weekdays: repeatType === "weekly" ? [...weekdays].sort() : [],
        timeOfDay: timeOfDay.trim() || null,
        startDate,
        endDate: endDate.trim() || null,
        learningItemId: Number(itemId),
      };
      if (rule) {
        await api.updateRecurringRule({ id: rule.id, ...payload });
      } else {
        // v013 Profile First：profileId 必填；goal 可选（无目标也完全正常）
        await api.createRecurringRule({
          profileId,
          goalId: activeGoal?.id ?? null,
          ...payload,
        });
      }
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
        <div className="modal__title">{rule ? "编辑重复任务" : "新建重复任务"}</div>
        {error && <div className="modal__error">{error}</div>}

        <label className="modal__field">
          名称 *
          <input className="modal__input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="如：背英语单词" autoFocus />
        </label>

        <div className="taskmodal__picker">
          <button
            className={"chip" + (repeatType === "daily" ? " chip--active" : "")}
            onClick={() => setRepeatType("daily")}
          >
            每天
          </button>
          <button
            className={"chip" + (repeatType === "weekly" ? " chip--active" : "")}
            onClick={() => setRepeatType("weekly")}
          >
            每周
          </button>
        </div>

        {repeatType === "weekly" && (
          <div className="taskmodal__picker">
            {WEEKDAY_NAMES.map((w, i) => (
              <button
                key={w}
                className={"chip" + (weekdays.includes(i + 1) ? " chip--active" : "")}
                onClick={() =>
                  setWeekdays((ws) =>
                    ws.includes(i + 1) ? ws.filter((x) => x !== i + 1) : [...ws, i + 1]
                  )
                }
              >
                {w}
              </button>
            ))}
          </div>
        )}

        <label className="modal__field">
          关联知识 *
          <input
            className="modal__input"
            value={itemId ? items.find((i) => i.id === itemId)?.name ?? `#${itemId}` : ""}
            readOnly
            placeholder="在下方选择…"
          />
        </label>
        <div className="taskmodal__picker">
          <input
            className="modal__input taskmodal__search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索知识…"
          />
        </div>
        <div className="taskmodal__list">
          {filtered.slice(0, 60).map((i) => (
            <button
              key={i.id}
              className={"taskmodal__item" + (itemId === i.id ? " taskmodal__item--active" : "")}
              onClick={() => setItemId(i.id)}
            >
              {i.name}
            </button>
          ))}
        </div>

        <div className="taskmodal__daterow">
          <label className="modal__field">
            开始日期 *
            <input className="modal__input" type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} />
          </label>
          <label className="modal__field">
            结束日期（可选）
            <input className="modal__input" type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} />
          </label>
          <label className="modal__field">
            时间（可选）
            <input className="modal__input" type="time" value={timeOfDay} onChange={(e) => setTimeOfDay(e.target.value)} />
          </label>
        </div>

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : rule ? "保存修改" : "创建规则"}
          </button>
          <button className="btn" onClick={onClose}>取消</button>
        </div>
      </div>
    </div>
  );
}
