import { useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { getMemoryDashboard } from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { queryKeys } from "../query/keys";
import type {
  CognitiveMemoryDashboard,
  CognitiveMemoryKind,
  CognitiveMemoryRow,
  CognitiveMemoryUnitStatus,
} from "../types";
import { formatDateTime } from "../utils";

/**
 * COGNITIVE CORE V1.2 §25 —— Memory 页（第一个真正的记忆界面）。
 *
 * # 这不是 Anki
 *
 * 页面只回答一件事：**什么正在被遗忘，什么时候该回想。**
 * 因此它**没有**「掌握度百分比」、**没有**学习任务清单、**没有**评分。
 *
 * # 一个后端视图，一次 IPC
 *
 * 全部判断（压力分类、到期队列、下一次复习、理由顺序）都由后端
 * `get_memory_dashboard(profile_id, limit)` 一次给出（§25 禁止 N+1 调用）。
 * 前端只做渲染与**无损**文案格式化，绝不重算、不重排、不新增数字。
 *
 * # 空状态是结论，不是缺陷（§36）
 *
 * 一条 MemoryUnit 都没有时，唯一正确的呈现是「记忆节奏正在建立」。
 * 这里**不会**出现任何 demo 行、占位数字或示例知识点。
 */

/** §25：到期列表「初始最多 20 条」；后端还会再兜底封顶一次。 */
export const MEMORY_DUE_LIMIT = 20;

/** §13 锁定的 7 种记忆种类（展示名只是翻译，不改变种类集合）。 */
export const MEMORY_KIND_LABEL: Record<CognitiveMemoryKind, string> = {
  vocabulary: "词汇",
  definition: "定义",
  formula: "公式",
  fact: "事实",
  distinction: "辨析",
  protocol_field: "协议字段",
  short_answer: "简答",
};

/** §11 Stability 轴口径的行状态（**不是**掌握度等级）。 */
export const MEMORY_STATUS_LABEL: Record<CognitiveMemoryUnitStatus, string> = {
  new: "新加入",
  due: "到期",
  stable: "稳定",
};

/** §13 锁定的压力分类文案。`insufficient` 永远不会走到这里（它走空状态）。 */
export const MEMORY_PRESSURE_LABEL: Record<string, string> = {
  insufficient: "还没有足够记录",
  calm: "节奏平稳",
  watch: "需要留意",
  high: "压力偏高",
};

/**
 * §25「为什么现在复习」——把后端给的**机器 token** 无损转成中文。
 *
 * 返回 `null` 表示「这条理由没有可安全展示的值」，此时整条不渲染；
 * 任何无法识别的 value **一律不渲染** —— 宁可不显示，也绝不把
 * `due=3` 这类内部串抛给用户（§36）。所有数字都直接来自后端。
 */
export function formatMemoryReason(code: string, value: string | null): string | null {
  const v = (value ?? "").trim();

  switch (code) {
    case "memory_due": {
      const m = /^due=(\d+)$/.exec(v);
      if (!m) return null;
      const n = Number(m[1]);
      if (n <= 0) return null;
      return `现在有 ${n} 个知识点到了该复习的时间`;
    }
    case "memory_high_risk": {
      const m = /^high_risk=(\d+)$/.exec(v);
      if (!m) return null;
      const n = Number(m[1]);
      if (n <= 0) return null;
      return `有 ${n} 个知识点记忆正在变弱`;
    }
    case "memory_calm":
      return "现在没有到期的记忆，按当前节奏继续就好";
    default:
      return null;
  }
}

/**
 * 一行的时间信息。
 *
 * - 已逾期 → 「已逾期 N 天」（正数，来自后端 `overdue_days`）；
 * - 今天到期 → 「今天到期」；
 * - 尚未到期（`upcoming_units`）→ 直接展示**真实的** `next_review_at`。
 */
function timingLabel(row: CognitiveMemoryRow): string {
  if (row.overdue_days > 0) return `已逾期 ${row.overdue_days} 天`;
  if (row.overdue_days === 0) return "今天到期";
  return formatDateTime(row.unit.next_review_at);
}

function MemoryRowView({ row }: { row: CognitiveMemoryRow }) {
  return (
    <li className="hc-memrow" data-status={row.status}>
      <span className="hc-memrow__main">
        <span className="hc-memrow__label">
          {/* 取不到真实展示名时不回退成内部编号（编号不是给用户看的东西） */}
          {row.learning_item_label ?? "（未命名学习项）"}
        </span>
        <span className="hc-memrow__kind">{MEMORY_KIND_LABEL[row.unit.memory_kind]}</span>
      </span>
      <span className="hc-memrow__timing">{timingLabel(row)}</span>
      <span className="hc-memrow__count">复习 {row.unit.review_count} 次</span>
      <span className="hc-memrow__status">{MEMORY_STATUS_LABEL[row.status]}</span>
    </li>
  );
}

/** §25 压力摘要。只展示真实字段。 */
function MemoryPressureSummary({ dash }: { dash: CognitiveMemoryDashboard }) {
  const p = dash.pressure;
  // 有到期 → 讲「最早到期」；没有到期 → 才谈「下一次复习」。
  // 绝不把过去的时间点标成「下一次」。
  const timing =
    p.due_count > 0 && p.oldest_due_at
      ? { label: "最早到期", value: formatDateTime(p.oldest_due_at) }
      : p.next_due_at
        ? { label: "下一次复习", value: formatDateTime(p.next_due_at) }
        : null;

  return (
    <section className="hc-memsum" aria-label="记忆压力">
      <div className="hc-memsum__head">
        <span className={`hc-memsum__band hc-memsum__band--${p.status}`}>
          {MEMORY_PRESSURE_LABEL[p.status] ?? p.status}
        </span>
        <span className="hc-memsum__counts">
          共 {p.total_units} 个知识点 · 到期 {p.due_count} · 高风险 {p.high_risk_count}
        </span>
      </div>
      {timing && (
        <p className="hc-memsum__timing">
          {timing.label}：{timing.value}
        </p>
      )}
    </section>
  );
}

function Memory() {
  const { activeProfile } = useActiveProfile();
  const profileId = activeProfile?.id ?? null;

  const dashQuery = useQuery({
    queryKey: queryKeys.cognitiveMemory.view(profileId ?? -1, MEMORY_DUE_LIMIT),
    queryFn: () => getMemoryDashboard(profileId as number, MEMORY_DUE_LIMIT),
    enabled: profileId != null,
  });

  const retry = useCallback(() => {
    void dashQuery.refetch();
  }, [dashQuery]);

  const dash = dashQuery.data;
  const isEmpty = dash != null && dash.pressure.status === "insufficient";

  return (
    <div className="page page--wide hc-memory">
      <header className="page__header">
        <h1 className="page__title">Memory</h1>
        <p className="page__subtitle">什么正在被遗忘，什么时候需要回想</p>
      </header>

      {dashQuery.isLoading && (
        <p className="hc-memory__loading" role="status">
          正在读取你的记忆状态…
        </p>
      )}

      {dashQuery.isError && (
        <div className="alert alert--error" role="alert">
          <p>读取记忆状态时出错：{String(dashQuery.error)}</p>
          <button type="button" className="btn btn--ghost btn--small" onClick={retry}>
            重新尝试
          </button>
        </div>
      )}

      {/* §25 空状态：一条 MemoryUnit 都没有。文案为任务书锁定文案，不得改写。 */}
      {isEmpty && (
        <section className="hc-memory__empty" aria-label="还没有记忆记录">
          <p className="hc-memory__empty-primary">
            Higher 还没有足够的回忆记录来建立你的记忆节奏。
          </p>
          <p className="hc-memory__empty-hint">
            当你在学习中完成主动回忆后，这里会逐渐形成复习安排。
          </p>
        </section>
      )}

      {dash != null && !isEmpty && (
        <>
          <MemoryPressureSummary dash={dash} />

          <section className="hc-memlist" aria-label="现在需要复习">
            <h2 className="hc-memlist__title">现在需要复习</h2>
            {dash.due_units.length === 0 ? (
              <p className="hc-memlist__empty">现在没有需要复习的内容。</p>
            ) : (
              <ul className="hc-memlist__items">
                {dash.due_units.map((row) => (
                  <MemoryRowView key={row.unit.id} row={row} />
                ))}
              </ul>
            )}
          </section>

          <section className="hc-memlist" aria-label="下一次复习">
            <h2 className="hc-memlist__title">下一次复习</h2>
            {dash.upcoming_units.length === 0 ? (
              <p className="hc-memlist__empty">当前没有尚未到期的复习安排。</p>
            ) : (
              <ul className="hc-memlist__items">
                {dash.upcoming_units.map((row) => (
                  <MemoryRowView key={row.unit.id} row={row} />
                ))}
              </ul>
            )}
          </section>

          <section className="hc-memwhy" aria-label="为什么现在复习">
            <h2 className="hc-memwhy__title">为什么现在复习</h2>
            <ul className="hc-memwhy__list">
              {dash.rationale.map((item) => {
                const shown = formatMemoryReason(item.code, item.value);
                if (shown == null) return null;
                return (
                  <li
                    key={item.code}
                    className={`hc-memwhy__item hc-memwhy__item--${item.trend}`}
                    data-trend={item.trend}
                  >
                    {shown}
                  </li>
                );
              })}
            </ul>
          </section>
        </>
      )}
    </div>
  );
}

export default Memory;
