/**
 * 原生 SVG 空心圆环（DEV-0029）：无 Chart Library。
 * - r=34 / stroke=10，圆心百分比，环下实际值；denominator=0 → 不显示 0%
 */
export function Donut({
  percent,
  center,
  sub,
  title,
}: {
  /** 0~100；null = 无数据（显示「暂无」） */
  percent: number | null;
  center: string;
  sub: string;
  title: string;
}) {
  const R = 34;
  const C = 2 * Math.PI * R;
  const clamped = percent == null ? 0 : Math.max(0, Math.min(100, percent));
  const dash = (clamped / 100) * C;

  return (
    <div className="donut">
      <div className="donut__ring">
        <svg width="88" height="88" viewBox="0 0 88 88">
          <circle cx="44" cy="44" r={R} fill="none" stroke="var(--border)" strokeWidth="10" />
          {percent != null && (
            <circle
              cx="44"
              cy="44"
              r={R}
              fill="none"
              stroke="var(--accent)"
              strokeWidth="10"
              strokeLinecap="round"
              strokeDasharray={`${dash} ${C - dash}`}
              transform="rotate(-90 44 44)"
            />
          )}
          <text x="44" y="42" textAnchor="middle" fontSize="16" fontWeight="700" fill="var(--fg)">
            {percent == null ? "—" : center}
          </text>
          {sub && (
            <text x="44" y="58" textAnchor="middle" fontSize="9.5" fill="var(--fg-muted)">
              {percent == null ? "" : sub}
            </text>
          )}
        </svg>
      </div>
      <div className="donut__title">{title}</div>
      <div className="donut__sub">{percent == null ? "暂无数据" : sub}</div>
    </div>
  );
}
