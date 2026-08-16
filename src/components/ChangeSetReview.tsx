import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  applyAiChangeSet,
  getAiChangeSet,
  getChangeSetApplySummary,
  listAiChangeSetOperations,
  rejectAiChangeSet,
  setAiChangeOpSelected,
  undoAiChangeSet,
} from "../api";
import { formatDateTime } from "../utils";
import type { ChangeOperation, ChangeSet } from "../types";

/**
 * AI ChangeSet 审阅（DEV-0052 §128-133 / DEV-0053 §106-109）：
 * - 头部：title + summary + 按 entity_type×action 聚合的统计行
 * - 复杂规划（operations>8 或 goal+knowledge 混合）分组渲染：
 *   目标结构（goal）/ 知识结构（knowledge·document）/ 今日与近期任务（task）/ 其他；
 *   分组统计如 `目标 +15 · 知识节点 +42 · 任务 +38`
 * - 每条 operation：checkbox（set_ai_change_op_selected）+ 实体中文标签 + action 徽标
 * - Field Diff（§130）：update 逐字段对比 `- old` / `+ new`；create 全绿；delete 全红
 * - 实体行可点击（§133）：deep_link → 简化 hash 跳转（higher://task/123 → "/?task=123"）
 * - 底部（§109）：应用全部 / 选择性应用 / 继续调整 / 拒绝；applied 后可撤销
 * - Apply 成功（DEV-0053 §11）：调 get_change_set_apply_summary 展示真实结果行，
 *   并经 onApplied(lines) 让 Panel 把每行插入当前会话（system 消息）
 */
