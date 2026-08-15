import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  archiveTask,
  completeTask,
  createRecurringRule,
  deleteTask,
  getActiveSession,
  getProfileDaySessions,
  listEvaluationsByLearningItem,
  listGoalsByProfile,
  listLearningItemsByProfile,
  listRecurringRulesByProfile,
  listTodayTasksByProfile,
  materializeRecurringTasks,
  startQuickSession,
  startTaskSession,
  syncNotifications,
  uncompleteTask,
  updateRecurringRule,
  updateTask,
} from "../api";
import EvaluationModal from "../components/EvaluationModal";
import TaskModal from "../components/TaskModal";
import { RecurringModal } from "../components/PlanningCalendar";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type {
  Evaluation,
  Goal,
  LearningItem,
  RecurringRule,
  StudySession,
  Task,
} from "../types";
import {
  formatDateTime,
  formatDuration,
  friendlyDate,
  todayDate,
} from "../utils";

/** SQLite datetime（UTC）→ ms。 */
function parseUtcMs(raw: string): number {
  const normalized = raw.includes("T") ? raw : raw.replace(" ", "T") + "Z";
  return new Date(normalized).getTime();
}

type Filter = "all" | "pending" | "completed";

/**
 * 今日任务 Cockpit（DEV-0042 §32-37 / 51-52）。
 *
 * - Header：`今日任务 · M月d日 周X` + `完成 x/y · 学习 Xh Ym` + [⚡ 快速学习] [+ 新建任务]
 * - 任务行：☐ 标题（时间·知识tag·重复icon）+ [开始] [···]
 *   ··· 菜单：编辑 / 改时间 / 改日期 / 关联知识 / 取消知识关联 / 加入或修改重复 / 归档 / 删除
 * - 底部轻量 AI 行：`✨ AI 复盘今天`（当天有学习时高亮）+ `✨ AI 看看今天怎么安排`
 */
