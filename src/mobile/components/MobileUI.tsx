/**
 * DEV-MOBILE-002 §7-9 · Mobile Presentation 组件库（纯 UI，零业务）。
 *
 * 汇总导出：MobilePageHeader / MobileSegmentedControl / MobileBottomSheet /
 * MobileActionSheet / MobileEmptyState / MobileIconButton（对应任务书 §7 六件）。
 * 样式见 mobile.css（.mp- 前缀）。不访问 DB / 不持有 AI Runtime / Windows 不引用。
 */
import type { ReactNode } from "react";

/** §8：标题 + 可选 subtitle + 右侧最多一个 Action。 */
export function MobilePageHeader({
  title,
  subtitle,
  action,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="mp-pageheader">
      <div className="mp-pageheader__main">
        <div className="mp-pageheader__title">{title}</div>
        {subtitle && <div className="mp-pageheader__subtitle">{subtitle}</div>}
      </div>
      {action && <div className="mp-pageheader__action">{action}</div>}
    </div>
  );
}

/** SegmentedControl：整行 Tab（计划/日历/目标）。 */
export function MobileSegmentedControl<T extends string>({
  items,
  value,
  onChange,
  ariaLabel,
}: {
  items: readonly { key: T; label: string }[];
  value: T;
  onChange: (key: T) => void;
  ariaLabel: string;
}) {
  return (
    <div className="mp-seg" role="tablist" aria-label={ariaLabel}>
      {items.map((it) => (
        <button
          key={it.key}
          role="tab"
          aria-selected={value === it.key}
          className={"mp-seg__item" + (value === it.key ? " mp-seg__item--active" : "")}
          onClick={() => onChange(it.key)}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}

/** §9 BottomSheet：遮罩 + 底部 86dvh + safe-bottom + body 滚动 + 遮罩点击关闭。 */
export function MobileBottomSheet({
  open,
  title,
  onClose,
  children,
  dismissable = true,
}: {
  open: boolean;
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  /** 破坏性确认可禁用遮罩关闭 */
  dismissable?: boolean;
}) {
  if (!open) return null;
  return (
    <>
      <div
        className="mp-sheet-backdrop"
        onClick={() => dismissable && onClose()}
        aria-hidden="true"
      />
      <div className="mp-sheet" role="dialog" aria-modal="true" aria-label={typeof title === "string" ? title : undefined}>
        <div className="mp-sheet__grab" aria-hidden="true" />
        <div className="mp-sheet__head">
          <div className="mp-sheet__title">{title}</div>
          <button className="mp-sheet__close" title="关闭" aria-label="关闭" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="mp-sheet__body">{children}</div>
      </div>
    </>
  );
}

export type MobileAction = {
  key: string;
  label: string;
  danger?: boolean;
  onSelect: () => void;
};

/** ActionSheet：一行一动作（低频管理行为）。 */
export function MobileActionSheet({
  open,
  title,
  actions,
  onClose,
}: {
  open: boolean;
  title: ReactNode;
  actions: MobileAction[];
  onClose: () => void;
}) {
  return (
    <MobileBottomSheet open={open} title={title} onClose={onClose}>
      {actions.map((a) => (
        <button
          key={a.key}
          className={"mp-action" + (a.danger ? " mp-action--danger" : "")}
          onClick={() => {
            onClose();
            a.onSelect();
          }}
        >
          {a.label}
        </button>
      ))}
    </MobileBottomSheet>
  );
}

export function MobileEmptyState({ title, sub }: { title: ReactNode; sub?: ReactNode }) {
  return (
    <div className="mp-empty">
      <div className="mp-empty__title">{title}</div>
      {sub && <div className="mp-empty__sub">{sub}</div>}
    </div>
  );
}

/** ≥44x44 图标按钮（‹ / ⋯ / +）。 */
export function MobileIconButton({
  label,
  onClick,
  ghost,
  children,
}: {
  label: string;
  onClick: () => void;
  ghost?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      className={"mp-iconbtn" + (ghost ? " mp-iconbtn--ghost" : "")}
      title={label}
      aria-label={label}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
