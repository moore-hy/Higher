/**
 * COGNITIVE CORE V1.2 §24 —— Cognitive Orb。
 *
 * 纯 DOM / CSS / SVG 的存在感与品牌元素（§23：**禁止** WebGL / canvas）。
 * - 中心文案（锁定）：Higher / 理解你 · 陪伴你 / 让学习成为一种更自由的生活方式
 * - V1 **不需要点击**：它是 presence/brand，不是隐藏的彩蛋按钮。
 * - 动效只用 transform / opacity，周期 16–24s；`prefers-reduced-motion` 下全部静止。
 * - 不承载任何数据，因此不可能产生假数据。
 */
export default function CognitiveOrb() {
  return (
    <div className="hc-orb">
      {/* 品牌刻印：读屏可读；图形本身 aria-hidden（避免重复朗读） */}
      <p className="hc-orb__sr">{`Higher —— 理解你 · 陪伴你 · 让学习成为一种更自由的生活方式`}</p>

      <svg
        className="hc-orb__svg"
        viewBox="0 0 240 240"
        aria-hidden="true"
        focusable="false"
      >
        <defs>
          <radialGradient id="hcOrbCore" cx="50%" cy="46%" r="54%">
            <stop offset="0%" stopColor="rgba(129, 239, 210, 0.42)" />
            <stop offset="52%" stopColor="rgba(120, 227, 255, 0.16)" />
            <stop offset="100%" stopColor="rgba(120, 227, 255, 0)" />
          </radialGradient>
          <linearGradient id="hcOrbRim" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="rgba(120, 227, 255, 0.78)" />
            <stop offset="60%" stopColor="rgba(151, 214, 255, 0.22)" />
            <stop offset="100%" stopColor="rgba(129, 239, 210, 0.34)" />
          </linearGradient>
        </defs>

        <circle cx="120" cy="120" r="108" fill="url(#hcOrbCore)" />

        {/* 环 1：主环 + 一颗游标 */}
        <g className="hc-orb__layer hc-orb__layer--a">
          <circle
            cx="120"
            cy="120"
            r="88"
            fill="none"
            stroke="url(#hcOrbRim)"
            strokeWidth="1"
          />
          <circle cx="120" cy="32" r="2.6" fill="#78e3ff" />
        </g>

        {/* 环 2：内虚线环 */}
        <g className="hc-orb__layer hc-orb__layer--b">
          <circle
            cx="120"
            cy="120"
            r="70"
            fill="none"
            stroke="rgba(151, 214, 255, 0.2)"
            strokeWidth="1"
            strokeDasharray="3 9"
          />
        </g>

        {/* 环 3：倾斜椭圆 */}
        <g className="hc-orb__layer hc-orb__layer--c">
          <ellipse
            cx="120"
            cy="120"
            rx="98"
            ry="36"
            fill="none"
            stroke="rgba(129, 239, 210, 0.18)"
            strokeWidth="1"
          />
        </g>

        <circle
          className="hc-orb__core"
          cx="120"
          cy="120"
          r="27"
          fill="rgba(8, 20, 31, 0.86)"
          stroke="rgba(151, 230, 255, 0.42)"
          strokeWidth="1"
        />
      </svg>

      <div className="hc-orb__copy" aria-hidden="true">
        <div className="hc-orb__brand">Higher</div>
        <div className="hc-orb__tagline">理解你 · 陪伴你</div>
        <div className="hc-orb__promise">让学习成为一种更自由的生活方式</div>
      </div>
    </div>
  );
}
