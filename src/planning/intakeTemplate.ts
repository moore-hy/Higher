/**
 * PRODUCT-2.0 §24.2 —— Higher 个人规划任务书模板 + 确定性解析。
 *
 * 模板为用户可编辑的 Markdown；解析只做**确定性**字段抽取与完成度统计，
 * 不调用 AI（AI 追问属于 Agent 路径，见 §24.1「和 AI 一起填写」）。
 *
 * 解析结果进入 `planning_intake_drafts.structured_json / completeness_json`，
 * 仍是 Draft（§0A.4），不构成 Formal Truth。
 */

export const INTAKE_TEMPLATE_FILE_NAME = "Higher-个人规划任务书.md";

/** 模板结构：段标题 → 该段字段标签（顺序即渲染/解析顺序）。 */
export const INTAKE_SECTIONS: { title: string; fields: string[] }[] = [
  {
    title: "1 我的目标",
    fields: ["目标是什么", "为什么", "目标日期", "成功标准"],
  },
  {
    title: "2 当前情况",
    fields: ["当前水平", "已经完成", "最大短板", "已有资源"],
  },
  {
    title: "3 可投入时间",
    fields: ["工作日", "周末", "固定不可用时间", "每天上限"],
  },
  {
    title: "4 学习内容",
    fields: ["科目/技能", "优先级", "已掌握", "薄弱", "完全没开始"],
  },
  {
    title: "5 学习偏好",
    fields: ["更喜欢", "更不喜欢", "理论耐受", "练习偏好"],
  },
  {
    title: "6 现实约束",
    fields: ["工作/学校", "健康/作息限制", "预算", "设备/地点"],
  },
  {
    title: "7 外部目标信息",
    fields: ["学校/证书/岗位/项目", "已知网址", "希望 AI 帮我查什么"],
  },
  {
    title: "8 强度偏好",
    fields: ["保守 / 正常 / 冲刺", "可接受调整幅度"],
  },
  {
    title: "9 其它",
    fields: [],
  },
];

/** 生成可导出/保存的模板 Markdown（与 §24.2 模板逐字一致）。 */
export function buildIntakeTemplate(): string {
  const lines: string[] = [];
  for (const s of INTAKE_SECTIONS) {
    lines.push(`# ${s.title}`);
    if (s.fields.length === 0) {
      lines.push("");
      continue;
    }
    for (const f of s.fields) lines.push(`${f}：`);
    lines.push("");
  }
  return lines.join("\n").trimEnd() + "\n";
}

export interface IntakeStructured {
  /** 段标题 → { 字段标签: 值 } */
  sections: Record<string, Record<string, string>>;
  /** 「9 其它」无固定字段，整段自由文本 */
  freeform: string;
}

export interface IntakeCompleteness {
  /** 有值字段数 */
  filled: number;
  /** 模板字段总数（不含「其它」自由段） */
  total: number;
  /** 完成率 0..1（total=0 时为 0） */
  ratio: number;
  /** 仍为空的字段（用于 UI 提示"还差这些"，不阻塞） */
  missing: string[];
}

/**
 * 解析任务书 Markdown → 结构化草稿。
 *
 * 宽容策略（§0B.1：不得因为格式不完美而阻塞用户）：
 * - 容忍 `#` / `##` 标题、全角/半角冒号、多余空白；
 * - 无法识别的段归入 freeform；
 * - 任何输入都不抛异常。
 */
export function parseIntakeTemplate(raw: string): IntakeStructured {
  const sections: Record<string, Record<string, string>> = {};
  const freeformLines: string[] = [];
  let currentSection: string | null = null;
  const knownFields = new Map<string, string>(); // 字段标签 → 段标题
  for (const s of INTAKE_SECTIONS) {
    sections[s.title] = {};
    for (const f of s.fields) knownFields.set(f, s.title);
  }

  const normalize = (t: string) => t.replace(/^#+\s*/, "").replace(/\s+/g, "").trim();

  for (const rawLine of raw.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line === "") continue;

    const heading = /^#{1,6}\s*(.+)$/.exec(line);
    if (heading) {
      const key = normalize(heading[1]);
      const match = INTAKE_SECTIONS.find((s) => normalize(s.title) === key);
      currentSection = match ? match.title : null;
      if (!match) freeformLines.push(line);
      continue;
    }

    // 字段行：`标签：值` / `标签: 值`
    const field = /^([^：:]{1,40})[：:]\s*(.*)$/.exec(line);
    if (field) {
      const label = field[1].trim();
      const value = field[2].trim();
      const owner = knownFields.get(label);
      if (owner) {
        sections[owner][label] = value;
        continue;
      }
    }

    if (currentSection) {
      // 段内自由行：追加到「其它」以外的段时并入首个空字段之外的自由记录
      const free = sections[currentSection]["__free__"] ?? "";
      sections[currentSection]["__free__"] = free ? `${free}\n${line}` : line;
    } else {
      freeformLines.push(line);
    }
  }

  return { sections, freeform: freeformLines.join("\n") };
}

/** 完成度统计（确定性；不阻塞用户）。 */
export function computeCompleteness(structured: IntakeStructured): IntakeCompleteness {
  let filled = 0;
  let total = 0;
  const missing: string[] = [];
  for (const s of INTAKE_SECTIONS) {
    for (const f of s.fields) {
      total += 1;
      const v = structured.sections[s.title]?.[f] ?? "";
      if (v.trim() === "") missing.push(f);
      else filled += 1;
    }
  }
  return {
    filled,
    total,
    ratio: total === 0 ? 0 : filled / total,
    missing,
  };
}
