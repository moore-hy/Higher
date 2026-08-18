import { useCallback, useEffect, useState } from "react";
import {
  activateGoalTarget,
  createGoalTarget,
  listActiveGoalTargets,
  listGoalTargets,
  listLegacyGoalCandidates,
  replaceGoalTarget,
} from "../api";
import type { GoalTarget, LegacyGoalCandidate } from "../types";
import { formatDateTime } from "../utils";

/**
 * DEV-0059 §27：Goal Target 手工操作面板（不加新一级导航；挂载于 Planning 顶部摘要）。
 *
 * - 考研 Profile：REACH（冲刺）/ SAFETY（保底）槽位；其他 Profile：通用 GoalTarget
 * - 每个槽位：当前 active / 编辑替换（版本+1，old→historical）/ 查看历史 / 查看来源
 * - 不存在 active：空态，绝不自动猜旧 Goal（legacy 候选只列出来，用户自己决定）
 */
export default function GoalTargetPanel({
  profileId,
  profileType,
  onChanged,
}: {
  profileId: number;
  profileType?: string | null;
  onChanged?: () => void;
}) {
  const postgraduate = profileType === "kaoyan";
  const [active, setActive] = useState<GoalTarget[]>([]);
  const [all, setAll] = useState<GoalTarget[]>([]);
  const [legacy, setLegacy] = useState<LegacyGoalCandidate[]>([]);
  const [error, setError] = useState("");
  const [modal, setModal] = useState<
    | { kind: "closed" }
    | { kind: "create"; scenarioType: string; role: string }
    | { kind: "edit"; target: GoalTarget }
    | { kind: "history" }
    | { kind: "sources"; target: GoalTarget }
  >({ kind: "closed" });

  const load = useCallback(async () => {
    const [a, allT, l] = await Promise.all([
      listActiveGoalTargets(profileId).catch(() => [] as GoalTarget[]),
      listGoalTargets(profileId).catch(() => [] as GoalTarget[]),
      listLegacyGoalCandidates(profileId).catch(() => [] as LegacyGoalCandidate[]),
    ]);
    setActive(a);
    setAll(allT);
    setLegacy(l);
  }, [profileId]);

  useEffect(() => {
    void load();
  }, [load]);

  const reach = active.find((t) => t.role === "reach");
  const safety = active.find((t) => t.role === "safety");
  const generic = active.filter((t) => !postgraduate || (t.role !== "reach" && t.role !== "safety"));
  const empty = active.length === 0;

  async function guard(e: unknown): Promise<boolean> {
    setError("");
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes("close_active") || msg.includes("conflict")) return true;
    setError(msg);
    return false;
  }

  function closeModal(andRefresh = true) {
    setModal({ kind: "closed" });
    if (andRefresh) {
      void load();
      onChanged?.();
    }
  }

  /** 槽位卡片（考研 reach/safety 或通用目标） */
  function SlotCard({ t, label }: { t: GoalTarget | undefined; label: string }) {
    return (
      <div className="pt-slot">
        <span className="pt-slot__label">{label}</span>
        {t ? (
          <>
            <b className="pt-slot__title">{t.title}</b>
            <span className="pt-slot__meta muted">
              {t.target_date ? `目标日期 ${t.target_date}` : "未设日期"}
              {t.version > 1 ? ` · v${t.version}` : ""}
            </span>
            <span className="pt-slot__actions">
              <button className="btn btn--small" onClick={() => setModal({ kind: "edit", target: t })}>
                编辑
              </button>
              <button
                className="btn btn--small"
                onClick={() => setModal({ kind: "sources", target: t })}
              >
                来源
              </button>
              <button className="btn btn--small" onClick={() => setModal({ kind: "history" })}>
                历史
              </button>
            </span>
          </>
        ) : (
          <>
            <span className="pt-slot__empty muted">未设置</span>
            <span className="pt-slot__actions">
              {postgraduate && (
                <button
                  className="btn btn--small"
                  onClick={() =>
                    setModal({
                      kind: "create",
                      scenarioType: "postgraduate",
                      role: label === "冲刺" ? "reach" : "safety",
                    })
                  }
                >
                  创建
                </button>
              )}
            </span>
          </>
        )}
      </div>
    );
  }

  return (
    <section className="card pt-panel">
      <div className="pt-panel__head">
        <span className="pt-panel__title">正式目标</span>
        {!postgraduate && (
          <button
            className="btn btn--small"
            onClick={() => setModal({ kind: "create", scenarioType: "general", role: "" })}
          >
            + 新建目标
          </button>
        )}
      </div>
      {error && <div className="alert alert--error">{error}</div>}

      {postgraduate ? (
        <div className="pt-slots">
          <SlotCard t={reach} label="冲刺 REACH" />
          <SlotCard t={safety} label="保底 SAFETY" />
        </div>
      ) : empty ? (
        <p className="muted pt-panel__empty">
          还没有正式目标。创建后 Higher 才会基于它规划学习。
          {legacy.length > 0 && " 检测到旧数据中的目标描述，可参考导入。"}
        </p>
      ) : (
        <div className="pt-slots">
          {generic.map((t) => (
            <SlotCard key={t.id} t={t} label={t.scenario_type === "postgraduate" ? "考研目标" : "通用目标"} />
          ))}
        </div>
      )}

      {legacy.length > 0 && (
        <div className="pt-legacy">
          <p className="pt-legacy__head muted">旧数据中的目标描述（仅参考，不自动激活）：</p>
          <ul className="pt-legacy__list">
            {legacy.map((c, i) => (
              <li key={i} className="pt-legacy__item">
                <span>
                  <b>{c.title}</b>
                  {c.target_date ? ` · ${c.target_date}` : ""}
                  {c.detail ? ` · ${c.detail.slice(0, 60)}` : ""}
                </span>
                <button
                  className="btn btn--small"
                  onClick={() =>
                    setModal({
                      kind: "create",
                      scenarioType: postgraduate ? "postgraduate" : "general",
                      role: postgraduate ? "reach" : "",
                    })
                  }
                  title="用该描述预填创建表单"
                >
                  据此创建
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {modal.kind === "create" && (
        <GoalTargetForm
          profileId={profileId}
          postgraduate={postgraduate}
          initialScenario={modal.scenarioType}
          initialRole={modal.role}
          onClose={() => closeModal(false)}
          onSaved={() => closeModal(true)}
        />
      )}
      {modal.kind === "edit" && (
        <GoalTargetForm
          profileId={profileId}
          postgraduate={postgraduate}
          target={modal.target}
          onClose={() => closeModal(false)}
          onSaved={() => closeModal(true)}
        />
      )}
      {modal.kind === "history" && (
        <HistoryModal
          all={all}
          onClose={() => closeModal(false)}
          onActivate={async (id) => {
            try {
              await activateGoalTarget(profileId, id);
              closeModal(true);
            } catch (e) {
              void guard(e);
            }
          }}
        />
      )}
      {modal.kind === "sources" && (
        <SourcesModal target={modal.target} onClose={() => closeModal(false)} />
      )}
    </section>
  );
}

/** 创建 / 编辑（替换）表单 Modal */
function GoalTargetForm({
  profileId,
  postgraduate,
  target,
  initialScenario,
  initialRole,
  onClose,
  onSaved,
}: {
  profileId: number;
  postgraduate: boolean;
  target?: GoalTarget;
  initialScenario?: string;
  initialRole?: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const isEdit = !!target;
  const [scenario, setScenario] = useState(target?.scenario_type ?? initialScenario ?? "general");
  const [role, setRole] = useState(target?.role ?? initialRole ?? "");
  const [title, setTitle] = useState(target?.title ?? "");
  const [targetDate, setTargetDate] = useState(target?.target_date ?? "");
  const [dataJson, setDataJson] = useState(() => {
    if (target?.data_json) return target.data_json;
    return "{}";
  });
  // DEV-0059.2 §6：考研 UI 直接提供字段（不从 data_json 手写）；从现有 data_json 回填
  const [pg, setPg] = useState(() => {
    const d = (() => {
      try {
        return target?.data_json ? (JSON.parse(target.data_json) as Record<string, unknown>) : {};
      } catch {
        return {};
      }
    })();
    return {
      institution_name: String(d.institution_name ?? ""),
      school_unit: String(d.school_unit ?? ""),
      program_name: String(d.program_name ?? ""),
      program_code: String(d.program_code ?? ""),
      exam_year: String(d.exam_year ?? ""),
      exam_subjects: Array.isArray(d.exam_subjects)
        ? (d.exam_subjects as unknown[]).join(", ")
        : String(d.exam_subjects ?? ""),
      degree_type: String(d.degree_type ?? ""),
      study_mode: String(d.study_mode ?? ""),
    };
  });
  const [showJson, setShowJson] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const setPgField = (k: keyof typeof pg, v: string) =>
    setPg((prev) => ({ ...prev, [k]: v }));

  async function save() {
    setError("");
    if (!title.trim()) {
      setError("请填写目标名称。");
      return;
    }
    const scenarioType = scenario === "postgraduate" ? "postgraduate" : "general";
    let data: Record<string, unknown>;
    if (scenarioType === "postgraduate") {
      // §6：字段 → data_json（UI 自动 serialize；不要求用户手写 JSON）
      if (!pg.institution_name.trim() || !pg.program_name.trim()) {
        setError("考研目标必须填写院校（institution_name）与专业名称（program_name）。");
        return;
      }
      data = {
        institution_name: pg.institution_name.trim(),
        school_unit: pg.school_unit.trim() || undefined,
        program_name: pg.program_name.trim(),
        program_code: pg.program_code.trim() || undefined,
        exam_year: pg.exam_year.trim() || undefined,
        exam_subjects: pg.exam_subjects
          .split(/[,，、]/)
          .map((s) => s.trim())
          .filter(Boolean),
        degree_type: pg.degree_type.trim() || undefined,
        study_mode: pg.study_mode.trim() || undefined,
      };
    } else {
      try {
        data = JSON.parse(dataJson || "{}") as Record<string, unknown>;
      } catch {
        setError("高级详情 data_json 不是合法 JSON。");
        return;
      }
    }
    setSaving(true);
    try {
      if (isEdit) {
        await replaceGoalTarget({
          profileId,
          id: target!.id,
          title: title.trim(),
          targetDate: targetDate || null,
          dataJson: JSON.stringify(data),
          provenanceJson: target!.provenance_json,
        });
      } else {
        await createGoalTarget({
          profileId,
          scenarioType,
          role: scenarioType === "postgraduate" ? (role || "reach") : "",
          title: title.trim(),
          targetDate: targetDate || null,
          dataJson: JSON.stringify(data),
          provenanceJson: JSON.stringify({ source: "user_manual", created_via: "goal_target_form" }),
          status: "active",
        });
      }
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal__title">{isEdit ? "编辑 / 替换正式目标" : "新建正式目标"}</h3>
        {isEdit && (
          <p className="modal__context muted">
            {scenarioTypeOf(target?.scenario_type) === "postgraduate"
              ? "替换后旧版本进入历史，当前槽位（REACH/SAFETY）升级为新版本；角色不可在编辑中切换，如需换槽位请走新建流程。"
              : "替换后旧版本进入历史，当前槽位升级为新版本。"}
          </p>
        )}
        {error && <div className="modal__error">{error}</div>}
        <div className="modal__field">
          <label className="form-label">场景</label>
          <select
            className="modal__input"
            value={scenario}
            disabled={isEdit}
            onChange={(e) => setScenario(e.target.value)}
          >
            <option value="general">通用</option>
            <option value="postgraduate">考研（冲刺/保底）</option>
          </select>
        </div>
        {scenario === "postgraduate" && !isEdit && (
          <div className="modal__field">
            <label className="form-label">角色</label>
            <select className="modal__input" value={role} onChange={(e) => setRole(e.target.value)}>
              <option value="reach">冲刺 REACH</option>
              <option value="safety">保底 SAFETY</option>
            </select>
          </div>
        )}
        <div className="modal__field">
          <label className="form-label">目标名称</label>
          <input
            className="modal__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={scenario === "postgraduate" ? "如：华中科技大学 软件工程" : "如：通过 PMP 认证"}
          />
        </div>
        <div className="modal__field">
          <label className="form-label">目标日期（可选，官方未确定时可留空）</label>
          <input
            className="modal__input"
            type="date"
            value={targetDate}
            onChange={(e) => setTargetDate(e.target.value)}
          />
        </div>

        {scenario === "postgraduate" ? (
          <>
            {/* DEV-0059.2 §6：考研字段表单（不要求手写 JSON） */}
            <div className="modal__field">
              <label className="form-label">院校（institution_name）*</label>
              <input
                className="modal__input"
                value={pg.institution_name}
                onChange={(e) => setPgField("institution_name", e.target.value)}
                placeholder="如：华中科技大学"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">学院 / 培养单位（school_unit）</label>
              <input
                className="modal__input"
                value={pg.school_unit}
                onChange={(e) => setPgField("school_unit", e.target.value)}
                placeholder="如：计算机科学与技术学院"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">专业名称（program_name）*</label>
              <input
                className="modal__input"
                value={pg.program_name}
                onChange={(e) => setPgField("program_name", e.target.value)}
                placeholder="如：软件工程"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">专业代码（program_code）</label>
              <input
                className="modal__input"
                value={pg.program_code}
                onChange={(e) => setPgField("program_code", e.target.value)}
                placeholder="如：085405"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">考试年份（exam_year）</label>
              <input
                className="modal__input"
                value={pg.exam_year}
                onChange={(e) => setPgField("exam_year", e.target.value)}
                placeholder="如：2027"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">考试科目（exam_subjects，逗号分隔）</label>
              <input
                className="modal__input"
                value={pg.exam_subjects}
                onChange={(e) => setPgField("exam_subjects", e.target.value)}
                placeholder="如：101 思想政治理论, 201 英语一, 301 数学一, 408 计算机学科专业基础"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">学位类型（degree_type）</label>
              <input
                className="modal__input"
                value={pg.degree_type}
                onChange={(e) => setPgField("degree_type", e.target.value)}
                placeholder="如：专业学位（专硕）/ 学术学位（学硕）"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">学习方式（study_mode）</label>
              <input
                className="modal__input"
                value={pg.study_mode}
                onChange={(e) => setPgField("study_mode", e.target.value)}
                placeholder="如：全日制 / 非全日制"
              />
            </div>
          </>
        ) : (
          <>
            {/* §6：generic 不强迫 JSON；高级详情默认折叠 */}
            <div className="modal__field">
              <button
                className="btn btn--small"
                onClick={() => setShowJson((s) => !s)}
              >
                {showJson ? "收起高级详情 JSON" : "高级详情 JSON（可选）"}
              </button>
            </div>
            {showJson && (
              <div className="modal__field">
                <label className="form-label">详情 data_json（JSON）</label>
                <textarea
                  className="modal__input pt-form-json"
                  rows={5}
                  value={dataJson}
                  onChange={(e) => setDataJson(e.target.value)}
                  placeholder='{"note": "可选高级字段"}'
                />
              </div>
            )}
          </>
        )}
        <div className="modal__actions">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn btn--primary" disabled={saving} onClick={() => void save()}>
            {saving ? "保存中…" : isEdit ? "保存（新版本）" : "创建并激活"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** §6：GoalTarget scenario_type → 界面场景标签（兼容 'general' / 'generic' 历史值） */
function scenarioTypeOf(scenario: string | undefined): string {
  const s = scenario ?? "";
  return s === "postgraduate" ? "postgraduate" : "general";
}

/** 历史版本列表 */
function HistoryModal({
  all,
  onClose,
  onActivate,
}: {
  all: GoalTarget[];
  onClose: () => void;
  onActivate: (id: number) => void;
}) {
  const statusLabel: Record<string, string> = {
    active: "当前",
    historical: "历史",
    draft: "草稿",
    candidate: "候选",
    dismissed: "已放弃",
  };
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal__title">目标历史</h3>
        {all.length === 0 ? (
          <p className="muted">暂无历史。</p>
        ) : (
          <ul className="pt-hist__list">
            {all.map((t) => (
              <li key={t.id} className="pt-hist__item">
                <div>
                  <b>{t.title}</b>
                  <span className="pt-hist__tag">
                    {statusLabel[t.status] ?? t.status}
                    {t.version > 1 ? ` v${t.version}` : ""}
                  </span>
                </div>
                <span className="muted" style={{ fontSize: 11 }}>
                  {t.target_date ? `目标 ${t.target_date} · ` : ""}
                  {formatDateTime(t.updated_at)}
                </span>
                {t.status !== "active" && (
                  <button className="btn btn--small" onClick={() => onActivate(t.id)}>
                    激活
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="modal__actions">
          <button className="btn" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

/** 来源 / provenance 查看 */
function SourcesModal({ target, onClose }: { target: GoalTarget; onClose: () => void }) {
  let prov: unknown = null;
  try {
    prov = JSON.parse(target.provenance_json || "null");
  } catch {
    prov = target.provenance_json;
  }
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="modal__title">目标来源</h3>
        <p>
          <b>{target.title}</b> <span className="muted">v{target.version}</span>
        </p>
        <pre className="pt-src-pre">{JSON.stringify(prov, null, 2)}</pre>
        <div className="modal__actions">
          <button className="btn" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
