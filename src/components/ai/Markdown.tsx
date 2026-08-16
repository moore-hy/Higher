import { memo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * AI 回答 Markdown 渲染（DEV-0054 PHASE O §83-§90）。
 *
 * - react-markdown + remark-gfm（轻量白名单；§84 禁 rehypeRaw —— 不启用 raw HTML）
 * - heading 统一 15-16px 加粗（§86 禁巨大 heading；样式见 styles.css `.md h1/h2/h3`）
 * - table 外包 `.md__tablewrap` 横向滚动（§85）
 * - code（非 inline）block 渲染；a 新窗口打开
 * - [[S1]] 引用：调用方先把 `[[S1]]` 预处理为 `[1](#cite-S1)` 链接语法，
 *   这里在 `a` 覆写中拦截 `#cite-` 前缀 → 渲染为可点击上标 [1]（§47）。
 */

/** 引用信息（无来源/不可点击时传 null 仍显示纯上标） */
export interface MdCitation {
  title?: string | null;
  onClick?: () => void;
}

const CITE_HREF_PREFIX = "#cite-";

/** `[[S1]]` → `[1](#cite-S1)`（在传给 Markdown 前做；渲染层再还原为上标） */
export function mdCiteText(text: string): string {
  return text.replace(/\[\[(S\d+)\]\]/g, (_m, sid: string) => `[${sid.slice(1)}](${CITE_HREF_PREFIX}${sid})`);
}

function buildComponents(citeOf?: (sid: string) => MdCitation | null): Components {
  return {
    h1: ({ children }) => <h3 className="md__h">{children}</h3>,
    h2: ({ children }) => <h3 className="md__h">{children}</h3>,
    h3: ({ children }) => <h3 className="md__h md__h--sub">{children}</h3>,
    /* §85：table 外包横向滚动容器 */
    table: ({ children }) => (
      <div className="md__tablewrap">
        <table>{children}</table>
      </div>
    ),
    a: ({ href, children }) => {
      if (typeof href === "string" && href.startsWith(CITE_HREF_PREFIX)) {
        const sid = href.slice(CITE_HREF_PREFIX.length);
        const cite = citeOf?.(sid) ?? null;
        return (
          <sup
            className={"aipanel__cite" + (cite?.onClick ? "" : " aipanel__cite--plain")}
            title={cite?.title ?? undefined}
            onClick={cite?.onClick}
          >
            {children}
          </sup>
        );
      }
      return (
        <a href={href} target="_blank" rel="noreferrer">
          {children}
        </a>
      );
    },
  };
}

function Markdown({
  text,
  citeOf,
}: {
  text: string;
  /** [[Sx]] 上标的信息解析（sid → 可点击来源）；不传则全部渲染为纯上标 */
  citeOf?: (sid: string) => MdCitation | null;
}) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={buildComponents(citeOf)}>
        {mdCiteText(text)}
      </ReactMarkdown>
    </div>
  );
}

export default memo(Markdown);
