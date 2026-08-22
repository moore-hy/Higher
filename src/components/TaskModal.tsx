import { useMemo, useState } from "react";
import {
  createChildLearningItem,
  createRootLearningItem,
  createTask,
  createRecurringRule,
  materializeRecurringTasks,
  updateTask,
} from "../api";
import type { Goal, LearningItem, Task } from "../types";
import { todayDate } from "../utils";

/**
 * 任务 Modal（DEV-0031 重做：Quick Add；DEV-0042 v013 Profile First）。
 *
 * 第一屏只有：任务名称 *（自动聚焦）+ 日期（默认今天）。
 * 其余全部收进「更多设置」：时间 / 目标（可选）/ 关联知识（可选：搜索+快速新建）/ 重复（仅新建）。
 * - Enter 创建；Esc 关闭；「创建并继续」连续添加
 * - 唯一必填 = 任务名称（§八：输入"数学"回车即成功；Goal/Knowledge 都不是前提）
 */
export default function TaskModal({
  mode,
  profileId,
  defaultDate,
  defaultGoalId,
  task,
  items,
  goals,
  onClose,
  onSaved,
}: {
  mode: "create" | "edit";
  /** v013 Profile First：任务直挂 profile */
  profileId: number;
  defaultDate?: string;
  /** DEV-0050：从目标树/下一步进入时预填关联目标（仅 create 生效） */
  defaultGoalId?: number | null;
  task?: Task | null;
  items: LearningItem[];
  goals: Goal[];
  onClose: () => void;
  onSaved: (opts?: { keepOpen?: boolean }) => Promise<void> | void;
}) {
  const [title, setTitle] = useState(task?.title ?? "");
  const [date, setDate] = useState(task?.planned_date ?? defaultDate ?? todayDate());
  const [showMore, setShowMore] = useState(false);
  const [time, setTime] = useState(task?.planned_time ?? "");
  const [goalId, setGoalId] = useState<number | "">(
    task?.goal_id ?? defaultGoalId ?? goals.find((g) => g.status === "active")?.id ?? ""
  );
  const [search, setSearch] = useState("");
  const [itemId, setItemId] = useState<number | null>(task?.learning_item_id ?? null);
  const [showQuick, setShowQuick] = useState(false);
  const [quickName, setQuickName] = useState("");
  const [quickParent, setQuickParent] = useState<number | null>(null);
  // 重复（仅创建模式；与 Planning 重复任务同一规则体系）
  const [repeat, setRepeat] = useState<"none" | "daily" | "weekly">("none");
  const [weekdays, setWeekdays] = useState<number[]>([1, 3, 5]);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const selectedGoal = goalId === "" ? null : goals.find((g) => g.id === goalId) ?? null;
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? items.filter((i) => i.name.toLowerCase().includes(q)) : items;
  }, [items, search]);

  async function quickCreate() {
    if (!quickName.trim()) return;
    setError("");
    try {
      const created = quickParent
        ? await createChildLearningItem(
            profileId,
            quickParent,
            selectedGoal?.id ?? null,
            quickName.trim()
          )
        : await createRootLearningItem(profileId, selectedGoal?.id ?? null, quickName.trim());
      setShowQuick(false);
      setQuickName("");
      setItemId(created.id);
    } catch (e) {
      setError(String(e));
    }
  }

  async function save(keepOpen: boolean) {
    if (!title.trim()) {
      setError("请输入任务名称");
      return;
    }
    if (repeat === "weekly" && weekdays.length === 0) {
      setError("每周重复需选择至少一个星期");
      return;
    }
    setSaving(true);
    setError("");
    try {
      if (mode === "create") {
        if (repeat !== "none") {
          // DEV-0060.1 PART G：选了重复 → 只建规则，不先建无 rule_id 的普通 Task；
          // 首日任务由规则 materialize 生成（带 rule_id，exists_for_rule_date 幂等）。
          // Knowledge Optional：不再要求先关联知识（AI-INV-007）。
          await createRecurringRule({
            profileId,
            goalId: selectedGoal?.id ?? null,
            learningItemId: itemId,
            title: title.trim(),
            repeatType: repeat,
            weekdays: repeat === "weekly" ? [...weekdays].sort() : [],
            timeOfDay: time.trim() || null,
            startDate: date || todayDate(),
            endDate: null,
          });
          // 首日任务由规则物化（命中当日 recurrence 才生成；幂等）
          await materializeRecurringTasks(profileId, date || todayDate());
        } else {
          // v013：profileId 必填；goal 可空（title-only 任务不依赖目标）
          await createTask({
            profileId,
            goalId: selectedGoal?.id ?? null,
            title: title.trim(),
            plannedDate: date || null,
            plannedTime: time.trim() || null,
            learningItemId: itemId,
            planId: null,
          });
        }
      } else if (task) {
        await updateTask({
          id: task.id,
          title: title.trim(),
          plannedDate: date || null,
          plannedTime: time.trim() || null,
          learningItemId: itemId,
        });
      }
      await onSaved({ keepOpen });
      if (keepOpen) {
        setTitle("");
        setSearch("");
        setItemId(null);
        setTime("");
        setRepeat("none");
      }
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

        <input
          className="modal__input taskmodal__name"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="要做什么？如：数学"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void save(false);
            } else if (e.key === "Escape") {
              onClose();
            }
          }}
        />
        <input
          className="modal__input"
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save(false);
            if (e.key === "Escape") onClose();
          }}
        />

        <button className="taskmodal__more-toggle" onClick={() => setShowMore((v) => !v)}>
          {showMore ? "收起更多设置 ▴" : "更多设置（时间 / 目标 / 关联知识 / 重复） ▾"}
        </button>

        {showMore && (
          <div className="taskmodal__more">
            <label className="modal__field">
              时间（可选）
              <input
                className="modal__input"
                type="time"
                value={time}
                onChange={(e) => setTime(e.target.value)}
              />
            </label>

            {goals.length > 0 && (
              <label className="modal__field">
                目标（可选——不选也能创建任务）
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
              <span className="taskmodal__field-label">
                关联知识（可选——不关联也可以创建任务）
              </span>
              <div className="taskmodal__picker">
                <input
                  className="modal__input taskmodal__search"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="搜索知识…"
                />
                <button className="btn btn--small" onClick={() => setShowQuick((v) => !v)}>
                  + 快速新建知识
                </button>
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
                    className={
                      "taskmodal__item" + (itemId === i.id ? " taskmodal__item--active" : "")
                    }
                    onClick={() => setItemId(i.id)}
                  >
                    {i.name}
                  </button>
                ))}
                {filtered.length === 0 && (
                  <span className="muted" style={{ fontSize: 12 }}>
                    没有匹配的知识。可以不关联，或快速新建。
                  </span>
                )}
              </div>
              {itemId != null && (
                <p className="muted" style={{ fontSize: 11, margin: "4px 0 0" }}>
                  已关联：{items.find((i) => i.id === itemId)?.name ?? `#${itemId}`}
                </p>
              )}
              {showQuick && (
                <div className="taskmodal__quick">
                  <input
                    className="modal__input"
                    value={quickName}
                    onChange={(e) => setQuickName(e.target.value)}
                    placeholder="新知识名称…"
                  />
                  <select
                    className="modal__input"
                    value={quickParent ?? ""}
                    onChange={(e) =>
                      setQuickParent(e.target.value ? Number(e.target.value) : null)
                    }
                  >
                    <option value="">作为顶级知识</option>
                    {items.slice(0, 80).map((i) => (
                      <option key={i.id} value={i.id}>
                        放在「{i.name}」下
                      </option>
                    ))}
                  </select>
                  <button className="btn btn--small btn--primary" onClick={() => void quickCreate()}>
                    创建并关联
                  </button>
                </div>
              )}
            </div>

            {mode === "create" && (
              <div className="modal__field">
                <span className="taskmodal__field-label">重复（可选）</span>
                <div className="taskmodal__picker">
                  {(["none", "daily", "weekly"] as const).map((r) => (
                    <button
                      key={r}
                      className={"chip" + (repeat === r ? " chip--active" : "")}
                      onClick={() => setRepeat(r)}
                    >
                      {r === "none" ? "不重复" : r === "daily" ? "每天" : "每周"}
                    </button>
                  ))}
                </div>
                {repeat === "weekly" && (
                  <div className="taskmodal__picker">
                    {["一", "二", "三", "四", "五", "六", "日"].map((w, i) => (
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
              </div>
            )}
          </div>
        )}

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save(false)} disabled={saving}>
            {saving ? "保存中…" : mode === "create" ? "创建" : "保存"}
          </button>
          {mode === "create" && (
            <button className="btn" onClick={() => void save(true)} disabled={saving}>
              创建并继续
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
