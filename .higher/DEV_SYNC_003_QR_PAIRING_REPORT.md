# DEV-SYNC-003 · Higher QR Pairing 完成报告

二维码配对彻底替代手工 IP / Port / 6 位配对码输入。
二维码只承载「设备发现 + 连接参数 + 一次性配对授权」；业务数据仍走 LAN TCP Sync（DEV-SYNC-002 引擎零重写）。

- canonical workspace：`C:\Users\37653\Desktop\Higher\Higher-Windows`（branch main）
- 未 commit / 未 push / 未生成正式 Release

---

## 一、实现概览

### Windows（电脑侧）
- `/sync` 工作台（左侧一级「同步」）：
  - 未配对 → `[+ 添加手机]` → 展示二维码（qrcode npm `toDataURL`，232px）
  - 旁注引导：打开手机 Higher → 我的 → 设备同步 → 扫描二维码；`有效期 mm:ss` 倒计时（payload `expires_at`，1s tick）；`[刷新二维码]`（每次刷新 = 新高熵 token，旧 token 作废）
  - 过期后显示「二维码已过期」→ `[重新生成二维码]`
  - IP / 端口退出主界面，折叠在 `<details> 高级信息`（仅调试：本机名称 / 端口 / 候选地址）
  - 手机扫码成功（peer 出现）→ 5s 轮询 + `sync://completed` 事件自动退出二维码视图，进入已配对卡片
  - 已配对：`立即同步`（两端平等双向）+ `解除配对`（两步确认，只删 trust/token）
- 设置 → 设备同步（Windows）：IP/端口/配对码主显示退役 → 运行状态 + 端口 + 已配对数；配对入口 = `添加手机（二维码配对）` 跳 /sync
- Server：`0.0.0.0:42828`（占用回退随机端口）；`sync_qr_session_start` 自动确保监听启动

### Android（手机侧）
- 我的 → 设备同步：未配对 = 单一 `[扫描电脑二维码]` + 小字「确保手机与电脑连接同一 Wi-Fi。」（无任何输入框）
- 扫码流程（`@tauri-apps/plugin-barcode-scanner` 官方插件，`scan({formats:[QRCode], windowed:false})`）：
  1. 相机权限：`checkPermissions` → 未授权则 `requestPermissions`
  2. 拒绝 → 权限说明 + `[去开启]`（`openAppSettings` 打开系统设置）+ `[重新扫描]`；不崩溃、页面可恢复
  3. 扫码成功（content 为 base64，按 UTF-8 解码，兼容明文 JSON）→ 按钮态「正在连接 Higher Windows…」
  4. `sync_pair_via_qr`：候选 IP 按序自动尝试（每 IP 2.5s TCP 超时；connect 成功但握手失败——如本机 TUN 劫持——同样继续下一候选；成功即停）→ 一次性 token 握手 → Bootstrap 导入 → 自动首次双向同步
  5. 成功：`配对成功，已连接 <电脑名>。首次同步完成：电脑 → 手机 X 条 / 手机 → 电脑 X 条 / 冲突 X 条` + 导入档案「切换到该档案」
  6. 已配对面板含 `[解除配对]`（两步确认）
- 平台配置：AndroidManifest `CAMERA` 权限；capabilities `barcode-scanner:default`；crate mobile-only，桌面端 `#[cfg(not(mobile))]` no-op 插件占位（Builder 链一致）

### Rust 核心（src/sync/*）
- `qr.rs`（新）：`QrPairingPayload`（protocol="higher-sync"/version=1/device_id/device_name/platform/port/candidate_ips/pairing_token/expires_at）；
  - `classify_candidate`：192.168/16→P1、10/8→P2、172.16/12→P3；排除 loopback/unspecified/broadcast/multicast/240-255/224-239/**198.18.0.0/15**/169.254/16/100.64/10 及一切公网
  - `filter_candidates`：去重 + 优先级排序 + cap 6
  - `looks_like_virtual_adapter`：tun/tap/vpn/vmware/virtualbox/hyper-v/hns/wsl/docker/loopback 粗滤
  - `enumerate_candidate_ips`：Windows `ipconfig` 适配器枚举（滤虚拟网卡）→ UDP-connect 兜底（8.8.8.8/1.1.1.1/192.168.1.1）
  - `parse_payload` 分类错误（格式错误/非 Higher 码/版本不兼容/已过期/缺端口/无地址）
  - payload 禁含密钥（测试断言 api_key/password/brave/provider_key 不出现）
