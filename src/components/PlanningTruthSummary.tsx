import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  activatePlanningBlueprint,
  addPlanningMilestone,
  addPlanningPhase,
  createPlanningBlueprint,
  deletePlanningMilestone,
  deletePlanningPhase,
  getActivePlanningBlueprint,
  getPlanningReviewRisk,
  importPlanningSource,
  isPlanningReviewDue,
  listPlanningBlueprints,
  listPlanningMilestones,
  listPlanningPhases,
  listPlanningReviews,
  listPlanningSources,
  listTasksByRangeByProfile,
  listActiveGoalTargets,
  prepareCurrentPlanningReview,
  runPlanningReviewAi,
  updatePlanningBlueprintMeta,
  updatePlanningMilestone,
  updatePlanningPhase,
  updatePlanningReviewCadence,
  writeExportFile,
} from "../api";
import ChangeSetReview from "./ChangeSetReview";
import GoalTargetPanel from "./GoalTargetPanel";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { useAiPanel } from "./ai/AiPanelContext";
import type {
  GoalTarget,
  PlanningBlueprint,
  PlanningMilestone,
  PlanningPhase,
  PlanningReview,
  PlanningSource,
} from "../types";
import { addDaysISO, formatDateTime, todayDate } from "../utils";

/**
 * DEV-0059 §27-28：Planning 顶部「正式目标与规划」区。
 * - §27：GoalTargetPanel（正式目标槽位：active/编辑/替换/历史/来源/空态/legacy 候选）
 * - §28：Active Blueprint 摘要 + 规划资料计数 + 操作区（导入规划资料 / 开始复盘 / 导出）
 * 数据只读展示为主；真实操作入口保持克制（不堆按钮）。
 */
