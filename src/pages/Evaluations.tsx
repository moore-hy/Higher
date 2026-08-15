import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  createEvaluation,
  deleteEvaluation,
  getLearningItemPath,
  listEvaluationsByGoal,
  listGoalsByProfile,
  listLearningItemsByGoal,
  listRecentEvaluationsByProfile,
  updateEvaluation,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import {
  EVALUATION_TYPE_LABELS,
  EVALUATION_TYPES,
  OUTCOME_LABELS,
  OUTCOMES,
} from "../types";
import type {
  Evaluation,
  EvaluationType,
  Goal,
  LearningItem,
  Outcome,
} from "../types";
import { formatDateTime } from "../utils";

/**
 * Evaluations 页（Evaluation System V1）。
 *
 * 只回答两个问题：
 *   1. 我最近进行了哪些学习验证？
 *   2. 记录一次新的验证。
 *
 * 不做统计 Dashboard、不分析结果（Feedback System）。
 *
 * URL 参数支持（§46 轻量入口）：
 *   ?goal=ID        预选 Goal
 *   &item=ID        预选 Learning Item（必须属于对应 Goal）
 */
function Evaluations() {
  const [searchParams, setSearchParams] = useSearchParams();
  const goalIdParam = searchParams.get("goal");
  const itemIdParam = searchParams.get("item");

  const { activeProfile, refreshKey } = useActiveProfile();

  // ======== 基础状态 ========
  const [goals, setGoals] = useState<Goal[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [evals, setEvals] = useState<Evaluation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // viewMode = "goal"  仅显示当前 Goal 下 Evaluation
  // viewMode = "recent" 显示最近 N 条（跨 Goal）
  const [viewMode, setViewMode] = useState<"goal" | "recent">("recent");

  const goalId = goalIdParam ? Number(goalIdParam) : null;

  // ======== 创建表单状态（分组：基本 / 结果 / 备注） ========
  const [formGoalId, setFormGoalId] = useState<number | "">("");
  const [formItemId, setFormItemId] = useState<number | "">("");
  const [formTitle, setFormTitle] = useState("");
  const [formType, setFormType] = useState<EvaluationType>("test");
  const [formSource, setFormSource] = useState("");
  const [formOccurred, setFormOccurred] = useState(""); // 留空 = 服务器当前时间

  const [formTotal, setFormTotal] = useState("");
  const [formCorrect, setFormCorrect] = useState("");
  const [formIncorrect, setFormIncorrect] = useState("");
  const [formScore, setFormScore] = useState("");
  const [formMaxScore, setFormMaxScore] = useState("");
  const [formOutcome, setFormOutcome] = useState<Outcome>("unrated");
  const [formNote, setFormNote] = useState("");

  // ======== 行内编辑状态 ========
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editType, setEditType] = useState<EvaluationType>("test");
  const [editSource, setEditSource] = useState("");
  const [editOccurred, setEditOccurred] = useState("");
  const [editTotal, setEditTotal] = useState("");
  const [editCorrect, setEditCorrect] = useState("");
  const [editIncorrect, setEditIncorrect] = useState("");
  const [editScore, setEditScore] = useState("");
  const [editMaxScore, setEditMaxScore] = useState("");
  const [editOutcome, setEditOutcome] = useState<Outcome>("unrated");
  const [editNote, setEditNote] = useState("");

  // ======== Item Path 缓存（避免每条都查） ========
  const [pathMap, setPathMap] = useState<Record<number, string>>({});

  const fetchPath = useCallback(async (itemId: number) => {
    if (pathMap[itemId]) return pathMap[itemId];
    try {
      const p = await getLearningItemPath(itemId);
      setPathMap((prev) => ({ ...prev, [itemId]: p }));
      return p;
    } catch {
      const fallback = items.find((i) => i.id === itemId)?.name ?? `#${itemId}`;
      setPathMap((prev) => ({ ...prev, [itemId]: fallback }));
      return fallback;
    }
  }, [items, pathMap]);

  // 对 evals 中涉及的所有 learning_item_id 预热 path
  useEffect(() => {
    const ids = new Set<number>();
    for (const e of evals) {
      if (e.learning_item_id != null) ids.add(e.learning_item_id);
    }
    for (const id of ids) {
      if (!pathMap[id]) fetchPath(id);
    }
  }, [evals, fetchPath, pathMap]);

  // ======== 列表刷新 ========
  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [goalList, evalList] = await Promise.all([
        listGoalsByProfile(activeProfile!.id),
        viewMode === "goal" && goalId != null
          ? listEvaluationsByGoal(goalId)
          : listRecentEvaluationsByProfile(activeProfile!.id, 100),
      ]);
      setGoals(goalList);
      setEvals(evalList);

      // 若 URL 带了 Goal，也拉取该 Goal 的 Learning Item 给下拉 + path 缓存
      if (goalId != null) {
        const its = await listLearningItemsByGoal(goalId);
        setItems(its);
        // 立即预热这些 items 的 path（下拉显示用）
        const pm: Record<number, string> = { ...pathMap };
        await Promise.all(
          its.map(async (it) => {
            if (!pm[it.id]) {
              try {
                pm[it.id] = await getLearningItemPath(it.id);
              } catch {
                pm[it.id] = it.name;
              }
            }
          })
        );
        setPathMap(pm);
      } else {
        setItems([]);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [viewMode, goalId, pathMap, activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 初次加载：预选 goal / item（URL 参数写入表单）
  useEffect(() => {
    if (goalIdParam != null) {
      const gid = Number(goalIdParam);
      if (!Number.isNaN(gid)) setFormGoalId(gid);
    }
    if (itemIdParam != null) {
      const iid = Number(itemIdParam);
      if (!Number.isNaN(iid)) setFormItemId(iid);
    }
    // 仅初次（加载完成时）设置，避免每次 refresh 都覆盖用户输入
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 切换 Goal：写入 URL
  const handleGoalChange = (raw: string) => {
    const newGoal = raw ? Number(raw) : null;
    if (newGoal != null) {
      const p: Record<string, string> = { goal: String(newGoal) };
      if (itemIdParam) p.item = itemIdParam;
      setSearchParams(p, { replace: true });
    } else {
      setSearchParams({}, { replace: true });
    }
    setFormGoalId(newGoal ?? "");
    // 清空 item 选择（换了 Goal 后旧 item 不属于这个 Goal）
    setFormItemId("");
  };

  // ======== 辅助：空字符串 → null，数字字符串转 number ========
  const numOrNull = (s: string): number | null => {
    const v = s.trim();
    if (!v) return null;
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  };

  // ======== 创建 ========
  const handleCreate = async () => {
    if (formGoalId === "" || !Number.isFinite(Number(formGoalId))) {
      setError("请选择目标");
      return;
    }
    if (!formTitle.trim()) {
      setError("标题不能为空");
      return;
    }
    setError("");
    try {
      await createEvaluation({
        profileId: activeProfile!.id,
        goalId: Number(formGoalId),
        learningItemId: formItemId === "" ? null : Number(formItemId),
        title: formTitle.trim(),
        evaluationType: formType,
        source: formSource.trim() || null,
        occurredAt: formOccurred.trim() || null,
        totalItems: numOrNull(formTotal),
        correctItems: numOrNull(formCorrect),
        incorrectItems: numOrNull(formIncorrect),
        score: numOrNull(formScore),
        maxScore: numOrNull(formMaxScore),
        outcome: formOutcome,
        note: formNote.trim() || null,
      });
      // 清空表单（保留 Goal 以便连续录入）
      setFormTitle("");
      setFormSource("");
      setFormOccurred("");
      setFormTotal("");
      setFormCorrect("");
      setFormIncorrect("");
      setFormScore("");
      setFormMaxScore("");
      setFormOutcome("unrated");
      setFormNote("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  // ======== 编辑按钮 ========
  const handleStartEdit = (e: Evaluation) => {
    setEditingId(e.id);
    setEditTitle(e.title);
    setEditType((e.evaluation_type as EvaluationType) ?? "other");
    setEditSource(e.source ?? "");
    setEditOccurred(e.occurred_at?.slice(0, 19).replace(" ", "T") ?? "");
    setEditTotal(e.total_items?.toString() ?? "");
    setEditCorrect(e.correct_items?.toString() ?? "");
    setEditIncorrect(e.incorrect_items?.toString() ?? "");
    setEditScore(e.score?.toString() ?? "");
    setEditMaxScore(e.max_score?.toString() ?? "");
    setEditOutcome((e.outcome as Outcome) ?? "unrated");
    setEditNote(e.note ?? "");
  };

  const handleSaveEdit = async () => {
    if (editingId == null) return;
    if (!editTitle.trim()) {
      setError("标题不能为空");
      return;
    }
    setError("");
    try {
      // 日期格式归一：HTML datetime-local 输出 "YYYY-MM-DDTHH:MM"，
      // 写入 Rust 端需要的 "YYYY-MM-DD HH:MM:SS"。
      const raw = editOccurred.trim();
      const occurredAt = raw
        ? raw.includes("T")
          ? raw.replace("T", " ") + ":00".slice(0, Math.max(0, 19 - (raw.replace("T", " ").length)))
          : raw
        : "";
      await updateEvaluation({
        id: editingId,
        title: editTitle.trim(),
        evaluationType: editType,
        source: editSource.trim() || null,
        occurredAt: occurredAt || new Date().toISOString().slice(0, 19).replace("T", " "),
        totalItems: numOrNull(editTotal),
        correctItems: numOrNull(editCorrect),
        incorrectItems: numOrNull(editIncorrect),
        score: numOrNull(editScore),
        maxScore: numOrNull(editMaxScore),
        outcome: editOutcome,
        note: editNote.trim() || null,
      });
      setEditingId(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  // ======== 删除 ========
  const handleDelete = async (id: number) => {
    if (!window.confirm("确定删除这条验证记录？删除后不可恢复。")) return;
    try {
      await deleteEvaluation(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  // ======== 视图：基本信息 / 结果 / 正确率 ========
  const typeLabel = (t: string) =>
    EVALUATION_TYPE_LABELS[t as EvaluationType] ?? t;
  const outcomeBadge = (o: string) => {
    if (o === "passed") return { cls: "badge--done", label: OUTCOME_LABELS.passed };
    if (o === "partial") return { cls: "badge--pending", label: OUTCOME_LABELS.partial };
    if (o === "failed") return { cls: "badge--active", label: OUTCOME_LABELS.failed };
    return { cls: "", label: OUTCOME_LABELS.unrated };
  };

  const countSummary = (e: Evaluation): string | null => {
    if (e.total_items == null && e.correct_items == null && e.incorrect_items == null) {
      return null;
    }
    const parts: string[] = [];
    if (e.total_items != null) parts.push(`${e.total_items} 题`);
    if (e.correct_items != null) parts.push(`${e.correct_items} 对`);
    if (e.incorrect_items != null) parts.push(`${e.incorrect_items} 错`);
    return parts.join(" / ");
  };

  const scoreSummary = (e: Evaluation): string | null => {
    if (e.score == null && e.max_score == null) return null;
    if (e.score == null) return null;
    if (e.max_score != null) return `${e.score} / ${e.max_score}`;
    return `${e.score}`;
  };

  const accuracyPercent = (e: Evaluation): string | null => {
    if (e.total_items == null || e.total_items <= 0) return null;
    if (e.correct_items == null) return null;
    const p = (e.correct_items / e.total_items) * 100;
    return `${p.toFixed(0)}%`;
  };

  // ======== 渲染 ========
  const optionsForItems = useMemo(
    () =>
      items.map((it) => ({
        id: it.id,
        label: pathMap[it.id] ?? it.name,
      })),
    [items, pathMap]
  );

  return (
    <div className="page">
      <h1 className="page__title">验证</h1>
      <p className="muted">
        记录一次学习验证（练习 / 测试 / 回忆 / 应用 / 其他）的事实与结果。
        此处不自动分析问题、不自动调整计划。
      </p>

      {error && <div className="alert alert--error">{error}</div>}

      {/* 顶栏：Goal 选择 + 视图切换 */}
      <section className="card">
        <div className="toolbar">
          <label className="field">
            <span className="field__label">目标</span>
            <select
              className="input"
              value={goalIdParam ?? ""}
              onChange={(e) => handleGoalChange(e.target.value)}
            >
              <option value="">（不筛选，跨 Goal 查看最近）</option>
              {goals.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </label>
          <div className="btn-row">
            <button
              className={"btn" + (viewMode === "recent" ? " btn--primary" : "")}
              onClick={() => setViewMode("recent")}
            >
              最近
            </button>
            <button
              className={"btn" + (viewMode === "goal" ? " btn--primary" : "")}
              onClick={() => goalId != null && setViewMode("goal")}
              disabled={goalId == null}
              title={goalId == null ? "请先选择目标" : ""}
            >
              当前 Goal
            </button>
          </div>
        </div>
      </section>

      {/* 创建表单（分组） */}
      <section className="card">
        <h2 className="card__title">记录新的验证</h2>

        <div className="form-group">
          <h3 className="form-group__title">基本信息</h3>
          <div className="form-row">
            <label className="field">
              <span className="field__label">目标 *</span>
              <select
                className="input"
                value={formGoalId}
                onChange={(e) => setFormGoalId(e.target.value === "" ? "" : Number(e.target.value))}
              >
                <option value="">请选择…</option>
                {goals.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="field__label">学习对象（可选）</span>
              <select
                className="input"
                value={formItemId}
                onChange={(e) => setFormItemId(e.target.value === "" ? "" : Number(e.target.value))}
                disabled={formGoalId === "" || items.length === 0}
              >
                <option value="">（不绑定）</option>
                {optionsForItems.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="form-row">
            <label className="field field--grow">
              <span className="field__label">标题 *</span>
              <input
                className="input"
                placeholder="例如：极限第一轮自测"
                value={formTitle}
                onChange={(e) => setFormTitle(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">类型 *</span>
              <select
                className="input"
                value={formType}
                onChange={(e) => setFormType(e.target.value as EvaluationType)}
              >
                {EVALUATION_TYPES.map((t: EvaluationType) => (
                  <option key={t} value={t}>
                    {EVALUATION_TYPE_LABELS[t]}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="field__label">来源（可选）</span>
              <input
                className="input"
                placeholder="王道 / 2025真题 / 自测 / LeetCode"
                value={formSource}
                onChange={(e) => setFormSource(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">发生时间</span>
              <input
                type="datetime-local"
                className="input"
                value={formOccurred}
                onChange={(e) => setFormOccurred(e.target.value)}
              />
            </label>
          </div>
        </div>

        <div className="form-group">
          <h3 className="form-group__title">结果数据（练习/测试填，回忆/应用可留空）</h3>
          <div className="form-row">
            <label className="field">
              <span className="field__label">总题数</span>
              <input
                type="number"
                min="0"
                className="input"
                placeholder="10"
                value={formTotal}
                onChange={(e) => setFormTotal(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">正确数</span>
              <input
                type="number"
                min="0"
                className="input"
                placeholder="7"
                value={formCorrect}
                onChange={(e) => setFormCorrect(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">错误数</span>
              <input
                type="number"
                min="0"
                className="input"
                placeholder="3"
                value={formIncorrect}
                onChange={(e) => setFormIncorrect(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">得分</span>
              <input
                type="number"
                min="0"
                step="0.1"
                className="input"
                placeholder="70"
                value={formScore}
                onChange={(e) => setFormScore(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">满分</span>
              <input
                type="number"
                min="1"
                step="0.1"
                className="input"
                placeholder="100"
                value={formMaxScore}
                onChange={(e) => setFormMaxScore(e.target.value)}
              />
            </label>
            <label className="field">
              <span className="field__label">结果</span>
              <select
                className="input"
                value={formOutcome}
                onChange={(e) => setFormOutcome(e.target.value as Outcome)}
              >
                {OUTCOMES.map((o: Outcome) => (
                  <option key={o} value={o}>
                    {OUTCOME_LABELS[o]}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>

        <div className="form-group">
          <h3 className="form-group__title">备注（可选）</h3>
          <textarea
            className="input textarea"
            placeholder="例如：函数极限部分仍然容易出错；多级反馈队列说不完整"
            rows={3}
            value={formNote}
            onChange={(e) => setFormNote(e.target.value)}
          />
        </div>

        <div className="btn-row">
          <button className="btn btn--primary" onClick={handleCreate}>
            保存验证
          </button>
        </div>
      </section>

      {/* 列表 */}
      <section className="card">
        <h2 className="card__title">
          {viewMode === "goal" ? "当前目标下的验证记录" : "最近的验证记录"}
        </h2>
        {loading ? (
          <div className="muted">加载中…</div>
        ) : evals.length === 0 ? (
          <div className="muted">还没有验证记录。</div>
        ) : (
          <ul className="list">
            {evals.map((e) => {
              const ob = outcomeBadge(e.outcome);
              return (
                <li key={e.id} className="list__item">
                  <div className="list__head">
                    <span className={"badge " + ob.cls}>{ob.label}</span>
                    <span className="badge">{typeLabel(e.evaluation_type)}</span>
                    <strong className="list__title">{e.title}</strong>
                    <span className="muted">{formatDateTime(e.occurred_at)}</span>
                  </div>
                  <div className="list__body">
                    {e.learning_item_id && (
                      <div className="muted">{pathMap[e.learning_item_id] ?? `#${e.learning_item_id}`}</div>
                    )}
                    {(countSummary(e) || scoreSummary(e) || accuracyPercent(e)) && (
                      <div className="meta">
                        {countSummary(e) && <span>{countSummary(e)}</span>}
                        {scoreSummary(e) && <span>{scoreSummary(e)}</span>}
                        {accuracyPercent(e) && <span>正确率 {accuracyPercent(e)}</span>}
                      </div>
                    )}
                    {e.source && <div className="muted">来源：{e.source}</div>}
                    {e.note && <div className="note">{e.note}</div>}
                  </div>
                  {editingId === e.id ? (
                    <div className="card card--inline">
                      <div className="form-row">
                        <label className="field field--grow">
                          <span className="field__label">标题 *</span>
                          <input className="input" value={editTitle} onChange={(ev) => setEditTitle(ev.target.value)} />
                        </label>
                        <label className="field">
                          <span className="field__label">类型</span>
                          <select className="input" value={editType} onChange={(ev) => setEditType(ev.target.value as EvaluationType)}>
                            {EVALUATION_TYPES.map((t: EvaluationType) => (
                              <option key={t} value={t}>{EVALUATION_TYPE_LABELS[t]}</option>
                            ))}
                          </select>
                        </label>
                      </div>
                      <div className="form-row">
                        <label className="field">
                          <span className="field__label">来源</span>
                          <input className="input" value={editSource} onChange={(ev) => setEditSource(ev.target.value)} />
                        </label>
                        <label className="field">
                          <span className="field__label">发生时间</span>
                          <input type="datetime-local" className="input" value={editOccurred} onChange={(ev) => setEditOccurred(ev.target.value)} />
                        </label>
                        <label className="field">
                          <span className="field__label">结果</span>
                          <select className="input" value={editOutcome} onChange={(ev) => setEditOutcome(ev.target.value as Outcome)}>
                            {OUTCOMES.map((o: Outcome) => (
                              <option key={o} value={o}>{OUTCOME_LABELS[o]}</option>
                            ))}
                          </select>
                        </label>
                      </div>
                      <div className="form-row">
                        <label className="field"><span className="field__label">总题数</span>
                          <input type="number" min="0" className="input" value={editTotal} onChange={(ev) => setEditTotal(ev.target.value)} />
                        </label>
                        <label className="field"><span className="field__label">正确</span>
                          <input type="number" min="0" className="input" value={editCorrect} onChange={(ev) => setEditCorrect(ev.target.value)} />
                        </label>
                        <label className="field"><span className="field__label">错误</span>
                          <input type="number" min="0" className="input" value={editIncorrect} onChange={(ev) => setEditIncorrect(ev.target.value)} />
                        </label>
                        <label className="field"><span className="field__label">得分</span>
                          <input type="number" min="0" step="0.1" className="input" value={editScore} onChange={(ev) => setEditScore(ev.target.value)} />
                        </label>
                        <label className="field"><span className="field__label">满分</span>
                          <input type="number" min="1" step="0.1" className="input" value={editMaxScore} onChange={(ev) => setEditMaxScore(ev.target.value)} />
                        </label>
                      </div>
                      <textarea className="input textarea" rows={2} placeholder="备注…" value={editNote} onChange={(ev) => setEditNote(ev.target.value)} />
                      <div className="btn-row">
                        <button className="btn btn--primary" onClick={handleSaveEdit}>保存</button>
                        <button className="btn" onClick={() => setEditingId(null)}>取消</button>
                      </div>
                    </div>
                  ) : (
                    <div className="btn-row">
                      <button className="btn" onClick={() => handleStartEdit(e)}>编辑</button>
                      <button className="btn btn--danger" onClick={() => handleDelete(e.id)}>删除</button>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}

export default Evaluations;