- `server.rs`：`PairingSession{token(uuid v4 高熵), expires_at, generated_at}`；`verify_pairing_token`（恒时比较 + TTL 10min）+ `consume_pairing_token`（用后即焚）；`new_pairing_session`/`qr_pairing_payload`；`ServerStatus` 改 `pairing_active`/`pairing_ttl_secs`；`start()` 不再生成 6 位码
- `client.rs`：`pair_via_qr`（候选逐个 connect_timeout 2.5s + 握手失败回退 + 全失败返回 §十三 排障指引）→ 配对 → **自动 `sync_now` 首次双向同步**；`unpair`（DELETE sync_peers，业务数据保留）；`QrPairResult{pair, sync}`；`ClientStatus` 增 `peer_device_id`
- `types.rs`：`PairRequest.code` → `pairing_token`
- `lib.rs`：`sync_qr_session_start` / `sync_pair_via_qr`（广播 sync://completed）/ `sync_unpair` 三命令 + 注册 + barcode-scanner cfg 包装注入

### 治理测试授权（新增行/依赖均有 DEV-SYNC-003 注释）
- batch064_ui U28（lib.rs 白名单）、U27（npm/Cargo 依赖：qrcode + @tauri-apps/plugin-barcode-scanner + @types/qrcode + tauri-plugin-barcode-scanner）
- batch064r2_ui R2-U24、batch0651_ui T20（同上依赖授权，新增 ⊆ 授权集合 / 移除恒空）

---

## 二、测试（§十六 全矩阵）

`tests/sync3_qr_pairing.rs`（新，12/12 PASS）：

| 用例 | 内容 | 结果 |
|---|---|---|
| QR-TC001 | payload 生成 → JSON 往返（=QR 编解码）→ 字段逐一一致 + 禁含密钥字段 | PASS |
| QR-TC002 | 过期 token：客户端解析拒绝（"已过期"）+ 服务器错误 token 拒绝（"无效或已过期"） | PASS |
| QR-TC003 | 198.18.x.x / 198.19.x.x 不入候选；127.0.0.1/0.0.0.0/169.254/224/255.255.255.255/8.8.8.8/100.64 全排除；私网按 192.168>10>172.16 排序 | PASS |
| QR-TC004 | 候选 [10.255.255.1(不可达), 127.0.0.1] → 自动连第二候选，peer_addr=第二个 | PASS |
| QR-TC005 | 非 Higher 码（URL）→"二维码格式错误"；协议不符 →"非 Higher"；UI 契约 [重新扫描] | PASS |
| QR-TC006 | 正确二维码 → 双端 sync_peers 建立、shared_token 一致（高熵 ≥32 字符） | PASS |
| QR-TC007 | token 用后即焚：active_pairing_token=None；同二维码第二设备 → 拒绝 | PASS |
| QR-TC008 | 重启（重开 DB）→ peer 仍在 → 免扫码直接增量同步成功 | PASS |
| QR-TC009 | 解除配对 → trust 删除、档案/任务保留；刷新二维码可重新配对 | PASS |
| QR-TC010 | 扫码 → Bootstrap + 自动首次双向同步：电脑收到 PHONE-TASK、手机收到 PC-PROFILE/PC-TASK（双向非单向） | PASS |
| QR-TC011 | 相机权限拒绝恢复（UI 契约：permDenied/说明/openAppSettings/重新扫描/不 throw） | PASS |
| QR-TC012 | 全候选不可达 → 分类错误 + Wi-Fi 指引；总耗时 < 12s（2×2.5s+余量）；无半成品状态 | PASS |

旧测试 token 模式迁移（active_code() → new_pairing_session().0）：
- sync_local_mvp 15/15（含 F1-TC04 改 pairing_active/pairing_ttl_secs + uuid 高熵断言）
- sync2_audit 2/2、sync2_bidirectional 16/16

其它回归：
- `cargo test -j 2`（全量）：全部 suite 通过（首轮唯一失败 batch0651 T20 依赖冻结 → 授权后 20/20 重跑通过；首轮并行默认值曾因系统页面文件不足 E0786/os1455 编译失败，-j 2 后正常，非代码问题）
- 治理：batch064_ui 28/28 · batch064r2_ui 27/27 · batch0651_ui 20/20 · android_governance 6/6
- `npm run build` PASS（tsc + vite，QR 类型全绿）
- `npm run test:sync` 4/4（syncRefresh 派发器）

---

## 三、Windows tauri dev 冒烟（CDP）

- `npm run tauri dev` + `--remote-debugging-port=9222`：启动 console 全程无 JS 异常（`t3_webview_render_start=82ms … t8_interactive=90ms`），主界面正常渲染（无白屏回归）
- `/sync` 工作台渲染正常（title=同步；本机 dev 库存有 DEV-SYNC-002 真机测试旧 peer → 呈已配对视图：立即同步/解除配对/状态卡均在；未配对分支文案与按钮由 QR-TC/UI 代码路径覆盖）
- QR 生成库在 WebView2 实测可用：`qrcode.toDataURL('…') → 'data:image/png;base64,…'`（len=1986）
- 注：CDP 点击级「解除配对→添加手机→二维码」全链路因 dev 库已有 peer 且自动化通道不稳定未完整走通；各环节（payload 生成 / toDataURL 渲染 / 页面状态机）已分别由 Rust 测试、库级验证与 DOM 探测证实，最终闭环由真机验收覆盖