function Today() {
  const navigate = useNavigate();
  const { activeProfile, refreshKey } = useActiveProfile();
  const [todayTasks, setTodayTasks] = useState<Task[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [active, setActive] = useState<StudySession | null>(null);
  const [daySessions, setDaySessions] = useState<StudySession[]>([]);
  const [rules, setRules] = useState<RecurringRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const [filter, setFilter] = useState<Filter>("all");
  const [showCreate, setShowCreate] = useState(false);
  const [keepCreateOpen, setKeepCreateOpen] = useState(false);
  const [editTask, setEditTask] = useState<Task | null>(null);
  const [menuFor, setMenuFor] = useState<number | null>(null);
  /** 删除确认：null | task */
  const [deleting, setDeleting] = useState<Task | null>(null);
  /** 改日期 / 改时间（轻量小弹层） */
  const [dateFor, setDateFor] = useState<Task | null>(null);
  const [dateValue, setDateValue] = useState("");
  const [timeFor, setTimeFor] = useState<Task | null>(null);
  const [timeValue, setTimeValue] = useState("");
  /** 加入或修改重复（RecurringModal 预填） */
  const [repeatFor, setRepeatFor] = useState<Task | null>(null);
  const [toast, setToast] = useState("");
  const toastTimer = useRef<number | null>(null);

  const { runAction: aiRunAction, setPageContext } = useAiPanel();

  const [evalFor, setEvalFor] = useState<{
    profileId: number;
    goalId: number | null;
    itemId: number;
    defaultTitle: string;
  } | null>(null);
  const [evalDoneHint, setEvalDoneHint] = useState("");
  const [evalDoneTimer, setEvalDoneTimer] = useState<number | null>(null);
  const [confirmComplete, setConfirmComplete] = useState<Task | null>(null);

  const [now, setNow] = useState(() => Date.now());
  const [dismissedStale, setDismissedStale] = useState(false);

  function showToast(msg: string) {
    if (toastTimer.current != null) window.clearTimeout(toastTimer.current);
    setToast(msg);
    toastTimer.current = window.setTimeout(() => setToast(""), 2600);
  }

  const refresh = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    try {
      const today = todayDate();
      await materializeRecurringTasks(activeProfile.id, today).catch(() => {});
      const [tasks, itemList, goalList, activeSess, daySess, ruleList] = await Promise.all([
        listTodayTasksByProfile(activeProfile.id),
        listLearningItemsByProfile(activeProfile.id),
        listGoalsByProfile(activeProfile.id).catch(() => [] as Goal[]),
        getActiveSession(),
        getProfileDaySessions(activeProfile.id, today),
        listRecurringRulesByProfile(activeProfile.id).catch(() => [] as RecurringRule[]),
      ]);
      setTodayTasks(tasks);
      setItems(itemList);
      setGoals(goalList);
      setActive(activeSess);
      setDaySessions(daySess);
      setRules(ruleList);
      // 任务/规则变化后对齐系统学习提醒（fire-and-forget，失败静默）
      void syncNotifications().catch(() => {});
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!activeProfile) return;
    const t = window.setInterval(() => {
      void materializeRecurringTasks(activeProfile.id, todayDate())
        .then((n) => (n > 0 ? refresh() : undefined))
        .catch(() => {});
    }, 30000);
    return () => window.clearInterval(t);
  }, [activeProfile, refresh]);

  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active?.id]);

  useEffect(() => {
    setDismissedStale(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active?.id]);

  useEffect(() => {
    if (activeProfile) setPageContext({ page: "today", pageLabel: "今日任务" });
  }, [activeProfile, setPageContext]);

  const itemOf = (id: number | null) => (id == null ? undefined : items.find((i) => i.id === id));

  /** ⚡ 快速学习（v013 §38-39）：一键创建 Session 直达编辑页，无 Goal 前提 */
  async function handleQuickStart() {
    if (!activeProfile) return;
    setError("");
    try {
      const s = await startQuickSession(activeProfile.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** 任务 [开始]（v013 §40）：从 Task 开始，title=task.title */
  async function handleTaskStart(taskId: number) {
    setError("");
    try {
      const s = await startTaskSession(taskId);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function toggleComplete(t: Task) {
    setError("");
    const was = t.status;
    setTodayTasks((list) =>
      list.map((x) =>
        x.id === t.id ? { ...x, status: was === "completed" ? "pending" : "completed" } : x
      )
    );
    try {
      await (was === "completed" ? uncompleteTask(t.id) : completeTask(t.id));
    } catch (e) {
      setTodayTasks((list) => list.map((x) => (x.id === t.id ? { ...x, status: was } : x)));
      setError(String(e));
    }
  }

  async function requestComplete(t: Task) {
    const hasSession = daySessions.some((s) => s.task_id === t.id);
    if (!hasSession) {
      await toggleComplete(t);
      return;
    }
    try {
      const evals = await listEvaluationsByLearningItem(t.learning_item_id ?? -1);
      if (evals.length === 0) {
        setConfirmComplete(t);
        return;
      }
    } catch {
      /* 不阻塞 */
    }
    await toggleComplete(t);
  }

  /** 删除（§18-20）：无历史直接删；有历史 → archive（保留学习历史） */
  async function handleDelete(t: Task) {
    setError("");
    try {
      const outcome = await deleteTask(t.id);
      if (outcome.deleted) {
        setDeleting(null);
        showToast("已删除");
      } else {
        await archiveTask(t.id);
        setDeleting(null);
        showToast("已从任务列表移除（学习历史保留）");
      }
      setMenuFor(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** 改日期（保留时间与知识关联） */
  async function reschedule(t: Task, date: string | null) {
    setError("");
    try {
      await updateTask({
        id: t.id,
        title: t.title,
        plannedDate: date,
        plannedTime: t.planned_time ?? null,
        learningItemId: t.learning_item_id,
      });
      setDateFor(null);
      setMenuFor(null);
      showToast(date ? `已改到 ${date}` : "已清除日期");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** 改时间（保留日期与知识关联） */
  async function retime(t: Task, time: string | null) {
    setError("");
    try {
      await updateTask({
        id: t.id,
        title: t.title,
        plannedDate: t.planned_date,
        plannedTime: time,
        learningItemId: t.learning_item_id,
      });
      setTimeFor(null);
      setMenuFor(null);
      showToast(time ? `已改到 ${time}` : "已清除时间");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** 取消知识关联（learning_item_id → null） */
  async function unlinkKnowledge(t: Task) {
    setError("");
    try {
      await updateTask({
        id: t.id,
        title: t.title,
        plannedDate: t.planned_date,
        plannedTime: t.planned_time ?? null,
        learningItemId: null,
      });
      setMenuFor(null);
      showToast("已取消知识关联");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** ··· 菜单：关联知识（复用 TaskModal 编辑模式的知识选择器） */
  function openLinkKnowledge(t: Task) {
    setMenuFor(null);
    setEditTask(t);
  }

  /** ··· 菜单：加入或修改重复（RecurringModal 预填；有规则则编辑该规则） */
  function openRepeat(t: Task) {
    setMenuFor(null);
    setRepeatFor(t);
  }

  /** 底部 ✨ AI 复盘今天（DEV-0046：daily_review，默认今天） */
  function runTodayReview() {
    setPageContext({
      page: "review",
      pageLabel: `今日复盘 · ${friendlyDate(todayDate())}`,
    });
    void aiRunAction("daily_review");
  }

  function showEvalHint(e: Evaluation, typeLabel: string, outcomeLabel: string) {
    if (evalDoneTimer != null) window.clearTimeout(evalDoneTimer);
    setEvalDoneHint(`已记录验证：${typeLabel} · ${outcomeLabel}`);
    const timer = window.setTimeout(() => setEvalDoneHint(""), 4000);
    setEvalDoneTimer(timer);
  }

  const visible = useMemo(() => {
    const list = todayTasks.filter((t) =>
      filter === "all" ? true : filter === "pending" ? t.status !== "completed" : t.status === "completed"
    );
    return [...list].sort((a, b) => {
      const ad = a.status === "completed" ? 1 : 0;
      const bd = b.status === "completed" ? 1 : 0;
      if (ad !== bd) return ad - bd;
      return a.id - b.id;
    });
  }, [todayTasks, filter]);

  const pendingCount = todayTasks.filter((t) => t.status !== "completed").length;
  const completedCount = todayTasks.length - pendingCount;
  const todayStudySeconds = daySessions
    .filter((s) => s.status === "completed")
    .reduce((acc, s) => acc + (s.duration_seconds ?? 0), 0);

  const activeItemName = active
    ? itemOf(active.learning_item_id)?.name ?? active.title
    : "";

  const repeatRuleOf = (t: Task): RecurringRule | null =>
    t.recurring_rule_id != null ? rules.find((r) => r.id === t.recurring_rule_id) ?? null : null;

  return (
    <div className="page page--wide">
      {/* Header（§32）：`今日任务 · M月d日 周X` + 统计行 + 按钮组 */}
      <header className="page__header today-head">
        <div className="today-head__info">
          <h1 className="page__title">今日任务 · {friendlyDate(todayDate())}</h1>
          <p className="today-head__sub">
            <b>
              完成 {completedCount}/{todayTasks.length}
            </b>
            {" · "}
            学习 {formatDuration(todayStudySeconds)}
          </p>
        </div>
        <div className="today-head__btns">
          <button className="btn" onClick={() => void handleQuickStart()}>
            ⚡ 快速学习
          </button>
          <button className="btn btn--primary" onClick={() => setShowCreate(true)}>
            + 新建任务
          </button>
        </div>
      </header>

      {toast && <div className="toast toast--ok">{toast}</div>}
      {error && <div className="alert alert--error">{error}</div>}
      {evalDoneHint && <div className="alert alert--ok">{evalDoneHint}</div>}

      {/* 当前学习（存在时） */}
      {active && (
        <section className="card today-active">
          <div className="today-active__head">
            <span className="badge badge--active">正在学习</span>
            <span className="today-active__name">{activeItemName}</span>
          </div>
          <div className="today-active__time">
            开始于 {formatDateTime(active.started_at)} · 已学习{" "}
            {formatDuration(Math.max(0, Math.floor((now - parseUtcMs(active.started_at)) / 1000)))}
          </div>
          {isStale(active) && !dismissedStale && (
            <div className="alert alert--error">上次学习仍未结束（可在学习工作区中结束）</div>
          )}
          <div className="btn-row">
            <button className="btn btn--primary" onClick={() => navigate(`/learn/${active.id}`)}>
              进入学习工作区
            </button>
            <button className="btn" onClick={() => setDismissedStale(true)}>
              稍后处理
            </button>
          </div>
        </section>
      )}

      {/* 任务列表（§33 行布局：☐ 标题 (时间·知识tag·重复icon) | [开始] [···]） */}
      <section className="card">
        <div className="today__filters">
          {(["all", "pending", "completed"] as Filter[]).map((f) => (
            <button
              key={f}
              className={"chip" + (filter === f ? " chip--active" : "")}
              onClick={() => setFilter(f)}
            >
              {f === "all"
                ? `全部 ${todayTasks.length}`
                : f === "pending"
                  ? `未完成 ${pendingCount}`
                  : `已完成 ${completedCount}`}
            </button>
          ))}
        </div>

        {loading ? (
          <p className="muted">加载中…</p>
        ) : visible.length === 0 ? (
          <div className="today-empty">
            <p className="today-empty__title">今天还没有任务</p>
            <p className="muted">输入一个名字就能开始，例如“数学”。</p>
            <div className="btn-row">
              <button className="btn btn--primary" onClick={() => setShowCreate(true)}>
                + 新建任务
              </button>
              <button className="btn" onClick={() => navigate("/planning")}>
                去学习日历
              </button>
            </div>
          </div>
        ) : (
          <ul className="taskrow-list" onClick={() => setMenuFor(null)}>
            {visible.map((t) => {
              const done = t.status === "completed";
              const kName = itemOf(t.learning_item_id)?.name;
              return (
                <li key={t.id} className={"taskrow" + (done ? " taskrow--done" : "")}>
                  <button
                    className="taskrow__check"
                    onClick={() => (done ? void toggleComplete(t) : void requestComplete(t))}
                    title={done ? "恢复未完成" : "完成"}
                  >
                    {done ? "☑" : "☐"}
                  </button>
                  <div className="taskrow__main">
                    <span className="taskrow__title">{t.title}</span>
                    {(t.planned_time || kName || t.recurring_rule_id != null) && (
                      <span className="taskrow__meta">
                        {t.planned_time}
                        {t.planned_time && kName ? " · " : ""}
                        {kName}
                        {t.recurring_rule_id != null ? " · 重复" : ""}
                      </span>
                    )}
                  </div>
                  <div className="taskrow__actions">
                    {!active && !done && (
                      <button
                        className="btn btn--small"
                        onClick={() => void handleTaskStart(t.id)}
                      >
                        开始
                      </button>
                    )}
                    <div className="taskmenu" onClick={(e) => e.stopPropagation()}>
                      <button className="taskmenu__btn" onClick={() => setMenuFor(menuFor === t.id ? null : t.id)}>
                        ⋯
                      </button>
                      {menuFor === t.id && (
                        <div className="taskmenu__pop">
                          <button onClick={() => { setEditTask(t); setMenuFor(null); }}>编辑</button>
                          <button onClick={() => { setTimeFor(t); setTimeValue(t.planned_time ?? ""); setMenuFor(null); }}>
                            改时间…
                          </button>
                          <button onClick={() => { setDateFor(t); setDateValue(t.planned_date ?? todayDate()); setMenuFor(null); }}>
                            改日期…
                          </button>
                          <button onClick={() => openLinkKnowledge(t)}>关联知识…</button>
                          {t.learning_item_id != null && (
                            <button onClick={() => void unlinkKnowledge(t)}>取消知识关联</button>
                          )}
                          <button onClick={() => openRepeat(t)}>
                            {t.recurring_rule_id != null ? "修改重复…" : "加入重复…"}
                          </button>
                          <button
                            onClick={async () => {
                              await archiveTask(t.id);
                              setMenuFor(null);
                              showToast("已归档（学习历史保留）");
                              await refresh();
                            }}
                          >
                            归档
                          </button>
                          <button className="taskmenu__danger" onClick={() => { setDeleting(t); setMenuFor(null); }}>
                            删除
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* 底部轻量 AI 行（§51-52；替代大 Card） */}
      <button
        className={"today-ai-line today-ai-line--action" + (daySessions.length > 0 ? " today-ai-line--primary" : "")}
        onClick={runTodayReview}
      >
        <span>✨ AI 复盘今天</span>
        <span className="muted">
          {daySessions.length > 0 ? "今天有学习记录，看看 AI 怎么说" : "还没有学习记录，也可以让 AI 看看"}
        </span>
      </button>
      <button
        className="today-ai-line today-ai-line--action"
        onClick={() => void aiRunAction("today_suggestion")}
      >
        <span>✨ AI 看看今天怎么安排</span>
        <span className="muted">在右侧面板查看；不会自动创建任务</span>
      </button>

      {/* 新建 / 编辑 */}
      {(showCreate || keepCreateOpen || editTask) && activeProfile && (
        <TaskModal
          mode={showCreate || keepCreateOpen ? "create" : "edit"}
          task={editTask}
          profileId={activeProfile.id}
          items={items}
          goals={goals}
          defaultDate={todayDate()}
          onClose={() => {
            setShowCreate(false);
            setKeepCreateOpen(false);
            setEditTask(null);
          }}
          onSaved={async (opts) => {
            if (!opts?.keepOpen) {
              setShowCreate(false);
              setKeepCreateOpen(false);
              setEditTask(null);
            } else {
              setShowCreate(false);
              setKeepCreateOpen(true); // 创建并继续：保持 Modal
            }
            await refresh();
          }}
        />
      )}

      {/* 改时间（轻量小弹层） */}
      {timeFor && (
        <div className="modal-overlay" onClick={() => setTimeFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">「{timeFor.title}」几点开始？</div>
            <input
              className="modal__input"
              type="time"
              value={timeValue}
              onChange={(e) => setTimeValue(e.target.value)}
              autoFocus
            />
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void retime(timeFor, timeValue || null)}>
                确定
              </button>
              {timeFor.planned_time && (
                <button className="btn" onClick={() => void retime(timeFor, null)}>
                  清除时间
                </button>
              )}
              <button className="btn" onClick={() => setTimeFor(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 改日期 */}
      {dateFor && (
        <div className="modal-overlay" onClick={() => setDateFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">把「{dateFor.title}」改到哪天？</div>
            <input
              className="modal__input"
              type="date"
              value={dateValue}
              onChange={(e) => setDateValue(e.target.value)}
              autoFocus
            />
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void reschedule(dateFor, dateValue)}>
                确定
              </button>
              <button className="btn" onClick={() => setDateFor(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 加入或修改重复（RecurringModal 预填；DEV-0042） */}
      {repeatFor && activeProfile && (
        <RecurringModal
          rule={repeatRuleOf(repeatFor)}
          profileId={activeProfile.id}
          items={items}
          goals={goals}
          prefill={{
            title: repeatFor.title,
            itemId: repeatFor.learning_item_id,
          }}
          onClose={() => setRepeatFor(null)}
          onSaved={async () => {
            setRepeatFor(null);
            showToast("重复任务已保存");
            await refresh();
          }}
          api={{ createRecurringRule, updateRecurringRule }}
        />
      )}

      {/* 删除确认（§19-20：人话 + 贴近操作） */}
      {deleting && (
        <div className="modal-overlay" onClick={() => setDeleting(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除「{deleting.title}」？</div>
            <p className="evmodal__note">
              如果这个任务已有学习记录，会改为「从任务列表移除并保留学习历史」，你的学习时间与笔记不会丢失。
            </p>
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void handleDelete(deleting)}>
                删除 / 移除
              </button>
              <button className="btn" onClick={() => setDeleting(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 完成前轻提示 */}
      {confirmComplete && (
        <div className="modal-overlay" onClick={() => setConfirmComplete(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">完成这项任务？</div>
            <p className="evmodal__note">这项学习还没有验证记录。</p>
            <div className="modal__actions">
              {(() => {
                const item = itemOf(confirmComplete.learning_item_id);
                return (
                  <button
                    className="btn btn--primary"
                    disabled={!item}
                    onClick={() => {
                      if (item) {
                        const t = confirmComplete;
                        setConfirmComplete(null);
                        setEvalFor({
                          profileId: item.profile_id,
                          goalId: item.goal_id,
                          itemId: item.id,
                          defaultTitle: `${t.title}验证`,
                        });
                      }
                    }}
                  >
                    记录验证
                  </button>
                );
              })()}
              <button
                className="btn"
                onClick={() => {
                  void toggleComplete(confirmComplete);
                  setConfirmComplete(null);
                }}
              >
                仍然完成
              </button>
              <button className="btn" onClick={() => setConfirmComplete(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 验证 Modal */}
      {evalFor && (
        <EvaluationModal
          profileId={evalFor.profileId}
          goalId={evalFor.goalId}
          learningItemId={evalFor.itemId}
          defaultTitle={evalFor.defaultTitle}
          onClose={() => setEvalFor(null)}
          onCreated={(e) => {
            setEvalFor(null);
            showEvalHint(e, evalTypeLabel(e.evaluation_type), evalOutcomeLabel(e.outcome));
            void refresh();
          }}
        />
      )}
    </div>
  );
}

function isStale(s: StudySession): boolean {
  const started = new Date(parseUtcMs(s.started_at));
  return started.toDateString() !== new Date().toDateString();
}

function evalTypeLabel(t: string): string {
  const map: Record<string, string> = {
    practice: "练习",
    test: "测试",
    recall: "回忆",
    application: "应用",
  };
  return map[t] ?? t;
}

function evalOutcomeLabel(o: string | null): string {
  const map: Record<string, string> = {
    passed: "通过",
    partial: "部分掌握",
    failed: "未通过",
    unrated: "未评级",
  };
  return map[o ?? ""] ?? (o ?? "");
}

export default Today;
