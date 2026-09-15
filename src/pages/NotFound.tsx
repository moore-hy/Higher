import { useLocation, useNavigate } from "react-router-dom";

/**
 * PRODUCT-2.0 §8C.2 — Route Fallback。
 *
 * 未知 route 不再静默空白/render nothing，而是给出友好提示并允许一击回到「今日」。
 * 已知的旧 deep link 仍在 App.tsx 中显式 redirect（/items → /knowledge、/review → /planning…），
 * 这里只兜住真正未识别的路径。
 */
export default function NotFound() {
  const navigate = useNavigate();
  const location = useLocation();

  return (
    <div className="not-found" data-testid="route-not-found">
      <div className="not-found__card">
        <h1 className="not-found__title">这个页面不存在</h1>
        <p className="not-found__desc">
          我们找不到 <code>{location.pathname}</code>。它可能已被移动，或链接有误。
        </p>
        <div className="not-found__actions">
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => navigate("/", { replace: true })}
            data-testid="route-not-found-today"
          >
            返回今日
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => navigate(-1)}
            data-testid="route-not-found-back"
          >
            返回上一页
          </button>
        </div>
      </div>
    </div>
  );
}
