# ============================================================================
# Clean-Higher-Android-Workspace.ps1（DEV-MOBILE-006 §十一 · 安全清理机制）
#
# 默认 DryRun（只展示：删什么/多少文件/释放多少空间）；显式 -Apply 才执行。
# -Deep 才包含 src-tauri/target/（Rust 编译缓存，删除后下次全量编译很慢）。
#
# Safe Cleanup 范围（全部为可再生生成物，前提 = Stable 已 Promotion）：
#   dist/ · .mobile-test-build/ · .ai-runtime-test-build/ · .higher/tmp/*
#   gen/android/{.gradle,build,buildSrc/{build,.gradle,.kotlin}} · app/build/
#   jniLibs/**/*.so（cargo 复制产物，保留目录） · assets/ generated frontend
#   release 收口：仅留 Stable 三件套；rc/internal 历史 APK 删除、审计文本归档 audit/<ver>/
#
# 永不触碰：src/ · src-tauri/{src,tests,icons} · tests/ · scripts/ 两个构建/晋升脚本
#   gen/android 工程源（gradle/buildSrc src/Manifest/res/java）· branding/ · .higher/{archive,*.md}
#   tauri.conf/tauri.android.conf/Cargo.{toml,lock}/package*.json · .toolchain/ · node_modules/
#   ~/.higher-secrets/（任何 jks/keystore） · Stable APK 本体
# ============================================================================
[CmdletBinding()]
param(
    [switch]$Apply,
    [switch]$Deep
)

$ErrorActionPreference = 'Stop'
$RepoRoot  = $PSScriptRoot | Split-Path
$ReleaseOut= Join-Path $RepoRoot 'release\android'
$GenAndroid= Join-Path $RepoRoot 'src-tauri\gen\android'

function Fail($msg) { Write-Host "[Clean] FAIL: $msg" -ForegroundColor Red; exit 1 }

# ---------- 守卫 ----------
if ($RepoRoot -notmatch 'Higher-Android$') { Fail "当前目录不是 Higher-Android：$RepoRoot" }
$branch = git -C $RepoRoot rev-parse --abbrev-ref HEAD
if ($branch -ne 'android/dev') { Fail "当前分支 = $branch（必须 android/dev）" }

# Stable 前提：当前版本 Stable APK 存在且 SHA == 自身 sha256.txt 记录
$conf = Get-Content (Join-Path $RepoRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$Version = $conf.version
$stableApk  = Join-Path $ReleaseOut ("Higher-v$Version.apk")
$stableMeta = Join-Path $ReleaseOut ("Higher-v$Version.sha256.txt")
if (-not (Test-Path $stableApk)) { Fail "Stable 不存在：$stableApk（先完成 RC 验收 + Promotion，再清理）" }
$stableSha = (Get-FileHash $stableApk -Algorithm SHA256).Hash.ToLowerInvariant()
if (Test-Path $stableMeta) {
    $recSha = ((Get-Content $stableMeta) | Where-Object { $_ -match '^apk_sha256:' }) -replace '^apk_sha256:\s*',''
    if ($recSha -and $recSha -ne $stableSha) { Fail "Stable SHA 与 $stableMeta 记录不一致（先修复再清理）" }
}
Write-Host "[Clean] Stable 前提 OK：Higher-v$Version.apk = $stableSha"

# ---------- 计划收集 ----------
$plan = New-Object System.Collections.Generic.List[object]
function AddPlan([string]$label, [scriptblock]$enum) {
    $files = @(& $enum)
    if ($files.Count) {
        $mb = [math]::Round([double](($files | Get-Item -Force | Measure-Object Length -Sum).Sum)/1MB, 1)
        $script:plan.Add([pscustomobject]@{ Label=$label; Files=$files; Count=$files.Count; MB=$mb })
    }
}

# §三 A 级：可再生生成物
AddPlan 'dist/（vite）'                { @(Join-Path $RepoRoot 'dist\*') }
AddPlan '.mobile-test-build/（tsc）'   { Get-ChildItem (Join-Path $RepoRoot '.mobile-test-build') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }
AddPlan '.ai-runtime-test-build/（tsc）' { Get-ChildItem (Join-Path $RepoRoot '.ai-runtime-test-build') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }
AddPlan '.higher/tmp/*（临时）'        { Get-ChildItem (Join-Path $RepoRoot '.higher\tmp') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }
foreach ($g in @('.gradle','build','buildSrc\build','buildSrc\.gradle','buildSrc\.kotlin')) {
    $p = Join-Path $GenAndroid $g
    AddPlan "gen/android/$g（gradle/kotlin）" { Get-ChildItem $p -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }
}
AddPlan 'app/build/（AGP）'            { Get-ChildItem (Join-Path $GenAndroid 'app\build') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }

# §四 jniLibs：仅 *.so（保留目录结构）
AddPlan 'jniLibs/**/*.so（cargo 复制产物）' { Get-ChildItem (Join-Path $GenAndroid 'app\src\main\jniLibs') -Recurse -Filter *.so -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }

# §五 assets generated frontend（固定三样；构建 Step13 清空重镜像）
AddPlan 'assets/ generated frontend' {
    $a = Join-Path $GenAndroid 'app\src\main\assets'
    @(Join-Path $a 'index.html') + @(Join-Path $a 'higher-build-meta.json') +
    @(Get-ChildItem (Join-Path $a 'assets') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName) |
        Where-Object { Test-Path $_ }
}

# §六 Release 根收口：保留 Stable 三件套，其余历史 APK 删除
AddPlan "release 根历史 APK（非 Stable-v$Version）" {
    Get-ChildItem $ReleaseOut -Filter '*.apk' -File -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne "Higher-v$Version.apk" } | ForEach-Object FullName
}

