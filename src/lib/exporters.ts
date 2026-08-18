/**
 * DEV-0059 §31-35：Office 导出（docx / exceljs 懒加载，点击时才 dynamic import）。
 *
 * - 只生成字节流 + 建议文件名；保存动作由调用侧完成
 *   （dialog save path → writeExportFile(path, base64)，§31.3 只写用户所选路径）
 * - 导出不是新事实源（§32）：只读当前 confirmed/active 数据
 */
import {
  getActivePlanningBlueprint,
  getPersonalizationProfile,
  listActiveGoalTargets,
  listPlanningBlueprints,
  listPlanningMilestones,
  listPlanningPhases,
  listPlanningReviews,
  listPlanningSources,
  listSourcesForPersonalProfileVersion,
  listTasksByRangeByProfile,
} from "../api";
import { addDaysISO, todayDate } from "../utils";
import type {
  GoalTarget,
  PersonalizationProfile,
  PlanningBlueprint,
  PlanningMilestone,
  PlanningPhase,
  PlanningReview,
  PlanningSource,
  Task,
} from "../types";

export interface ExportResult {
  bytes: Uint8Array;
  fileName: string;
}

export function base64FromBytes(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** structured_json → 可读 "key: value" 行（递归展开对象/数组，防止 JSON schema 变化破坏导出）。 */
function flattenJson(obj: unknown, prefix = ""): string[] {
  if (obj == null) return [];
  if (typeof obj === "string" || typeof obj === "number" || typeof obj === "boolean") {
    return [`${prefix}: ${String(obj)}`];
  }
  if (Array.isArray(obj)) {
    return obj.flatMap((v, i) => flattenJson(v, `${prefix}[${i}]`));
  }
  if (typeof obj === "object") {
    return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
      flattenJson(v, prefix ? `${prefix}.${k}` : k)
    );
  }
  return [];
}

