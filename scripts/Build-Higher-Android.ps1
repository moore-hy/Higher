# ============================================================================
# Build-Higher-Android.ps1（唯一 Android 打包入口）
#
# 发布阶段模型（DEV-MOBILE-005 · 禁止文档与实现漂移）：
#   Debug    开发回归（applicationId com.higher.android.debug · 仅 arm64 · debug 签名）
#   RC       正式签名候选 → release/android/rc/Higher-v<ver>-rc.apk
#            默认 DistributionProfile=Public（arm64-v8a + armeabi-v7a，无 x86/x86_64）
#   Internal QA/模拟器全 ABI → release/android/internal/Higher-v<ver>-internal-universal.apk
#            DistributionProfile=InternalUniversal（arm64-v8a + armeabi-v7a + x86_64）
#   Stable   禁止由本脚本产生——只能经 scripts/Promote-Higher-Android-RC.ps1
#            对已真机验收的 RC 做字节级 Promotion（SHA256(RC)==SHA256(Stable)）
#
# 用法：
#   .\scripts\Build-Higher-Android.ps1                                              # Debug
#   .\scripts\Build-Higher-Android.ps1 -Configuration Release -Channel RC `
#       -DistributionProfile Public                        # 正式 RC（默认 Public）
#   .\scripts\Build-Higher-Android.ps1 -Configuration Release `
#       -DistributionProfile InternalUniversal             # 内部 QA 全 ABI
#   .\scripts\Promote-Higher-Android-RC.ps1               # RC 验收通过后升级为 Stable
#   可选 -SkipRust：跳过 cargo（复用既有 .so；不得用于交付包）
#
# 共同 Gate：目录/分支守卫 · 平台标识（HIGHER_TARGET_PLATFORM=android）·
#   dist meta=android · compile target=android · .so 禁 devUrl · 图标同步。
# Release 追加：tauri.properties 版本再生成（唯一真相=tauri.conf.json）·
#   keystore.properties 签名 · aapt 身份/ABI/Manifest Freeze · apksigner 证书 ·
#   RC metadata（sourceFingerprint/abi_profile/…）· 体积 Gate · 包体审计。
#
# Artifact Parity Gates（F1 起保留，F5 起 UNIVERSAL_ABI 更名 ABI_PROFILE）：
#   SOURCE_FINGERPRINT（meta.sourceFingerprint 64hex + PS 端独立复算一致）
#   ANDROID_PLATFORM / DIST_FRESH_BUILD（Release 先删 dist/ 重建）
#   DIST_ASSETS_PARITY（dist→gen assets 清空后 mirror，逐文件 SHA-256 全等）
#   APK_ASSETS_PARITY（APK 内 assets/*[非 dexopt] 与 dist 逐文件 SHA-256 全等）
#   MOBILE_SHELL_META（meta.mobileShellRevision = mobile-ai-bottomnav-v1，
#     APK 内 higher-build-meta.json 与 dist 逐字节一致）
#   PACKAGE_IDENTITY / ABI_PROFILE / SIGNING / LOCALHOST_SCAN
#   证据落盘 rc/Higher-v<ver>-rc-gates.txt（Promotion 前置校验物）
# ============================================================================
[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Debug',
    # Release 阶段：RC=正式候选（唯一 Stable 来源）；Debug 忽略此参数
    [ValidateSet('RC')]
    [string]$Channel = 'RC',
    # Public = arm64-v8a + armeabi-v7a（正式用户，无 x86/x86_64）
    # InternalUniversal = 三 ABI（内部 QA / 模拟器）
    [ValidateSet('Public', 'InternalUniversal')]
    [string]$DistributionProfile = 'Public',
    [switch]$SkipRust
)

$ErrorActionPreference = 'Stop'

# ---------- 常量 ----------
$SdkDir    = "$env:LOCALAPPDATA\Android\Sdk"
$NdkDir    = "$SdkDir\ndk\26.1.10909125"
$NdkBin    = "$NdkDir\toolchains\llvm\prebuilt\windows-x86_64\bin"
$RepoRoot  = $PSScriptRoot | Split-Path
$SrcTauri  = Join-Path $RepoRoot 'src-tauri'
$GenAndroid = Join-Path $SrcTauri 'gen\android'
$AppDir    = Join-Path $GenAndroid 'app'
$AssetsDir = Join-Path $AppDir 'src\main\assets'
$ResDir    = Join-Path $AppDir 'src\main\res'
$JniLibs   = Join-Path $AppDir 'src\main\jniLibs\arm64-v8a'
$IcoSrc    = Join-Path $SrcTauri 'icons\android'
$Jdk21     = Join-Path $RepoRoot '.toolchain\jdk-21.0.12.1+1'
$ReleaseOut= Join-Path $RepoRoot 'release\android'
$KeyProps  = Join-Path $GenAndroid 'keystore.properties'
$IsRelease = $Configuration -eq 'Release'

# ---------- ABI Profile（DEV-MOBILE-005 §二/§三：禁止硬编码单一 universal）----------
# triple → (clang 前缀, jniLibs ABI 目录)；与 RustPlugin -PabiList/-ParchList/-PtargetList 并行集合一致
$AllAbiMatrix = @(
    @{ T='aarch64-linux-android';   Clang='aarch64-linux-android24-clang.cmd';      Dir='arm64-v8a'   },
    @{ T='armv7-linux-androideabi'; Clang='armv7a-linux-androideabi24-clang.cmd';   Dir='armeabi-v7a' },
    @{ T='x86_64-linux-android';    Clang='x86_64-linux-android24-clang.cmd';       Dir='x86_64'     }
)
# Public = 正式手机用户（arm64-v8a + armeabi-v7a）；InternalUniversal = 内部 QA/模拟器（+ x86_64）
$ProfileMatrix = if ($DistributionProfile -eq 'Public') { @($AllAbiMatrix[0], $AllAbiMatrix[1]) } else { $AllAbiMatrix }
$ProfileAbis   = ($ProfileMatrix | ForEach-Object { $_.Dir }) -join ','
$ProfileArchs  = (@('arm64', 'arm') + $(if ($DistributionProfile -eq 'InternalUniversal') { @('x86_64') } else { @() })) -join ','
$ProfileTargets= (@('aarch64', 'armv7') + $(if ($DistributionProfile -eq 'InternalUniversal') { @('x86_64') } else { @() })) -join ','
# 输出目录：RC → rc/（Stable 只能由 Promotion 产生）；InternalUniversal → internal/
$IsInternalProfile = $DistributionProfile -eq 'InternalUniversal'
$ChannelName = if ($IsInternalProfile) { 'Internal' } else { "RC($Channel)" }

