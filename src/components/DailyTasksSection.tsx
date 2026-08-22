import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  archiveTask,
  completeTask,
  createTaskV2,
  deleteTask,
  startTaskSession,
  uncompleteTask,
  updateTaskV2,
} from "../api";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "./ActiveSessionConflictModal";
import type { DailyTaskRow, Goal, LearningItem } from "../types";
import { todayDate } from "../utils";

/**
 * 今日任务区（DEV-0053 §14-23 / DEV-0054 §32-37 → DEV-0063 §21-§23）。
 *
 * Today 与 Calendar Daily Report 共用同一套 Task 渲染（§72-73 不写两套）：
 * - 分组：核心（structured+core）→ 常规（structured+normal）→ 积累（accumulation）；
 *   空组不显示；分组标题为轻量 Section Label（§39），组名同时进 Row Meta（§34）
 * - Task Row：Checkbox + Title(15px/600) + Meta（组名 · 预计 · 知识归属）
 *   + 右侧直接动作：未完成=开始学习(primary)/⋯；完成=查看/⋯
 * - ⋯ 菜单（DEV-0063 §22）：编辑（同一编辑 Modal，全部字段可改）+ 分隔线 + 删除；
 *   日期/目标/知识/类型调整统一并入编辑 Modal
 */

/** §21/§127：任务视觉分组 */
export function taskGroupOf(t: DailyTaskRow): "core" | "normal" | "accumulation" {
  if (t.task_kind === "accumulation") return "accumulation";
  return t.priority === "core" ? "core" : "normal";
}

const GROUP_LABELS: Record<"core" | "normal" | "accumulation", string> = {
  core: "核心",
  normal: "常规",
  accumulation: "积累",
};

