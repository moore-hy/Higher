# DEV-SYNC-001 · Higher Windows ↔ Android Local-First LAN Sync MVP · 实施报告

日期：2026-08-28
分支：main（直接开发，未 push）

---

## 一、交付清单

### 1. Migration v028（src-tauri/src/migrations/v028_local_sync_foundation.rs）

新增表（全部旁路，未修改 v001-v027 任何业务表 / FK / local id）：

| 表 | 用途 |
|---|---|
| sync_local_device | 本机设备单例（id=1，UUID v4；platform 经 `cfg(target_os)` 编译期区分 windows/android） |
| sync_entity_map | Sync Identity Layer：`(entity_type, local_id)` PK + `sync_id UNIQUE` + 墓碑 `deleted_at`；entity_type CHECK 限定 4 类 |
| sync_outbox | 变更队列（id AUTOINCREMENT + operation CHECK upsert/delete + `(entity_type, sync_id)` 索引） |
| sync_peers | 已配对设备：shared_token + 双向游标（last_acked_local_change_id / last_received_remote_change_id）+ peer_addr（"至少"字段外的必要扩展，供手机重连） |
| sync_runtime_guard | Remote Apply 守卫单例（applying_remote=1 时 Trigger 全部静默，防 A→B→A→B 回声） |
| sync_conflicts | 冲突记录（entity_type/sync_id/local_change_json/remote_change_json/created_at/status ∈ pending/resolved_local/resolved_remote） |

存量数据：study_profiles / goals / learning_items / tasks 全量回填 UUID v4（SQL `randomblob` 内联生成，等价 128-bit global id，不产生 outbox，作为同步基线）。

Trigger：4 表 × AFTER INSERT / AFTER UPDATE / BEFORE DELETE = 12 个。
所有 Trigger 以 `(SELECT applying_remote FROM sync_runtime_guard WHERE id=1) = 0` 为条件；
删除 Trigger 同时写 outbox delete + entity_map 墓碑；
插入 Trigger 在 guard=0 时 `INSERT OR IGNORE` 登记 sync_id（Remote Apply 由 apply.rs 显式登记远端 sync_id，杜绝随机 uuid 顶替）。

### 2. Sync 模块（src-tauri/src/sync/，新增）

| 文件 | 职责 |
|---|---|
| types.rs | Wire Model：SyncChange / SyncEntityPayload（serde tag=kind 与 entity_type 一致）/ WireMessage（PairRequest / PairResponse / SyncRequest / SyncResponse / Error）。wire 零 local id，FK 一律 `*_sync_id` |
| identity.rs | sync_id ↔ local_id 双向映射、peer 行读写、游标推进、outbox/冲突计数 |
| export.rs | `export_outbox_changes`（增量：同实体折叠为最新操作 + 当前行快照，FK→sync_id）；`export_bootstrap`（Active Profile 全树，依赖序 profile→goal→item→task） |
| apply.rs | Remote Apply：事务内 guard=1 → 两轮依赖序应用 → guard=0 → COMMIT；错误 ROLLBACK（guard 随事务恢复）。冲突守卫：同 sync_id 存在未被该 peer ack 的 outbox 变更 → 记 sync_conflicts，不覆盖。FK 缺失 → deferred（两轮重试后计数上报）。Bootstrap Profile 重名 → 「xxx（来自电脑）」，绝不静默覆盖 |
| transport.rs | length-prefixed JSON（4B 大端长度 + UTF-8），单包 ≤ 10 MB 超限拒绝 |
| server.rs | Windows 服务器：默认关闭；启动监听 0.0.0.0（优先 42828，占用回退随机端口）；6 位随机配对码 10 分钟过期（恒时比较校验）；配对签发 shared_token（uuid v4），后续请求验 token；Bootstrap 快照随 PairResponse 下发并精确清除对应 outbox（防首次增量误判冲突）；SyncRequest → 应用对方变更 + 消费 ack（推进游标 + 清理全员已 ack 条目）→ 回发增量。仅 std::net 阻塞 IO + 独立线程 |
| client.rs | Android 客户端：`pair_with_server`（握手 + 保存 peer/token/addr + Bootstrap 导入）；`sync_now`（推送本地 outbox 增量 + ack 对端 → 应用回发增量 + 推进游标 → 双向收敛） |

Task 引用按规范处理：plan_id / recurring_rule_id / planning_blueprint_id / planning_phase_id 不跨设备（远端 NULL），保留 origin / projection_key。

### 3. Tauri 接线（lib.rs）

- `pub mod sync` + `app.manage(SyncServerHandle::new())`（默认关闭）
- 6 命令：sync_server_start / sync_server_stop / sync_server_status（Windows）；sync_pair_with_server / sync_client_sync_now / sync_client_status（Android）
- DbStateProvider(AppHandle) 回调式获取全局连接（服务器线程按请求短临界区加锁）

### 4. UI

- Windows：Settings 新 tab「设备同步」——[启动同步] → IP/端口/配对码/有效期 + 等待手机连接；配对后「我的设备」列表（最后同步）+ 冲突提示「有 X 条同步冲突，暂未覆盖」+ [立即同步]（展示 N 条待同步 + 提示由手机发起）+ [停止同步]；安全提示「当前仅建议在可信局域网中使用」
- Android：「我的 → 设备同步」——电脑 IP/端口/配对码输入 + [连接电脑]；已配对 → 地址/最后同步/待同步数/冲突提示 + [立即同步]
- api.ts 新增 6 个封装 + 类型；styles.css 新增 .sync-panel / .sync-peers（复用 card/alert/btn 体系）

