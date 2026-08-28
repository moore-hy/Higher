/**
 * DEV-MOBILE-004-F2 §七 · CSS 级联行为测试（computed style 计算，非 marker contains）。
 *
 * 解析真实 src/styles.css + src/mobile/mobile.css，按
 * （@media viewport 匹配 → 选择器匹配 → 特异性 → 声明顺序）计算 winning
 * declaration，断言移动端 AI 根/Composer/BottomNav 的 computed 布局属性。
 *
 * 锁定的回归正是本次真机 bug：styles.css @media(max-width:1100px) 的
 * .aipanel{position:absolute;top/right/bottom:0;z-index:200} 在手机宽度命中，
 * 若 .platform-android .aipanel--mobile 未显式覆盖 → AI 变 viewport 全屏浮层。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as url from "node:url";

function findRepoRoot(start: string): string {
  let dir = start;
  while (dir !== path.dirname(dir)) {
    if (fs.existsSync(path.join(dir, "package.json")) && fs.existsSync(path.join(dir, "src"))) return dir;
    dir = path.dirname(dir);
  }
  throw new Error("repo root not found from " + start);
}
const ROOT = findRepoRoot(path.dirname(url.fileURLToPath(import.meta.url)));

// ---------- mini CSS parser ----------
interface Rule {
  selectors: string[];       // 逗号拆分后的单选择器
  decls: Array<[string, string]>;
  specificity: number;       // class*10 + tag
  order: number;             // 声明出现顺序（后胜）
  mediaMaxWidth: number | null; // @media (max-width: Npx)；null=无条件
}

function parseSheet(css: string, startOrder: number): { rules: Rule[]; nextOrder: number } {
  const src = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: Rule[] = [];
  let order = startOrder;

  const parseBlock = (body: string, mediaMaxWidth: number | null) => {
    let i = 0;
    while (i < body.length) {
      const open = body.indexOf("{", i);
      if (open === -1) break;
      const head = body.slice(i, open).trim();
      // 找配对 '}'（支持一层嵌套 @media）
      let depth = 1, j = open + 1;
      while (j < body.length && depth > 0) {
        if (body[j] === "{") depth++;
        else if (body[j] === "}") depth--;
        j++;
      }
      const inner = body.slice(open + 1, j - 1);
      if (head.startsWith("@media")) {
        const m = head.match(/max-width:\s*(\d+(?:\.\d+)?)px/);
        parseBlock(inner, m ? Number(m[1]) : 0);
      } else if (head && !head.startsWith("@")) {
        const decls: Array<[string, string]> = [];
        for (const d of inner.split(";")) {
          const idx = d.indexOf(":");
          if (idx > 0) {
            const prop = d.slice(0, idx).trim().toLowerCase();
            const val = d.slice(idx + 1).trim();
            if (!prop) continue;
            if (prop === "inset") {
              // 简写展开（与真实级联一致：inset:auto → 四边 longhand auto，覆盖此前 top/right/bottom）
              decls.push(["top", val], ["right", val], ["bottom", val], ["left", val]);
            } else {
              decls.push([prop, val]);
            }
          }
        }
        const selectors = head.split(",").map((s) => s.trim()).filter(Boolean);
        if (selectors.length && decls.length) {
          const sel = selectors[0]; // 测试目标规则均为单选择器（无逗号）；保守取首个
          let spec = 0;
          for (const part of sel.split(/\s+/)) {
            for (const piece of part.split(/(?=\.)/)) {
              if (piece.startsWith(".")) spec += 10;
              else if (/^[a-zA-Z]/.test(piece)) spec += 1;
            }
          }
          rules.push({ selectors, decls, specificity: spec, order: order++, mediaMaxWidth });
        }
      }
      i = j;
    }
  };
  parseBlock(src, null);
  return { rules, nextOrder: order };
}

const stylesCss = fs.readFileSync(path.join(ROOT, "src/styles.css"), "utf8");
const mobileCss = fs.readFileSync(path.join(ROOT, "src/mobile/mobile.css"), "utf8");
const parsedStyles = parseSheet(stylesCss, 0);
const parsedMobile = parseSheet(mobileCss, 1_000_000); // mobile.css 在 styles.css 之后加载（同特异性后胜）
const RULES: Rule[] = [...parsedStyles.rules, ...parsedMobile.rules];

/** 级联计算：element(tag, classes) + 祖先 class 池（近似后代匹配），viewport 宽度决定 @media 生效。 */
function computedStyle(
  tag: string,
  classes: string[],
  ancestorClasses: string[],
  viewportWidth: number,
): Record<string, string> {
  const pool = new Set([...classes, ...ancestorClasses]);
  const out: Record<string, string> = {};
  const matched = RULES
    .filter((r) => r.mediaMaxWidth === null || r.mediaMaxWidth >= viewportWidth)
    .filter((r) =>
      r.selectors.some((sel) =>
        sel.split(/\s+/).every((part) => {
          const pieces = part.split(/(?=\.)/).filter(Boolean);
          return pieces.every((p) =>
            p.startsWith(".") ? pool.has(p.slice(1)) : p.toLowerCase() === tag.toLowerCase(),
          );
        }),
      ),
    )
    .sort((a, b) => a.specificity - b.specificity || a.order - b.order);
  for (const r of matched) for (const [prop, val] of r.decls) out[prop] = val;
  return out;
}

const ANDROID = ["platform-android"];

