// DEV-SYNC-002 §十六 · SYNC2-UI-TC01 / SYNC2-UI-TC02
// sync refresh dispatcher 单元测试（不依赖 React/Tauri，node:test 直跑编译产物）
import test from "node:test";
import assert from "node:assert/strict";

import {
  changedKinds,
  createSyncRefreshDispatcher,
  shouldRefresh,
  type SyncCompletedPayload,
} from "../../src/sync/syncRefresh.js";

function payload(over: Partial<SyncCompletedPayload>): SyncCompletedPayload {
  return {
    peer_device_id: "peer-1",
    profiles_changed: 0,
    goals_changed: 0,
    learning_items_changed: 0,
    tasks_changed: 0,
    conflicts: 0,
    timestamp: "1787920000",
    ...over,
  };
}

// ---- SYNC2-UI-TC01：tasks_changed > 0 → Today 页面的 tasks handler 被调用 ----
test("SYNC2-UI-TC01: tasks_changed>0 触发 tasks 刷新（Today loadTasks 通道）", () => {
  let tasksReloads = 0;
  let goalsReloads = 0;
  const dispatch = createSyncRefreshDispatcher({
    tasks: () => {
      tasksReloads += 1;
    },
    goals: () => {
      goalsReloads += 1;
    },
  });

  const fired = dispatch(payload({ tasks_changed: 2 }));
  assert.equal(fired, 1);
  assert.equal(tasksReloads, 1, "Today 的任务重载必须被触发");
  assert.equal(goalsReloads, 0, "goals 未变化不得触发");
  assert.equal(shouldRefresh(payload({ tasks_changed: 1 }), "tasks"), true);
});

// ---- SYNC2-UI-TC02：profiles_changed > 0 → ProfileSelector 的 profiles handler 被调用 ----
test("SYNC2-UI-TC02: profiles_changed>0 触发 profiles 刷新（ProfileSelector 通道）", () => {
  let profileReloads = 0;
  const dispatch = createSyncRefreshDispatcher({
    profiles: () => {
      profileReloads += 1;
    },
  });

  dispatch(payload({ profiles_changed: 1, tasks_changed: 3 }));
  assert.equal(profileReloads, 1, "ProfileSelector 重读必须被触发");
  assert.equal(shouldRefresh(payload({ profiles_changed: 1 }), "profiles"), true);
  assert.equal(shouldRefresh(payload({ profiles_changed: 0 }), "profiles"), false);
});

test("零变化 payload 不触发任何 handler（Today 30s 轮询外无谓刷新）", () => {
  const calls: string[] = [];
  const dispatch = createSyncRefreshDispatcher({
    profiles: () => calls.push("profiles"),
    goals: () => calls.push("goals"),
    learningItems: () => calls.push("learningItems"),
    tasks: () => calls.push("tasks"),
  });
  const fired = dispatch(payload({}));
  assert.equal(fired, 0);
  assert.deepEqual(calls, []);
});

test("changedKinds 覆盖四类实体且互不串扰", () => {
  assert.deepEqual(changedKinds(payload({})), []);
  assert.deepEqual(changedKinds(payload({ goals_changed: 1, learning_items_changed: 2 })), [
    "goals",
    "learningItems",
  ]);
  assert.deepEqual(
    changedKinds(payload({ profiles_changed: 1, goals_changed: 1, learning_items_changed: 1, tasks_changed: 1 })),
    ["profiles", "goals", "learningItems", "tasks"]
  );
});
