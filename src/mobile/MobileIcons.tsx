/**
 * DEV-MOBILE-001 F1 §十二 · Mobile Nav Icons。
 *
 * 简洁 inline SVG：Calendar / Target / Book / Sparkles / Person。
 * 线条统一 stroke=currentColor，1.7–2px；Apple-like 安静风格，
 * 融入 Higher 深色设计体系。禁止 emoji 导航图标。
 */

type IconProps = {
  size?: number;
  className?: string;
};

function base(size: number, className?: string) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    focusable: false as const,
    className,
  };
}

/** 今日 · Calendar */
export function IconCalendar({ size = 24, className }: IconProps) {
  return (
    <svg {...base(size, className)}>
      <rect x="3.5" y="5" width="17" height="15.5" rx="2.5" />
      <path d="M3.5 9.5h17" />
      <path d="M8 3v4M16 3v4" />
    </svg>
  );
}

/** 规划 · Target */
export function IconTarget({ size = 24, className }: IconProps) {
  return (
    <svg {...base(size, className)}>
      <circle cx="12" cy="12" r="8.5" />
      <circle cx="12" cy="12" r="4.5" />
      <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** 知识 · Book */
export function IconBook({ size = 24, className }: IconProps) {
  return (
    <svg {...base(size, className)}>
      <path d="M4 5.5A2 2 0 0 1 6 3.5h13v17H6a2 2 0 0 1-2-2v-13Z" />
      <path d="M4 17.5h15" />
      <path d="M8.5 3.5v13" />
    </svg>
  );
}

/** AI · Sparkles */
export function IconSparkles({ size = 24, className }: IconProps) {
  return (
    <svg {...base(size, className)}>
      <path d="M12 4.5l1.6 4.1 4.1 1.6-4.1 1.6L12 15.9l-1.6-4.1L6.3 10.2l4.1-1.6L12 4.5Z" />
      <path d="M18.5 15.5l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8.8-2Z" />
    </svg>
  );
}

/** 我的 · Person */
export function IconPerson({ size = 24, className }: IconProps) {
  return (
    <svg {...base(size, className)}>
      <circle cx="12" cy="8.5" r="3.5" />
      <path d="M5 20c1.2-3.2 3.8-5 7-5s5.8 1.8 7 5" />
    </svg>
  );
}