// ---------- F2-TC003：AI root computed style 非 viewport 全屏浮层 ----------
test("F2-TC003 .aipanel--mobile computed position=static / z-index=auto / 无 inset / 无 box-shadow", () => {
  for (const w of [360, 390, 412, 430]) {
    const cs = computedStyle("aside", ["aipanel", "aipanel--mobile"], ANDROID, w);
    assert.equal(cs["position"], "static", `viewport ${w}：必须覆盖桌面响应式 absolute`);
    assert.equal(cs["z-index"], "auto", `viewport ${w}：取消 200 浮层层级`);
    assert.equal(cs["box-shadow"], "none", `viewport ${w}：去浮层投影`);
    assert.ok(
      ["top", "right", "bottom"].every((k) => cs[k] === undefined || cs[k] === "auto"),
      `viewport ${w}：inset:auto 覆盖后不得再锚定 viewport（top/right/bottom ≠ 0）`,
    );
    assert.equal(cs["width"], "100%", `viewport ${w}`);
    assert.equal(cs["height"], "100%", `viewport ${w}：占满 .ai-slot，而非 100vh`);
    assert.ok(!Object.values(cs).some((v) => v.includes("100vh") || v.includes("100dvh")),
      `viewport ${w}：不得使用 viewport 单位高度`);
  }
});

test("F2-TC003r 桌面响应式规则仍在（裸 .aipanel 窄屏仍 fixed Drawer —— Windows 行为不变）", () => {
  // 真机元凶 = 1279px 断点 fixed Drawer（后声明，覆盖 1100px 断点的 absolute）。
  // 桌面规则必须原样保留；mobile 侧由 .aipanel--mobile 的 static 覆盖接管。
  for (const w of [360, 390, 900, 1000, 1200]) {
    const cs = computedStyle("aside", ["aipanel"], [], w);
    assert.equal(cs["position"], "fixed", `viewport ${w}：桌面 @media(max-width:1279px) Drawer 规则必须保留`);
  }
});

// ---------- 高度链：ai-slot / mobile-main--ai ----------
test("F2 高度链 .ai-slot--visible(flex:1,min-height:0) 与 .mobile-main--ai(flex column, overflow hidden)", () => {
  const slot = computedStyle("div", ["ai-slot--visible"], [], 390);
  assert.equal(slot["display"], "flex");
  assert.equal(slot["flex"], "1");
  assert.equal(slot["min-height"], "0");

  const main = computedStyle("main", ["mobile-main", "mobile-main--ai"], ANDROID, 390);
  assert.equal(main["overflow"], "hidden");
  assert.equal(main["display"], "flex");
  assert.equal(main["flex-direction"], "column");
});

// ---------- F2-TC005（级联半边）：BottomNav / Composer / Subtitle 非 fixed 覆盖 ----------
test("F2-TC005c BottomNav 无 fixed/z-index 覆盖；composer 与 subtitle 无 fixed/inset", () => {
  const nav = computedStyle("nav", ["mobile-bottomnav"], ANDROID, 390);
  assert.notEqual(nav["position"], "fixed");
  assert.notEqual(nav["position"], "absolute");
  assert.ok(nav["z-index"] === undefined || nav["z-index"] === "auto", "BottomNav 不应有浮层层级");
  assert.ok((nav["padding-bottom"] ?? "").includes("env(safe-area-inset-bottom)"), "手势条避让仍在");

  const composer = computedStyle("div", ["aipanel__inputbar"], ANDROID, 390);
  assert.notEqual(composer["position"], "fixed", "composer 不得 fixed 覆盖 BottomNav");
  assert.notEqual(composer["position"], "absolute");
  assert.ok(composer["top"] === undefined && composer["bottom"] === undefined, "composer 不得声明 top/bottom 锚点");

  const subtitle = computedStyle("div", ["aipanel__mobile-subtitle"], ANDROID, 390);
  assert.notEqual(subtitle["position"], "fixed");
  assert.notEqual(subtitle["position"], "absolute");
  assert.ok(subtitle["top"] === undefined, "subtitle 不得自定顶部锚点（safe-area 由根承担）");
});

// ---------- F2-TC006：四分辨率横向不溢出契约 ----------
test("F2-TC006 360/390/412/430 无横向溢出契约（main clip + AI 100% + input max-width）", () => {
  for (const w of [360, 390, 412, 430]) {
    const main = computedStyle("main", ["mobile-main"], ANDROID, w);
    assert.equal(main["overflow-x"], "clip", `viewport ${w}`);
    assert.equal(main["min-width"], "0", `viewport ${w}`);

    const ai = computedStyle("aside", ["aipanel", "aipanel--mobile"], ANDROID, w);
    assert.equal(ai["width"], "100%", `viewport ${w}`);
    assert.equal(ai["min-width"], "0", `viewport ${w}`);

    const input = computedStyle("input", [], ANDROID, w);
    assert.equal(input["max-width"], "100%", `viewport ${w}`);

    const img = computedStyle("img", [], ANDROID, w);
    assert.equal(img["max-width"], "100%", `viewport ${w}`);
  }
});

// ---------- Safe Area 根统一（§四）----------
test("F2 §四 safe-area 由 .mobile-layout 根统一承担（AI 复用，不自建）", () => {
  const layout = computedStyle("div", ["mobile-layout"], [], 390);
  assert.ok((layout["padding-top"] ?? "").includes("env(safe-area-inset-top)"), "根 padding-top");
  assert.equal(layout["padding-bottom"], "0", "底部交由 BottomNav");
});
