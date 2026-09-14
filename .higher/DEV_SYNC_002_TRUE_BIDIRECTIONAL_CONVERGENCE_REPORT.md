# DEV-SYNC-002 · Higher True Bidirectional Sync — 实施报告

日期：2026-08-28 · 分支 main（未 commit / 未 push）

---

## 一、§四 审计结论（真实失败层定位，测试可复现：tests/sync2_audit.rs）

| 真机现象 | 失败层 | 证据（修复前复现） |
|---|---|---|
| ① Android 新建 Task → 推送 1 条 → Windows 不出现 | **entity map resolution**：v028 存量实体只建 sync_entity_map 不建 sync_outbox → Android 存量 Profile「2028测试」从未推送 → Task 的 profile_sync_id 在 Windows 无映射 → apply 静默 `MissingDependency(deferred)` | `audit1b: pushed=1, deferred=1, A 端 Task=0 + Profile=0` |
| ② Windows 新建 → Android「看不到」 | 数据层**成功**（落「2028考研」），但 ③ Profile 语义（非当前档案）+ 无导入去向提示 + UI 不刷新 | `audit2: pulled=1 applied=1, 任务落 2028考研, B active=2028测试` |
| ③ 待同步不归零/递增 | **ACK cursor**：pending=全量 COUNT(*) + client 端从不 trim | `audit: B COUNT 2→2（ack 推进游标但条目不删）` |
| ④ Windows「立即同步」无传输 | server 被动模型，按钮仅查状态 | 代码走读（F1 版 syncNow 实现） |

## 二、修复实现

**数据层**
- **v029_local_sync_backfill_outbox**：全部存活存量实体补一条 `upsert` outbox（含 NOT EXISTS 防重）→ 存量数据进入同步系统，配对后首次同步即全量收敛（根治 ①）
- **Bootstrap 全量 Profile**（§五）：export_bootstrap 不再只导 Active，按 sync_id 区分并存；绝不按 name 合并（TC011：A/B 各 3 档案 → 双方各 6 个）
- **Pending per-peer + trim**（§六）：`pending_outbox_count_for(peer)` + client 端成功 ack 后 `trim_acked_outbox` → 归零（TC009：1→0）
- **两端平等**（§七）：PairRequest/SyncRequest 携带 `client_listen_addr`；Android 详情页启动 listener（复用 server 命令）→ Windows 可主动反向连接；两端「立即同步」统一走 `sync_client_sync_now`
- **per-entity ApplyOutcome** + `sync://completed` 事件（§九）：server 侧（ConnProvider.emit）与 client 侧（命令层 AppHandle.emit）双端广播，payload 含 profiles/goals/items/tasks_changed + conflicts + timestamp