export default function ChangeSetReview({
  profileId,
  changeSetId,
  onClose,
  onApplied,
}: {
  profileId: number;
  changeSetId: number;
  onClose: () => void;
  onApplied?: (lines?: string[]) => void;
}) {
  const navigate = useNavigate();
  const [cs, setCs] = useState<ChangeSet | null>(null);
  const [ops, setOps] = useState<ChangeOperation[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [doneMsg, setDoneMsg] = useState("");
  /** §11：Apply 成功后的真实结果行（✓ 已创建任务「…」） */
  const [applyLines, setApplyLines] = useState<string[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [c, o] = await Promise.all([
        getAiChangeSet(profileId, changeSetId),
        listAiChangeSetOperations(profileId, changeSetId),
      ]);
      setCs(c);
      setOps(o);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [profileId, changeSetId]);

  useEffect(() => {
    void load();
  }, [load]);

  /** 统计行：按 entity_type 聚合 create/update/delete 数 */
  const statLine = useMemo(() => {
    const byEntity = new Map<string, { create: number; update: number; delete: number }>();
    for (const op of ops) {
      const s = byEntity.get(op.entity_type) ?? { create: 0, update: 0, delete: 0 };
      if (op.action === "create") s.create++;
      else if (op.action === "update") s.update++;
      else if (op.action === "delete") s.delete++;
      byEntity.set(op.entity_type, s);
    }
    return [...byEntity.entries()].map(([et, s]) => ({
      label: ENTITY_LABELS[et] ?? et,
      text: [
        s.create > 0 ? `+${s.create}` : "",
        s.update > 0 ? `~${s.update}` : "",
        s.delete > 0 ? `-${s.delete}` : "",
      ]
        .filter(Boolean)
        .join(" "),
    }));
  }, [ops]);

  /** §106：复杂规划分组（operations>8 或 goal+knowledge 混合） */
  const opGroups = useMemo(() => {
    const hasGoal = ops.some((o) => o.entity_type === "goal");
    const hasKnowledge = ops.some(
      (o) => o.entity_type === "knowledge" || o.entity_type === "document"
    );
    const complex = ops.length > 8 || (hasGoal && hasKnowledge);
    if (!complex) return null;
    const groups: { key: string; label: string; statName: string; ops: ChangeOperation[] }[] = [
      { key: "goal", label: "目标结构", statName: "目标", ops: [] },
      { key: "knowledge", label: "知识结构", statName: "知识节点", ops: [] },
      { key: "task", label: "今日与近期任务", statName: "任务", ops: [] },
      { key: "other", label: "其他修改", statName: "其他", ops: [] },
    ];
    for (const op of ops) {
      if (op.entity_type === "goal") groups[0].ops.push(op);
      else if (op.entity_type === "knowledge" || op.entity_type === "document")
        groups[1].ops.push(op);
      else if (op.entity_type === "task") groups[2].ops.push(op);
      else groups[3].ops.push(op);
    }
    return groups.filter((g) => g.ops.length > 0);
  }, [ops]);

  async function toggleOp(op: ChangeOperation, selected: boolean) {
    setOps((prev) => prev.map((o) => (o.id === op.id ? { ...o, selected } : o)));
    try {
      await setAiChangeOpSelected(profileId, changeSetId, op.id, selected);
    } catch (e) {
      setError(String(e));
      void load();
    }
  }

  async function run(kind: "apply-selected" | "apply-all" | "reject" | "undo") {
    setBusy(true);
    setError("");
    setDoneMsg("");
    try {
      if (kind === "apply-selected") {
        await applyAiChangeSet(profileId, changeSetId, true);
        setDoneMsg("已应用选中的修改（本次操作已记录到保险箱）");
        await emitApplySummary();
      } else if (kind === "apply-all") {
        await applyAiChangeSet(profileId, changeSetId, false);
        setDoneMsg("已应用全部修改（本次操作已记录到保险箱）");
        await emitApplySummary();
      } else if (kind === "reject") {
        await rejectAiChangeSet(profileId, changeSetId);
        setDoneMsg("已拒绝本次全部修改");
      } else {
        await undoAiChangeSet(profileId, changeSetId);
        setDoneMsg("已撤销本次修改");
      }
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §11：拉取真实后端结果行 → 本组件展示 + 通知 Panel 插入会话 */
  async function emitApplySummary() {
    let lines: string[] = [];
    try {
      lines = await getChangeSetApplySummary(profileId, changeSetId);
    } catch {
      lines = [];
    }
    setApplyLines(lines);
    onApplied?.(lines);
  }

  const status = cs?.status ?? "";
  const isApplied = status === "applied" || status === "undone_applied";
  const isSettled = status !== "pending";

  const opRow = (op: ChangeOperation) => (
    <div key={op.id} className="csr__op">
      <label className="csr__op-check">
        <input
          type="checkbox"
          checked={op.selected}
          disabled={busy || isSettled}
          onChange={(e) => void toggleOp(op, e.target.checked)}
        />
      </label>
      <div className="csr__op-main">
        <button
          className="csr__op-entity"
          title={op.deep_link || ""}
          onClick={() => openDeepLink(op.deep_link, navigate)}
        >
          <span className={"csr__badge csr__badge--" + op.action}>
            {ACTION_LABELS[op.action] ?? op.action}
          </span>
          <span className="csr__badge csr__badge--entity">
            {ENTITY_LABELS[op.entity_type] ?? op.entity_type}
          </span>
          <span className="csr__op-name">{entityTitle(op)}</span>
        </button>
        {op.reason && <p className="muted csr__op-reason">{op.reason}</p>}
        <FieldDiff op={op} />
      </div>
    </div>
  );

  return (
    <div className="modal-overlay" onClick={() => !busy && onClose()}>
      <div className="modal modal--wide csr" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title csr__title">🧩 AI 修改提案</div>

        {loading && <p className="muted">加载修改内容……</p>}
        {error && <div className="alert alert--error">{error}</div>}
        {doneMsg && <div className="alert alert--ok">{doneMsg}</div>}

        {/* §11：Apply 成功的真实结果行（由后端结果生成，模型自己不能生成） */}
        {applyLines.length > 0 && (
          <ul className="csr__applied">
            {applyLines.map((line, i) => (
              <li key={i}>{line}</li>
            ))}
          </ul>
        )}

        {!loading && cs && (
          <>
            <div className="csr__head">
              <div className="csr__head-title">{cs.title}</div>
              {cs.summary && <p className="muted csr__summary">{cs.summary}</p>}
              <div className="csr__stats">
                {statLine.length === 0 ? (
                  <span className="muted">没有可显示的修改</span>
                ) : (
                  statLine.map((s) => (
                    <span key={s.label} className="csr__stat">
                      {s.label} <b>{s.text}</b>
                    </span>
                  ))
                )}
                <span className={"csr__status csr__status--" + (isApplied ? "applied" : status)}>
                  {STATUS_LABELS[status] ?? status}
                </span>
              </div>
              <p className="muted csr__meta">
                创建于 {formatDateTime(cs.created_at)}
                {cs.applied_at ? ` · 应用于 ${formatDateTime(cs.applied_at)}` : ""}
              </p>
            </div>

            {ops.length === 0 && <p className="muted">这个提案没有任何操作。</p>}

            {/* §106-108：复杂规划分组渲染；简单提案保持平铺 */}
            {opGroups ? (
              <div className="csr__list csr__list--grouped">
                {opGroups.map((g) => {
                  const created = g.ops.filter((o) => o.action === "create").length;
                  const updated = g.ops.filter((o) => o.action === "update").length;
                  const removed = g.ops.filter((o) => o.action === "delete").length;
                  return (
                    <div key={g.key} className="csr__group">
                      <div className="csr__group-head">
                        <span className="csr__group-title">{g.label}</span>
                        <span className="csr__group-stat">
                          {g.statName}{" "}
                          {[
                            created > 0 ? `+${created}` : "",
                            updated > 0 ? `~${updated}` : "",
                            removed > 0 ? `-${removed}` : "",
                          ]
                            .filter(Boolean)
                            .join(" ")}
                        </span>
                      </div>
                      {g.ops.map(opRow)}
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="csr__list">{ops.map(opRow)}</div>
            )}

            {/* §109：最终操作 */}
            <div className="csr__actions">
              {isApplied ? (
                <button
                  className="btn btn--small"
                  disabled={busy}
                  onClick={() => void run("undo")}
                >
                  {busy ? "处理中…" : "撤销本次修改"}
                </button>
              ) : (
                <>
                  <button
                    className="btn btn--small btn--primary"
                    disabled={busy || ops.length === 0}
                    onClick={() => void run("apply-all")}
                  >
                    应用全部
                  </button>
                  <button
                    className="btn btn--small btn--primary"
                    disabled={busy || !ops.some((o) => o.selected)}
                    onClick={() => void run("apply-selected")}
                  >
                    选择性应用
                  </button>
                  <button className="btn btn--small" disabled={busy} onClick={onClose}>
                    继续调整
                  </button>
                  <button
                    className="btn btn--small"
                    disabled={busy || ops.length === 0}
                    onClick={() => void run("reject")}
                  >
                    拒绝
                  </button>
                </>
              )}
            </div>
            <p className="muted csr__note">
              应用前会自动创建数据库快照；所有 AI 写入都会记录到保险箱审计日志。
            </p>
          </>
        )}
      </div>
    </div>
  );
}

// ---------------- Field Diff（§130） ----------------

function FieldDiff({ op }: { op: ChangeOperation }) {
  const rows = diffRows(op);
  if (rows.length === 0) return null;
  return (
    <div className="csr__diff">
      {rows.map((r, i) => (
        <div key={i} className={"csr__diff-row csr__diff-row--" + r.kind}>
          <span className="csr__diff-field">{r.field}</span>
          {r.kind !== "add" && <span className="csr__diff-old">- {r.old}</span>}
          {r.kind !== "del" && <span className="csr__diff-new">+ {r.new}</span>}
        </div>
      ))}
    </div>
  );
}

type DiffRow = { kind: "add" | "del" | "mod"; field: string; old: string; new: string };

function diffRows(op: ChangeOperation): DiffRow[] {
  const before = op.before_json ?? {};
  const after = op.after_json ?? {};
  const keys = new Set([...Object.keys(before), ...Object.keys(after)]);
  const rows: DiffRow[] = [];
  for (const k of keys) {
    const oldV = before[k];
    const newV = after[k];
    const oldS = prettyVal(oldV);
    const newS = prettyVal(newV);
    if (oldS === newS) continue;
    if (oldV === undefined || oldV === null) {
      rows.push({ kind: "add", field: k, old: oldS, new: newS });
    } else if (newV === undefined || newV === null) {
      rows.push({ kind: "del", field: k, old: oldS, new: newS });
    } else {
      rows.push({ kind: "mod", field: k, old: oldS, new: newS });
    }
  }
  return rows;
}

function prettyVal(v: unknown): string {
  if (v === null || v === undefined) return "（空）";
  if (typeof v === "string") return v.length > 160 ? v.slice(0, 160) + "…" : v;
  try {
    const s = JSON.stringify(v);
    return s.length > 160 ? s.slice(0, 160) + "…" : s;
  } catch {
    return String(v);
  }
}

/** 实体行标题：优先 name/title 字段，否则 #id。 */
function entityTitle(op: ChangeOperation): string {
  const src = op.action === "delete" ? op.before_json : op.after_json;
  const name =
    (src && typeof src === "object" && (src as Record<string, unknown>)["name"]) ||
    (src && typeof src === "object" && (src as Record<string, unknown>)["title"]) ||
    null;
  const text = typeof name === "string" && name.trim() ? name : "";
  return text || (op.entity_id != null ? `#${op.entity_id}` : "新实体");
}

// ---------------- deep link 简化跳转（§133） ----------------

/** higher://task/123 → "#/?task=123"（简化实现：目标页面已有 query 处理的不强求）。 */
export function openDeepLink(deepLink: string, navigate: (to: string) => void) {
  const m = /^higher:\/\/([a-z_]+)(?:\/(\d+))?/i.exec(deepLink.trim());
  if (!m) return;
  const [, entity, id] = m;
  const routes: Record<string, string> = {
    task: "/",
    goal: "/knowledge",
    knowledge: "/knowledge",
    document: "/knowledge",
    session: "/planning",
    evaluation: "/planning",
    personalization: "/settings",
  };
  const base = routes[entity] ?? "/";
  navigate(id ? `${base}${base.includes("?") ? "&" : "?"}${entity}=${id}` : base);
}

// ---------------- 标签映射 ----------------

const ENTITY_LABELS: Record<string, string> = {
  goal: "目标",
  task: "任务",
  knowledge: "知识",
  document: "文档",
  session: "学习记录",
  evaluation: "验证",
  personalization: "私人档案",
};

const ACTION_LABELS: Record<string, string> = {
  create: "新增",
  update: "修改",
  delete: "删除",
  status_change: "状态",
};

const STATUS_LABELS: Record<string, string> = {
  pending: "待审阅",
  applied: "已应用",
  rejected: "已拒绝",
  undone: "已撤销",
  undone_applied: "已撤销",
};
