# ============================================================================
# Promote-Higher-Android-RC.ps1（DEV-MOBILE-005 §五/§六 · RC → Stable 字节级 Promotion）
#
# 模型：SOURCE → Build RC → 全自动 Gate → rc/Higher-v<ver>-rc.apk
#       → 真人真机验收 → 【本脚本】→ release/android/Higher-v<ver>.apk
#
# 铁律：
#   SHA256(RC APK) == SHA256(Stable APK)   —— 禁止重新编译/重打包 Stable
#   Promotion 过程禁止：cargo / vite build / gradle build / 修改源码 / 修改版本 / 重新签名
#   只做"将已经过人工验收的 RC 二进制升级为正式发布物"。
#
# 用法：
#   .\scripts\Promote-Higher-Android-RC.ps1           # 真实 Promotion
#   .\scripts\Promote-Higher-Android-RC.ps1 -DryRun   # 只做全部校验 + 字节一致性演练（不落 Stable）
# ============================================================================
[CmdletBinding()]
param(
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

$SdkDir   = "$env:LOCALAPPDATA\Android\Sdk"
$Aapt2    = Join-Path $SdkDir 'build-tools\35.0.0\aapt2.exe'
$Apksigner= Join-Path $SdkDir 'build-tools\35.0.0\apksigner.bat'
$RepoRoot = $PSScriptRoot | Split-Path
$SrcTauri = Join-Path $RepoRoot 'src-tauri'
$ReleaseOut = Join-Path $RepoRoot 'release\android'
$RcDir    = Join-Path $ReleaseOut 'rc'
$SignerRoot = '0c864d48a9e422d33e6cb175b8bc65d493bcd53c37d6d0f7ebfedb04f3d33c02'   # §九 签名冻结

function Fail($msg) { Write-Host "[Promote] FAIL: $msg" -ForegroundColor Red; exit 1 }
function Ok($msg)   { Write-Host "[Promote] OK: $msg" -ForegroundColor Green }

# §七 Source Lock：与 Build-Higher-Android.ps1 同算法的独立复算
function Get-SourceFingerprintPS {
    $files = New-Object System.Collections.Generic.List[string]
    Get-ChildItem (Join-Path $RepoRoot 'src') -Recurse -File | ForEach-Object {
        $files.Add($_.FullName.Substring($RepoRoot.Length + 1).Replace('\', '/'))
    }
    foreach ($f in @('index.html', 'vite.config.ts', 'package.json', 'scripts/build-android-frontend.mjs')) {
        $files.Add($f)
    }
    $sorted = $files.ToArray()
    [System.Array]::Sort($sorted, [System.StringComparer]::Ordinal)
    $agg = [System.Security.Cryptography.SHA256]::Create()
    $aggBytes = New-Object System.Collections.Generic.List[byte]
    $enc = [Text.Encoding]::UTF8
    foreach ($f in $sorted) {
        $aggBytes.AddRange($enc.GetBytes($f + "`n"))
        $h = [System.Security.Cryptography.SHA256]::Create()
        $hb = $h.ComputeHash([IO.File]::ReadAllBytes((Join-Path $RepoRoot ($f -replace '/', '\'))))
        $h.Dispose()
        $aggBytes.AddRange($enc.GetBytes(([BitConverter]::ToString($hb)).Replace('-', '').ToLowerInvariant() + "`n"))
    }
    $out = $agg.ComputeHash($aggBytes.ToArray()); $agg.Dispose()
    return ([BitConverter]::ToString($out)).Replace('-', '').ToLowerInvariant()
}

# ---------- 0 守卫 ----------
Write-Host "[Promote] DEV-MOBILE-005 RC → Stable Promotion$(if ($DryRun) { '（DRY RUN：不落 Stable）' })" -ForegroundColor Cyan
if ($RepoRoot -notmatch 'Higher-Android$') { Fail "当前目录不是 Higher-Android：$RepoRoot" }
$branch = git -C $RepoRoot rev-parse --abbrev-ref HEAD
if ($branch -ne 'android/dev') { Fail "当前分支 = $branch（必须 android/dev）" }

# ---------- 1 定位 RC ----------
$conf = Get-Content (Join-Path $SrcTauri 'tauri.conf.json') -Raw | ConvertFrom-Json
$Version = $conf.version
$vParts = $Version.Split('.') | ForEach-Object { [int]$_ }
$VersionCode = $vParts[0] * 1000000 + $vParts[1] * 1000 + $vParts[2]
$rcApk  = Join-Path $RcDir ("Higher-v$Version-rc.apk")
$rcMeta = Join-Path $RcDir ("Higher-v$Version-rc.sha256.txt")
$rcGates= Join-Path $RcDir ("Higher-v$Version-rc-gates.txt")
Write-Host "        version=$Version (versionCode=$VersionCode)"
if (-not (Test-Path $rcApk))  { Fail "RC 不存在：$rcApk（先 Build-Higher-Android.ps1 -Configuration Release -Channel RC）" }
if (-not (Test-Path $rcMeta)) { Fail "RC metadata 缺失：$rcMeta" }
if (-not (Test-Path $rcGates)){ Fail "RC gates 证据缺失：$rcGates（RC 未通过自动 Gate）" }
Ok "RC 定位：$rcApk"

# ---------- 2 读 RC metadata ----------
$meta = @{}
foreach ($line in (Get-Content $rcMeta)) {
    if ($line -match '^([a-z_0-9]+):\s+(.+)$') { $meta[$matches[1]] = $matches[2].Trim() }
}
foreach ($k in @('apk_sha256', 'source_fingerprint', 'version', 'version_code', 'package', 'signer_sha256', 'abi_profile', 'abis', 'mobile_shell_revision')) {
    if (-not $meta[$k]) { Fail "RC metadata 缺字段：$k" }
}
Ok "metadata 读取（$(($meta.Keys | Measure-Object).Count) 字段）"

# ---------- 3 RC APK 当前 SHA == metadata ----------
$rcShaNow = (Get-FileHash $rcApk -Algorithm SHA256).Hash.ToLowerInvariant()
if ($rcShaNow -ne $meta['apk_sha256']) { Fail "RC APK 当前 SHA256 与 metadata 不一致（RC 被改动？）" }
Ok "RC SHA256 未变：$rcShaNow"

# ---------- 4 package / version ----------
$env:JAVA_HOME = Join-Path $RepoRoot '.toolchain\jdk-21.0.12.1+1'
$badging = & $Aapt2 dump badging $rcApk 2>$null
$pkgLine = ($badging | Select-String "^package:").Line
if ($pkgLine -notmatch "name='com\.higher\.android' ") { Fail "package 异常：$pkgLine" }
if ($pkgLine -notmatch ("versionName='" + [regex]::Escape($Version) + "'")) { Fail "versionName 异常（期望 $Version）：$pkgLine" }
if ($pkgLine -notmatch ("versionCode='" + $VersionCode + "'")) { Fail "versionCode 非 canonical（期望 $VersionCode）：$pkgLine" }
if ($meta['version'] -ne $Version -or [int]$meta['version_code'] -ne $VersionCode) { Fail "metadata 版本与 tauri.conf.json 不一致" }
Ok "package/version 一致（com.higher.android $Version/$VersionCode）"

# ---------- 5 签名（§九 冻结根） ----------
$signOut = & $Apksigner verify --verbose --print-certs $rcApk 2>&1
$signTxt = $signOut -join "`n"
if ($LASTEXITCODE -ne 0 -or $signTxt -match 'DOES NOT VERIFY') { Fail "apksigner verify 失败" }
$certLine = @(($signTxt -split "`r?`n") | Where-Object { $_ -match 'Signer #1 certificate SHA-256 digest:' })[0]
$cert = $certLine -replace '^.*digest:\s*',''
if ($cert -ne $SignerRoot) { Fail "签名根漂移：$cert ≠ $SignerRoot（§九 冻结）" }
if ($meta['signer_sha256'] -ne $SignerRoot) { Fail "metadata signer 与冻结根不一致" }
Ok "签名根 == 冻结根 0c864d48…d33c02"

# ---------- 6 Source Lock（§七）----------
$fpNow = Get-SourceFingerprintPS
if ($fpNow -ne $meta['source_fingerprint']) {
    Fail "RC 验收后源码已发生变化；请重新生成并重新验收 RC。`n        RC  fingerprint: $($meta['source_fingerprint'])`n        当前 fingerprint: $fpNow"
}
Ok "Source Lock：工作区 == RC 指纹 $fpNow"

# ---------- 7 Public ABI（§六-6）----------
$abiLine = ($badging | Select-String 'native-code').Line
foreach ($need in @('arm64-v8a', 'armeabi-v7a')) {
    if ($abiLine -notmatch [regex]::Escape($need)) { Fail "ABI 缺少 $need：$abiLine" }
}
foreach ($bad in @('x86_64', "'x86'", 'i686')) {
    if ($abiLine -match $bad) { Fail "Public ABI 违规（含 $bad）：$abiLine" }
}
if ($meta['abi_profile'] -ne 'Public') { Fail "RC abi_profile=$($meta['abi_profile'])（Promotion 仅接受 Public）" }
Ok "ABI_PROFILE(Public)：arm64-v8a + armeabi-v7a / 无 x86_64·x86·i686"

# ---------- 8 Gates 证据（§六-7）----------
$gatesTxt = (Get-Content $rcGates) -join "`n"
foreach ($g in @('SOURCE_FINGERPRINT', 'ANDROID_PLATFORM', 'DIST_FRESH_BUILD', 'DIST_ASSETS_PARITY',
                 'APK_ASSETS_PARITY', 'MOBILE_SHELL_META', 'PACKAGE_IDENTITY', 'ABI_PROFILE',
                 'SIGNING', 'LOCALHOST_SCAN')) {
    if ($gatesTxt -notmatch ($g + '\s+PASS')) { Fail "gates 证据缺 $g PASS：$rcGates" }
}
Ok "10 artifact gates 证据齐备（$rcGates）"

# ---------- 9 字节级 Promotion（§五）----------
if ($meta['mobile_shell_revision'] -ne 'mobile-ai-bottomnav-v1') { Fail "mobileShellRevision 异常：$($meta['mobile_shell_revision'])" }

if ($DryRun) {
    $cand = Join-Path $env:TEMP ("Higher-v$Version-stable-candidate.apk")
    Copy-Item $rcApk $cand -Force
    $candSha = (Get-FileHash $cand -Algorithm SHA256).Hash.ToLowerInvariant()
    Remove-Item $cand -Force
    if ($candSha -ne $rcShaNow) { Fail "DRY RUN：字节级复制后 SHA 不一致（$candSha）" }
    Ok "DRY RUN：RC SHA == Stable candidate SHA == $rcShaNow（未写正式 Stable）"
    Write-Host ""
    Write-Host "[Promote] DRY RUN PASS —— 全部前置校验通过。真机验收确认后去掉 -DryRun 执行正式 Promotion。" -ForegroundColor Green
    exit 0
}

$stableApk = Join-Path $ReleaseOut ("Higher-v$Version.apk")
Copy-Item $rcApk $stableApk -Force
$stableSha = (Get-FileHash $stableApk -Algorithm SHA256).Hash.ToLowerInvariant()
if ($stableSha -ne $rcShaNow) { Fail "FATAL：Stable SHA ≠ RC SHA（$stableSha ≠ $rcShaNow）—— 立即撤回此文件" }
Ok "字节级 Promotion：SHA256(RC) == SHA256(Stable) == $stableSha"

# Stable 侧文件：sha256.txt（含 promotion 溯源）+ size-report
$stableMeta = @(
    "Higher-v$Version.apk",
    "apk_sha256:             $stableSha",
    "promoted_from_rc:       Higher-v$Version-rc.apk ($($meta['apk_sha256']))",
    "promoted_at:            $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
    "source_fingerprint:     $($meta['source_fingerprint'])",
    "mobile_shell_revision:  $($meta['mobile_shell_revision'])",
    "built_at:               $($meta['built_at'])",
    "version:                $Version",
    "version_code:           $VersionCode",
    "package:                com.higher.android",
    "signer_sha256:          $($meta['signer_sha256'])",
    "abi_profile:            $($meta['abi_profile'])",
    "abis:                   $($meta['abis'])"
)
$stableMeta | Set-Content (Join-Path $ReleaseOut ("Higher-v$Version.sha256.txt")) -Encoding utf8
Copy-Item (Join-Path $RcDir ("Higher-v$Version-rc-size-report.txt")) (Join-Path $ReleaseOut ("Higher-v$Version-size-report.txt")) -Force

Write-Host ""
Write-Host "[Promote] STABLE PROMOTED（字节级，未重编译）" -ForegroundColor Green
Write-Host "  Stable : $stableApk"
Write-Host "  SHA256 : $stableSha"
Write-Host "  溯源   : promoted_from_rc = Higher-v$Version-rc.apk"
Write-Host "  普通用户只分发该文件。"