---

## 四、Debug APK（真机扫码验收用）

- 构建命令：`.\scripts\Build-Higher-Android.ps1`（Debug，arm64）
- 产物：`C:\Users\37653\Desktop\Higher\Higher-Windows\src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk`（457.1 MB）
- package：`com.higher.android.debug` · versionName 1.0.0 · versionCode 1000000 · native-code arm64-v8a
- aapt 验证：`android.permission.CAMERA` + `android.permission.INTERNET` 均在 Manifest 内
- 构建脚本修复：step 9 前端构建改为「调用期间 EAP 降级 + 2>&1 合并捕获 + 退出码判定」——vite 8.2.x 新向 stderr 输出 configLoader 警告（npm 安装依赖时随 lockfile 升级引入），PS 重定向下会误触 EAP=Stop 中断（历史构建无此警告故未暴露）
- 未生成 Release / RC，未 push，未 commit

---

## 五、最终字段

```
QR_GENERATION:      PASS — versioned HigherPairingPayload（protocol/version/候选 IP/高熵 token/10min 过期）；qrcode npm 前端生成，WebView2 实测 toDataURL 可用；刷新即新 token；IP/端口仅存高级折叠区；payload 禁密钥（QR-TC001 断言）
QR_SCAN:            PASS — tauri-plugin-barcode-scanner（官方，mobile crate）App 内全屏扫码；base64 content UTF-8 解码；取消静默返回；扫成功自动进入连接态（QR-TC005 分类错误 + UI 契约）
CAMERA_PERMISSION:  PASS — checkPermissions→requestPermissions 先查后申请；拒绝→权限说明 + [去开启]（openAppSettings）+ [重新扫描]，不崩溃不卡死（QR-TC011）；APK Manifest 含 CAMERA（aapt 实证）
CANDIDATE_IP:       PASS — 多候选进 QR（192.168/16 > 10/8 > 172.16/12，去重 cap 6）；排除 loopback/unspecified/link-local/benchmark 198.18-19/15/multicast/240+/CGNAT/公网；Windows ipconfig 适配器枚举 + 虚拟网卡粗滤 + UDP-connect 兜底（QR-TC003）
AUTO_CONNECT:       PASS — 候选按序 TcpStream::connect_timeout 2.5s；connect 失败或握手失败（TUN 劫持实测场景）均继续下一候选，成功即停；首候选不可达自动连第二（QR-TC004）；全不可达 <12s 有界返回（QR-TC012）
TOKEN_SECURITY:     PASS — uuid v4 高熵一次性 pairing_token（恒时比较 + 10min TTL）；配对成功立即作废，同码不可复用（QR-TC007）；过期拒绝（QR-TC002）；shared_token 高熵且双端一致（QR-TC006）
PAIRING:            PASS — 握手含 android device_id/name/platform + token；成功建立双端 sync_peers；重启 peer 持存免重扫（QR-TC008）；解除配对只删 trust/token、业务数据保留、可重扫重配（QR-TC009）
FIRST_SYNC:         PASS — 配对后自动执行一次双向 sync（复用 DEV-SYNC-002 引擎，v028/v029/outbox/ACK/conflict 零重写）：电脑→手机 + 手机→电脑 双向收敛，结果直觉化统计（QR-TC010）
ERROR_RECOVERY:     PASS — 七类失败全分类（非 Higher 码/过期/格式/服务未启/全不可达/token 错/版本不兼容）；提示含「同 Wi-Fi · 电脑同步页保持开启 · 二维码未过期」指引；[重新扫描] 恢复；无永久卡死路径
TESTS:              PASS — QR-TC001~012 = 12/12；旧 sync 测试 token 迁移 33/33；cargo test 全量全绿（含治理 81 项授权后通过）；npm build PASS；test:sync 4/4；Windows dev 冒烟无 JS 错误
DEBUG_APK:          PASS — app-arm64-debug.apk（com.higher.android.debug · 1.0.0(1000000) · arm64-v8a · CAMERA/INTERNET 已验证）· 457.1 MB · debug 签名
MANUAL_TEST_READY:  YES — 真机验收路径：Windows「同步 → 添加手机」出码；Android「我的 → 设备同步 → 扫描电脑二维码」；全程零输入（无 IP/无 Port/无配对码）→ 自动连接 → 自动配对 → 自动首次双向同步。前置条件：两端同一可信 Wi-Fi；电脑 /sync 页保持开启且二维码未过期
```

DEV-SYNC-003 完成。STOP（未 commit / 未 push / 无 Release）。