**UI**
- Windows：Sidebar 新增「同步」一级入口（/sync，[pages/Sync.tsx](file:///c:/Users/37653/Desktop/Higher/Higher-Windows/src/pages/Sync.tsx)：peer 卡片 ●已连接/最后同步/待发送、[立即同步]、双向新增·更新·删除明细、无变化态「两台设备数据已是最新」、冲突 bulk 保留本机/对端版本）；Settings 设备同步保留配对/启停/IP/端口/配对码 + 新增 [打开同步]
- 刷新（§九）：[syncRefresh.ts](file:///c:/Users/37653/Desktop/Higher/Higher-Windows/src/sync/syncRefresh.ts) 纯函数 dispatcher → ActiveProfileProvider 全局监听（profiles→refreshGate+triggerRefresh；tasks/goals/items→triggerRefresh，Today/Planning/Goals/Tasks 经 refreshKey 自动重读）+ ProfileSelector 局部监听（SYNC2-UI-TC02）——无需退出页面/重启/切档案
- Android（§十二）：BottomNav 五项不变；「我的」设备同步项显示状态摘要（Higher Windows · 已连接 · 待同步 X 条）；详情页配对后显示「已从对端导入学习档案：xxx」+ [切换到该档案]（active_profile_id 仍为本机状态）；结果文案直觉化（发送到电脑 X 新增·更新·删除 / 从电脑接收 X / 冲突 X）
- 冲突（§十三）：`sync_conflicts_resolve("local"|"remote")` bulk；local=本机重新入队推送覆盖对端，remote=冲突记录中的远端 payload 强制覆盖本机（force_overwrite 跳过守卫）

## 三、最终结论

```
ROOT_CAUSE:             PASS（4 层定位并测试复现：存量 outbox 缺失→静默 deferred；Profile 语义+UI；per-peer pending；被动 server）
PROFILE_MAPPING:        PASS（全量 Bootstrap + sync_id 并存绝不按 name 合并；TC001/TC011；导入清单+切换按钮）
WINDOWS_TO_ANDROID:     PASS（TC002/TC005/TC007：A 建/改/删 → B 同步后一致）
ANDROID_TO_WINDOWS:     PASS（TC003/TC006/TC008：B 建/完成/删 → A 一致，含 A 主动拉取）
ACK_CURSOR:             PASS（per-peer last_acked/last_received 独立维护；server ack 前先 apply+commit）
PENDING_COUNT:          PASS（TC009：成功 push+ack 后 1→0；语义=针对 peer 未确认，非历史总量）
REMOTE_APPLY:           PASS（TC010 零 echo；guard 事务回滚恢复；FK 依赖拓扑序 TC012，缺依赖明确 deferred 计数，不静默 NULL）
UI_REFRESH:             PASS（sync://completed 两端广播；dispatcher 单测 SYNC2-UI-TC01/02 4/4；全局+ProfileSelector 接入）
DELETE_SYNC:            PASS（TC007/TC008 双向删除，按 sync_id）
CONFLICT:               PASS（TC013 不静默覆盖 + sync_conflicts pending；TC013b bulk 保留本机/对端版本收敛）
WINDOWS_SYNC_WORKSPACE: PASS（Sidebar「同步」/sync；workspace_status 卡片；立即同步=主动连接对端 listener 完成双向）
ANDROID_SYNC_UI:        PASS（BottomNav 不变；我的摘要；详情页 listener+导入档案切换+直觉文案）
TESTS:                  PASS（SYNC2-TC001~015 + TC013b = 16/16；sync_local_mvp 15/15；sync2_audit 2/2；
                              UI dispatcher 4/4；npm build ✓；test:mobile 17/17；test:ai-runtime 13/13；
                              治理 batch064_ui 28/28、batch064r2_ui 27/27、android_governance 28/28；
                              全量 cargo test 除 android_artifact_tests 2 例预存环境失败（Android 构建产物
                              .gitignore 排除未生成，与本任务无关）外全部通过；cargo build dev 冒烟 ✓）
MANUAL_TEST_READY:      是（见下）
```

## 四、真机验收步骤（§十七 场景 A-D）

前置：Windows `npm run tauri dev`（**先完全退出旧实例**）；Android 需重新构建 Debug APK（本轮含后端 v029 与新 UI——按 F1 流程：`scripts/Build-Higher-Android.ps1`，从 canonical Higher-Windows 直接构建）。两端同一 Wi-Fi。

1. 首次：Windows 设置→设备同步→启动同步 → 手机 我的→设备同步→输入 IP/配对码→连接电脑 → 应显示「已从对端导入学习档案：2028考研」+ [切换到该档案]；两侧各自档案并存（双方最终各有 2028考研 + 2028测试）
2. 场景 A：Windows 建 `WINDOWS-SYNC-001` → 手机点「立即同步」→ 手机 Today（切到 2028考研 档案）立即出现
3. 场景 B：手机建 `ANDROID-SYNC-001`（在 2028测试 档案亦可）→ 手机点「立即同步」→ Windows 切到 2028测试 档案查看；或任一端在 /sync 点「立即同步」
4. 场景 C：手机完成 `WINDOWS-SYNC-001` → **任意一端**点立即同步（Windows /sync 亦可主动）→ Windows 显示 completed
5. 场景 D：Windows 删除 `ANDROID-SYNC-001` → 任意一端同步 → Android 任务消失
6. 全程观察「待发送」：成功同步后归零；页面数据无需重启即刷新（sync://completed）
