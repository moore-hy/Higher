import { useCallback, useEffect, useState } from "react";
import { getFinalGoalState, saveFinalGoalBrief } from "../api";
import { useAiPanel } from "./ai/AiPanelContext";
import type { GoalBrief, GoalState } from "../types";

/**
 * Final Goal Card（DEV-0055 PART 37 §139-141 / §159；DEV-0058 PART E-H 收口）。
 *
 * Planning 第一屏的目标卡：
 * - 完整（§35-38）：标题 + 一句 outcome + 截止；主按钮「AI 生成计划」+ 次「编辑目标」；详情点击展开
 * - 信息不足（§31-33）：`目标待完善` + [完善目标](primary) + [AI 帮我梳理](ghost)
 * - conflicts 非空（§69-72 不自动选）：黄色警示条
 * - DEV-0058 §47/§51/§167：「AI 生成计划」= 统一 Planner 入口（与 Today「AI安排」/对话写意图
 *   同一 Planning Pipeline——后端 planning_gate 确定性分流）
 */

/** DEV-0058 §51-53/§85：三入口统一的规划请求文案（含明确写意图词，确保命中 planning_gate） */
export const PLAN_REQUEST_MESSAGE =
  "根据我的最终目标和个人情况，帮我安排未来14天学习计划，并加入 Higher。";

/** 多行文本 ↔ string[]（一行一条；空行忽略） */
function linesToList(v: string): string[] {
  return v
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);
}

function listToLines(list: string[]): string {
  return list.filter(Boolean).join("\n");
}

