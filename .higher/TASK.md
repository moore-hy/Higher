DEV-MOBILE-003 · Higher Android Release Packaging 精确施工任务书

版本：FINAL v1.0
目标版本：Higher Android 0.1.0
施工目录：C:\Users\37653\Desktop\Higher\Higher-Android
施工分支：android/dev
本阶段目标：建立可长期升级、正式签名、可直接分发安装的 Higher Android Release APK。
最终发布物：Higher-v0.1.0-arm64.apk
可选发布物：Higher-v0.1.0.aab
最终状态：AUTOMATION PASS 后仍需 REAL DEVICE 验收，Trae 不得自行宣布最终 PASS。

0. 冻结现状

当前 Android 真机 UI、AI Root Navigation、BottomNav、Safe Area 已基本完成。

本阶段不再修改产品 UI，除非 Release 构建暴露出明确的 Release-only 问题。

严禁：

修改 Higher-Windows；

重构 AI Runtime；

重构 Planner / Memory / ChangeSet / Permission；

修改数据库 schema / migration；

为 Release 复制第二套业务逻辑；

把签名密码、keystore、API Key 提交 Git；

为减包体随意删除业务资源；

破坏现有 Debug 构建。

1. Release 产品身份

Debug：

applicationId: com.higher.android.debug
用途：开发测试

Release：

App Name: Higher
applicationId: com.higher.android
versionName: 0.1.0
ABI: arm64-v8a
用途：日常使用 / 直接分发安装

Debug 与 Release 允许同时安装。

注意：因为 package id 不同，Release 不会自动继承 Debug 的 Android App sandbox 数据。需要历史数据时，走 Higher 自身导出/导入，不要让 Release 使用 .debug 包名。

2. 版本管理

检查当前 src-tauri/tauri.conf.json / Android config 的版本来源。

正式版本统一为：

0.1.0

优先使用 Tauri 自身 version 配置作为唯一 versionName truth。除非当前工程已有明确自定义 bundle.android.versionCode 规则，否则不要创建第二套版本系统。

报告必须记录：

versionName
versionCode
applicationId

3. Signing Secret 安全边界

Keystore 推荐保存：

C:\Users\37653\.higher-secrets\android\higher-release.jks

不得保存到 Higher-Android / Higher-Windows / src-tauri / .git / release。

Android repo .gitignore 至少覆盖：

