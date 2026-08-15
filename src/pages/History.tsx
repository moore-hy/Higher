import { useCallback, useEffect, useState } from "react";
import { listLearningItems, listRecentSessionsByProfile } from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { LearningItem, StudySession } from "../types";
import { formatDateTime, formatDuration } from "../utils";

function History() {
  const { activeProfile, refreshKey } = useActiveProfile();
  const [sessions, setSessions] = useState<StudySession[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [sessList, itemList] = await Promise.all([
        listRecentSessionsByProfile(activeProfile!.id, 100),
        listLearningItems(),
      ]);
      setSessions(sessList);
      setItems(itemList);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const itemName = (id: number | null) =>
    id == null ? "自由学习（未关联知识）" : items.find((i) => i.id === id)?.name ?? `#${id}`;

  // 简单统计
  const completed = sessions.filter((s) => s.status === "completed");
  const totalSeconds = completed.reduce(
    (acc, s) => acc + (s.duration_seconds ?? 0),
    0
  );

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">学习历史</h1>
        <p className="page__subtitle">最近的学习记录与累计统计</p>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      <section className="card">
        <h2 className="card__title">累计统计</h2>
        <div className="stats-row">
          <div className="stat">
            <span className="stat__label">总 Session 数</span>
            <span className="stat__value">{sessions.length}</span>
          </div>
          <div className="stat">
            <span className="stat__label">已完成</span>
            <span className="stat__value">{completed.length}</span>
          </div>
          <div className="stat">
            <span className="stat__label">累计学习时长</span>
            <span className="stat__value">{formatDuration(totalSeconds)}</span>
          </div>
        </div>
      </section>

      <section className="card">
        <h2 className="card__title">最近 100 条 Session</h2>
        {loading ? (
          <p className="muted">加载中…</p>
        ) : sessions.length === 0 ? (
          <p className="muted">还没有学习记录。</p>
        ) : (
          <ul className="session-list">
            {sessions.map((s) => (
              <li key={s.id} className="session-list__item">
                <div className="session-list__head">
                  <span className="session-list__item-name">
                    {itemName(s.learning_item_id)}
                  </span>
                  <span className={"badge " + (s.status === "active" ? "badge--active" : "badge--done")}>
                    {s.status === "active" ? "进行中" : formatDuration(s.duration_seconds)}
                  </span>
                </div>
                <div className="session-list__time">
                  {formatDateTime(s.started_at)} → {formatDateTime(s.ended_at)}
                </div>
                {s.note && <div className="session-list__note">备注：{s.note}</div>}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

export default History;
