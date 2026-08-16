// =============== 前端通用工具 ===============

/** 将 ISO/SQLite datetime 字符串转为本地友好显示。 */
export function formatDateTime(raw: string | null | undefined): string {
  if (!raw) return "—";
  // SQLite datetime 格式: "YYYY-MM-DD HH:MM:SS"（UTC）
  // 浏览器 new Date 可解析 "YYYY-MM-DDTHH:MM:SSZ"
  const normalized = raw.includes("T")
    ? raw
    : raw.replace(" ", "T") + "Z";
  const d = new Date(normalized);
  if (isNaN(d.getTime())) return raw;
  return d.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** 仅显示日期部分（YYYY-MM-DD → MM-DD）。 */
export function formatDate(raw: string | null | undefined): string {
  if (!raw) return "未计划";
  return raw.length >= 10 ? raw.slice(0, 10) : raw;
}

/** 将秒数格式化为 "Xh Ym" / "Ym Zs"。 */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null) return "—";
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m < 60) return `${m}m ${s}s`;
  const h = Math.floor(m / 60);
  const mm = m % 60;
  return `${h}h ${mm}m`;
}

/** 返回今天日期 YYYY-MM-DD（本地时区）。 */
export function todayDate(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** 本周一（YYYY-MM-DD，本地时区）。 */
export function weekStartDate(base = new Date()): string {
  const d = new Date(base);
  const dow = d.getDay() === 0 ? 7 : d.getDay(); // 周一=1…周日=7
  d.setDate(d.getDate() - (dow - 1));
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 明天日期 YYYY-MM-DD。 */
export function tomorrowDate(): string {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// =============== LearningDocument（Session Note v2；DEV-0024） ===============

export type NoteBlock =
  | { t: "text"; c: string }
  | { t: "image" | "video" | "drawing"; a: number; n: string };

/** 解析 note → blocks（旧纯文本 = 单 text 块；非法 JSON 同样回退文本）。 */
export function parseNoteBlocks(note: string | null | undefined): NoteBlock[] {
  if (!note || !note.trim()) return [];
  const s = note.trim();
  if (!s.startsWith("{")) return [{ t: "text", c: s }];
  try {
    const doc = JSON.parse(s) as { v?: number; blocks?: NoteBlock[] };
    if (doc.v === 2 && Array.isArray(doc.blocks)) return doc.blocks;
  } catch {
    /* 回退文本 */
  }
  return [{ t: "text", c: s }];
}

/** blocks → note（v2 JSON）。 */
export function serializeNoteBlocks(blocks: NoteBlock[]): string {
  return JSON.stringify({ v: 2, blocks });
}

/** 用户可读纯文本（媒体 → 占位标记；AI/摘要/字数通用）。 */
export function notePlainText(note: string | null | undefined): string {
  return parseNoteBlocks(note)
    .map((b) => (b.t === "text" ? b.c : b.t === "video" ? "[视频]" : b.t === "drawing" ? "[画图]" : "[图片]"))
    .join("\n");
}

/** 文本字数（不含媒体块）。 */
export function noteTextLen(note: string | null | undefined): number {
  return parseNoteBlocks(note)
    .reduce((acc, b) => (b.t === "text" ? acc + b.c.trim().length : acc), 0);
}

/** 媒体统计 [图片数(含画图), 视频数]。 */
export function noteMediaCounts(note: string | null | undefined): [number, number] {
  let img = 0, vid = 0;
  for (const b of parseNoteBlocks(note)) {
    if (b.t === "video") vid++;
    else if (b.t === "image" || b.t === "drawing") img++;
  }
  return [img, vid];
}

/** Note 摘要（Review 卡片：80–180 字，去多余空白）。 */
export function noteSummary(note: string | null | undefined, max = 140): string {
  const plain = notePlainText(note).replace(/\s+/g, " ").trim();
  if (!plain) return "";
  return plain.length <= max ? plain : plain.slice(0, max) + "…";
}

/** File → base64（无 data: 前缀；粘贴/拖入图片与视频导入用）。 */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const r = String(reader.result ?? "");
      const idx = r.indexOf(";base64,");
      resolve(idx >= 0 ? r.slice(idx + 8) : r);
    };
    reader.onerror = () => reject(new Error("读取文件失败"));
    reader.readAsDataURL(file);
  });
}

/** Blob → base64（剪贴板图片）。 */
export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const r = String(reader.result ?? "");
      const idx = r.indexOf(";base64,");
      resolve(idx >= 0 ? r.slice(idx + 8) : r);
    };
    reader.onerror = () => reject(new Error("读取图片失败"));
    reader.readAsDataURL(blob);
  });
}

/** 'YYYY-MM-DD' → 星期（1=周一…7=周日；日历/重复任务用）。 */
export function weekdayOfDate(date: string): number {
  const d = new Date(date + "T00:00:00");
  return d.getDay() === 0 ? 7 : d.getDay();
}

// =============== 学习日（UTC+8）统一解释（DEV-0049 §11.4） ===============

/** Higher 学习日时区偏移（分钟）。 */
export const STUDY_TZ_OFFSET_MIN = 8 * 60;

/**
 * UTC SQLite datetime → 学习日（UTC+8 日历日）"YYYY-MM-DD"。
 * 与后端 SQL `date(started_at, '+8 hours')` 完全一致；前端禁止再裸 slice(0,10)。
 */
export function studyDayOf(raw: string | null | undefined): string {
  if (!raw) return "";
  const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
  if (isNaN(d.getTime())) return raw.slice(0, 10);
  return new Date(d.getTime() + STUDY_TZ_OFFSET_MIN * 60_000)
    .toISOString()
    .slice(0, 10);
}

/** 月份标题与网格（周一起始；DEV-0026 月历）。 */
export function monthGrid(year: number, month: number): (string | null)[] {
  const first = new Date(year, month - 1, 1);
  const daysInMonth = new Date(year, month, 0).getDate();
  const lead = (first.getDay() === 0 ? 7 : first.getDay()) - 1;
  const cells: (string | null)[] = Array(lead).fill(null);
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push(`${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`);
  }
  while (cells.length % 7 !== 0) cells.push(null);
  return cells;
}

/** 日期 → "8月15日 星期六"（Day Panel 标题）。 */
export function friendlyDate(date: string): string {
  const d = new Date(date + "T00:00:00");
  const names = ["日", "一", "二", "三", "四", "五", "六"];
  return `${d.getMonth() + 1}月${d.getDate()}日 星期${names[d.getDay()]}`;
}

/** 纯前端文本文件下载（Blob + a.download；不依赖 fs 插件）。 */
export function downloadTextFile(fileName: string, content: string, mime = "text/plain") {
  const blob = new Blob([content], { type: `${mime};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
