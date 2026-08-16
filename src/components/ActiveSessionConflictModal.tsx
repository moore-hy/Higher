import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { deleteSession, endSession } from "../api";
import type { ActiveSessionBrief } from "../types";
import { formatDateTime } from "../utils";

/**
 * Start Guard 冲突弹窗（DEV-0054 PHASE F §28-§30）。
 *
 * 后端在已有 Active Session 时返回 Err 字符串：`ActiveSessionConflict:{json}`，
 * json = { message, multiple, sessions: [{id,title,started_at,learning_item_id,task_id}] }。
 * - 单条（§28）：`你已有一项学习正在进行。` + 继续当前学习 / 结束当前学习 / 取消
 * - 多条（§30）：`检测到历史测试数据中存在多条进行中的学习记录。` + 列表（打开/结束/删除）+ 取消
 */

const CONFLICT_PREFIX = "ActiveSessionConflict:";

export interface ActiveSessionConflictData {
  message: string;
  multiple: boolean;
  sessions: ActiveSessionBrief[];
}

/** 解析 Err 字符串中的 ActiveSessionConflict 负载；非该格式返回 null。 */
export function parseActiveSessionConflict(err: unknown): ActiveSessionConflictData | null {
  const s = String(err);
  const idx = s.indexOf(CONFLICT_PREFIX);
  if (idx < 0) return null;
  try {
    const data = JSON.parse(s.slice(idx + CONFLICT_PREFIX.length)) as ActiveSessionConflictData;
    if (!Array.isArray(data.sessions) || data.sessions.length === 0) return null;
    return data;
  } catch {
    return null;
  }
}

/**
 * 共享 Start Guard hook：catch 后调用 `guard(e)`；
 * 返回 true = 已识别为冲突并弹窗（调用方不再走普通 setError），false = 普通错误。
 */
export function useActiveSessionConflict() {
  const [conflict, setConflict] = useState<ActiveSessionConflictData | null>(null);

  const guard = useCallback((err: unknown): boolean => {
    const data = parseActiveSessionConflict(err);
    if (!data) return false;
    setConflict(data);
    return true;
  }, []);

  const close = useCallback(() => setConflict(null), []);

  return { conflict, guard, close };
}

export default function ActiveSessionConflictModal({
  conflict,
  onClose,
  onResolved,
}: {
  conflict: ActiveSessionConflictData | null;
  onClose: () => void;
  /** 结束/删除成功后的数据刷新 */
  onResolved?: () => void | Promise<void>;
}) {
  const navigate = useNavigate();
  const [busyId, setBusyId] = useState<number | null>(null);
  /** 可变副本：结束/删除一条后即时更新剩余列表（归零则关闭） */
  const [sessions, setSessions] = useState<ActiveSessionBrief[]>([]);

  useEffect(() => {
    setSessions(conflict ? conflict.sessions : []);
  }, [conflict]);

  if (!conflict || sessions.length === 0) return null;

  async function endOne(id: number) {
    setBusyId(id);
    try {
      await endSession(id);
      const rest = sessions.filter((s) => s.id !== id);
      setSessions(rest);
      if (rest.length === 0) onClose();
      await onResolved?.();
    } catch {
      /* 失败保留弹窗，用户可重试或取消 */
    } finally {
      setBusyId(null);
    }
  }

  async function deleteOne(id: number) {
    setBusyId(id);
    try {
      await deleteSession(id);
      const rest = sessions.filter((s) => s.id !== id);
      setSessions(rest);
      if (rest.length === 0) onClose();
      await onResolved?.();
    } catch {
      /* 失败保留弹窗 */
    } finally {
      setBusyId(null);
    }
  }

  const single = sessions.length === 1 ? sessions[0] : null;

  return (
    <div className="modal-overlay" onClick={() => busyId == null && onClose()}>
      <div className="modal modal--quick" onClick={(e) => e.stopPropagation()}>
        {single ? (
          <>
            <div className="modal__title">你已有一项学习正在进行。</div>
            <p className="evmodal__note">
              正在进行：<b>{single.title || `学习记录 #${single.id}`}</b>
              <br />
              开始于 {formatDateTime(single.started_at)}。同一时间只保留一项学习，可以先继续或结束它。
            </p>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                onClick={() => {
                  onClose();
                  navigate(`/learn/${single.id}`);
                }}
              >
                继续当前学习
              </button>
              <button className="btn" disabled={busyId != null} onClick={() => void endOne(single.id)}>
                {busyId === single.id ? "结束中…" : "结束当前学习"}
              </button>
              <button className="btn btn--ghost" onClick={onClose}>
                取消
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="modal__title">检测到历史测试数据中存在多条进行中的学习记录。</div>
            <p className="evmodal__note muted">
              这通常是历史测试数据造成的。可以逐条打开确认、结束归档，或直接删除测试记录。
            </p>
            <ul className="ascm__list">
              {sessions.map((s) => (
                <li key={s.id} className="ascm__row">
                  <div className="ascm__row-main">
                    <span className="ascm__row-title">{s.title || `学习记录 #${s.id}`}</span>
                    <span className="ascm__row-time">开始于 {formatDateTime(s.started_at)}</span>
                  </div>
                  <div className="ascm__row-actions">
                    <button
                      className="btn btn--small"
                      onClick={() => {
                        onClose();
                        navigate(`/learn/${s.id}`);
                      }}
                    >
                      打开
                    </button>
                    <button
                      className="btn btn--small"
                      disabled={busyId != null}
                      onClick={() => void endOne(s.id)}
                    >
                      {busyId === s.id ? "结束中…" : "结束"}
                    </button>
                    <button
                      className="btn btn--small taskmenu__danger"
                      disabled={busyId != null}
                      onClick={() => void deleteOne(s.id)}
                    >
                      删除
                    </button>
                  </div>
                </li>
              ))}
            </ul>
            <div className="modal__actions">
              <button className="btn" onClick={onClose} disabled={busyId != null}>
                取消
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
