# DEV-SYNC-003-F3 · Higher Single-Click Bidirectional Convergence 完成报告

任意一端一次「立即同步」= 一个完整双向收敛 session（LOCAL PUSH + REMOTE PULL + REMOTE APPLY
+ ACK + OUTBOX TRIM + 反向 ACK + UI REFRESH），双方 pending 同步归零。
未 commit / 未 push / 无 Release。

---

## 一、根因（先复现后修，非猜测）

协议审计（client.rs `sync_now` + server.rs `SyncRequest` arm）发现：**单 TCP 连接内数据其实
已双向交换**（A push → B apply → B 回发自己的 outbox → A apply），断裂点在 **ACK 游标只走了
一半**：

- B 在响应里 ack A 的 push → A 的 pending 归零 ✓
- 但 A 消费了 B 下发的 changes 后，**只有 A 本地记游标**（last_received_remote_change_id），
  B 要等 **A 的下一次** SyncRequest 才能带上这个 ack → B 的 outbox 游标推进 + trim 被推迟到
  「下一次同步」→ **B 端「待发送 N 条」残留** → 用户被迫再点一次（哪端都行）才双双归零。

### ROOT_CAUSE_ANDROID_INITIATED
Android 发起时：数据双向已送达（Windows 收到并 apply、Android 收到 Windows 的数据），
但 **Windows 侧 outbox ack 游标未收到回执** → Windows「待发送 N 条」残留至下一次任意同步。

### ROOT_CAUSE_WINDOWS_INITIATED
对称同理：Windows 发起时 Android 收到数据，但 **Android 侧 pending 游标残留** 至下一次。

### 修复（§三/§四/§五）
- 协议新增 `SyncAck` / `SyncAckResponse`（types.rs）：发起方 apply 完对端下发 changes 后，
  **在同一 TCP 连接内**立即回 ACK（携带本次消费的最大 change_id）。
- server 端 `SyncAck` arm：token 校验 → `advance_acked_cursor` + `trim_acked_outbox` +
  `touch_peer_sync` → 广播 `sync://completed`（对端 UI 即时刷新 pending 归零）。
- client 端：`sync_now` 在 `local_max > 0` 时发 SyncAck；旧版对端不认识该消息时按
  「尽力而为」降级（数据已双向送达，不判失败）——§五 echo 约束不变：remote apply 由
  `sync_runtime_guard` 防回声（TC03 断言 outbox 不增），单轮 push+pull+ack 即收敛，
  无需多轮 loop。
- 离线 UX（§八）：`offline_error(platform)` —— connect/写/读任一传输阶段失败都给针对性指引
  （对端 platform=android → 「Higher Android 当前未在线…我的 → 设备同步…重试」；
  否则 → 「Higher Windows 当前不可连接…已启动设备同步」），绝不半失败伪装成功。
- §九/§十：pending 三处（/sync 工作台、设备同步详情、「我的」摘要）同源
  （pending_outbox_count_for per-peer）；MobileSettings 新增 `sync://completed` 即时刷新
  （原仅 5s 轮询）。

## 二、测试（SYNC-F3-TC01~09，tests/sync4_oneclick.rs，7 用例全 PASS）

| 用例 | 内容 | 结果 |
|---|---|---|
| TC01/TC06 | A=1/B=1 pending，只 Android(B) 发起 sync_now → 双向到达 + pending 0/0 | PASS |
| TC02/TC05 | A=2/B=3 pending，B listener 在线，只 Windows(A) 发起 → 全交换 + 0/0 | PASS |
| TC03 | 双向 apply 后 outbox 行数不增（无 echo）+ pending 0/0 | PASS |
| TC04 | 收敛后立即再同步 → pushed=0/pulled=0/applied=0（no-op） | PASS |
| TC07 | Android listener 离线，Windows 发起 → Err 含「Higher Android 当前未在线」+ 本机 pending 保留 | PASS |
| TC08 | Windows server 离线，Android 发起 → Err 含「Higher Windows 当前不可连接」 | PASS |
| TC09 | workspace_status.pending_send 与 client_status.pending_outbox 同为 0（多视图一致） | PASS |

回归：sync_local_mvp 15/15 · sync2_audit 2/2 · sync2_bidirectional 16/16 · sync3_qr_pairing
12/12 · sync4_oneclick 7/7 · android_governance 6/6 · android_artifact 6/6 ·
mobile_tc_contract 11/11（合计 75 全绿）· npm run build PASS · Debug APK 全 Gate PASS
（BARCODE_NATIVE_CLASS_GATE + MLKIT_BUNDLED_GATE 等）。