export default function PlanningTruthSummary({
  profileId,
  profileType,
  onChanged,
}: {
  profileId: number;
  profileType?: string | null;
  onChanged?: () => void;
}) {
  const [blueprint, setBlueprint] = useState<PlanningBlueprint | null>(null);
  const [sources, setSources] = useState<PlanningSource[]>([]);
  const [reviews, setReviews] = useState<PlanningReview[]>([]);
  const [due, setDue] = useState(false);
  const [risk, setRisk] = useState("unknown");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [error, setError] = useState("");
  const { sendChat, setOpen: setAiOpen, setPageContext } = useAiPanel();
  const { triggerRefresh } = useActiveProfile();
  // DEV-0059.1 §2：参与本次审查的规划资料（默认选所有 ready/imported）
  const [selectedSourceIds, setSelectedSourceIds] = useState<number[]>([]);
  // DEV-0059.1 §3：复盘 AI 全链状态（准备后待用户确认才调 Provider）
  const [preparedReviewId, setPreparedReviewId] = useState<number | null>(null);
  const [snapshot, setSnapshot] = useState<Record<string, unknown> | null>(null);
  const [aiBusy, setAiBusy] = useState(false);
  // DEV-0059.2 §1：从 Planning 页直接审阅 Review 生成的 ChangeSet（复用现有 ChangeSetReview）
  const [reviewChangeSetId, setReviewChangeSetId] = useState<number | null>(null);
  // DEV-0059.1 §10/§11/§12：手工维护规划 + 复盘节奏 + 近期计划提示
  const [drafts, setDrafts] = useState<PlanningBlueprint[]>([]);
  const [nearTasks, setNearTasks] = useState<number>(0);
  const [manualOpen, setManualOpen] = useState(false);
  const [bpTitle, setBpTitle] = useState("");
  const [bpContent, setBpContent] = useState("");
  const [cadenceCustom, setCadenceCustom] = useState("14");
  const [phases, setPhases] = useState<PlanningPhase[]>([]);
  const [milestones, setMilestones] = useState<PlanningMilestone[]>([]);
  const [newPhase, setNewPhase] = useState({ title: "", start: "", end: "", objective: "" });
  const [newMs, setNewMs] = useState({ title: "", start: "", end: "", precision: "day", status: "estimated" });
  // DEV-0059.2 §10：无 AI 创建第一份 Blueprint（手工新建）
  const [createBpOpen, setCreateBpOpen] = useState(false);
  const [scenarioDefault, setScenarioDefault] = useState("generic");
  const [newBp, setNewBp] = useState({ title: "", content: "", interval: "14", scenario: "generic" });

  const load = useCallback(async () => {
    const [b, s, r, d, rk, bs, near, gts] = await Promise.all([
      getActivePlanningBlueprint(profileId).catch(() => null),
      listPlanningSources(profileId).catch(() => [] as PlanningSource[]),
      listPlanningReviews(profileId).catch(() => [] as PlanningReview[]),
      isPlanningReviewDue(profileId).catch(() => false),
      getPlanningReviewRisk(profileId).catch(() => "unknown"),
      listPlanningBlueprints(profileId).catch(() => [] as PlanningBlueprint[]),
      // DEV-0059.1 §12：未来 7 天未完成的计划任务（滚动提示，不生成）
      listTasksByRangeByProfile(profileId, todayDate(), addDaysISO(todayDate(), 7)).catch(
        () => [] as import("../types").Task[]
      ),
      // DEV-0059.2 §10：手工新建 Blueprint 的场景建议（postgraduate REACH 主目标 → postgraduate）
      listActiveGoalTargets(profileId).catch(() => [] as GoalTarget[]),
    ]);
    setBlueprint(b);
    setSources(s);
    setReviews(r);
    setDue(d);
    setRisk(rk);
    setDrafts(bs.filter((x) => x.status === "draft"));
    setNearTasks(
      near.filter((t) => t.origin === "blueprint" && t.status !== "completed").length
    );
    const pgReach = gts.find(
      (t) => t.scenario_type === "postgraduate" && t.role === "reach" && t.status === "active"
    );
    setScenarioDefault(pgReach ? "postgraduate" : "generic");
    // 默认选择所有 ready/imported source
    setSelectedSourceIds((prev) => {
      const ids = s
        .filter((x) => x.status === "ready" || x.status === "imported")
        .map((x) => x.id);
      return prev.length === 0 ? ids : prev;
    });
  }, [profileId]);

  useEffect(() => {
    void load();
  }, [load]);

  const latest = reviews[0];

  /**
   * §31/DEV-0059.1 §13：导入规划资料。
   * kind=user_file → 普通外部文件；kind=export_reimport → 重新导入 Higher 导出文件。
   */
  async function importSources(kind: "user_file" | "export_reimport") {
    setBusy(true);
    setMsg("");
    setError("");
    try {
      const picked = await openDialog({
        multiple: true,
        filters: [
          {
            name: "规划资料（txt / md / docx / pdf / xlsx）",
            extensions: ["txt", "md", "docx", "pdf", "xlsx"],
          },
        ],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked.map(String) : [String(picked)];
      const created: string[] = [];
      for (const p of paths) {
        const r = await importPlanningSource(profileId, p, kind);
        created.push(`${r.name}（${r.chars}字）`);
      }
      setMsg(
        kind === "export_reimport"
          ? `已重新导入 ${created.length} 份 Higher 导出文件（标记 export_reimport）：${created.join("、")}`
          : `已导入 ${created.length} 份规划资料：${created.join("、")}`
      );
      await load();
      onChanged?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /**
   * §2：开始复盘——只走后端正式路径（cadence 周期 + open review dedupe；不调 AI）。
   * waiting_approval → 复用原 review，展示「审阅 AI 调整」。
   */
  async function startReview() {
    setBusy(true);
    setError("");
    setMsg("");
    try {
      const r = await prepareCurrentPlanningReview(profileId, "manual");
      if (r.status === "waiting_approval") {
        setPreparedReviewId(null);
        setSnapshot(null);
        setMsg("该复盘已有 AI 调整提案待审阅，请点击「审阅 AI 调整」。");
      } else {
        setPreparedReviewId(r.review_id);
        setSnapshot(r.snapshot as Record<string, unknown> | null);
        setMsg("已准备本期复盘（周期按当前复盘节奏计算）。确认后启动 AI 评估（仅在你确认后才调用）。");
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §3：用户确认后才调用 Provider；NO_CHANGE→completed；ADJUSTMENT_PROPOSAL→ChangeSet waiting_approval */
  async function confirmAndRunAi() {
    if (preparedReviewId == null) return;
    if (!window.confirm("将调用 AI（DeepSeek）基于本期真实证据评估学习执行情况。是否继续？")) return;
    setAiBusy(true);
    setError("");
    setMsg("");
    try {
      const outcome = await runPlanningReviewAi(profileId, preparedReviewId);
      setPreparedReviewId(null);
      setSnapshot(null);
      if (outcome === "completed") {
        setMsg("复盘完成：本期无需调整，已更新下次复盘时间（正式数据未变化）。");
      } else if (outcome === "waiting_approval") {
        setMsg("AI 建议调整蓝图（vN+1）。已生成修改提案，请到 AI 面板审阅并应用；应用后复盘自动完成。");
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      await load();
    } finally {
      setAiBusy(false);
    }
  }

  /**
   * §31-35：导出（docx/exceljs 点击时才 dynamic import；§31.3 只写用户所选路径）。
   * kind: profile-docx / profile-xlsx / plan-docx / plan-xlsx
   */
  async function runExport(
    kind: "profile-docx" | "profile-xlsx" | "plan-docx" | "plan-xlsx"
  ) {
    setBusy(true);
    setError("");
    setMsg("");
    try {
      const mod = await import("../lib/exporters");
      const gen =
        kind === "profile-docx"
          ? mod.exportPersonalProfileDocx
          : kind === "profile-xlsx"
            ? mod.exportPersonalProfileXlsx
            : kind === "plan-docx"
              ? mod.exportBlueprintDocx
              : mod.exportBlueprintXlsx;
      const { bytes, fileName } = await gen(profileId);
      const isXlsx = kind.endsWith("xlsx");
      const path = await saveDialog({
        defaultPath: fileName,
        filters: [
          {
            name: isXlsx ? "Excel 工作簿" : "Word 文档",
            extensions: [isXlsx ? "xlsx" : "docx"],
          },
        ],
      });
      if (!path) return;
      await writeExportFile(path, mod.base64FromBytes(bytes));
      setMsg(`已导出：${path}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §10/§11/§12：手工维护规划——加载蓝图编辑表单与阶段/里程碑 */
  async function openManual() {
    if (!blueprint) return;
    setBpTitle(blueprint.title);
    setBpContent(blueprint.content_md ?? "");
    setCadenceCustom(String(blueprint.review_interval_days ?? 14));
    const [ps, ms] = await Promise.all([
      listPlanningPhases(blueprint.id).catch(() => [] as PlanningPhase[]),
      listPlanningMilestones(blueprint.id).catch(() => [] as PlanningMilestone[]),
    ]);
    setPhases(ps);
    setMilestones(ms);
    setManualOpen(true);
  }

  /** §10：保存蓝图 title + content（summary 写在 content 首段） */
  async function saveBpMeta() {
    if (!blueprint) return;
    setBusy(true);
    setError("");
    try {
      await updatePlanningBlueprintMeta(profileId, blueprint.id, bpTitle.trim(), bpContent);
      setMsg("蓝图已保存。");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §11：复盘节奏（7/14/30/自定义 N/关闭）；只改 cadence 三字段，不调 AI */
  async function setCadence(enabled: boolean, days: number) {
    if (!blueprint) return;
    setBusy(true);
    setError("");
    try {
      await updatePlanningReviewCadence(profileId, blueprint.id, enabled, enabled ? days : null);
      setMsg(enabled ? `已设置每 ${days} 天复盘提醒。` : "已关闭复盘提醒。");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §10：Phase 增删改 */
  async function addPhase() {
    if (!blueprint || !newPhase.title.trim()) return;
    try {
      await addPlanningPhase({
        blueprintId: blueprint.id,
        phaseKey: newPhase.title.trim(),
        title: newPhase.title.trim(),
        startDate: newPhase.start || null,
        endDate: newPhase.end || null,
        objectiveMd: newPhase.objective,
        sortOrder: phases.length,
      });
      setNewPhase({ title: "", start: "", end: "", objective: "" });
      setPhases(await listPlanningPhases(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  async function savePhase(p: PlanningPhase) {
    try {
      await updatePlanningPhase({
        blueprintId: p.blueprint_id,
        phaseId: p.id,
        title: p.title,
        startDate: p.start_date,
        endDate: p.end_date,
        objectiveMd: p.objective_md,
        sortOrder: p.sort_order,
      });
      setMsg("阶段已保存。");
      if (blueprint) setPhases(await listPlanningPhases(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  async function delPhase(p: PlanningPhase) {
    if (!window.confirm(`删除阶段「${p.title}」？`)) return;
    try {
      await deletePlanningPhase(p.blueprint_id, p.id);
      if (blueprint) setPhases(await listPlanningPhases(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  /** §10：Milestone 增删改 */
  async function addMilestone() {
    if (!blueprint || !newMs.title.trim()) return;
    try {
      await addPlanningMilestone({
        blueprintId: blueprint.id,
        phaseId: null,
        milestoneKey: newMs.title.trim(),
        title: newMs.title.trim(),
        startDate: newMs.start || null,
        endDate: newMs.end || null,
        datePrecision: newMs.precision,
        dateStatus: newMs.status,
        provenanceJson: JSON.stringify({ source: "manual" }),
      });
      setNewMs({ title: "", start: "", end: "", precision: "day", status: "estimated" });
      setMilestones(await listPlanningMilestones(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  async function saveMilestone(m: PlanningMilestone) {
    try {
      await updatePlanningMilestone({
        blueprintId: m.blueprint_id,
        milestoneId: m.id,
        title: m.title,
        startDate: m.start_date,
        endDate: m.end_date,
        datePrecision: m.date_precision,
        dateStatus: m.date_status,
      });
      setMsg("里程碑已保存。");
      if (blueprint) setMilestones(await listPlanningMilestones(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  async function delMilestone(m: PlanningMilestone) {
    if (!window.confirm(`删除里程碑「${m.title}」？`)) return;
    try {
      await deletePlanningMilestone(m.blueprint_id, m.id);
      if (blueprint) setMilestones(await listPlanningMilestones(blueprint.id).catch(() => []));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  /** §10：手工激活 Draft（激活即投影未来 14 天；旧 active → superseded） */
  async function activateDraft(id: number) {
    if (!window.confirm("激活该草稿？当前正式计划将被取代（旧版 → superseded），并投影未来 14 天任务。")) return;
    setBusy(true);
    setError("");
    try {
      await activatePlanningBlueprint(profileId, id);
      setMsg("草稿已激活为正式计划。");
      setManualOpen(false);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** DEV-0059.2 §10：手工新建第一份 Blueprint（无 AI 完整可运行；draft → 后续激活） */
  async function createBp() {
    if (!newBp.title.trim()) {
      setError("请填写蓝图标题。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const created = await createPlanningBlueprint({
        profileId,
        scenarioType: newBp.scenario || scenarioDefault || "generic",
        title: newBp.title.trim(),
        contentMd: newBp.content,
        structuredJson: null,
        sourceSnapshotJson: "{}",
        provenanceJson: JSON.stringify({ source: "manual" }),
        reviewIntervalDays: Number(newBp.interval) || 14,
      });
      setBlueprint(created);
      setBpTitle(created.title);
      setBpContent(created.content_md ?? "");
      setCadenceCustom(String(created.review_interval_days ?? 14));
      setCreateBpOpen(false);
      setMsg(`已创建蓝图草稿 v${created.version}。可在「手工维护规划」中补充阶段/里程碑并激活。`);
      const [ps, ms] = await Promise.all([
        listPlanningPhases(created.id).catch(() => [] as PlanningPhase[]),
        listPlanningMilestones(created.id).catch(() => [] as PlanningMilestone[]),
      ]);
      setPhases(ps);
      setMilestones(ms);
      setManualOpen(true);
      void load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §28：让 Higher AI 生成规划（打开 AI 面板并发送 blueprint 模式请求；无 AI 也一切可用） */
  function askAiPlanning() {
    setPageContext({ page: "planning", pageLabel: "学习规划" });
    setAiOpen(true);
    void sendChat("请基于我的正式目标与规划资料，生成一份长期学习蓝图（blueprint 模式）：包含阶段划分、里程碑与未来 14 天任务。").catch(() => {});
  }

  /** §2/DEV-0059.2 §9：审查并整理规划（只读选中 source；显式 source_id；长文件分页读取，禁止假装读完） */
  function askAiReviewSources() {
    const sel = sources.filter((s) => selectedSourceIds.includes(s.id));
    const refs = sel.map((s) => `[source_id=${s.id}] ${s.original_name}`);
    setPageContext({ page: "planning", pageLabel: "学习规划" });
    setAiOpen(true);
    const body =
      refs.length > 0
        ? `本次只审查用户选中的以下规划资料（${refs.join("；")}）：用 read_planning_source 分页读取（start_char/max_chars）直到 has_more=false；若总量超出本次 context 预算，请明确告诉用户「资料过长，本次未完整读取」，禁止声称已读全文。结合我的正式目标整理成一份可执行的长期蓝图（blueprint 模式），并给出规划资料审查意见（source_review：原内容/建议内容/修改理由/依据；decision=modify 必须给 reason 与 suggested；不确定写 conflict/missing，禁止编造）。`
        : "请结合我的正式目标，整理一份可执行的长期学习蓝图（blueprint 模式）。";
    void sendChat(body).catch(() => {});
  }

  const riskLabel: Record<string, string> = {
    unknown: "",
    normal: "",
    attention: "需要留意",
    off_reach: "偏离冲刺目标",
    near_safety: "接近保底线",
    below_safety: "低于保底线",
  };
  const riskText = riskLabel[risk] ?? "";

  return (
    <section className="card pt-summary">
      <div className="pt-summary__head">
        <span className="pt-summary__title">正式目标与规划</span>
        <span className="muted pt-summary__meta">规划资料 {sources.length} 份</span>
      </div>

      {/* §27：正式目标槽位（操作入口） */}
      <GoalTargetPanel profileId={profileId} profileType={profileType} onChanged={onChanged} />

      {msg && <div className="alert pt-summary__msg">{msg}</div>}
      {error && <div className="alert alert--error pt-summary__msg">{error}</div>}

      {/* §28：Active Blueprint 摘要 */}
      {blueprint ? (
        <div className="pt-summary__bp">
          <div className="pt-summary__bp-row">
            <span className="muted">当前计划</span>
            <b>{blueprint.title || `规划 v${blueprint.version}`}</b>
            <span className="muted">
              v{blueprint.version} · {blueprint.scenario_type === "postgraduate" ? "考研" : "通用"} · 每 {blueprint.review_interval_days} 天复盘
              {blueprint.next_review_at
                ? ` · 下次 ${formatDateTime(blueprint.next_review_at)}`
                : ""}
            </span>
          </div>
          {riskText && (
            <span className="pt-summary__risk" title="基于最新已确认复盘的客观判断">
              {riskText}
            </span>
          )}
        </div>
      ) : (
        /* DEV-0059.2 §10：无 AI 创建第一份 Blueprint */
        <div className="pt-summary__bp pt-summary__bp--empty">
          <span className="muted">暂无正式规划蓝图</span>
          <button className="btn btn--small" onClick={() => setCreateBpOpen(true)}>
            手工新建规划
          </button>
        </div>
      )}

      {/* §12：滚动提示（只读；不自动生成） */}
      {blueprint && nearTasks < 3 && (
        <p className="pt-summary__hint">
          近期计划不足 7 天（未来 7 天仅 {nearTasks} 项计划任务）。可「生成规划 / 审查并整理」补充，或手工维护。
        </p>
      )}

      {/* §10/§11：手工维护规划（无 AI 也可完整维护正式蓝图） */}
      {blueprint && (
        <div className="pt-manual">
          <button
            className="btn btn--small"
            onClick={() => void openManual()}
            title="手工编辑蓝图 / 阶段 / 里程碑 / 复盘节奏"
          >
            手工维护规划
          </button>
          {manualOpen && (
            <div className="pt-manual__panel">
              <div className="pt-manual__sec">
                <span className="muted" style={{ fontSize: 11 }}>
                  蓝图基础信息
                </span>
                <input
                  className="modal__input"
                  value={bpTitle}
                  onChange={(e) => setBpTitle(e.target.value)}
                  placeholder="蓝图标题"
                />
                <textarea
                  className="modal__input"
                  style={{ minHeight: 64 }}
                  value={bpContent}
                  onChange={(e) => setBpContent(e.target.value)}
                  placeholder="蓝图摘要 / 内容（首段可作为 summary）"
                />
                <button className="btn btn--small btn--primary" disabled={busy} onClick={() => void saveBpMeta()}>
                  保存蓝图
                </button>
              </div>

              <div className="pt-manual__sec">
                <span className="muted" style={{ fontSize: 11 }}>
                  复盘提醒（只改 cadence，不调 AI）
                </span>
                <div className="pt-manual__chips">
                  {[7, 14, 30].map((n) => (
                    <button
                      key={n}
                      className={
                        "chip" +
                        (blueprint.review_enabled && blueprint.review_interval_days === n
                          ? " chip--active"
                          : "")
                      }
                      onClick={() => void setCadence(true, n)}
                    >
                      {n}天
                    </button>
                  ))}
                  <input
                    className="modal__input"
                    style={{ width: 64 }}
                    value={cadenceCustom}
                    onChange={(e) => setCadenceCustom(e.target.value.replace(/\D/g, ""))}
                    placeholder="N"
                  />
                  <button
                    className="btn btn--small"
                    onClick={() => void setCadence(true, Number(cadenceCustom) || 14)}
                  >
                    自定义
                  </button>
                  <button
                    className="btn btn--small"
                    onClick={() => void setCadence(false, blueprint.review_interval_days)}
                  >
                    关闭
                  </button>
                </div>
              </div>

              <div className="pt-manual__sec">
                <span className="muted" style={{ fontSize: 11 }}>
                  阶段（Phase）
                </span>
                {phases.map((p) => (
                  <div key={p.id} className="pt-manual__row">
                    <input
                      className="modal__input"
                      style={{ flex: 1 }}
                      value={p.title}
                      onChange={(e) => setPhases((prev) => prev.map((x) => (x.id === p.id ? { ...x, title: e.target.value } : x)))}
                    />
                    <input
                      className="modal__input"
                      style={{ width: 110 }}
                      type="date"
                      value={p.start_date ?? ""}
                      onChange={(e) => setPhases((prev) => prev.map((x) => (x.id === p.id ? { ...x, start_date: e.target.value || null } : x)))}
                    />
                    <button className="btn btn--small" onClick={() => void savePhase(p)}>存</button>
                    <button className="btn btn--small" onClick={() => void delPhase(p)}>删</button>
                  </div>
                ))}
                <div className="pt-manual__row">
                  <input
                    className="modal__input"
                    style={{ flex: 1 }}
                    value={newPhase.title}
                    onChange={(e) => setNewPhase((prev) => ({ ...prev, title: e.target.value }))}
                    placeholder="新阶段名称"
                  />
                  <button className="btn btn--small" onClick={() => void addPhase()}>新增阶段</button>
                </div>
              </div>

              <div className="pt-manual__sec">
                <span className="muted" style={{ fontSize: 11 }}>
                  里程碑（Milestone）
                </span>
                {milestones.map((m) => (
                  <div key={m.id} className="pt-manual__row">
                    <input
                      className="modal__input"
                      style={{ flex: 1 }}
                      value={m.title}
                      onChange={(e) => setMilestones((prev) => prev.map((x) => (x.id === m.id ? { ...x, title: e.target.value } : x)))}
                    />
                    <input
                      className="modal__input"
                      style={{ width: 110 }}
                      type="date"
                      value={m.start_date ?? ""}
                      onChange={(e) => setMilestones((prev) => prev.map((x) => (x.id === m.id ? { ...x, start_date: e.target.value || null } : x)))}
                    />
                    <button className="btn btn--small" onClick={() => void saveMilestone(m)}>存</button>
                    <button className="btn btn--small" onClick={() => void delMilestone(m)}>删</button>
                  </div>
                ))}
                <div className="pt-manual__row">
                  <input
                    className="modal__input"
                    style={{ flex: 1 }}
                    value={newMs.title}
                    onChange={(e) => setNewMs((prev) => ({ ...prev, title: e.target.value }))}
                    placeholder="新里程碑名称"
                  />
                  <button className="btn btn--small" onClick={() => void addMilestone()}>新增里程碑</button>
                </div>
              </div>

              {/* §10：手工确认 / 激活 Draft */}
              {drafts.length > 0 && (
                <div className="pt-manual__sec">
                  <span className="muted" style={{ fontSize: 11 }}>
                    待激活草稿（激活后投影未来 14 天）
                  </span>
                  {drafts.map((d) => (
                    <div key={d.id} className="pt-manual__row">
                      <span style={{ flex: 1 }}>v{d.version} {d.title || "未命名"}</span>
                      <button className="btn btn--small btn--primary" disabled={busy} onClick={() => void activateDraft(d.id)}>
                        激活
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* §28：Review 状态 */}
      <div className="pt-summary__review">
        <span className="muted">复盘状态</span>
        {due ? (
          <b className="pt-summary__review-due">该进行阶段复盘了</b>
        ) : latest ? (
          <span>
            {latest.status === "completed"
              ? `上次复盘完成于 ${formatDateTime(latest.completed_at ?? latest.updated_at)}`
              : latest.status === "running"
                ? "进行中（已生成证据快照）"
                : latest.status === "waiting_approval"
                  ? "AI 已给出调整提案，待审阅应用"
                  : latest.status === "failed"
                    ? "上次 AI 评估失败（正式数据未变化）"
                    : `进行中（${latest.status}）`}
          </span>
        ) : (
          <span className="muted">暂无复盘记录</span>
        )}
        <button
          className="btn btn--small"
          disabled={busy || aiBusy || (latest?.status === "waiting_approval")}
          onClick={() => void startReview()}
        >
          开始复盘
        </button>
        {latest?.status === "waiting_approval" && latest.change_set_id != null && (
          <button
            className="btn btn--small btn--primary"
            disabled={busy}
            onClick={() => setReviewChangeSetId(latest.change_set_id!)}
          >
            审阅 AI 调整
          </button>
        )}
      </div>

      {/* §3：证据快照摘要 + 用户确认后才启动 AI */}
      {snapshot && (
        <div className="pt-snapshot">
          <div className="pt-snapshot__head">
            <span className="muted" style={{ fontSize: 11 }}>
              本次复盘证据（真实数据只读快照）
            </span>
          </div>
          <p className="pt-snapshot__line">
            {(() => {
              const bp = snapshot["active_blueprint"] as { version?: number; title?: string } | null;
              const tasks = (snapshot["period_tasks"] as unknown[] | undefined)?.length ?? 0;
              const sess = (snapshot["trusted_sessions"] as unknown[] | undefined)?.length ?? 0;
              const evs = (snapshot["trusted_evaluations"] as unknown[] | undefined)?.length ?? 0;
              const gts = (snapshot["active_goal_targets"] as unknown[] | undefined)?.length ?? 0;
              const phases = (snapshot["phases"] as unknown[] | undefined)?.length ?? 0;
              return `蓝图 ${bp ? `v${bp.version} ${bp.title}` : "无"} · 阶段 ${phases} · 周期任务 ${tasks} · 可信学习 ${sess} 次 · 可信验证 ${evs} 次 · 正式目标 ${gts} 个`;
            })()}
          </p>
          <button
            className="btn btn--small btn--primary"
            disabled={aiBusy}
            onClick={() => void confirmAndRunAi()}
          >
            {aiBusy ? "AI 评估中…" : "确认并启动 AI 评估"}
          </button>
          <span className="muted" style={{ fontSize: 11 }}>
            仅在你确认后才会调用 AI；失败不改变任何正式数据。
          </span>
        </div>
      )}

      {/* §2：规划资料列表（文件名/类型/状态/是否参与审查；默认选所有 ready） */}
      {sources.length > 0 && (
        <div className="pt-sources">
          <div className="pt-sources__head">
            <span className="muted" style={{ fontSize: 11 }}>
              参与本次审查的规划资料（{selectedSourceIds.length}/{sources.length}）
            </span>
          </div>
          <ul className="pt-sources__list">
            {sources.map((s) => {
              const checked = selectedSourceIds.includes(s.id);
              return (
                <li key={s.id} className="pt-sources__item">
                  <label className="pt-sources__label">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() =>
                        setSelectedSourceIds((prev) =>
                          checked ? prev.filter((x) => x !== s.id) : [...prev, s.id]
                        )
                      }
                    />
                    <span className="pt-sources__name" title={s.original_path}>
                      {s.original_name}
                    </span>
                    <span className="pt-sources__meta muted">
                      {s.file_type.toUpperCase()} · {s.status}
                      {s.source_kind === "export_reimport" ? " · 重新导入" : ""}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {/* §28/§2：操作区（生成规划 / 审查并整理 / 导入 / 导出） */}
      <div className="pt-summary__actions">
        <button className="btn btn--small" onClick={askAiPlanning}>
          生成规划
        </button>
        <button className="btn btn--small" onClick={askAiReviewSources}>
          审查并整理规划
        </button>
        <button className="btn btn--small" disabled={busy} onClick={() => void importSources("user_file")}>
          导入规划资料
        </button>
        <button
          className="btn btn--small"
          title="重新导入 Higher 导出的规划文件（标记 export_reimport）"
          disabled={busy}
          onClick={() => void importSources("export_reimport")}
        >
          重新导入（Higher 导出）
        </button>
        <span className="pt-summary__act-group">
          <span className="muted" style={{ fontSize: 11 }}>
            导出：
          </span>
          <button
            className="btn btn--small"
            disabled={busy}
            onClick={() => void runExport("profile-docx")}
          >
            档案 Word
          </button>
          <button
            className="btn btn--small"
            disabled={busy}
            onClick={() => void runExport("profile-xlsx")}
          >
            档案 Excel
          </button>
          <button
            className="btn btn--small"
            disabled={busy}
            onClick={() => void runExport("plan-docx")}
          >
            规划 Word
          </button>
          <button
            className="btn btn--small"
            disabled={busy}
            onClick={() => void runExport("plan-xlsx")}
          >
            规划 Excel
          </button>
        </span>
      </div>

      {/* DEV-0059.2 §10：手工新建第一份 Blueprint */}
      {createBpOpen && (
        <div className="modal-overlay" onClick={() => setCreateBpOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3 className="modal__title">手工新建规划（无需 AI）</h3>
            <p className="modal__context muted">
              创建后为草稿：可补充阶段/里程碑，最后「激活」成为正式计划。
            </p>
            {error && <div className="modal__error">{error}</div>}
            <div className="modal__field">
              <label className="form-label">蓝图标题 *</label>
              <input
                className="modal__input"
                value={newBp.title}
                onChange={(e) => setNewBp((p) => ({ ...p, title: e.target.value }))}
                placeholder="如：2027 考研全程规划"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">内容 / 摘要（summary）</label>
              <textarea
                className="modal__input"
                style={{ minHeight: 64 }}
                value={newBp.content}
                onChange={(e) => setNewBp((p) => ({ ...p, content: e.target.value }))}
                placeholder="规划摘要（可选）"
              />
            </div>
            <div className="modal__field">
              <label className="form-label">复盘间隔（天）</label>
              <input
                className="modal__input"
                value={newBp.interval}
                onChange={(e) => setNewBp((p) => ({ ...p, interval: e.target.value.replace(/\D/g, "") }))}
              />
            </div>
            <div className="modal__field">
              <label className="form-label">场景（建议：{scenarioDefault === "postgraduate" ? "考研" : "通用"}）</label>
              <select
                className="modal__input"
                value={newBp.scenario}
                onChange={(e) => setNewBp((p) => ({ ...p, scenario: e.target.value }))}
              >
                <option value="generic">通用</option>
                <option value="postgraduate">考研（冲刺/保底）</option>
              </select>
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setCreateBpOpen(false)}>
                取消
              </button>
              <button className="btn btn--primary" disabled={busy} onClick={() => void createBp()}>
                {busy ? "创建中…" : "创建草稿"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* DEV-0059.2 §1：Planning 页直接审阅 Review 生成的 ChangeSet（复用现有 ChangeSetReview） */}
      {reviewChangeSetId != null && (
        <ChangeSetReview
          profileId={profileId}
          changeSetId={reviewChangeSetId}
          onClose={() => {
            setReviewChangeSetId(null);
            void load();
          }}
          onApplied={() => {
            setReviewChangeSetId(null);
            void load();
            triggerRefresh();
          }}
        />
      )}
    </section>
  );
}