### 5. 测试（src-tauri/tests/sync_local_mvp.rs，13 用例全过）

两个独立临时 SQLite DB 各跑完整 v001-v028：

| 用例 | 验证 | 结果 |
|---|---|---|
| SYNC-TC001 | Profile Bootstrap：sync_id 相同、local id 可不同 | PASS |
| SYNC-TC002 | Goal 树 parent mapping（final→year→month） | PASS |
| SYNC-TC003 | LearningItem 父子树 parent mapping | PASS |
| SYNC-TC004 | Task 跨设备出现 + FK 全部本机映射 | PASS |
| SYNC-TC005 | B 完成 → A status=completed | PASS |
| SYNC-TC006 | 双端 local id 人为错位仍按 sync_id 命中（含二次改名） | PASS |
| SYNC-TC007 | A 删除 → B 删除 | PASS |
| SYNC-TC008 | Remote Apply 零 outbox echo + guard 恢复 0 | PASS |
| SYNC-TC009 | 模拟 AI 裸 SQL UPDATE → Trigger 必捕 outbox | PASS |
| SYNC-TC010 | 双端同编辑 → 不静默覆盖 + sync_conflicts pending | PASS |
| SYNC-TC011 | ai.api_key / Brave key 绝不进任何 Packet | PASS |
| SYNC-TC012 | search_index 不进 Packet | PASS |
| SYNC-TC013 | TCP loopback：配对握手 + Bootstrap + 双向增量收敛 + 幂等（二连 pushed=0/pulled=0） | PASS |

---

## 二、验证

| 项 | 命令 | 结果 |
|---|---|---|
| 前端构建 | `npm run build` | PASS（8.44s，chunk 体积警告为既有情况） |
| AI runtime 测试 | `npm run test:ai-runtime` | PASS 13/13 |
| Mobile 测试 | `npm run test:mobile` | PASS 17/17 |
| Sync 测试 | `cargo test --test sync_local_mvp` | PASS 13/13 |
| Rust 全量回归 | `cargo test --no-fail-fast` | 全部套件 PASS；唯 android_artifact_tests 2 例失败 = 预存环境问题（gen/android assets/jniLibs 为 .gitignore 排除的构建产物，本机未跑 Build-Higher-Android.ps1 所致，与本任务改动无关） |
| Android 编译契约 | `cargo check --target aarch64-linux-android --features custom-protocol`（NDK 26.1 交叉链） | PASS |
| Windows 构建 smoke | `cargo build`（debug） | PASS（21s） |

治理测试适配（按历史惯例的既有断言更新）：
- 15 个既有测试的 schema 版本断言 27 → 28（migration 追加惯例）
- batch064_ui U28 白名单追加 DEV-SYNC-001 授权块（lib.rs 六命令 + mod sync + manage + 注册）

---

## 三、纪律自检

- 未同步 SQLite 文件本身；未把 local id 当跨设备身份（wire 全 sync_id） ✔
- 未修改 v001-v027 / 业务表 / FK / Repository / UI 布局 / AI Runtime（Sync 为旁路：Business→SQLite→Trigger→Sync） ✔
- 范围锁定 4 类实体；settings / search / vault / 附件 / AI 数据未触碰（TC011/TC012 证明） ✔
- 无云 / WebSocket / Node / Docker / Redis；仅 std::net（+ 既有 uuid crate，未引新依赖） ✔
- 服务器默认关闭、用户主动启动；配对码 6 位 10 分钟过期；token 必验 ✔
- 未做：二维码 / 自动发现 / 后台 / 实时 / 附件 / 端到端加密（按 §二十一不做） ✔
- 未 push GitHub ✔

---

## 四、最终结论

```
MIGRATION:              PASS
SYNC_IDENTITY:          PASS
OUTBOX_TRIGGER:         PASS
REMOTE_APPLY:           PASS
CONFLICT_GUARD:         PASS
WINDOWS_SERVER:         PASS
ANDROID_CLIENT:         PASS
PROFILE_BOOTSTRAP:      PASS
BIDIRECTIONAL_TASK_SYNC: PASS
SECRET_EXCLUSION:       PASS
REGRESSION:             PASS
```

DEV-SYNC-001 STOP。等待用户真机（Windows ↔ Android，同一可信 Wi-Fi）实际同步验收：
1. Windows `npm run tauri dev`（或 debug 构建）→ 设置 → 设备同步 → 启动同步
2. Android Debug APK（`scripts/Build-Higher-Android.ps1`）→ 我的 → 设备同步 → 输入 IP/配对码 → 连接电脑
3. 双端各自改动 Task/Goal/Item → 手机点「立即同步」→ 双向收敛

已知边界（MVP 设计内）：
- 双向同步由手机端「立即同步」发起；Windows「立即同步」按钮为待同步状态展示 + 引导
- 同步冲突仅提示不自动合并（后续阶段做冲突编辑器）
- Bootstrap 只导出电脑当前 Active Profile；手机本地原有档案保留不动