/** md_content → 段落（空行分隔），去除纯格式符号。 */
function mdParagraphs(md: string): string[] {
  return md
    .split(/\n{2,}/)
    .map((p) => p.replace(/[#*_>`\-~]/g, "").trim())
    .filter((p) => p.length > 0);
}

/** §13 数据：Profile + Sources（Planning）+ Personal Sources（版本 snapshot）+ Targets + Blueprint + Phases/Milestones + Reviews */
async function gather(profileId: number) {
  const [profile, sources, targets, blueprint, blueprints, reviews] = await Promise.all([
    getPersonalizationProfile(profileId).catch(() => null as PersonalizationProfile | null),
    listPlanningSources(profileId).catch(() => [] as PlanningSource[]),
    listActiveGoalTargets(profileId).catch(() => [] as GoalTarget[]),
    getActivePlanningBlueprint(profileId).catch(() => null as PlanningBlueprint | null),
    listPlanningBlueprints(profileId).catch(() => [] as PlanningBlueprint[]),
    listPlanningReviews(profileId).catch(() => [] as PlanningReview[]),
  ]);
  // DEV-0059.1 §8：PersonalProfile 导出必须用「当前版本」的 Personal Source snapshot，
  // 不是 Planning Sources
  const personalSources = profile
    ? await listSourcesForPersonalProfileVersion(profileId, profile.id).catch(
        () => [] as import("../types").PersonalizationSource[]
      )
    : ([] as import("../types").PersonalizationSource[]);
  const [phases, milestones] = blueprint
    ? await Promise.all([
        listPlanningPhases(blueprint.id).catch(() => [] as PlanningPhase[]),
        listPlanningMilestones(blueprint.id).catch(() => [] as PlanningMilestone[]),
      ])
    : [[], []];
  const t0 = todayDate();
  const tasks14 = blueprint
    ? await listTasksByRangeByProfile(profileId, t0, addDaysISO(t0, 14)).catch(
        () => [] as Task[]
      )
    : ([] as Task[]);
  return { profile, sources, personalSources, targets, blueprint, phases, milestones, reviews, blueprints, tasks14 };
}

// =====================================================================
// §32 · PersonalProfile Export（DOCX 必选；XLSX 建议）
// =====================================================================

export async function exportPersonalProfileDocx(profileId: number): Promise<ExportResult> {
  const { Document, HeadingLevel, Packer, Paragraph, TextRun } = await import("docx");
  const { profile, personalSources } = await gather(profileId);
  const children: import("docx").Paragraph[] = [];
  const h1 = (t: string) => children.push(new Paragraph({ text: t, heading: HeadingLevel.HEADING_1 }));
  const body = (t: string) => children.push(new Paragraph({ children: [new TextRun({ text: t })] }));
  if (!profile) {
    h1("个人学习档案");
    body("尚未形成正式个人档案（无 confirmed 版本）");
  } else {
    h1(`个人学习档案 v${profile.version}`);
    body(`生成时间：${new Date().toLocaleString("zh-CN")}`);
    body(`确认时间：${profile.confirmed_at ?? "—"}`);
    body(`基础版本：${profile.based_on_version_id ?? "—"}`);
    // 结构化字段（能力/强弱项/时间条件/习惯/偏好/限制/待确认）
    if (profile.structured_json) {
      try {
        const sj = JSON.parse(profile.structured_json) as Record<string, unknown>;
        const labels: Record<string, string> = {
          capabilities: "能力",
          strengths_weaknesses: "强弱项",
          time_conditions: "时间条件",
          habits: "习惯",
          preferences: "偏好",
          constraints: "限制",
          unresolved: "待确认",
        };
        for (const [k, v] of Object.entries(sj)) {
          h1(labels[k] ?? k);
          for (const line of flattenJson(v)) body(line);
        }
      } catch {
        /* ignore */
      }
    }
    for (const p of mdParagraphs(profile.md_content)) body(p);
  }
  h1(`个人档案 Source（版本 v${profile?.version ?? "?"} 快照）`);
  if (personalSources.length === 0) body("（无）");
  for (const s of personalSources) body(`${s.file_name}（${s.file_type} · ${s.status}）`);
  const doc = new Document({ sections: [{ children }] });
  const bytes = await Packer.toBuffer(doc);
  return {
    bytes: new Uint8Array(bytes),
    fileName: `Higher_个人学习档案_v${profile?.version ?? 1}.docx`,
  };
}

export async function exportPersonalProfileXlsx(profileId: number): Promise<ExportResult> {
  const ExcelJS = await import("exceljs");
  const { profile, personalSources } = await gather(profileId);
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet("个人档案");
  const rows: (string | number)[][] = [];
  if (!profile) {
    rows.push(["个人学习档案"]);
    rows.push(["尚未形成正式个人档案（无 confirmed 版本）"]);
  } else {
    rows.push(["个人学习档案", `v${profile.version}`]);
    rows.push(["生成时间", new Date().toLocaleString("zh-CN")]);
    rows.push(["确认时间", profile.confirmed_at ?? "—"]);
    rows.push(["基础版本", profile.based_on_version_id ?? "—"]);
    if (profile.structured_json) {
      try {
        const sj = JSON.parse(profile.structured_json) as Record<string, unknown>;
        for (const [k, v] of Object.entries(sj)) {
          rows.push([k]);
          for (const line of flattenJson(v)) rows.push([line]);
        }
      } catch {
        /* ignore */
      }
    }
    for (const p of mdParagraphs(profile.md_content)) rows.push([p]);
  }
  rows.push([`个人档案 Source（版本 v${profile?.version ?? "?"} 快照）`]);
  if (personalSources.length === 0) rows.push(["（无）"]);
  for (const s of personalSources) rows.push([`${s.file_name}（${s.file_type} · ${s.status}）`]);
  for (const r of rows) ws.addRow(r);
  ws.columns.forEach((c) => {
    if (c && c.width === undefined) c.width = 40;
  });
  const bytes = await wb.xlsx.writeBuffer();
  return {
    bytes: new Uint8Array(bytes),
    fileName: `Higher_个人学习档案_v${profile?.version ?? 1}.xlsx`,
  };
}

// =====================================================================
// §33 · PlanningBlueprint Word Export
// =====================================================================

export async function exportBlueprintDocx(profileId: number): Promise<ExportResult> {
  const { Document, HeadingLevel, Packer, Paragraph, TextRun } = await import("docx");
  const g = await gather(profileId);
  const bp = g.blueprint;
  const paragraphs: import("docx").Paragraph[] = [];
  const h1 = (t: string) => paragraphs.push(new Paragraph({ text: t, heading: HeadingLevel.HEADING_1 }));
  const h2 = (t: string) => paragraphs.push(new Paragraph({ text: t, heading: HeadingLevel.HEADING_2 }));
  const body = (t: string) => paragraphs.push(new Paragraph({ children: [new TextRun({ text: t })] }));

  // 标题 / version / 更新时间
  h1(bp ? bp.title || `学习规划 v${bp.version}` : "学习规划（暂无 Active Blueprint）");
  if (bp) {
    body(`版本：v${bp.version}`);
    body(`更新时间：${bp.updated_at}`);
    body(`下次复盘：${bp.next_review_at ?? "—"}`);
  }

  // Personal summary（confirmed 档案摘要）
  h2("个人情况摘要");
  if (g.profile) {
    for (const p of mdParagraphs(g.profile.md_content).slice(0, 12)) body(p);
    if (g.profile.structured_json) {
      try {
        const sj = JSON.parse(g.profile.structured_json) as Record<string, unknown>;
        for (const [k, v] of Object.entries(sj).slice(0, 8))
          for (const line of flattenJson(v).slice(0, 12)) body(`${k} · ${line}`);
      } catch {
        /* ignore */
      }
    }
  } else {
    body("（尚未形成正式个人档案）");
  }

  // Goal Targets
  h2("正式目标 Goal Targets");
  if (g.targets.length === 0) body("（未设置正式目标）");
  for (const t of g.targets)
    body(`${t.scenario_type === "postgraduate" ? `【${t.role === "reach" ? "冲刺" : "保底"}】` : "【通用】"} ${t.title}${t.target_date ? `（目标 ${t.target_date}）` : ""}`);

  // 规划依据
  h2("规划依据（资料）");
  if (g.sources.length === 0) body("（无规划资料）");
  for (const s of g.sources) body(`${s.original_name}（${s.file_type} · ${s.status}）`);

  // Blueprint summary
  h2("蓝图摘要");
  if (bp) {
    for (const p of mdParagraphs(bp.content_md)) body(p);
  } else {
    body("（暂无 Active Blueprint）");
  }

  // Phases
  h2("阶段 Phases");
  if (g.phases.length === 0) body("（无）");
  for (const ph of g.phases) {
    body(`【${ph.title}】${ph.start_date ?? ""}${ph.end_date ? " ~ " + ph.end_date : ""}`);
    if (ph.objective_md) for (const p of mdParagraphs(ph.objective_md)) body(p);
  }

  // Milestones
  h2("里程碑 Milestones");
  if (g.milestones.length === 0) body("（无）");
  for (const ms of g.milestones)
    body(`◆ ${ms.title}（${ms.start_date ?? ""}${ms.end_date ? " ~ " + ms.end_date : ""} · ${ms.date_status}）`);

  // Subject plan / Monthly / stage plan（来自 structured_json，若有）
  if (bp && bp.structured_json) {
    try {
      const sj = JSON.parse(bp.structured_json) as Record<string, unknown>;
      for (const key of ["subject_plan", "monthly_plan", "stage_plan"]) {
        if (!(key in sj)) continue;
        h2(key === "subject_plan" ? "科目计划 Subject Plan" : key === "monthly_plan" ? "月计划 Monthly Plan" : "阶段计划 Stage Plan");
        for (const line of flattenJson(sj[key]).slice(0, 80)) body(line);
      }
    } catch {
      /* ignore */
    }
  }

  // 当前 14-day plan
  h2("当前 14 天计划");
  if (g.tasks14.length === 0) body("（未来 14 天暂无计划任务）");
  for (const t of g.tasks14)
    body(`${t.planned_date ?? ""} · ${t.status === "completed" ? "✓" : "○"} ${t.title}`);

  // Risks
  h2("风险 Risks");
  const risky = g.reviews.filter((r) => ["off_reach", "near_safety", "below_safety", "attention"].includes(r.risk_state));
  if (risky.length === 0) body("（最近复盘无风险标记）");
  for (const r of risky) body(`${r.risk_state}（${r.period_start} ~ ${r.period_end}）`);

  // unresolved
  h2("待确认 unresolved");
  if (bp && bp.structured_json) {
    try {
      const sj = JSON.parse(bp.structured_json) as Record<string, unknown>;
      const u = sj.unresolved;
      if (u != null) for (const line of flattenJson(u)) body(line);
      else body("（无）");
    } catch {
      body("（无）");
    }
  } else {
    body("（无）");
  }

  // Changelog / Review history
  h2("复盘历史 Review History");
  if (g.reviews.length === 0) body("（暂无复盘记录）");
  for (const r of g.reviews)
    body(`${r.created_at} · ${r.status}${r.risk_state !== "unknown" ? ` · ${r.risk_state}` : ""}`);

  const doc = new Document({ sections: [{ children: paragraphs }] });
  const bytes = await Packer.toBuffer(doc);
  return {
    bytes: new Uint8Array(bytes),
    fileName: `Higher_${bp ? bp.title.replace(/[\\/:*?"<>|]/g, "_").slice(0, 30) || "学习规划" : "学习规划"}_v${bp?.version ?? 1}.docx`,
  };
}

// =====================================================================
// §34 · PlanningBlueprint Excel Export（至少 10 sheets）
// =====================================================================

export async function exportBlueprintXlsx(profileId: number): Promise<ExportResult> {
  const ExcelJS = await import("exceljs");
  const g = await gather(profileId);
  const bp = g.blueprint;
  const wb = new ExcelJS.Workbook();
  const sheet = (name: string, rows: (string | number)[][]) => {
    const ws = wb.addWorksheet(name);
    for (const r of rows) ws.addRow(r);
    ws.columns.forEach((c) => {
      if (c && c.width === undefined) c.width = 40;
    });
  };

  // Overview
  sheet("Overview", [
    ["规划概览"],
    ["标题", bp?.title ?? "—"],
    ["版本", bp ? `v${bp.version}` : "—"],
    ["更新时间", bp?.updated_at ?? "—"],
    ["下次复盘", bp?.next_review_at ?? "—"],
    ["复盘间隔(天)", bp?.review_interval_days ?? "—"],
    ["阶段数", g.phases.length],
    ["里程碑数", g.milestones.length],
    ["规划资料数", g.sources.length],
  ]);

  // Targets
  sheet("Targets", [
    ["scenario", "role", "title", "target_date", "status", "version"],
    ...g.targets.map((t) => [t.scenario_type, t.role, t.title, t.target_date ?? "", t.status, t.version]),
  ]);

  // Phases
  sheet("Phases", [
    ["phase_key", "title", "start", "end", "objective_md", "sort_order", "status"],
    ...g.phases.map((p) => [p.phase_key, p.title, p.start_date ?? "", p.end_date ?? "", p.objective_md, p.sort_order, p.status]),
  ]);

  // Milestones
  sheet("Milestones", [
    ["key", "title", "start", "end", "precision", "date_status", "status"],
    ...g.milestones.map((m) => [m.milestone_key, m.title, m.start_date ?? "", m.end_date ?? "", m.date_precision, m.date_status, m.status]),
  ]);

  // Monthly / Subject / 14-day / Risks / Sources / Changelog
  const sj: Record<string, unknown> = bp?.structured_json
    ? (() => {
        try {
          return JSON.parse(bp.structured_json) as Record<string, unknown>;
        } catch {
          return {};
        }
      })()
    : {};
  sheet("Monthly Plan", [["month", "plan"], ...(flattenJson(sj.monthly_plan).map((l) => [l]) as [string][])]);
  sheet("Subject Plan", [["subject", "plan"], ...(flattenJson(sj.subject_plan).map((l) => [l]) as [string][])]);
  sheet("14-day Plan", [
    ["date", "title", "status"],
    ...g.tasks14.map((t) => [t.planned_date ?? "", t.title, t.status]),
  ]);
  sheet("Risks & Adjustments", [
    ["period_start", "period_end", "risk_state", "status", "user_decision"],
    ...g.reviews
      .filter((r) => r.risk_state !== "unknown" || r.user_decision)
      .map((r) => [r.period_start, r.period_end, r.risk_state, r.status, r.user_decision]),
  ]);
  sheet("Sources", [
    ["name", "type", "kind", "status", "created_at"],
    ...g.sources.map((s) => [s.original_name, s.file_type, s.source_kind, s.status, s.created_at]),
  ]);
  sheet("Changelog", [
    ["created_at", "status", "risk_state", "completed_at"],
    ...g.reviews.map((r) => [r.created_at, r.status, r.risk_state, r.completed_at ?? ""]),
  ]);

  const bytes = await wb.xlsx.writeBuffer();
  return {
    bytes: new Uint8Array(bytes),
    fileName: `Higher_${bp ? bp.title.replace(/[\\/:*?"<>|]/g, "_").slice(0, 30) || "学习规划" : "学习规划"}_v${bp?.version ?? 1}.xlsx`,
  };
}