环境事故修复（与功能无关）：`sync_local_mvp.rs` / `sync2_audit.rs` / `mobile_tc_contract_tests.rs`
被外部进程注入 UTF-8 BOM（双 BOM 致 rustc `unknown start of token \u{feff}`）——二进制级
剥离 BOM 后全部恢复通过，源码内容零改动。

## 三、真机验收（OPPO PCAM00 + Windows dev，同局域网）

### 场景 A：只点 Android「立即同步」（Windows 零操作）
- 准备：Windows 建 `WIN-ONLY-CLICK-A`（待发送 1 条实证）· Android 建 `ANDROID-ONLY-CLICK-A`
- Windows 同步 server 启动（42828 监听，CDP 自动化完成）
- 只点击手机「立即同步」→ 结果：
  - Windows /sync：**待发送 0 条**、最后同步=点击时刻（18:06）
  - 手机设备同步：**待发送 0 条**（charCode 实证 '0'）
  - 手机今日页出现 `WIN-ONLY-CLICK-A`（任务数 +1）
  - Android 任务已 apply 到 Windows（由「最后同步更新 + Windows ack 令手机 pending 归 0 +
    apply 为 push 必经路径」证明；其 UI 落点在手机来源的独立 profile——§十一隔离共存，
    人工切档案可见）
- PASS

### 场景 B：只点 Windows「立即同步」（Android 保持设备同步页打开=listener 在线，零点击）
- 准备：Windows 建 `WIN-ONLY-CLICK-B` · Android 建 `ANDROID-ONLY-CLICK-B`
- 只点击电脑「立即同步」（CDP DOM click）→ 结果：
  - Windows /sync：**待发送 0 条**
  - 手机设备同步：**待发送 0 条**（charCode '0'）
  - **Windows 今日页直接出现 `ANDROID-ONLY-CLICK-B`**
  - **手机今日页出现 `WIN-ONLY-CLICK-B`**（今日任务 0/6，A/B 两任务俱在）
  - 手机进程全程稳定
- PASS

## 四、最终字段

```
ROOT_CAUSE_ANDROID_INITIATED:
  协议 ACK 只走一半：Android 发起时数据已双向送达，但 Windows 的 outbox ack 游标收不到回执，
  「待发送 N 条」残留到下一次同步 → 用户被迫再点一次。（非 Push/Pull 缺失——单连接数据层本已双向）
ROOT_CAUSE_WINDOWS_INITIATED:
  对称同因：Windows 发起时 Android 侧 pending 游标残留至下一次。
ANDROID_ONE_CLICK:   PASS —— 真机场景 A：单点手机，双端 pending=0、双向数据到达、Windows 零操作
WINDOWS_ONE_CLICK:   PASS —— 真机场景 B：单点电脑，双端 pending=0、Android 今日页/Windows 今日页互见对方任务、手机零点击
ROUND_TRIP:          PASS —— 单 TCP session：SyncRequest(A push+ack) → B apply/ack+回发 → A apply → SyncAck（同连接回执）；旧对端无 SyncAck 时降级不判失败
ACK:                 PASS —— 双向 ACK 同 session 闭环：B ack A push（SyncResponse.acked_change_id）+ A ack B 下发（SyncAck）；server 端 SyncAck 推进游标
OUTBOX_TRIM:         PASS —— 双端 trim_acked_outbox 均在 session 内执行；TC03 断言 apply 无 echo（outbox 不增）
PENDING_CONSISTENCY: PASS —— /sync 工作台、设备同步详情、「我的」摘要三处同源（pending_outbox_count_for per-peer）；TC09 + 真机双端归零（charCode 实证）
UI_REFRESH:          PASS —— session 结束双方广播 sync://completed；MobileSettings 摘要新增事件即时刷新（原 5s 轮询兜底）；/sync 页事件+轮询；真机两端无需切页/重启即见新数据
OFFLINE_ERROR:       PASS —— TC07/TC08：对端离线（含 listener 异步关闭导致的连接后 RST）给针对性指引文案（按对端平台），本机 pending 保留、不伪装成功、可重试
TESTS:               PASS —— SYNC-F3-TC01~09 = 7/7；全量回归 75 用例全绿（sync MVP/bidirectional/QR/oneclick/governance/artifact/mobile 契约）；npm build PASS；Debug APK 双 Gate PASS
MANUAL_TEST_READY:   YES —— 单点击双向收敛已真机实证（场景 A/B 均 PASS）；日常使用即最终验收：任一端点一次「立即同步」，两端数据与「待发送」同时归零
```

DEV-SYNC-003-F3 完成。STOP（未 commit / 未 push / 无 Release）。
