/**
 * A2-3 §25 —— ME V1：最小可见产品面。
 *
 * # 这个组件**不**做什么
 *
 * ```text
 * 不判定 source_class   —— 那是后端的权威判定（A23-11）
 * 不把 Unknown 说成结论 —— 「没有数据」就是「没有数据」
 * 不用推断填满空白      —— §25：最后一个必须真的显示 Unknown
 * ```
 *
 * 它只做一件事：把后端给的 PersonStateSnapshot **如实**渲染出来，
 * 并且让每一条都能看到「Higher 为什么这么认为」（§26）。
 */

import { useEffect, useMemo, useState } from "react";
import {
  getPersonState,
  type CapabilityAxis,
  type GoalMode,
  type KnownCount,
  type KnownText,
  type PersonStateSnapshot,
  type SourceClass,
} from "../../api";

/** 来源类别 → 展示标签。**只做展示**，不做判定。 */
const CLASS_LABEL: Record<SourceClass, string> = {
  confirmed_by_user: "CONFIRMED",
  observed: "OBSERVED",
  inferred: "INFERRED",
  unknown: "UNKNOWN",
};

const CLASS_ORDER: SourceClass[] = ["confirmed_by_user", "observed", "inferred", "unknown"];

/** A2-4 §33：八条能力轴 → 展示标签。**只做展示**，值来自后端投影。 */
const AXIS_LABEL: Record<CapabilityAxis, string> = {
  understand: "理解",
  recall: "回忆",
  apply: "应用",
  independent: "独立",
  debug: "排错",
  build: "建构",
  transfer: "迁移",
  retain: "保持",
};

/** A2-4 §31：Goal Mode 词表 → 展示标签。没有结构化来源时后端给 `unclassified`。 */
const MODE_LABEL: Record<GoalMode, string> = {
  exam: "考试导向",
  growth: "能力导向",
  life: "生活",
  maintenance: "维持",
  unclassified: "未分类（不猜）",
};

interface Row {
  id: string;
  label: string;
  known: KnownText | KnownCount;
}

function KnownRow({ label, known }: { label: string; known: KnownText | KnownCount }) {
  const cls = known.source_class;
  const value =
    known.value === null || known.value === undefined
      ? null
      : typeof known.value === "number"
        ? String(known.value)
        : known.value;

  return (
    <div className="me__row" data-testid={`me-row-${label}`}>
      <div className="me__row-head">
        <span className={`me__tag me__tag--${cls}`}>{CLASS_LABEL[cls]}</span>
        <span className="me__row-label">{label}</span>
      </div>
      {/* §25：Unknown 必须真的显示 Unknown，不许渲染成一句听起来像结论的话。 */}
      <p className="me__row-value">
        {cls === "unknown" || value === null ? "还不知道" : value}
      </p>
      {/* §26：为什么 Higher 这么认为 —— 数据结构与 UI 都必须保留。 */}
      <p className="me__row-why">{known.reason}</p>
      {known.evidence_refs.length > 0 && (
        <p className="me__row-refs">依据：{known.evidence_refs.join(" · ")}</p>
      )}
    </div>
  );
}

