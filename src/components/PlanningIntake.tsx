import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  discardPlanningIntakeDraft,
  getPlanningIntakeDraft,
  importPersonalizationFiles,
  savePlanningIntakeDraft,
  writeExportFile,
} from "../api";
import { useAiPanel } from "./ai/AiPanelContext";
import {
  buildIntakeTemplate,
  computeCompleteness,
  INTAKE_TEMPLATE_FILE_NAME,
  parseIntakeTemplate,
} from "../planning/intakeTemplate";
import type { PlanningIntakeDraft } from "../types";

/**
 * PRODUCT-2.0 §24.1 / §24.2 / §24.4 —— Planning Intake：AI 规划入口。
 *
 * 只在**没有 Active Blueprint** 时出现。
 *
 * 三个入口（§24.1）：
 *   和 AI 一起填写（主按钮）
 *   导入规划任务书（模板 / 文件）
 *   直接告诉 AI 目标
 *
 * 硬约束：
 * - 这里产生的一切都是 **Draft**（§0A.4），绝不写入 goals / tasks / blueprints
 * - 不强制、不阻塞：任何一步都可以跳过
 * - 导入复用现有 source ingestion（§24.4），不另造 parser
 */
export default function PlanningIntake({
  profileId,
  onChanged,
}: {
  profileId: number;
  onChanged?: () => void | Promise<void>;
}) {
  const { sendChat } = useAiPanel();
  const [draft, setDraft] = useState<PlanningIntakeDraft | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [importText, setImportText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");

  const load = useCallback(async () => {
    try {
      setDraft(await getPlanningIntakeDraft(profileId));
    } catch (e) {
      setError(String(e));
    }
  }, [profileId]);

  useEffect(() => {
    void load();
  }, [load]);

  /** §24.1 入口一：和 AI 一起填写（打开 AI 面板，走同一 planner 管线） */
  function startWithAi() {
    void sendChat(
      "我想开始规划。请一步步问我：目标、当前水平、可投入时间、学习内容、偏好与约束，然后给我一份可确认的计划。"
    );
  }

  /** §24.1 入口三：直接告诉 AI 目标 */
  function tellGoal() {
    void sendChat("我的目标是：");
  }

  /** §24.2 导出模板（只写用户所选路径） */
  async function downloadTemplate() {
    setError("");
    try {
      const path = await saveDialog({
        defaultPath: INTAKE_TEMPLATE_FILE_NAME,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      const bytes = new TextEncoder().encode(buildIntakeTemplate());
      let binary = "";
      for (const b of bytes) binary += String.fromCharCode(b);
      await writeExportFile(path, btoa(binary));
      setInfo("模板已保存。填好后再导入即可。");
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * §24.4 从文件导入：复用现有 source ingestion
   * （md / txt / docx / pdf 统一走 import_personalization_files）。
   */
  async function importFromFile() {
    setError("");
    setBusy(true);
    try {
      const picked = await openDialog({
        multiple: false,
        filters: [{ name: "规划任务书 / 资料", extensions: ["md", "txt", "docx", "pdf"] }],
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      const outcomes = await importPersonalizationFiles(profileId, [path]);
      const base = path.split(/[\\/]/).pop() ?? path;
      const names = outcomes.length > 0 ? outcomes.map(() => base).join(", ") : base;
      setDraft(
        await savePlanningIntakeDraft({
          profileId,
          sourceKind: "import",
          rawText: names,
          status: "ready",
        })
      );
      setInfo(`已导入：${names}`);
      setImportOpen(false);
      await onChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 导入任务书正文（粘贴或文件内容）→ 确定性解析 → Draft */
  async function saveTaskbook() {
    const raw = importText.trim();
    if (!raw) {
      setError("请先粘贴任务书内容。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const structured = parseIntakeTemplate(raw);
      const completeness = computeCompleteness(structured);
      const saved = await savePlanningIntakeDraft({
        profileId,
        sourceKind: "taskbook",
        rawText: raw,
        structuredJson: JSON.stringify(structured),
        completenessJson: JSON.stringify(completeness),
        status: "ready",
      });
      setDraft(saved);
      setImportText("");
      setImportOpen(false);
      setInfo("任务书已保存为草稿（尚未写入正式计划）。");
      await onChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function discard() {
    setError("");
    try {
      await discardPlanningIntakeDraft(profileId);
      setDraft(null);
      setInfo("已丢弃草稿。正式计划不受影响。");
      await onChanged?.();
    } catch (e) {
      setError(String(e));
    }
  }

  const completeness = draft?.completeness_json
    ? (JSON.parse(draft.completeness_json) as { filled: number; total: number; missing: string[] })
    : null;

  return (
    <section className="card intake" aria-label="准备开始你的规划">
      <h2 className="intake__title">准备开始你的规划</h2>
      <p className="intake__desc">
        先记下你的目标与现状。这一步只会产生草稿，不会改动任何正式计划；
        计划会先给你预览，由你确认后才会生效。
      </p>

      {error && <div className="alert alert--error">{error}</div>}
      {info && <div className="intake__info">{info}</div>}

      <div className="intake__actions">
        <button className="btn btn--primary" onClick={startWithAi}>
          和 AI 一起填写
        </button>
        <button className="btn" onClick={() => setImportOpen((v) => !v)}>
          导入规划任务书
        </button>
        <button className="btn" onClick={tellGoal}>
          直接告诉 AI 目标
        </button>
        <button className="btn btn--ghost" onClick={() => void downloadTemplate()}>
          下载任务书模板
        </button>
      </div>

      {importOpen && (
        <div className="intake__import">
          <textarea
            className="intake__textarea"
            aria-label="规划任务书内容"
            placeholder="把《Higher-个人规划任务书》的内容粘贴到这里…"
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
            rows={6}
          />
          <div className="intake__import-actions">
            <button className="btn btn--primary" onClick={() => void saveTaskbook()} disabled={busy}>
              {busy ? "保存中…" : "保存为草稿"}
            </button>
            <button className="btn" onClick={() => void importFromFile()} disabled={busy}>
              从文件导入…
            </button>
            <span className="muted intake__hint">支持 .md / .txt / .docx / .pdf</span>
          </div>
        </div>
      )}

      {draft && (
        <div className="intake__draft">
          <div className="intake__draft-head">
            <span className="intake__draft-label">当前草稿</span>
            <span className="intake__draft-meta">
              来源 {labelOfSource(draft.source_kind)} · 状态 {labelOfStatus(draft.status)}
              {completeness ? ` · 完成 ${completeness.filled}/${completeness.total}` : ""}
            </span>
          </div>
          {completeness && completeness.missing.length > 0 && (
            <p className="muted intake__missing">
              还缺：{completeness.missing.slice(0, 8).join("、")}
              {completeness.missing.length > 8 ? " …" : ""}
            </p>
          )}
          <div className="intake__draft-actions">
            <button className="btn btn--small btn--primary" onClick={startWithAi}>
              让 AI 根据草稿出计划
            </button>
            <button className="btn btn--small" onClick={() => void discard()}>
              丢弃草稿
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function labelOfSource(kind: PlanningIntakeDraft["source_kind"]): string {
  switch (kind) {
    case "chat":
      return "和 AI 一起填写";
    case "taskbook":
      return "规划任务书";
    case "description":
      return "直接描述";
    case "import":
      return "文件导入";
    default:
      return kind;
  }
}

function labelOfStatus(status: PlanningIntakeDraft["status"]): string {
  switch (status) {
    case "draft":
      return "草稿";
    case "ready":
      return "可生成计划";
    case "consumed":
      return "已生成计划";
    default:
      return status;
  }
}
