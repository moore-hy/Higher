// =============== DEV-SYNC-002 §九/§十六 · Sync Refresh Dispatcher ===============
/**
 * sync://completed 事件 → 业务页面重读数据 的统一分发器。
 *
 * 纯函数模块（无 React / Tauri 依赖）：
 * - 后端在每次 Remote Apply 有变化后广播 sync://completed（payload 见 shouldRefresh）
 * - 各业务页面注册关心的实体类型；dispatch 只触发对应 handler
 * - 单元测试：tests/sync/syncRefresh.test.ts（SYNC2-UI-TC01/TC02 的可测内核）
 */

export interface SyncCompletedPayload {
  peer_device_id: string;
  profiles_changed: number;
  goals_changed: number;
  learning_items_changed: number;
  tasks_changed: number;
  conflicts: number;
  timestamp: string;
}

export type SyncEntityKind = "profiles" | "goals" | "learningItems" | "tasks";

/** payload 中发生变化的实体类型（计数 > 0）。 */
export function changedKinds(p: SyncCompletedPayload): SyncEntityKind[] {
  const kinds: SyncEntityKind[] = [];
  if (p.profiles_changed > 0) kinds.push("profiles");
  if (p.goals_changed > 0) kinds.push("goals");
  if (p.learning_items_changed > 0) kinds.push("learningItems");
  if (p.tasks_changed > 0) kinds.push("tasks");
  return kinds;
}

export function shouldRefresh(p: SyncCompletedPayload, kind: SyncEntityKind): boolean {
  return changedKinds(p).includes(kind);
}

export interface SyncRefreshHandlers {
  profiles?: () => void;
  goals?: () => void;
  learningItems?: () => void;
  tasks?: () => void;
}

/**
 * 构造分发器：收到 sync://completed 时，只调用「发生变化且已注册」的 handler。
 * 返回值为实际触发的 handler 数（便于断言/调试）。
 */
export function createSyncRefreshDispatcher(handlers: SyncRefreshHandlers) {
  return (p: SyncCompletedPayload): number => {
    let fired = 0;
    const kinds = changedKinds(p);
    if (kinds.includes("profiles") && handlers.profiles) {
      handlers.profiles();
      fired += 1;
    }
    if (kinds.includes("goals") && handlers.goals) {
      handlers.goals();
      fired += 1;
    }
    if (kinds.includes("learningItems") && handlers.learningItems) {
      handlers.learningItems();
      fired += 1;
    }
    if (kinds.includes("tasks") && handlers.tasks) {
      handlers.tasks();
      fired += 1;
    }
    return fired;
  };
}