export default function MePanel({ profileId }: { profileId: number }) {
  const [state, setState] = useState<PersonStateSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    getPersonState(profileId)
      .then((s) => {
        if (!alive) return;
        setState(s);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!alive) return;
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [profileId]);

  const rows = useMemo<Row[]>(() => {
    if (!state) return [];
    return [
      { id: "focus", label: "当前焦点", known: state.learning.current_focus },
      { id: "recall", label: "回忆状态", known: state.learning.recall_state },
      { id: "apply", label: "应用状态", known: state.learning.application_state },
      { id: "verified", label: "已验证证据", known: state.learning.verified_evidence_count },
      { id: "done", label: "已完成任务", known: state.execution.tasks_done_today },
      { id: "open", label: "未完成任务", known: state.execution.open_tasks },
      { id: "minutes", label: "今日时长", known: state.time.minutes_today },
    ];
  }, [state]);

  if (loading) return <p className="me__loading">读取 Higher 对你的理解…</p>;
  if (error) return <p className="me__error">读取失败：{error}</p>;
  if (!state) return null;

  const byClass = (cls: SourceClass) => rows.filter((r) => r.known.source_class === cls);

  return (
    <div className="me" data-testid="me-panel">
      <header className="me__head">
        <h2 className="me__title">ME</h2>
        <p className="me__sub">
          {state.profile_name} · 这台机器上一共有 {state.person_profile_count} 个工作区
        </p>
      </header>

      <section className="me__section">
        <h3 className="me__h3">Current Focus</h3>
        <KnownRow
          label="当前焦点"
          known={state.learning.current_focus}
        />
      </section>

      <section className="me__section">
        <h3 className="me__h3">Goals</h3>
        {state.goals.length === 0 ? (
          <p className="me__empty">还没有目标。</p>
        ) : (
          <ul className="me__goals">
            {state.goals.map((g) => (
              <li key={g.id} className="me__goal">
                <span className={`me__tag me__tag--${g.source_class}`}>
                  {CLASS_LABEL[g.source_class]}
                </span>
                <span>{g.name}</span>
                {/* A2-4 §31：mode 由后端判定，UI 绝不按标题猜；没有来源就是 unclassified。 */}
                <span className="me__mode" title={g.goal_mode.reason}>
                  {MODE_LABEL[g.goal_mode.mode]}
                </span>
                {g.goal_mode.focuses_on.length > 0 && (
                  <span className="me__muted">
                    关注 {g.goal_mode.focuses_on.join(" · ")}
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="me__section">
        <h3 className="me__h3">Higher Knows</h3>
        {CLASS_ORDER.map((cls) => {
          const group = byClass(cls);
          if (group.length === 0) return null;
          return (
            <div key={cls} className="me__group" data-testid={`me-group-${cls}`}>
              <p className="me__group-title">{CLASS_LABEL[cls]}</p>
              {group.map((r) => (
                <KnownRow key={r.id} label={r.label} known={r.known} />
              ))}
            </div>
          );
        })}
      </section>

      {/* A2-4 §33：能力是 Learner Model / Evidence 的**只读**投影。
          没有证据的轴 value 是 null —— 这里必须渲染成「还不知道」，
          绝不为了填满面板写 beginner / 50%。 */}
      <section className="me__section">
        <h3 className="me__h3">Capability</h3>
        {state.capability.axes.length === 0 ? (
          <p className="me__empty">还没有可投影的能力轴。</p>
        ) : (
          <ul className="me__axes">
            {state.capability.axes.map((a) => (
              <li key={a.axis} className="me__axis" data-testid={`me-axis-${a.axis}`}>
                <div className="me__row-head">
                  <span className={`me__tag me__tag--${a.source_class}`}>
                    {CLASS_LABEL[a.source_class]}
                  </span>
                  <span className="me__row-label">{AXIS_LABEL[a.axis]}</span>
                  <span className="me__axis-value">
                    {a.value === null ? "还不知道" : a.value}
                  </span>
                </div>
                <p className="me__row-why">{a.reason}</p>
                {a.evidence_refs.length > 0 && (
                  <p className="me__row-refs">依据：{a.evidence_refs.join(" · ")}</p>
                )}
              </li>
            ))}
          </ul>
        )}
        {state.capability.note && <p className="me__muted">{state.capability.note}</p>}
      </section>

      {/* §23：Body V1 恒为 Unknown（本轮不接手表）—— 必须真的显示出来。 */}
      <section className="me__section">
        <h3 className="me__h3">Body</h3>
        <KnownRow label="睡眠" known={state.body.sleep} />
        <KnownRow label="恢复" known={state.body.recovery} />
      </section>

      <section className="me__section">
        <h3 className="me__h3">Workspaces</h3>
        <ul className="me__goals">
          {state.workspaces.map((w) => (
            <li key={w.profile_id} className="me__goal">
              <span>{w.name}</span>
              <span className="me__muted">（{w.goal_count} 个目标）</span>
            </li>
          ))}
        </ul>
      </section>

      <p className="me__foot">
        以上是 Higher 目前「真的知道」的事。标着 UNKNOWN 的就是不知道 ——
        它不会被猜测填满。
      </p>
    </div>
  );
}