# §七 RC APK（== Stable 字节重复才删）
$rcApk = Join-Path $ReleaseOut ("rc\Higher-v$Version-rc.apk")
if (Test-Path $rcApk) {
    $rcSha = (Get-FileHash $rcApk -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($rcSha -eq $stableSha) {
        AddPlan "rc/Higher-v$Version-rc.apk（== Stable 字节重复）" { @($rcApk) }
    } else {
        Write-Host "[Clean] WARN：rc APK SHA != Stable（保留 rc APK 不删）" -ForegroundColor Yellow
    }
}

# §八 internal 历史 APK（全部可再生）
AddPlan 'internal/*.apk（QA 历史包，按需再生成）' {
    Get-ChildItem (Join-Path $ReleaseOut 'internal') -Filter '*.apk' -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName
}

# §十 Deep：src-tauri/target（人工选择）
if ($Deep) {
    AddPlan 'src-tauri/target/（Rust 编译缓存 · Deep）' { Get-ChildItem (Join-Path $RepoRoot 'src-tauri\target') -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object FullName }
}

# ---------- DryRun / Apply ----------
$totalFiles = ($plan | Measure-Object Count -Sum).Sum
$totalMB = [math]::Round([double](($plan | Measure-Object MB -Sum).Sum), 1)
$mode = if ($Apply) { 'APPLY（执行删除）' } else { 'DRY RUN（仅展示）' }
Write-Host ""
Write-Host "[Clean] $mode" -ForegroundColor $(if ($Apply) { 'Yellow' } else { 'Cyan' })
foreach ($p in $plan) {
    Write-Host ("  {0,8:N1} MB  {1,6} files  {2}" -f $p.MB, $p.Count, $p.Label)
}
Write-Host ("  --------")
Write-Host ("  {0,8:N1} MB  {1,6} files  TOTAL（预计释放）" -f $totalMB, $totalFiles)

if (-not $Apply) {
    Write-Host ""
    Write-Host "[Clean] DRY RUN 结束。确认后加 -Apply 执行$(if ($Deep) { '（-Deep 已包含 target/）' } else { '；如需连同 Rust target/ 缓存再加 -Deep' })。" -ForegroundColor Cyan
    exit 0
}

# ---- 执行 ----
$freed = 0; $deleted = 0
foreach ($p in $plan) {
    foreach ($f in $p.Files) {
        if (Test-Path $f) {
            $item = Get-Item $f -Force
            $freed += $item.Length; $deleted++
            Remove-Item $item.FullName -Force -Recurse
        }
    }
}
# 目录级清理（内容删完后移除空目录：build 缓存目录本身 + test-build 根 + dist 根）
foreach ($d in @(
    (Join-Path $RepoRoot 'dist'),
    (Join-Path $RepoRoot '.mobile-test-build'),
    (Join-Path $RepoRoot '.ai-runtime-test-build'),
    (Join-Path $GenAndroid '.gradle'),
    (Join-Path $GenAndroid 'build'),
    (Join-Path $GenAndroid 'buildSrc\build'),
    (Join-Path $GenAndroid 'buildSrc\.gradle'),
    (Join-Path $GenAndroid 'buildSrc\.kotlin'),
    (Join-Path $GenAndroid 'app\build'),
    (Join-Path $GenAndroid 'app\src\main\assets\assets'),
    (Join-Path $ReleaseOut 'internal'),
    $(if ($Deep) { Join-Path $RepoRoot 'src-tauri\target' })
)) {
    if ($d -and (Test-Path $d)) { Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue }
}
# release 根历史小 txt（APK 已删后的孤儿 sha256/size-report，归档 audit/<ver>/）
$audit = Join-Path $ReleaseOut ("audit\$Version")
New-Item -ItemType Directory -Force -Path $audit | Out-Null
Get-ChildItem $ReleaseOut -File -Force -ErrorAction SilentlyContinue | Where-Object {
    $_.Name -notmatch ("^Higher-v$Version(\.apk|\.sha256\.txt|-size-report\.txt)$")
} | Move-Item -Destination $audit -Force -ErrorAction SilentlyContinue
# §七：rc/ 审计文本 → audit/<ver>/（RC APK 已在计划中删除）
if (Test-Path (Join-Path $ReleaseOut 'rc')) {
    Get-ChildItem (Join-Path $ReleaseOut 'rc') -File -Force -ErrorAction SilentlyContinue |
        Move-Item -Destination $audit -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host ("[Clean] APPLY 完成：删除 {0} 文件，释放 {1:N1} MB" -f $deleted, ($freed/1MB)) -ForegroundColor Green
Write-Host "[Clean] Stable 未受影响：$stableApk"
