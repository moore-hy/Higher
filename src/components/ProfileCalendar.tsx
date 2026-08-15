import { useCallback, useEffect, useMemo, useState } from "react";
import { getProfileCalendar } from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { ProfileCalendarDay } from "../types";
import { formatDuration } from "../utils";

/**
 * 档案学习日历（轻量月历）。
 *
 * - 数据由真实学习数据自动聚合（Session / Task / Evaluation），不要求用户手动打卡
 * - 颜色深浅只表示活动量，不代表学习效果
 * - 点击日期显示当日详情
 */

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

/** 返回某年某月的总天数。 */
function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

/** 返回某年某月 1 号是星期几（0=周一 … 6=周日）。 */
function firstWeekdayOfMonth(year: number, month: number): number {
  const d = new Date(Date.UTC(year, month - 1, 1)).getUTCDay(); // 0=周日
  return (d + 6) % 7;
}

/** 活动强度：仅按学习时长粗略分档（颜色深 ≠ 学习效果好）。 */
function activityLevel(day: ProfileCalendarDay | undefined): 0 | 1 | 2 | 3 {
  if (!day || day.study_seconds <= 0) return 0;
  const hours = day.study_seconds / 3600;
  if (hours < 1) return 1;
  if (hours < 3) return 2;
  return 3;
}

function toLocalDateStr(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export default function ProfileCalendar() {
  const { activeProfile, refreshKey } = useActiveProfile();
  const now = new Date();
  const [year, setYear] = useState(now.getFullYear());
  const [month, setMonth] = useState(now.getMonth() + 1);
  const [days, setDays] = useState<ProfileCalendarDay[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    if (!activeProfile) return;
    try {
      setError("");
      const data = await getProfileCalendar(activeProfile.id, year, month);
      setDays(data);
    } catch (e) {
      setError(String(e));
    }
  }, [activeProfile, year, month, refreshKey]);

  useEffect(() => {
    load();
  }, [load]);

  const dayMap = useMemo(() => {
    const map = new Map<string, ProfileCalendarDay>();
    for (const d of days) map.set(d.date, d);
    return map;
  }, [days]);

  const todayStr = toLocalDateStr(new Date());

  // 组装月历格子（前置空白 + 日期）
  const cells: (string | null)[] = useMemo(() => {
    const lead = firstWeekdayOfMonth(year, month);
    const total = daysInMonth(year, month);
    const arr: (string | null)[] = Array.from({ length: lead }, () => null);
    for (let d = 1; d <= total; d++) {
      arr.push(`${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`);
    }
    return arr;
  }, [year, month]);

  const selectedDay = selected ? dayMap.get(selected) : undefined;

  function shiftMonth(delta: number) {
    let y = year;
    let m = month + delta;
    if (m < 1) {
      m = 12;
      y -= 1;
    } else if (m > 12) {
      m = 1;
      y += 1;
    }
    setYear(y);
    setMonth(m);
    setSelected(null);
  }

  return (
    <section className="card">
      <div className="calendar__header">
        <h2 className="card__title">本月学习</h2>
        <div className="calendar__nav">
          <button className="btn btn--small" onClick={() => shiftMonth(-1)}>←</button>
          <span className="calendar__month-label">{year}年{month}月</span>
          <button className="btn btn--small" onClick={() => shiftMonth(1)}>→</button>
        </div>
      </div>
      {error && <div className="alert alert--error">{error}</div>}
      <div className="calendar__grid calendar__grid--head">
        {WEEKDAYS.map((w) => (
          <div key={w} className="calendar__weekday">{w}</div>
        ))}
      </div>
      <div className="calendar__grid">
        {cells.map((date, idx) => {
          if (date === null) return <div key={`blank-${idx}`} className="calendar__cell calendar__cell--blank" />;
          const day = dayMap.get(date);
          const level = activityLevel(day);
          const isToday = date === todayStr;
          const isSelected = date === selected;
          const dayNum = Number(date.slice(8));
          return (
            <button
              key={date}
              className={
                "calendar__cell calendar__cell--day" +
                (isToday ? " calendar__cell--today" : "") +
                (isSelected ? " calendar__cell--selected" : "") +
                (level ? ` calendar__cell--level${level}` : "")
              }
              onClick={() => setSelected(date)}
              title={date}
            >
              <span className="calendar__day-num">{dayNum}</span>
              {day && (
                <span className="calendar__day-summary">
                  {day.study_seconds > 0 && formatDuration(day.study_seconds)}
                  {day.task_count > 0 && (
                    <span className="calendar__day-line">
                      {day.completed_task_count}/{day.task_count} 任务
                    </span>
                  )}
                  {day.evaluation_count > 0 && (
                    <span className="calendar__day-line">{day.evaluation_count} 次验证</span>
                  )}
                </span>
              )}
            </button>
          );
        })}
      </div>
      {selected && (
        <div className="calendar__detail">
          <div className="calendar__detail-title">
            {Number(selected.slice(5, 7))}月{Number(selected.slice(8))}日
          </div>
          {selectedDay ? (
            <div className="calendar__detail-stats">
              <div className="stat">
                <span className="stat__label">学习时间</span>
                <span className="stat__value">{formatDuration(selectedDay.study_seconds)}</span>
              </div>
              <div className="stat">
                <span className="stat__label">完成任务</span>
                <span className="stat__value">{selectedDay.completed_task_count} / {selectedDay.task_count}</span>
              </div>
              <div className="stat">
                <span className="stat__label">Session</span>
                <span className="stat__value">{selectedDay.session_count}</span>
              </div>
              <div className="stat">
                <span className="stat__label">验证</span>
                <span className="stat__value">{selectedDay.evaluation_count} 次</span>
              </div>
            </div>
          ) : (
            <p className="muted">这一天没有学习活动。</p>
          )}
        </div>
      )}
    </section>
  );
}