*.jks
*.keystore
src-tauri/gen/android/keystore.properties
release/android/*.apk
release/android/*.aab

允许提交：

src-tauri/gen/android/keystore.properties.example

但不得含真实密码。

4. HUMAN SECRET GATE —— Trae 必须暂停一次

Trae 先完成所有不涉及 secret 的施工。

到首次创建 Release signing key 时必须 STOP，并提示用户本人在 PowerShell 执行：

New-Item -ItemType Directory -Force "$env:USERPROFILE\.higher-secrets\android" | Out-Null

& "$env:JAVA_HOME\bin\keytool.exe" `
  -genkeypair `
  -v `
  -storetype JKS `
  -keystore "$env:USERPROFILE\.higher-secrets\android\higher-release.jks" `
  -alias higher-release `
  -keyalg RSA `
  -keysize 2048 `
  -validity 10000

如果 $env:JAVA_HOME 不存在，Trae 只负责定位现有 JDK 21 的 keytool.exe，然后把不含密码的命令交给用户。

密码由用户本人在 keytool 交互提示里输入。

Trae 不得：

要求用户把密码发进聊天；

把密码写进 TASK；

把密码写进 PowerShell 命令参数；

自动生成一个用户不知道的密码。

5. keystore.properties

Key 创建后，在：

src-tauri/gen/android/keystore.properties

创建本机私有配置。

优先按当前 Tauri v2 signing contract：

password=<LOCAL_PRIVATE_PASSWORD>
keyAlias=higher-release
storeFile=C:\\Users\\37653\\.higher-secrets\\android\\higher-release.jks

如果当前实际 Tauri/Gradle template 已采用 storePassword / keyPassword，则沿用真实 template，不同时维护两套。

要求：

文件必须被 .gitignore；

git status 不得出现它；

报告不得打印真实 password；

日志只能显示 [REDACTED]。

同时创建：

src-tauri/gen/android/keystore.properties.example

6. Gradle Release Signing

审计：

src-tauri/gen/android/app/build.gradle.kts

Release build 必须有真实 signing config。

等价结构：

import java.io.FileInputStream
import java.util.Properties

android {
    signingConfigs {
        create("release") {
            val keystorePropertiesFile = rootProject.file("keystore.properties")
            val keystoreProperties = Properties()
            if (keystorePropertiesFile.exists()) {
                keystoreProperties.load(FileInputStream(keystorePropertiesFile))
            }

            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = keystoreProperties["password"] as String
            storeFile = file(keystoreProperties["storeFile"] as String)
            storePassword = keystoreProperties["password"] as String
        }
    }

    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
        }
    }
}

但必须先读当前真实 build.gradle.kts，按当前 AGP / Tauri template 最小修改。禁止整文件覆盖、第二个 android{} block、硬编码密码。

7. Release Build Script

继续复用：

scripts\Build-Higher-Android.ps1

扩展为：

.\scripts\Build-Higher-Android.ps1 -Configuration Debug
.\scripts\Build-Higher-Android.ps1 -Configuration Release

不新增互相竞争的第二套 pipeline。

Release 必须走 Tauri v2 Android release build，不得手工拼 Gradle artifact。

正式 Release APK 构建语义：

tauri android build
--apk
--target aarch64
--split-per-abi

npm 项目可等价：

npm run tauri android build -- --apk --target aarch64 --split-per-abi

若本机 CLI 参数不同，以实际 android build --help 输出为准。

8. Release Artifact Truth Gate

必须继续证明：

HIGHER_TARGET_PLATFORM=android
dist platform = android
compile target = android
custom-protocol enabled
devUrl = null

最终 APK / native lib 不得包含：

localhost:1420
127.0.0.1:1420
http://localhost

发现任意 devUrl：

RELEASE = FAIL

9. Release Package Output

新增本地输出目录：

release/android/

最终复制真实 arm64 Release APK 为：

release/android/Higher-v0.1.0-arm64.apk

不要假定 Gradle 原始文件名；通过构建日志、build outputs、时间戳、ABI 找真实 Release APK。

不得拿 Debug APK 重命名冒充 Release。

10. Release Signing Verification

自动定位 Android SDK build-tools 中的 apksigner.bat。

必须执行等价：

apksigner verify --verbose --print-certs Higher-v0.1.0-arm64.apk

报告记录 signer SHA-256 certificate digest；不得记录私钥或 password。

今后每个 Release 的 signer digest 必须与 0.1.0 一致。

11. APK Identity Verification

用当前可用的 aapt / apkanalyzer 等读取最终 APK artifact，必须证明：

package = com.higher.android
versionName = 0.1.0
versionCode = <实际值>
label = Higher

不得只读源码推断。

12. ABI Verification

必须证明 Release APK 只包含：

arm64-v8a

不得误打：

x86
x86_64
armeabi-v7a

进 Higher-v0.1.0-arm64.apk。

13. Release APK 包体审计

当前历史 Debug APK 曾约 452MB，因此 Release 必须做 inventory。

输出：

release/android/Higher-v0.1.0-size-report.txt

至少包含：

APK total size
largest 30 entries
lib/ size
assets/ size
res/ size
classes*.dex size

判断：

<= 80MB        正常继续
80MB ~ 150MB   允许继续，但必须解释主要占用
> 150MB        RELEASE HOLD，先定位异常大文件/重复资产/多 ABI/开发产物

不得为过 gate 盲删 Higher 功能。

14. Release 不得包含开发垃圾

检查 APK / assets 不应携带：

node_modules
.git
.higher task docs
.ai-runtime-test-build
.mobile-test-build
.toolchain
src-tauri/target 整目录
tests
README
PowerShell build logs

15. Minify / Shrink

Higher 0.1.0 首个 Release 不强制开启：

minifyEnabled=true
shrinkResources=true

如果当前 template 已安全启用则保留；如果未启用，不要为了减包体引入新的 Release-only 崩溃变量。

16. AAB

APK 是本次主发布物。

如果 Release APK 完成且稳定，可额外构建：

tauri android build --aab

产出：

release/android/Higher-v0.1.0.aab

AAB 本次不作为真机 sideload 验收物。

17. Debug 回归

Release pipeline 完成后，原有：

.\scripts\Build-Higher-Android.ps1 -Configuration Debug

必须仍可构建。

18. Windows 零回归

确认：

git -C "C:\Users\37653\Desktop\Higher\Higher-Windows" status --porcelain

与施工前 baseline 一致。

19. Git Secret Gate

Release 完成后检查 git status / git ls-files，Git tracked files 中不得有：

*.jks
*.keystore
keystore.properties
真实密码

如果被 track：

P0 FAIL

20. Release 安装测试

正式 APK 构建后安装：

adb install -r "release\android\Higher-v0.1.0-arm64.apk"

Release 应安装为：

com.higher.android

Debug 如仍存在：

com.higher.android.debug

两者可并存。

21. Release Cold Start

Release 必须从手机桌面直接启动，不得要求：

PC
Vite server
localhost
adb reverse
Android Studio
Trae
npm

验收：

Higher UI 正常
无 localhost
无白屏

22. Release Smoke Test

至少测试：

今日
规划
知识
AI
我的

AI 页面、BottomNav、设置均正常。

Release 是新 package sandbox 时，API Key / 用户数据为空属于预期。配置后应正常工作。

23. 数据迁移

Debug 与 Release sandbox 独立。

首个 Release：

A. 干净开始
或
B. 使用 Higher 自身导出/导入

禁止 adb 手工复制私有数据库作为正式产品流程。

24. Future Upgrade Test

0.1.0 是未来升级链的根。

必须用同一 keystore 做一次不发布的更高 versionCode Release 测试，并通过：

adb install -r

覆盖 com.higher.android，确认 app data 不清空。

最终恢复正式 0.1.0 配置，临时 test artifact 不发布。

25. Keystore Backup Gate

首次 Release 前提醒用户备份：

C:\Users\37653\.higher-secrets\android\higher-release.jks

至少两份：

本机安全位置
独立 U 盘 / 加密云盘

密码也由用户本人安全保存。

26. 自动测试

执行现有 Gates：

npm run build
npm run test:ai-runtime
npm run test:mobile
cargo check --all-targets
cargo test --no-fail-fast

全部 0 FAILED。

27. Final Report

创建：

.higher/DEV_MOBILE_003_ANDROID_RELEASE_PACKAGING_REPORT.md

必须记录：

Git baseline / branch / commit
修改文件
Release applicationId
Debug applicationId
versionName
versionCode
Release APK 原始 path
Release APK 发布 path
APK size
ABI
apksigner verification
certificate SHA-256
artifact package metadata
devUrl scan
largest 30 APK entries
Debug regression
Windows baseline/end
Release install result
Cold start result
Upgrade-chain result
AAB result（若执行）
known issues

密码一律：

[REDACTED]

28. 最终输出

必须有：

release/android/Higher-v0.1.0-arm64.apk
release/android/Higher-v0.1.0-size-report.txt
.higher/DEV_MOBILE_003_ANDROID_RELEASE_PACKAGING_REPORT.md

可选：

release/android/Higher-v0.1.0.aab

29. STOP / PASS 规则

Trae 自动化完成后：

AUTOMATION: PASS / FAIL
SIGNING: PASS / FAIL
APK: PASS / FAIL
REAL_DEVICE: PENDING
OVERALL: HOLD

然后 STOP。

只有用户本人真机确认：

正式 Higher 可安装
正式 Higher 可启动
五主页面正常
AI 正常
重启正常
无 localhost

才最终 DEV-MOBILE-003 PASS。

30. 给 Trae 的直接开工指令

完整阅读本任务书后，从环境 / 构建 / Gradle / 版本审计开始。

先完成所有不涉及 secret 的施工。

到首次创建 Release Signing Key 时：

STOP；

让用户本人执行 HUMAN SECRET GATE；

用户只回复“密钥创建完成”；

不得索要密码。

随后继续：

keystore.properties
→ release signing
→ Release APK
→ artifact verification
→ package size audit
→ install
→ upgrade-chain test
→ report
→ STOP

禁止顺手修改 Higher 产品 UI、AI Runtime、数据库或 Windows。