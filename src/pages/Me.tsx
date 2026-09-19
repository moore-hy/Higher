/**
 * A2-3 §25 —— ME 页面（最小可见产品面）。
 *
 * 刻意**不**重做导航体系：这只是把 MePanel 挂到 `/me` 上。
 */

import MePanel from "../components/me/MePanel";
import { useActiveProfile } from "../contexts/ActiveProfileContext";

export default function Me() {
  const { activeProfile } = useActiveProfile();
  const profileId = activeProfile?.id ?? null;

  if (!profileId) {
    return <p className="me__loading">还没有选择档案。</p>;
  }
  return <MePanel profileId={profileId} />;
}