/** 分钟 → "2h36m" / "45m"（Header 统计与预计时间显示共用） */
export function minutesShort(minutes: number | null | undefined): string {
  if (minutes == null) return "—";
  const m = Math.round(minutes);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h${m % 60}m`;
  return `${m}m`;
}

export default function DailyTasksSection({
  profileId,
  tasks,
  items,
  goals,
  onChanged,
  defaultDate,
  createOpen,
  onCreateClose,
  emptyNote,
  onEmptyCreate,
  onEmptyQuickStart,
}: {
  profileId: number;
  tasks: DailyTaskRow[];
  items: LearningItem[];
  goals: Goal[];
  /** 任务/报告数据变化后刷新（完成、编辑、删除、新建） */
  onChanged: () => void | Promise<void>;
  /** 新建任务默认日期 */
  defaultDate?: string;
  /** 父级「+ 新建任务」按钮打开新建 Modal（受控） */
  createOpen?: boolean;
  onCreateClose?: () => void;
  /** 空态文案（Today：今天还没有计划任务。/ Calendar：这一天还没有任务。） */
  emptyNote?: string;
  /** 空态按钮（§109：仅 Today 传；Calendar 日报内不显示快捷入口） */
  onEmptyCreate?: () => void;
  onEmptyQuickStart?: () => void;
}) {
  const navigate = useNavigate();
  const [editing, setEditing] = useState<DailyTaskRow | null>(null);
  const [deleting, setDeleting] = useState<DailyTaskRow | null>(null);
  const [error, setError] = useState("");
  /** ⋯ 菜单（§36-37） */
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const { conflict, guard, close } = useActiveSessionConflict();

  const groups = useMemo(() => {
    const g: Record<"core" | "normal" | "accumulation", DailyTaskRow[]> = {
      core: [],
      normal: [],
      accumulation: [],
    };
    for (const t of tasks) g[taskGroupOf(t)].push(t);
    return g;
  }, [tasks]);

  const itemOf = (id: number | null) => (id == null ? undefined : items.find((i) => i.id === id));

  /** §83：知识轻量显示「父 / 子」两级，不做全条 Breadcrumb */
  function knowledgeLabel(t: DailyTaskRow): string | null {
    const it = itemOf(t.learning_item_id);
    if (!it) return t.knowledge_name ?? null;
    const parent = it.parent_id != null ? itemOf(it.parent_id) : undefined;
    return parent ? `${parent.name} / ${it.name}` : it.name;
  }

  /** §23：完成 / 取消完成（现有 complete/uncomplete 命令） */
  async function toggle(t: DailyTaskRow) {
    setError("");
    try {
      await (t.status === "completed" ? uncompleteTask(t.id) : completeTask(t.id));
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  }

  /** [开始学习]：从 Task 开始（start_task_session）；Active Session 冲突 → Start Guard 弹窗 */
  async function start(t: DailyTaskRow) {
    setError("");
    try {
      const s = await startTaskSession(t.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guard(e)) return;
      setError(String(e));
    }
  }

  /** §18-20 现有删除逻辑：无历史物理删除；有历史 → archive（学习历史保留） */
  async function handleDelete(t: DailyTaskRow) {
    setError("");
    try {
      const outcome = await deleteTask(t.id);
      if (!outcome.deleted) await archiveTask(t.id);
      setDeleting(null);
      setEditing(null);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  }

  // 父级受控 createOpen：直接受控渲染新建 Modal
  const showCreate = createOpen === true;

  return (
    <div className="today__tasks">
      {error && <div className="alert alert--error">{error}</div>}

      {tasks.length === 0 ? (
        <div className="today__empty">
          <p className="today__empty-note">{emptyNote ?? "这一天还没有任务。"}</p>
          {(onEmptyCreate || onEmptyQuickStart) && (
            <div className="btn-row today__empty-btns">
              {onEmptyCreate && (
                <button className="btn btn--small btn--primary" onClick={onEmptyCreate}>
                  新建任务
                </button>
              )}
              {onEmptyQuickStart && (
                <button className="btn btn--small" onClick={onEmptyQuickStart}>
                  快速学习
                </button>
              )}
            </div>
          )}
        </div>
      ) : (
        (["core", "normal", "accumulation"] as const).map((key) =>
          groups[key].length === 0 ? null : (
            <div key={key} className="today__group">
              <div className="today__group-title">{GROUP_LABELS[key]}</div>
              <ul className="today__tasklist">
                {groups[key].map((t) => {
                  const done = t.status === "completed";
                  /** §81-84 Meta：预计 Xmin · 知识归属（轻量两级；缺失项跳过） */
                  const meta = [
                    t.estimated_minutes != null ? `预计 ${t.estimated_minutes}m` : null,
                    knowledgeLabel(t),
                  ]
                    .filter((x): x is string => x != null)
                    .join(" · ");
                  return (
                    <li key={t.id} className={"today__task" + (done ? " today__task--done" : "")}>
                      <button
                        className="today__task-check"
                        title={done ? "恢复未完成" : "完成"}
                        onClick={() => void toggle(t)}
                      >
                        {done ? "☑" : "☐"}
                      </button>
                      <button className="today__task-main" onClick={() => setEditing(t)} title="编辑任务">
                        <span className="today__task-title">{t.title}</span>
                        {meta && <span className="today__task-meta">{meta}</span>}
                      </button>
                      <div className="today__task-acts">
                        {done ? (
                          <button className="btn btn--small" onClick={() => setEditing(t)}>
                            查看
                          </button>
                        ) : (
                          <button
                            className="btn btn--small btn--primary"
                            onClick={() => void start(t)}
                          >
                            开始
                          </button>
                        )}
                        <div className="taskmenu">
                          <button
                            className="taskmenu__btn"
                            title="更多操作"
                            onClick={(e) => {
                              e.stopPropagation();
                              setMenuFor(menuFor === t.id ? null : t.id);
                            }}
                          >
                            ⋯
                          </button>
                          {menuFor === t.id && (
                            <>
                              <div className="actrow__backdrop" onClick={() => setMenuFor(null)} />
                              <div className="taskmenu__pop">
                                {/* DEV-0063 §22：菜单收敛为 编辑 + 分隔线 + 删除；
                                    编辑继续打开同一 TaskFormModal（全部字段可改） */}
                                <button onClick={() => { setMenuFor(null); setEditing(t); }}>
                                  编辑
                                </button>
                                <span className="taskmenu__sep" aria-hidden="true" />
                                <button
                                  className="taskmenu__danger"
                                  onClick={() => {
                                    setMenuFor(null);
                                    setDeleting(t);
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
            </div>
          )
        )
      )}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={conflict}
        onClose={close}
        onResolved={() => void onChanged()}
      />

      {/* 编辑 Modal（§23/§84-85：点击 Task 打开 Detail/Edit，不只显示文字） */}
      {editing && (
        <TaskFormModal
          mode="edit"
          profileId={profileId}
          task={editing}
          items={items}
          goals={goals}
          onDelete={() => setDeleting(editing)}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await onChanged();
          }}
        />
      )}

      {/* 新建 Modal（父级按钮触发） */}
      {showCreate && !editing && (
        <TaskFormModal
          mode="create"
          profileId={profileId}
          items={items}
          goals={goals}
          defaultDate={defaultDate ?? todayDate()}
          onClose={() => onCreateClose?.()}
          onSaved={async () => {
            onCreateClose?.();
            await onChanged();
          }}
        />
      )}

      {/* 删除确认（§18-20 人话提示） */}
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

/**
 * Task V2 表单 Modal（DEV-0053 §15-23）：
 * 标题 / 日期 / 时间 / 预计分钟（1-1440）/ task_kind（结构型|积累型）/ priority（核心|常规）/
 * Goal 选择 / Knowledge 选择 → create_task_v2 / update_task_v2。
 */
export function TaskFormModal({
  mode,
  profileId,
  task,
  items,
  goals,
  defaultDate,
  onClose,
  onSaved,
  onDelete,
}: {
  mode: "create" | "edit";
  profileId: number;
  task?: DailyTaskRow | null;
  items: LearningItem[];
  goals: Goal[];
  defaultDate?: string;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  /** 编辑模式删除入口（由父级弹确认） */
  onDelete?: () => void;
}) {
  const [title, setTitle] = useState(task?.title ?? "");
  // DailyTaskRow 不带 planned_date：本区任务都属于渲染日（Today=今天 / Calendar=选中日）
  const [date, setDate] = useState(defaultDate ?? todayDate());
  const [time, setTime] = useState(task?.planned_time ?? "");
  const [estimate, setEstimate] = useState(
    task?.estimated_minutes != null ? String(task.estimated_minutes) : ""
  );
  const [kind, setKind] = useState<"structured" | "accumulation">(
    task?.task_kind ?? "structured"
  );
  const [priority, setPriority] = useState<"core" | "normal">(
    task?.priority ?? "normal"
  );
  const [goalId, setGoalId] = useState<number | "">(task?.goal_id ?? "");
  const [search, setSearch] = useState("");
  const [itemId, setItemId] = useState<number | null>(task?.learning_item_id ?? null);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? items.filter((i) => i.name.toLowerCase().includes(q)) : items;
  }, [items, search]);

  async function save() {
    const name = title.trim();
    if (!name) {
      setError("请输入任务名称");
      return;
    }
    let minutes: number | null = null;
    if (estimate.trim() !== "") {
      minutes = Number(estimate);
      if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440) {
        setError("预计分钟需为 1-1440 的整数");
        return;
      }
    }
    setSaving(true);
    setError("");
    try {
      const payload = {
        profileId,
        title: name,
        plannedDate: date || null,
        plannedTime: time.trim() || null,
        goalId: goalId === "" ? null : goalId,
        learningItemId: itemId,
        estimatedMinutes: minutes,
        taskKind: kind,
        // 积累型不区分 core/normal（§20），统一存 normal
        priority: kind === "accumulation" ? "normal" : priority,
      };
      if (mode === "create") {
        await createTaskV2(payload);
      } else if (task) {
        await updateTaskV2({ id: task.id, ...payload });
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
      <div className="modal modal--quick" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">{mode === "create" ? "新建任务" : "编辑任务"}</div>
        {error && <div className="modal__error">{error}</div>}

        <label className="modal__field">
          任务名称 *
          <input
            className="modal__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="要做什么？如：学习极限定义"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void save();
              } else if (e.key === "Escape") {
                onClose();
              }
            }}
          />
        </label>

        <div className="taskmodal__daterow">
          <label className="modal__field">
            日期
            <input
              className="modal__input"
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value)}
            />
          </label>
          <label className="modal__field">
            时间（可选）
            <input
              className="modal__input"
              type="time"
              value={time}
              onChange={(e) => setTime(e.target.value)}
            />
          </label>
          <label className="modal__field">
            预计分钟（可选）
            <input
              className="modal__input"
              type="number"
              min={1}
              max={1440}
              value={estimate}
              onChange={(e) => setEstimate(e.target.value)}
              placeholder="如 60"
            />
          </label>
        </div>

        <div className="taskmodal__daterow">
          <label className="modal__field">
            任务类型
            <select
              className="modal__input"
              value={kind}
              onChange={(e) => setKind(e.target.value as "structured" | "accumulation")}
            >
              <option value="structured">结构型（对应知识节点）</option>
              <option value="accumulation">积累型（如背单词）</option>
            </select>
          </label>
          {kind === "structured" && (
            <label className="modal__field">
              优先级
              <select
                className="modal__input"
                value={priority}
                onChange={(e) => setPriority(e.target.value as "core" | "normal")}
              >
                <option value="core">核心</option>
                <option value="normal">常规</option>
              </select>
            </label>
          )}
        </div>

        {goals.length > 0 && (
          <label className="modal__field">
            目标（可选）
            <select
              className="modal__input"
              value={goalId}
              onChange={(e) => setGoalId(e.target.value ? Number(e.target.value) : "")}
            >
              <option value="">不关联目标</option>
              {goals.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </label>
        )}

        <div className="modal__field">
          <span className="taskmodal__field-label">关联知识（可选）</span>
          <div className="taskmodal__picker">
            <input
              className="modal__input taskmodal__search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索知识…"
            />
            {itemId != null && (
              <button className="btn btn--small" onClick={() => setItemId(null)}>
                清除关联
              </button>
            )}
          </div>
          <div className="taskmodal__list">
            {filtered.slice(0, 40).map((i) => (
              <button
                key={i.id}
                className={"taskmodal__item" + (itemId === i.id ? " taskmodal__item--active" : "")}
                onClick={() => setItemId(i.id)}
              >
                {i.name}
              </button>
            ))}
            {filtered.length === 0 && (
              <span className="muted" style={{ fontSize: 12 }}>
                没有匹配的知识，可以不关联。
              </span>
            )}
          </div>
          {itemId != null && (
            <p className="muted" style={{ fontSize: 11, margin: "4px 0 0" }}>
              已关联：{items.find((i) => i.id === itemId)?.name ?? `#${itemId}`}
            </p>
          )}
        </div>

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : mode === "create" ? "创建" : "保存"}
          </button>
          {mode === "edit" && onDelete && (
            <button className="btn taskmenu__danger" onClick={onDelete} disabled={saving}>
              删除
            </button>
          )}
          <button className="btn" onClick={onClose}>
            取消（Esc）
          </button>
        </div>
      </div>
    </div>
  );
}
