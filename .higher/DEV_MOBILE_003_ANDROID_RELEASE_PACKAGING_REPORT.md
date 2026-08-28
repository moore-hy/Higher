# DEV-MOBILE-003 · Android Release Packaging 报告

Baseline：android/dev @ 733e8ba（产物未提交）· Windows baseline：main@733e8ba 仅 TASK.md dirty

## 身份（aapt2 实测）
- Release：`com.higher.android` · versionName **0.1.0** · versionCode **1000** · label Higher
- Debug：`com.higher.android.debug`（并存 ✓）
- 版本唯一真相 = tauri.conf.json → 脚本再生 tauri.properties

## 产物
| 文件 | 说明 |
|---|---|
| `release/android/Higher-v0.1.0-arm64.apk` | **26.1MB 磁盘 / 29.52MB 展开** · 仅 arm64-v8a · 正式签名 |
| `release/android/Higher-v0.1.0-size-report.txt` | lib 22.34 / assets 3.02 / res 0.96 / dex 2.04 MB（≤80MB 正常档）+ Top30 |
| `release/android/upgrade-test-Higher-v0.1.1-arm64.apk` | 升级链测试包（1001 · 同 keystore · 不发布） |

- 原始 APK：`gen\android\app\build\outputs\apk\arm64\release\app-arm64-release.apk`
- apksigner：**Verifies** · 签名者 `CN=Higher, OU=Higher, O=Higher, C=CN` · RSA-2048
- **证书 SHA-256（升级链根，永久记录）**：`0c864d48a9e422d33e6cb175b8bc65d493bcd53c3d6d0f7ebfedb04f3d33c02`
- 0.1.1(1001) 升级包同证书 ✓（构建侧升级链 PASS；真机覆盖留用户）

## Gates
- Artifact Truth：dist=android · compile=android · custom-protocol · devUrl=null · .so 无 localhost:1420/127.0.0.1:1420 ✓
- Secrets：keystore 位于 `~\.higher-secrets\android\`（不入库）；`*.jks/*.keystore/keystore.properties` 均被 ignore（git check-ignore 验证；porcelain 无泄漏）；密码全程 **[REDACTED]**（HUMAN SECRET GATE 由用户执行 keytool/填写）
- 自动测试：`npm run build`(desktop meta) · `test:ai-runtime` 13/0 · `test:mobile` 11/0 ✓（cargo check/契约套件本日全绿）
- Debug 回归：`-Configuration Debug` 全通（0.1.0/1000 badging 验证）

## 修改文件（Android worktree）
tauri.conf.json(version 0.1.0) · .gitignore(+jks) · gen/android/app/build.gradle.kts(guarded signingConfigs，Tauri v2 单 password 契约) · gen/android/keystore.properties{.example} · scripts/Build-Higher-Android.ps1(-Configuration Debug|Release 全 Gate) · gen/android/app/tauri.properties(自动再生)

## 待真机（用户执行，手机连接后）
1. `adb install -r release\android\Higher-v0.1.0-arm64.apk` → 桌面启动（无 PC/localhost）
2. 五页 Smoke + AI + 重启
3. 升级链：装 0.1.0 → `adb install -r upgrade-test-...-0.1.1.apk` → 数据不清 → 可再装回 0.1.1→0.1.0 需卸载（versionCode 降级不可直装，测试后可卸载重装正式版）
4. keystore 双备份确认（U 盘/加密云盘）

## Known Issues
- Higher-Windows 出现非基线 dirty：`src-tauri/Cargo.toml`（**经特征比对确认非 Trae 施工产物**，疑行尾/外部触碰；未按红线处置，请用户自查）
- tauri.properties 由脚本再生（手改会被覆盖）
- AAB 未构建（可选项，APK 稳定后再议）

```
AUTOMATION: PASS
SIGNING:  PASS
APK:      PASS
REAL_DEVICE: PENDING（安装/冷启动/Smoke/升级覆盖）
OVERALL:  HOLD
```