function Step($n, $msg) { Write-Host ("[Higher-Android {0,2}] {1}" -f $n, $msg) -ForegroundColor Cyan }
function Fail($msg)    { Write-Host "[Higher-Android] FAIL: $msg" -ForegroundColor Red; exit 1 }

# DEV-MOBILE-004-F1 §四：PS 端独立复算 source fingerprint（与 vite.config.ts 同算法：
# src/** 递归 + index.html + vite.config.ts + package.json + scripts/build-android-frontend.mjs，
# 路径稳定排序，每文件 "relpath\nsha256\n" 聚合 SHA-256）
function Get-SourceFingerprintPS {
    $files = New-Object System.Collections.Generic.List[string]
    $srcRoot = Join-Path $RepoRoot 'src'
    Get-ChildItem $srcRoot -Recurse -File | ForEach-Object {
        $files.Add($_.FullName.Substring($RepoRoot.Length + 1).Replace('\', '/'))
    }
    foreach ($f in @('index.html', 'vite.config.ts', 'package.json', 'scripts/build-android-frontend.mjs')) {
        $files.Add($f)
    }
    $sorted = $files.ToArray()
    [System.Array]::Sort($sorted, [System.StringComparer]::Ordinal)  # 与 JS Array.sort()（UTF-16 codepoint）一致
    $agg = [System.Security.Cryptography.SHA256]::Create()
    $aggBytes = New-Object System.Collections.Generic.List[byte]
    $enc = [Text.Encoding]::UTF8
    foreach ($f in $sorted) {
        $line1 = $enc.GetBytes($f + "`n")
        $h = [System.Security.Cryptography.SHA256]::Create()
        $hb = $h.ComputeHash([IO.File]::ReadAllBytes((Join-Path $RepoRoot ($f -replace '/', '\'))))
        $h.Dispose()
        $line2 = $enc.GetBytes(([BitConverter]::ToString($hb)).Replace('-', '').ToLowerInvariant() + "`n")
        $aggBytes.AddRange($line1); $aggBytes.AddRange($line2)
    }
    $out = $agg.ComputeHash($aggBytes.ToArray()); $agg.Dispose()
    return ([BitConverter]::ToString($out)).Replace('-', '').ToLowerInvariant()
}

$saved = @{}
foreach ($k in 'TAURI_ENV_PLATFORM','HIGHER_TARGET_PLATFORM','HIGHER_RELEASE_BUILD','ANDROID_HOME','NDK_HOME',
               'ANDROID_NDK_HOME','JAVA_HOME','CC_aarch64_linux_android',
               'CXX_aarch64_linux_android','AR_aarch64_linux_android',
               'CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER','TAURI_CONFIG') {
    $saved[$k] = [Environment]::GetEnvironmentVariable($k)
}

try {
    # ---------- 1-3 守卫 ----------
    Step 1 "验证施工目录 = Higher-Android（$Configuration）"
    if ($RepoRoot -notmatch 'Higher-Android$') { Fail "当前目录不是 Higher-Android：$RepoRoot" }
    Step 2 "验证分支 = main / android/dev / integrate/*（DEV-INTEGRATE-001：main=双平台 canonical mainline，可直接构建 Android RC）"
    $branch = git -C $RepoRoot rev-parse --abbrev-ref HEAD
    $branchAllowed = ($branch -eq 'main') -or ($branch -eq 'android/dev') -or ($branch -like 'integrate/*')
    if (-not $branchAllowed) { Fail "当前分支 = $branch（仅允许 main / android/dev / integrate/*）" }
    Step 3 "确认不在 Higher-Windows 工作树"
    if ($RepoRoot -match 'Higher-Windows') { Fail "禁止在 Higher-Windows 施工" }

    # ---------- 版本（唯一真相 = tauri.conf.json）----------
    $conf = Get-Content (Join-Path $SrcTauri 'tauri.conf.json') -Raw | ConvertFrom-Json
    $Version = $conf.version
    $vParts = $Version.Split('.') | ForEach-Object { [int]$_ }
    $VersionCode = $vParts[0] * 1000000 + $vParts[1] * 1000 + $vParts[2]
    Step 3.5 "版本：$Version（versionCode=$VersionCode）→ 生成 tauri.properties"
    @(
        '// THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.',
        "// Managed by scripts/Build-Higher-Android.ps1 (truth = tauri.conf.json v$Version)",
        "tauri.android.versionName=$Version",
        "tauri.android.versionCode=$VersionCode"
    ) | Set-Content (Join-Path $AppDir 'tauri.properties') -Encoding ascii

    if ($IsRelease) {
        Step 4R "Release：keystore.properties 存在性检查"
        if (-not (Test-Path $KeyProps)) {
            Fail "Release 需要 keystore.properties（先完成 HUMAN SECRET GATE 并填写 password）"
        }
        $kp = Get-Content $KeyProps -Raw
        if ($kp -match '__FILL_ME__') { Fail "keystore.properties password 未填写（__FILL_ME__）" }
        if (-not (Test-Path "$env:USERPROFILE\.higher-secrets\android\higher-release.jks")) {
            Fail "keystore 缺失：~\.higher-secrets\android\higher-release.jks"
        }
        Write-Host "        keystore OK（password [REDACTED]）"
    }

    # ---------- 4-8 环境 ----------
    Step 4 "HIGHER_TARGET_PLATFORM=android + TAURI_ENV_PLATFORM=android + HIGHER_RELEASE_BUILD=1"
    $env:HIGHER_TARGET_PLATFORM = 'android'
    $env:TAURI_ENV_PLATFORM = 'android'
    $env:HIGHER_RELEASE_BUILD = '1'

    Step 5 "系统 Android SDK：$SdkDir"
    if (-not (Test-Path "$SdkDir\platforms\android-36")) { Fail "缺少 platforms;android-36" }
    if (-not (Test-Path "$SdkDir\build-tools\35.0.0"))  { Fail "缺少 build-tools;35.0.0" }
    $env:ANDROID_HOME = $SdkDir

    Step 6 "NDK 26.1.10909125"
    if (-not (Test-Path $NdkBin)) { Fail "NDK 工具链缺失：$NdkBin" }
    $env:NDK_HOME = $NdkDir
    $env:ANDROID_NDK_HOME = $NdkDir

    Step 7 "JDK21（Gradle 8.14.3 兼容）：$Jdk21"
    if (-not (Test-Path "$Jdk21\bin\java.exe")) { Fail "JDK21 缺失（.toolchain\jdk-21.0.12.1+1）" }
    $env:JAVA_HOME = $Jdk21

    Step 8 "NDK 交叉编译器 env（cargo aarch64）"
    $env:CC_aarch64_linux_android  = "$NdkBin\clang.exe"
    $env:CXX_aarch64_linux_android = "$NdkBin\clang++.exe"
    $env:AR_aarch64_linux_android  = "$NdkBin\llvm-ar.exe"
    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$NdkBin\aarch64-linux-android24-clang.cmd"
    # 生产 .so codegen 覆盖 devUrl=null（二进制内禁 localhost:1420 字串）
    $env:TAURI_CONFIG = '{"build":{"devUrl":null}}'

    # ---------- 9-10 前端 ----------
    $distRoot = Join-Path $RepoRoot 'dist'
    if ($IsRelease) {
        Step 8.5 "Release：安全删除 dist/（Clean Frontend Build，禁复用旧 dist）"
        if (Test-Path $distRoot) { Remove-Item $distRoot -Recurse -Force }
        $distCleanedAt = Get-Date
        Write-Host "        dist/ removed at $($distCleanedAt.ToString('yyyy-MM-dd HH:mm:ss'))"
    }

    Step 9 "Android frontend build（vite + platform=android + release-hygiene）"
    Push-Location $RepoRoot
    node scripts\build-android-frontend.mjs
    if ($LASTEXITCODE -ne 0) { Pop-Location; Fail "build-android-frontend.mjs 失败" }
    Pop-Location

    Step 10 "校验 build platform（meta gate）"
    $metaFile = Join-Path $distRoot 'higher-build-meta.json'
    if (-not (Test-Path $metaFile)) { Fail "ANDROID_FRONTEND_PLATFORM_MISMATCH：$metaFile 不存在" }
    $meta = Get-Content $metaFile -Raw | ConvertFrom-Json
    if ($meta.platform -ne 'android') { Fail "ANDROID_FRONTEND_PLATFORM_MISMATCH：dist platform = '$($meta.platform)'" }
    Write-Host "        dist platform = android OK"

    # DEV-MOBILE-004-F1 Gate：SOURCE_FINGERPRINT（meta vs PS 独立复算，双实现同算法）
    if (-not $meta.sourceFingerprint -or $meta.sourceFingerprint -notmatch '^[0-9a-f]{64}$') {
        Fail "SOURCE_FINGERPRINT：dist meta 无有效 sourceFingerprint（vite.config.ts 未产出？）"
    }
    $fpPS = Get-SourceFingerprintPS
    if ($fpPS -ne $meta.sourceFingerprint) {
        Fail "SOURCE_FINGERPRINT：PS 复算 $fpPS != meta $($meta.sourceFingerprint)（构建期间源码变化？）"
    }
    $SrcFingerprint = $meta.sourceFingerprint
    Write-Host "        SOURCE_FINGERPRINT PASS：$SrcFingerprint"

    # DEV-MOBILE-004-F1 Gate：MOBILE_SHELL_META（源级：dist meta 的 revision 契约）
    if ($meta.mobileShellRevision -ne 'mobile-ai-bottomnav-v1') {
        Fail "MOBILE_SHELL_META：dist meta mobileShellRevision = '$($meta.mobileShellRevision)'（期望 mobile-ai-bottomnav-v1）"
    }
    Write-Host "        MOBILE_SHELL_META (dist) PASS：mobileShellRevision=$($meta.mobileShellRevision)"

    # DEV-MOBILE-004-F1 Gate：DIST_FRESH_BUILD（Release 时 dist 必须晚于清理时间重建）
    if ($IsRelease) {
        if ((Get-Item $metaFile).LastWriteTime -lt $distCleanedAt) {
            Fail "DIST_FRESH_BUILD：dist meta 早于 dist/ 删除时间（复用了旧 dist）"
        }
        Write-Host "        DIST_FRESH_BUILD PASS：dist 重建于 $($distCleanedAt.ToString('HH:mm:ss')) 之后"
    }

    # Shell 编译目标 Gate（__HIGHER_TARGET_PLATFORM__ → dataset var = "android"）
    $markerFile = Get-ChildItem (Join-Path $RepoRoot 'dist\assets') -Filter *.js -ErrorAction SilentlyContinue |
        Where-Object { (Get-Content $_.FullName -Raw) -match 'dataset\.higherPlatform' } | Select-Object -First 1
    if (-not $markerFile) { Fail "ANDROID_DESKTOP_SHELL_LEAK：dist JS 无 higherPlatform 标记" }
    $markerJs = Get-Content $markerFile.FullName -Raw
    $varMatch = [regex]::Match($markerJs, 'dataset\.higherPlatform=([A-Za-z_$][\w$]*)')
    if (-not $varMatch.Success) { Fail "ANDROID_DESKTOP_SHELL_LEAK：无法解析 higherPlatform 变量" }
    $platVar = $varMatch.Groups[1].Value
    if ($markerJs -notmatch ('[''"`"]?' + [regex]::Escape($platVar) + '[''"`"]?\s*=\s*[''"`"]android[''"`"]')) {
        Fail "ANDROID_DESKTOP_SHELL_LEAK：编译目标非 android（$platVar ≠ android）"
    }
    Write-Host "        compile target = android OK（$platVar=`"android`"）"

    # ---------- 11 cargo（DEV-MOBILE-005：矩阵 = DistributionProfile 决定）----------
    $cargoProfile = if ($IsRelease) { 'release' } else { 'debug' }
    # Debug 保持既有开发策略（仅 arm64 快速回归，不因本任务重构）
    $AbiMatrix = if ($IsRelease) { $ProfileMatrix } else { @($AllAbiMatrix[0]) }
    if (-not $SkipRust) {
        foreach ($abi in $AbiMatrix) {
            Step 11 "cargo build --target $($abi.T) --features custom-protocol（$cargoProfile）"
            $tr = $abi.T.Replace('-', '_').ToUpperInvariant() # CARGO_TARGET_<TR>_LINKER
            $envCC = "CC_" + $abi.T
            $envCXX = "CXX_" + $abi.T
            $envAR = "AR_" + $abi.T
            $saved[$envCC] = [Environment]::GetEnvironmentVariable($envCC)
            $saved[$envCXX] = [Environment]::GetEnvironmentVariable($envCXX)
            $saved[$envAR] = [Environment]::GetEnvironmentVariable($envAR)
            Set-Item "Env:CARGO_TARGET_${tr}_LINKER" "$NdkBin\$($abi.Clang)"
            Set-Item "Env:$envCC"  "$NdkBin\clang.exe"
            Set-Item "Env:$envCXX" "$NdkBin\clang++.exe"
            Set-Item "Env:$envAR"  "$NdkBin\llvm-ar.exe"
            Push-Location $SrcTauri
            cargo build --target $abi.T --features custom-protocol $(if ($IsRelease) { '--release' })
            $cargoCode = $LASTEXITCODE
            Pop-Location
            if ($cargoCode -ne 0) { Fail "cargo $($abi.T) 构建失败" }

            # P0 Gate：生产 .so 禁 devUrl（含 127.0.0.1 变体）
            $soThis = Join-Path $SrcTauri ("target\" + $abi.T + "\$cargoProfile\libapp_lib.so")
            $soText = [IO.File]::ReadAllText($soThis, [Text.Encoding]::GetEncoding('ISO-8859-1'))
            foreach ($dev in @('localhost:1420', '127.0.0.1:1420')) {
                if ($soText.Contains($dev)) { Fail "ANDROID_DEV_URL_IN_BINARY：$($abi.T) .so 含 $dev" }
            }
            Write-Host "        $($abi.Dir) devUrl gate OK"
        }
        # Debug=arm64 快速回归；Release=$DistributionProfile（Public 2 ABI / InternalUniversal 3 ABI）
        $NeedAbis = $AbiMatrix
    } else {
        Step 11 "SkipRust：复用既有 .so（不得用于最终交付包）"
        $NeedAbis = $AbiMatrix
    }

    # ---------- 12 .so → jniLibs ----------
    Step 12 "同步 libapp_lib.so → jniLibs（$($NeedAbis.Count) 个 ABI）"
    New-Item -ItemType Directory -Force -Path $JniLibs | Out-Null
    foreach ($abi in $NeedAbis) {
        $srcSo = Join-Path $SrcTauri ("target\" + $abi.T + "\$cargoProfile\libapp_lib.so")
        if (-not (Test-Path $srcSo)) { Fail "Rust 产物缺失：$srcSo" }
        $dstDir = Join-Path $AppDir ("src\main\jniLibs\" + $abi.Dir)
        New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
        Copy-Item $srcSo (Join-Path $dstDir 'libapp_lib.so') -Force
    }
    # 清理不在当前 profile 集合内的旧 jniLibs 目录（DEV-MOBILE-005：Public 构建必须移除
    # 历史 x86_64 残留——防止旧 .so 经 workspace 状态混入正式包）
    Get-ChildItem (Join-Path $AppDir 'src\main\jniLibs') -Directory |
        Where-Object { $_.Name -notin ($ProfileMatrix.Dir) } |
        Remove-Item -Recurse -Force

    # ---------- 13 assets ----------
    Step 13 "同步 dist → gen assets（清空后完整 mirror，禁覆盖式残留）"
    New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null
    Get-ChildItem $AssetsDir | Remove-Item -Recurse -Force
    Copy-Item (Join-Path $distRoot '*') $AssetsDir -Recurse -Force
    $assetsSyncTime = Get-Date

    # DEV-MOBILE-004-F1 Gate：DIST_ASSETS_PARITY（文件集合 + 逐文件 SHA-256 全等）
    function Get-DirShaMap([string]$dir) {
        $map = @{}
        Get-ChildItem $dir -Recurse -File | ForEach-Object {
            $rel = $_.FullName.Substring($dir.Length + 1).Replace('\', '/').ToLowerInvariant()
            $map[$rel] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        return $map
    }
    $distMap = Get-DirShaMap $distRoot
    $assetsMap = Get-DirShaMap $AssetsDir
    $onlyDist = @($distMap.Keys | Where-Object { -not $assetsMap.ContainsKey($_) })
    $onlyAssets = @($assetsMap.Keys | Where-Object { -not $distMap.ContainsKey($_) })
    $diffHash = @($distMap.Keys | Where-Object { $assetsMap.ContainsKey($_) -and $assetsMap[$_] -ne $distMap[$_] })
    if ($onlyDist.Count -or $onlyAssets.Count -or $diffHash.Count) {
        Fail ("DIST_ASSETS_PARITY：dist({0} files) != gen assets({1} files)；onlyDist={2} onlyAssets={3} hashDiff={4} {5}" -f `
            $distMap.Count, $assetsMap.Count, $onlyDist.Count, $onlyAssets.Count, $diffHash.Count,
            "$(if ($onlyAssets.Count) { '残留:' + ($onlyAssets -join ',') })")
    }
    Write-Host ("        DIST_ASSETS_PARITY PASS：{0} 个文件逐文件 SHA-256 全等" -f $distMap.Count)

    # ---------- 14 图标 ----------
    Step 14 "同步图标资源（src-tauri/icons/android → gen res）"
    foreach ($d in 'mipmap-mdpi','mipmap-hdpi','mipmap-xhdpi','mipmap-xxhdpi','mipmap-xxxhdpi','mipmap-anydpi-v26') {
        $dst = Join-Path $ResDir $d
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item (Join-Path $IcoSrc "$d\*") $dst -Force
    }
    Copy-Item (Join-Path $IcoSrc 'values\ic_launcher_background.xml') (Join-Path $ResDir 'values') -Force
    Remove-Item (Join-Path $ResDir 'drawable\ic_launcher_background.xml') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $ResDir 'drawable-v24\ic_launcher_foreground.xml') -Force -ErrorAction SilentlyContinue

    # ---------- 15 Gradle ----------
    # DEV-MOBILE-004：Release = Universal（单文件全 ABI）；Debug = arm64 快速回归
    if ($IsRelease) {
        # DEV-MOBILE-004-F1 §七：禁止 Gradle 复用旧 packaging 输出
        # （曾观察 package 任务增量复用旧 APK；Release 正确性优先于打包速度）
        foreach ($stale in @(
            (Join-Path $AppDir 'build\outputs\apk\universal'),
            (Join-Path $AppDir 'build\intermediates\apk')
        )) {
            if (Test-Path $stale) {
                Remove-Item $stale -Recurse -Force
                Write-Host "        removed stale gradle output: $stale"
            }
        }
    }
    $rustTaskX = if ($IsRelease) { 'rustBuildUniversalRelease' } else { 'rustBuildArm64Debug' }
    Step 15 "Gradle $(if ($IsRelease) { "assembleUniversalRelease（$DistributionProfile）" } else { 'assembleArm64Debug' })"
    Push-Location $GenAndroid
    if ($IsRelease) {
        # RustPlugin -P 并行集合（universal flavor ndk.abiFilters ← abiList）
        .\gradlew.bat assembleUniversalRelease -x $rustTaskX `
            "-PabiList=$ProfileAbis" `
            "-ParchList=$ProfileArchs" `
            "-PtargetList=$ProfileTargets" `
            "-Pkotlin.compiler.execution.strategy=in-process" --console=plain
    } else {
        .\gradlew.bat assembleArm64Debug -x $rustTaskX `
            "-Pkotlin.compiler.execution.strategy=in-process" --console=plain
    }
    $gradleCode = $LASTEXITCODE
    Pop-Location
    if ($gradleCode -ne 0) { Fail "Gradle 构建失败" }

    # ---------- 16 产物验证 ----------
    Step 16 "APK Artifact 验证（$Configuration）"
    $apkDir = Join-Path $AppDir ("build\outputs\apk\" + $(if ($IsRelease) { 'universal\release' } else { 'arm64\debug' }))
    $apk = Get-ChildItem $apkDir -Filter *.apk | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $apk) { Fail "APK 不存在：$apkDir" }
    if ($apk.LastWriteTime -lt $assetsSyncTime) { Fail "APK 时间戳早于 assets 同步（旧前端）" }
    $aapt2 = Join-Path $SdkDir 'build-tools\35.0.0\aapt2.exe'
    $badging = & $aapt2 dump badging $apk.FullName 2>$null
    $pkgLine = ($badging | Select-String "^package:").Line
    $abiLine = ($badging | Select-String 'native-code').Line
    Write-Host "        $pkgLine"
    if ($IsRelease) {
        if ($pkgLine -notmatch "name='com\.higher\.android' ") { Fail "Release applicationId 异常：$pkgLine" }
        if ($pkgLine -notmatch ("versionName='" + [regex]::Escape($Version) + "'")) { Fail "versionName 异常（期望 $Version）：$pkgLine" }
    } else {
        if ($pkgLine -notmatch 'com\.higher\.android\.debug') { Fail "applicationId 异常：$pkgLine" }
    }
    if ($IsRelease) {
        # DEV-MOBILE-005 §八：ABI_PROFILE Gate（按 DistributionProfile 断言）
        #   Public：arm64-v8a PRESENT · armeabi-v7a PRESENT · x86_64/x86/i686 ABSENT
        #   InternalUniversal：三者 PRESENT · x86/i686 ABSENT
        foreach ($need in $ProfileMatrix.Dir) {
            if ($abiLine -notmatch [regex]::Escape($need)) { Fail "ABI_PROFILE($DistributionProfile) 缺少 $need：$abiLine" }
        }
        foreach ($bad in @("'x86'", 'i686')) {
            if ($abiLine -match $bad) { Fail "ABI_PROFILE：不得含 x86/i686：$abiLine" }
        }
        if ($DistributionProfile -eq 'Public' -and $abiLine -match 'x86_64') {
            Fail "ABI_PROFILE(Public)：x86_64 不得进入正式用户包：$abiLine"
        }
        Write-Host "        ABI_PROFILE($DistributionProfile) PASS：$abiLine"
    } else {
        if ($abiLine -notmatch 'arm64-v8a') { Fail "ABI 异常：$abiLine" }
        foreach ($bad in @('x86_64', 'x86', 'armeabi-v7a')) {
            if ($abiLine -match $bad) { Fail "Debug 回归包不得包含 $bad：$abiLine" }
        }
    }

    if ($IsRelease) {
        # ---------- 16.5 APK Frontend Truth Gate（F1 §八/§九） ----------
        Step 16.5 "APK Frontend Truth：APK assets vs dist 逐文件 SHA-256 + meta 回读"
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [IO.Compression.ZipFile]::OpenRead($apk.FullName)
        try {
            $apkAssets = @{}   # rel(lower) -> sha256（排除 AGP 注入的 dexopt/*）
            $apkMetaRaw = $null
            $shaProv = [System.Security.Cryptography.SHA256]::Create()
            foreach ($e in $zip.Entries) {
                if (-not $e.FullName.StartsWith('assets/') -or $e.Name -eq '') { continue }
                $rel = $e.FullName.Substring(7).Replace('\', '/').ToLowerInvariant()
                if ($rel.StartsWith('dexopt/')) { continue }
                $s = $e.Open()
                $h = ([BitConverter]::ToString($shaProv.ComputeHash($s))).Replace('-', '').ToLowerInvariant()
                $s.Dispose()
                $apkAssets[$rel] = $h
                if ($rel -eq 'higher-build-meta.json') {
                    $r = New-Object IO.StreamReader($e.Open()); $apkMetaRaw = $r.ReadToEnd(); $r.Dispose()
                }
            }
            $shaProv.Dispose()
            # Gate：APK_ASSETS_PARITY
            $onlyD = @($distMap.Keys | Where-Object { -not $apkAssets.ContainsKey($_) })
            $onlyA = @($apkAssets.Keys | Where-Object { -not $distMap.ContainsKey($_) })
            $diffH = @($distMap.Keys | Where-Object { $apkAssets.ContainsKey($_) -and $apkAssets[$_] -ne $distMap[$_] })
            if ($onlyD.Count -or $onlyA.Count -or $diffH.Count) {
                Fail ("APK_ASSETS_PARITY：APK assets({0}) != dist({1})；onlyDist={2} onlyApk={3} hashDiff={4} 样例:{5}" -f `
                    $apkAssets.Count, $distMap.Count, $onlyD.Count, $onlyA.Count, $diffH.Count,
                    "$(@($onlyD + $onlyA + $diffH) | Select-Object -First 5)")
            }
            Write-Host ("        APK_ASSETS_PARITY PASS：{0} 个前端文件 SHA-256 全等" -f $distMap.Count)
            # Gate：MOBILE_SHELL_META（APK 内 meta 与 dist meta 逐字节一致）
            $distMetaRaw = [IO.File]::ReadAllText($metaFile)
            if (-not $apkMetaRaw -or ($apkMetaRaw.Trim() -ne $distMetaRaw.Trim())) {
                Fail "MOBILE_SHELL_META：APK 内 higher-build-meta.json 与 dist 不一致（打包复用旧资产？）"
            }
            $apkMeta = $apkMetaRaw | ConvertFrom-Json
            if ($apkMeta.sourceFingerprint -ne $SrcFingerprint -or $apkMeta.mobileShellRevision -ne 'mobile-ai-bottomnav-v1') {
                Fail "MOBILE_SHELL_META：APK meta 字段与当前构建不符"
            }
            Write-Host "        MOBILE_SHELL_META PASS：APK meta == dist meta（sourceFingerprint=$($apkMeta.sourceFingerprint.Substring(0,12))… revision=$($apkMeta.mobileShellRevision)）"
        } finally { $zip.Dispose() }
    }

    if ($IsRelease) {
        # ---------- 16.7 Manifest Freeze（DEV-MOBILE-005 §十）----------
        Step 16.7 "Manifest Freeze（package/version/sdk/cleartext/launcher/权限审计）"
        $sdkLine = ($badging | Select-String "^minSdkVersion:").Line
        $tgtLine = ($badging | Select-String "^targetSdkVersion:").Line
        if ($sdkLine -notmatch "minSdkVersion:'24'") { Fail "Manifest Freeze：minSdk 异常（期望 24）：$sdkLine" }
        if ($tgtLine -notmatch "targetSdkVersion:'36'") { Fail "Manifest Freeze：targetSdk 异常（期望 36）：$tgtLine" }
        if ($pkgLine -notmatch ("versionCode='" + $VersionCode + "'")) { Fail "Manifest Freeze：versionCode 非 canonical（期望 $VersionCode）" }
        $xmltree = & $aapt2 dump xmltree --file AndroidManifest.xml $apk.FullName 2>$null
        $xmlTxt = $xmltree -join "`n"
        if ($xmlTxt -match 'usesCleartextTraffic\(0x[0-9a-f]+\)=true') { Fail "Manifest Freeze：usesCleartextTraffic=true" }
        if ($xmlTxt -notmatch 'usesCleartextTraffic\(0x[0-9a-f]+\)=false') { Fail "Manifest Freeze：未显式 usesCleartextTraffic=false" }
        if ($xmlTxt -notmatch 'android\.intent\.action\.MAIN') { Fail "Manifest Freeze：MAIN intent 缺失" }
        if ($xmlTxt -notmatch 'android\.intent\.category\.LAUNCHER') { Fail "Manifest Freeze：LAUNCHER category 缺失" }
        if ($xmlTxt -match 'LEANBACK_LAUNCHER') { Fail "Manifest Freeze：不得存在 LEANBACK_LAUNCHER" }
        Write-Host "        minSdk=24 targetSdk=36 cleartext=false MAIN+LAUNCHER OK / 无 LEANBACK"
        # 权限审计（只记录来源与用途，不删——P2 决策留给报告）
        $permLines = @($badging | Select-String "^uses-permission:" | ForEach-Object { ($_.Line -replace "^uses-permission: name='", '') -replace "'.*$", '' })
        foreach ($p in $permLines) { Write-Host "        permission: $p" }
        if (-not $permLines) { Fail "Manifest Freeze：badging 未列出任何 uses-permission（解析异常）" }
        $PermsAudit = $permLines
    }

    if ($IsRelease) {
        # ---------- 17 apksigner ----------
        Step 17 "apksigner 签名验证"
        $apksigner = Join-Path $SdkDir 'build-tools\35.0.0\apksigner.bat'
        $signOut = & $apksigner verify --verbose --print-certs $apk.FullName 2>&1
        $signTxt = $signOut -join "`n"
        if ($LASTEXITCODE -ne 0 -or $signTxt -match 'DOES NOT VERIFY') { Fail "签名验证失败" }
        $sha256 = ($signTxt | Select-String 'SHA-256 digest:').Line
        Write-Host "        signer $sha256"
        $signerCertSha256 = @(($signTxt -split "`r?`n") | Where-Object { $_ -match 'Signer #1 certificate SHA-256 digest:' })[0] -replace '^.*digest:\s*',''

        # ---------- 18 发布物（DEV-MOBILE-005 §四 阶段模型）+ 包体审计 ----------
        #  RC(Public)           → release/android/rc/Higher-v<ver>-rc.apk（待真机验收 → Promotion）
        #  InternalUniversal    → release/android/internal/Higher-v<ver>-internal-universal.apk（QA/模拟器）
        #  Stable( Higher-v<ver>.apk ) 禁止在此产生——仅 Promote-Higher-Android-RC.ps1 字节级复制
        $internal = Join-Path $ReleaseOut 'internal'
        New-Item -ItemType Directory -Force -Path $internal | Out-Null
        Get-ChildItem $ReleaseOut -Filter 'Higher-v*-arm64.apk' -ErrorAction SilentlyContinue |
            Move-Item -Destination $internal -Force
        if ($IsInternalProfile) {
            $ChannelDir = $internal
            $ArtifactName = "Higher-v$Version-internal-universal"
        } else {
            $ChannelDir = Join-Path $ReleaseOut 'rc'
            $ArtifactName = "Higher-v$Version-rc"
        }
        New-Item -ItemType Directory -Force -Path $ChannelDir | Out-Null
        Step 18 "发布物：$ChannelName → $ChannelDir\$ArtifactName.apk（Stable 仅由 Promotion 产生）"
        $parityApk = Join-Path $ChannelDir ("$ArtifactName.apk")
        Copy-Item $apk.FullName $parityApk -Force
        $paritySha = (Get-FileHash $parityApk -Algorithm SHA256).Hash.ToLowerInvariant()

        # DEV-MOBILE-005 §七：RC metadata（Promotion 前置校验物，含 abi_profile/abis）
        $builtAt = (Get-Item $parityApk).LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss')
        $shaLines = @(
            "$ArtifactName.apk",
            "apk_sha256:             $paritySha",
            "source_fingerprint:     $SrcFingerprint",
            "mobile_shell_revision:  mobile-ai-bottomnav-v1",
            "built_at:               $builtAt",
            "version:                $Version",
            "version_code:           $VersionCode",
            "package:                com.higher.android",
            "signer_sha256:          $signerCertSha256",
            "abi_profile:            $DistributionProfile",
            "abis:                   $ProfileAbis"
        )
        $shaLines | Set-Content (Join-Path $ChannelDir ("$ArtifactName.sha256.txt")) -Encoding utf8

        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [IO.Compression.ZipFile]::OpenRead($parityApk)
        try {
            $entries = $zip.Entries | Sort-Object Length -Descending
            $total = ($entries | Measure-Object Length -Sum).Sum
            $lib = ($entries | Where-Object { $_.FullName -like 'lib/*' } | Measure-Object Length -Sum).Sum
            $assets = ($entries | Where-Object { $_.FullName -like 'assets/*' } | Measure-Object Length -Sum).Sum
            $res = ($entries | Where-Object { $_.FullName -like 'res/*' } | Measure-Object Length -Sum).Sum
            $dex = ($entries | Where-Object { $_.FullName -like 'classes*.dex' } | Measure-Object Length -Sum).Sum
            $other = $total - $lib - $assets - $res - $dex
            $report = @()
            $report += "$ArtifactName.apk ($DistributionProfile) size report"
            $report += "generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
            $report += "APK total:    {0:N2} MB" -f ($total/1MB)
            $report += "native libs:  {0:N2} MB" -f ($lib/1MB)
            $report += "assets:       {0:N2} MB" -f ($assets/1MB)
            $report += "dex:          {0:N2} MB" -f ($dex/1MB)
            $report += "resources:    {0:N2} MB" -f ($res/1MB)
            $report += "other:        {0:N2} MB" -f ($other/1MB)
            $report += ""
            $report += "largest 30 entries:"
            $entries | Select-Object -First 30 | ForEach-Object {
                $report += ("{0,12:N0}  {1}" -f $_.Length, $_.FullName)
            }
            $report | Set-Content (Join-Path $ChannelDir ("$ArtifactName-size-report.txt")) -Encoding utf8
            $sizeMb = [math]::Round($total/1MB, 1)
            Write-Host ("        APK {0:N1} MB（libs {1:N1} / assets {2:N1} / dex {3:N1} / res {4:N1} / other {5:N1}）" -f `
                ($total/1MB), ($lib/1MB), ($assets/1MB), ($dex/1MB), ($res/1MB), ($other/1MB))
            # DEV-MOBILE-005 §十一 体积 Gate（Public：≤50 PASS / 50-60 WARN / >60 FAIL）
            if ($IsInternalProfile) {
                if ($sizeMb -gt 150) { Fail "包体 > 150MB：RELEASE HOLD（见 size-report）" }
                elseif ($sizeMb -gt 80) { Write-Host "        Internal 80-150MB：继续（报告已解释占用）" -ForegroundColor Yellow }
            } else {
                if ($sizeMb -gt 60) { Fail "SIZE_GATE(Public)：包体 > 60 MiB：HOLD（Public 预期 42-45 MiB；见 size-report）" }
                elseif ($sizeMb -gt 50) { Write-Host "        SIZE_GATE(Public)：50-60 MiB WARN（Public 预期 42-45 MiB）" -ForegroundColor Yellow }
                else { Write-Host "        SIZE_GATE(Public)：≤50 MiB PASS" }
            }
        } finally { $zip.Dispose() }

        # DEV-MOBILE-005 §六/§七：Gate 证据落盘（Promotion 前置校验“10 gates 证据存在”）
        $gateLines = @(
            "artifact:    $ArtifactName.apk",
            "profile:     $DistributionProfile",
            "fingerprint: $SrcFingerprint",
            "apk_sha256:  $paritySha",
            "generated:   $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
            "",
            "SOURCE_FINGERPRINT      PASS",
            "ANDROID_PLATFORM        PASS",
            "DIST_FRESH_BUILD        PASS",
            "DIST_ASSETS_PARITY      PASS",
            "APK_ASSETS_PARITY       PASS",
            "MOBILE_SHELL_META       PASS",
            "PACKAGE_IDENTITY        PASS",
            "ABI_PROFILE             PASS  ($ProfileAbis)",
            "SIGNING                 PASS  ($signerCertSha256)",
            "LOCALHOST_SCAN          PASS",
            "",
            "MANIFEST_FREEZE         PASS  minSdk=24 targetSdk=36 cleartext=false MAIN+LAUNCHER no-LEANBACK",
            "SIZE_GATE               $(if ($IsInternalProfile) { "INFO (internal profile)" } elseif ($sizeMb -le 50) { 'PASS' } elseif ($sizeMb -le 60) { 'WARN' } else { 'FAIL' }) ($sizeMb MiB)"
        )
        $gateLines | Set-Content (Join-Path $ChannelDir ("$ArtifactName-gates.txt")) -Encoding utf8

        Write-Host ""
        Write-Host "[Higher-Android] ARTIFACT PARITY GATES（$ChannelName · $DistributionProfile）" -ForegroundColor Green
        foreach ($g in @(
            'SOURCE_FINGERPRINT      PASS  meta==PS 复算（工作区源码聚合 SHA-256）',
            'ANDROID_PLATFORM        PASS  dist meta platform=android + compile target=android',
            'DIST_FRESH_BUILD        PASS  dist/ 删除后重建（无旧 dist 复用）',
            'DIST_ASSETS_PARITY      PASS  dist → gen assets 逐文件 SHA-256 全等（清空 mirror）',
            'APK_ASSETS_PARITY       PASS  APK assets vs dist 逐文件 SHA-256 全等',
            'MOBILE_SHELL_META       PASS  APK 内 meta == dist meta（revision=mobile-ai-bottomnav-v1）',
            $('PACKAGE_IDENTITY      PASS  com.higher.android / versionName=' + $Version + ' / versionCode=' + $VersionCode),
            $('ABI_PROFILE           PASS  ' + $ProfileAbis + $(if ($IsInternalProfile) { '' } else { '（无 x86_64/x86/i686）' })),
            'SIGNING                 PASS  apksigner verify（正式 keystore）',
            ('LOCALHOST_SCAN          PASS  ' + $(if ($IsInternalProfile) { '三' } else { '两' }) + ' ABI .so 无 localhost:1420/127.0.0.1:1420'),
            'MANIFEST_FREEZE         PASS  minSdk=24 targetSdk=36 cleartext=false MAIN+LAUNCHER no-LEANBACK'
        )) { Write-Host "  $g" }
        Write-Host ""
        Write-Host "[Higher-Android] GATES PASS → $ChannelName ARTIFACT READY（等待真机验收 → Promote）" -ForegroundColor Green
        Write-Host "  artifact   : $parityApk"
        Write-Host "  metadata   : $(Join-Path $ChannelDir ("$ArtifactName.sha256.txt"))"
        Write-Host "  gates      : $(Join-Path $ChannelDir ("$ArtifactName-gates.txt"))"
        Write-Host "  source fp  : $SrcFingerprint"
        if (-not $IsInternalProfile) {
            Write-Host "  下一步     : 真机安装 $ArtifactName.apk 验收 → .\scripts\Promote-Higher-Android-RC.ps1 字节级升级 Stable"
        }
    } else {
        Write-Host ""
        Write-Host "[Higher-Android] DEBUG BUILD SUCCESSFUL" -ForegroundColor Green
        Write-Host "  APK  : $($apk.FullName)"
        Write-Host ("  Size : {0:N1} MB" -f ($apk.Length/1MB))
    }
}
finally {
    Step 17 "恢复环境变量"
    foreach ($k in $saved.Keys) {
        $v = $saved[$k]
        if ($null -eq $v) { Remove-Item "Env:$k" -ErrorAction SilentlyContinue }
        else { Set-Item "Env:$k" $v }
    }
}
