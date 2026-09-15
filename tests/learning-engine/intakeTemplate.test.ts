import { describe, expect, it } from "vitest";
import {
  buildIntakeTemplate,
  computeCompleteness,
  INTAKE_SECTIONS,
  INTAKE_TEMPLATE_FILE_NAME,
  parseIntakeTemplate,
} from "../../src/planning/intakeTemplate";

/**
 * PRODUCT-2.0 §24.2 —— 规划任务书模板与确定性解析。
 *
 * 断言：模板结构与 §24.2 逐段一致；解析宽容（不抛异常）；
 * 完成度统计确定性；无法识别的段归入自由文本，绝不丢内容。
 */

describe("§24.2 模板结构", () => {
  it("文件名与段结构冻结", () => {
    expect(INTAKE_TEMPLATE_FILE_NAME).toBe("Higher-个人规划任务书.md");
    expect(INTAKE_SECTIONS.map((s) => s.title)).toEqual([
      "1 我的目标",
      "2 当前情况",
      "3 可投入时间",
      "4 学习内容",
      "5 学习偏好",
      "6 现实约束",
      "7 外部目标信息",
      "8 强度偏好",
      "9 其它",
    ]);
  });

  it("§24.2 关键字段全部在模板里", () => {
    const md = buildIntakeTemplate();
    for (const f of [
      "目标是什么：",
      "为什么：",
      "目标日期：",
      "成功标准：",
      "当前水平：",
      "最大短板：",
      "工作日：",
      "周末：",
      "每天上限：",
      "科目/技能：",
      "更喜欢：",
      "工作/学校：",
      "希望 AI 帮我查什么：",
      "保守 / 正常 / 冲刺：",
    ]) {
      expect(md).toContain(f);
    }
  });

  it("模板可导出可回读（round-trip 不丢字段）", () => {
    const structured = parseIntakeTemplate(buildIntakeTemplate());
    const c = computeCompleteness(structured);
    expect(c.total).toBe(30);
    expect(c.filled).toBe(0);
    expect(c.missing).toHaveLength(c.total);
  });
});

describe("§24.2 解析 — 宽容且确定性", () => {
  it("解析 `标签：值`（全角冒号）", () => {
    const s = parseIntakeTemplate("# 1 我的目标\n目标是什么：通过英语四级\n为什么：毕业要求\n");
    expect(s.sections["1 我的目标"]["目标是什么"]).toBe("通过英语四级");
    expect(s.sections["1 我的目标"]["为什么"]).toBe("毕业要求");
  });

  it("解析半角冒号与多余空白", () => {
    const s = parseIntakeTemplate("## 1 我的目标\n  目标是什么 :  考研上岸  \n");
    expect(s.sections["1 我的目标"]["目标是什么"]).toBe("考研上岸");
  });

  it("无 `#` 标题时也能按字段标签归属到正确段", () => {
    const s = parseIntakeTemplate("目标是什么：减肥 10 公斤\n");
    expect(s.sections["1 我的目标"]["目标是什么"]).toBe("减肥 10 公斤");
  });

  it("无法识别的段/行进入自由文本，不丢失内容", () => {
    const s = parseIntakeTemplate("# 我的备忘录\n随便写的备注\n");
    expect(s.freeform).toContain("随便写的备注");
  });

  it("空输入不抛异常", () => {
    expect(() => parseIntakeTemplate("")).not.toThrow();
    const s = parseIntakeTemplate("");
    expect(computeCompleteness(s).filled).toBe(0);
  });

  it("同标签多次出现 → 保持确定性（后者覆盖，顺序无关）", () => {
    const a = parseIntakeTemplate("目标是什么：A\n# 1 我的目标\n目标是什么：B\n");
    expect(a.sections["1 我的目标"]["目标是什么"]).toBe("B");
  });

  it("同一输入重复解析结果完全一致（无时间/随机依赖）", () => {
    const raw = "# 3 可投入时间\n工作日：2 小时\n周末：5 小时\n每天上限：5 小时\n";
    const r1 = computeCompleteness(parseIntakeTemplate(raw));
    const r2 = computeCompleteness(parseIntakeTemplate(raw));
    expect(r1).toEqual(r2);
    expect(r1.filled).toBe(3);
    expect(r1.missing).not.toContain("工作日");
  });
});

describe("完成度统计", () => {
  it("filled/total/ratio/missing 一致", () => {
    const s = parseIntakeTemplate(buildIntakeTemplate());
    s.sections["1 我的目标"]["目标是什么"] = "考研";
    const c = computeCompleteness(s);
    expect(c.filled).toBe(1);
    expect(c.total).toBe(30);
    expect(c.ratio).toBeCloseTo(1 / 31);
    expect(c.missing).not.toContain("目标是什么");
  });

  it("「9 其它」不参与完成度（无固定字段）", () => {
    const c = computeCompleteness(parseIntakeTemplate("# 9 其它\n想到什么写什么\n"));
    expect(c.total).toBe(30);
    expect(c.filled).toBe(0);
  });
});