export default function FinalGoalCard({
  profileId,
  onChanged,
}: {
  profileId: number;
  /** 保存成功后通知父级（Planning 无需刷新其他数据，预留） */
  onChanged?: () => void;
}) {
  const { sendChat, setOpen } = useAiPanel();
  const [state, setState] = useState<GoalState | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setState(await getFinalGoalState(profileId));
    } catch {
      setState(null);
    } finally {
      setLoading(false);
    }
  }, [profileId]);

  useEffect(() => {
    reload();
  }, [reload]);

  const brief = state?.brief;
  const incomplete =
    !brief ||
    !brief.title.trim() ||
    !brief.outcome.trim() ||
    (state?.missing.length ?? 0) > 0;

  /** §159：AI 帮我梳理（打开 AI Panel 发预设消息；Clarification 流程由 AI 侧承接） */
  async function askAiClarify() {
    setOpen(true);
    await sendChat(
      "帮我梳理并完善我的最终目标：请先和我确认「最终想实现什么、截止时间、成功标准、范围与约束」，再帮我整理成一句清晰的目标。"
    );
  }

  /** DEV-0058 §38/§51/§167：AI 生成计划（三入口统一 Planner；Goal 已 Ready 才显示） */
  async function askAiPlan() {
    setOpen(true);
    await sendChat(PLAN_REQUEST_MESSAGE);
  }

  if (loading) {
    return (
      <section className="card fgcard">
        <p className="muted">加载目标…</p>
      </section>
    );
  }

  return (
    <section className="card fgcard">
      <div className="fgcard__head">
        <span className="fgcard__label">最终目标</span>
        <button className="btn btn--small" onClick={() => setEditing(true)}>
          编辑
        </button>
      </div>

      {/* §169：冲突不自动选，必须提示确认 */}
      {(state?.conflicts.length ?? 0) > 0 && (
        <div className="fgcard__conflict">目标信息存在冲突，需要确认。</div>
      )}

      {incomplete ? (
        <>
          <div className="fgcard__title fgcard__title--pending">目标待完善</div>
          {(state?.missing.length ?? 0) > 0 && (
            <ul className="fgcard__missing">
              {state?.missing.map((m) => (
                <li key={m}>还需要确认：{m}</li>
              ))}
            </ul>
          )}
          <div className="btn-row fgcard__actions">
            <button className="btn btn--small btn--primary" onClick={() => setEditing(true)}>
              完善目标
            </button>
            <button className="btn btn--small btn--ghost" onClick={() => void askAiClarify()}>
              AI 帮我梳理
            </button>
          </div>
        </>
      ) : (
        <>
          {/* §35-38：标题 + outcome + deadline；主按钮「AI 生成计划」+ 次「编辑目标」 */}
          <div className="fgcard__title">{brief!.title}</div>
          <p className="fgcard__outcome">{brief!.outcome}</p>
          <p className="fgcard__deadline">
            {brief!.deadline ? `截止 ${brief!.deadline}` : "无截止时间"}
          </p>
          <div className="btn-row fgcard__actions">
            <button className="btn btn--small btn--primary" onClick={() => void askAiPlan()}>
              AI 生成计划
            </button>
            <button className="btn btn--small btn--ghost" onClick={() => setEditing(true)}>
              编辑目标
            </button>
          </div>
          <button className="fgcard__more" onClick={() => setExpanded((v) => !v)}>
            {expanded ? "收起详情 ▴" : "查看详情 ▾"}
          </button>
          {expanded && (
            <div className="fgcard__detail">
              {brief!.success_criteria.length > 0 && (
                <div className="fgcard__detail-block">
                  <span className="fgcard__detail-label">成功标准</span>
                  <ul>
                    {brief!.success_criteria.map((s, i) => (
                      <li key={i}>{s}</li>
                    ))}
                  </ul>
                </div>
              )}
              {brief!.scope.length > 0 && (
                <div className="fgcard__detail-block">
                  <span className="fgcard__detail-label">范围</span>
                  <ul>
                    {brief!.scope.map((s, i) => (
                      <li key={i}>{s}</li>
                    ))}
                  </ul>
                </div>
              )}
              {brief!.constraints.length > 0 && (
                <div className="fgcard__detail-block">
                  <span className="fgcard__detail-label">约束</span>
                  <ul>
                    {brief!.constraints.map((s, i) => (
                      <li key={i}>{s}</li>
                    ))}
                  </ul>
                </div>
              )}
              {brief!.unresolved.length > 0 && (
                <div className="fgcard__detail-block">
                  <span className="fgcard__detail-label">未决事项</span>
                  <ul>
                    {brief!.unresolved.map((s, i) => (
                      <li key={i}>{s}</li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </>
      )}

      {editing && (
        <GoalBriefModal
          initial={brief ?? null}
          onClose={() => setEditing(false)}
          onSaved={async (next) => {
            setEditing(false);
            await saveFinalGoalBrief(profileId, next);
            await reload();
            onChanged?.();
          }}
        />
      )}
    </section>
  );
}

/** §139-141 / §198：目标 Brief 编辑表单（多行 = 一行一条） */
function GoalBriefModal({
  initial,
  onClose,
  onSaved,
}: {
  initial: GoalBrief | null;
  onClose: () => void;
  onSaved: (brief: GoalBrief) => Promise<void>;
}) {
  const [title, setTitle] = useState(initial?.title ?? "");
  const [outcome, setOutcome] = useState(initial?.outcome ?? "");
  const [deadline, setDeadline] = useState(initial?.deadline ?? "");
  const [criteria, setCriteria] = useState(listToLines(initial?.success_criteria ?? []));
  const [scope, setScope] = useState(listToLines(initial?.scope ?? []));
  const [constraints, setConstraints] = useState(listToLines(initial?.constraints ?? []));
  const [unresolved, setUnresolved] = useState(listToLines(initial?.unresolved ?? []));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function save() {
    if (!title.trim()) {
      setError("请填写目标标题");
      return;
    }
    setSaving(true);
    setError("");
    try {
      await onSaved({
        title: title.trim(),
        outcome: outcome.trim(),
        deadline: deadline || null,
        success_criteria: linesToList(criteria),
        scope: linesToList(scope),
        constraints: linesToList(constraints),
        unresolved: linesToList(unresolved),
      });
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">最终目标</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          标题 *
          <input
            className="modal__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="如：2027 考研"
            autoFocus
          />
        </label>
        <label className="modal__field">
          最终想实现什么（一句话）*
          <input
            className="modal__input"
            value={outcome}
            onChange={(e) => setOutcome(e.target.value)}
            placeholder="如：考上华中科技大学计算机专业"
          />
        </label>
        <label className="modal__field">
          截止日期（不确定可先留空）
          <input
            className="modal__input"
            type="date"
            value={deadline ?? ""}
            onChange={(e) => setDeadline(e.target.value)}
          />
        </label>
        <label className="modal__field">
          成功标准（一行一条）
          <textarea
            className="modal__input"
            rows={3}
            value={criteria}
            onChange={(e) => setCriteria(e.target.value)}
            placeholder={"如：初试总分 ≥ 380\n数学一 ≥ 130"}
          />
        </label>
        <label className="modal__field">
          范围（一行一条）
          <textarea
            className="modal__input"
            rows={2}
            value={scope}
            onChange={(e) => setScope(e.target.value)}
            placeholder="如：数学、英语、政治、408"
          />
        </label>
        <label className="modal__field">
          约束（一行一条）
          <textarea
            className="modal__input"
            rows={2}
            value={constraints}
            onChange={(e) => setConstraints(e.target.value)}
            placeholder="如：在职备考，工作日每天 3 小时"
          />
        </label>
        <label className="modal__field">
          未决事项（一行一条）
          <textarea
            className="modal__input"
            rows={2}
            value={unresolved}
            onChange={(e) => setUnresolved(e.target.value)}
            placeholder="还没有想清楚的问题"
          />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}
