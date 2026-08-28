import { useState, type ReactNode } from "react";
import {
  MobilePageHeader,
  MobileSegmentedControl,
  MobileBottomSheet,
  MobileActionSheet,
  MobileIconButton,
} from "../components/MobileUI";
import { initialPlanningState, switchPlanningTab, type PlanTab } from "./mobilePlanningState";

/**
 * DEV-MOBILE-002 §16-24 · MobilePlanningView（强制独立 Mobile View）。
 *
 * 纯 Presentation：接收 controller（Planning.tsx）既有的元素与回调；
 * 自身不发 API / 不建 Repository / 不复制数据逻辑（§17）。
 *
 * IA：[计划 | 日历 | 目标]（默认 计划，MOB-TC001）
 * - 计划 = NextStep + FinalGoal +「今天」入口（回答“下一步做什么”）
 * - 日历 = PlanningCalendar（Android CSS 收口 cell）；选中日期（selectedDate≠null）
 *   → 日期详情 BottomSheet（§23：不再月历下方堆 Desktop 日报；关闭=onClearDate，
 *   月历月份状态在组件内部，天然保留 → MOB-TC004）
 * - 目标 = FinalGoalCard（缺字段 CTA）+ GoalTreePanel 纵向（§20）
 * - ⋯ = 低频管理 Sheet（内嵌既有 PlanningTruthSummary，零重写 §19）
 */
export interface MobilePlanningViewProps {
  errorNode?: ReactNode;
  /** 既有 <PlanningTruthSummary/>（⋯ Sheet 内容） */
  truthSummary: ReactNode;
  /** 既有 <FinalGoalCard/> */
  finalGoal: ReactNode;
  /** 既有 <GoalTreePanel/>（纵向渲染） */
  goalTree: ReactNode;
  /** 既有 <NextStep/>（计划 Tab 主内容） */
  nextStep: ReactNode;
  /** 既有 <PlanningCalendar/> */
  calendar: ReactNode;
  /** 既有 DayReport section（原 JSX 原样搬入 Sheet） */
  dayReport: ReactNode;
  /** 选中日期（≠null → 详情 Sheet 打开） */
  selectedDate: string | null;
  onClearDate: () => void;
  onOpenToday: () => void;
  onCreateTask: () => void;
  globalModals?: ReactNode;
  legacyNote?: ReactNode;
  loading?: boolean;
}

const TAB_ITEMS: readonly { key: PlanTab; label: string }[] = [
  { key: "plan", label: "计划" },
  { key: "calendar", label: "日历" },
  { key: "goals", label: "目标" },
];

export default function MobilePlanningView(p: MobilePlanningViewProps) {
  const [state, setState] = useState(initialPlanningState);
  const [moreOpen, setMoreOpen] = useState(false);

  return (
    <div className="page page--wide mpplan">
      <MobilePageHeader
        title="学习规划"
        action={
          <MobileIconButton label="更多操作" ghost onClick={() => setMoreOpen(true)}>
            ⋯
          </MobileIconButton>
        }
      />
      <MobileSegmentedControl
        items={TAB_ITEMS}
        value={state.tab}
        onChange={(t) => setState((s) => switchPlanningTab(s, t))}
        ariaLabel="规划视图"
      />

      {p.errorNode}

      {state.tab === "plan" && (
        <>
          {p.nextStep}
          {p.finalGoal}
          <button className="btn" onClick={p.onOpenToday}>
            查看今天详情
          </button>
        </>
      )}

      {state.tab === "calendar" && (
        <>
          {p.calendar}
          <MobileBottomSheet
            open={p.selectedDate != null}
            title="日期详情"
            onClose={p.onClearDate}
          >
            {p.dayReport}
          </MobileBottomSheet>
        </>
      )}

      {state.tab === "goals" && (
        <div className="mpplan-goals">
          {p.finalGoal}
          {p.goalTree}
        </div>
      )}

      {p.legacyNote}
      {p.loading && <p className="muted">加载中…</p>}

      {/* ⋯：低频管理（新建任务 + 完整正式目标/规划面板） */}
      <MobileActionSheet
        open={moreOpen}
        title="规划管理"
        actions={[{ key: "create-task", label: "新建任务", onSelect: p.onCreateTask }]}
        onClose={() => setMoreOpen(false)}
      />
      <MobileBottomSheet open={moreOpen} title="正式目标与规划" onClose={() => setMoreOpen(false)}>
        {p.truthSummary}
      </MobileBottomSheet>

      {p.globalModals}
    </div>
  );
}
