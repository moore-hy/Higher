# ============================================================
# Build-Higher-Release.ps1 — DEV-0065.4 §9-§16
# Higher v1 Windows NSIS Release Build
#
# 单一官方 Windows 构建入口。
# 安装包版本号动态读取自 src-tauri/tauri.conf.json，不在脚本内硬编码。
#
# 步骤：
#   verify clean worktree → print HEAD → tsc → frontend build
#   → dist 卫生门 → cargo check → release tests → tauri build
#   → dist 卫生门（Tauri 二次构建后）→ locate NSIS installer
#   → copy to release\ → installer 尺寸预算 → SHA256 + 独立复验
#
# 轻量发布（DEV-0065.4）：
#   - HIGHER_RELEASE_BUILD=1：官方发布构建不携带 public/（浏览器 mock 不进 dist）
#   - 该变量对 npm run build 与 npm run tauri build（beforeBuildCommand 二次构建）同时生效
#   - finally 恢复原值，不污染开发环境
#
# 安全（§18）：本脚本不触碰 %LOCALAPPDATA%\com.higher.desktop\
# -AllowDirty 仅用于 Trae 禁止 commit 场景下的 Release Candidate 构建
# ============================================================

param(
    [switch]$AllowDirty
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Step([string]$Message) {
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

# dist 发布卫生门（§11）：dist 内出现任一禁止产物即 RELEASE_FRONTEND_CONTAMINATED
function Assert-DistHygiene([string]$Phase) {
    $bannedDirs = @('mock', '.higher', '.git', 'src', 'src-tauri', 'tests', 'node_modules', 'target', '.data', '.webview-data')
    $bannedFiles = @('README.md', 'package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', '.env')
    foreach ($d in $bannedDirs) {
        if (Test-Path (Join-Path $Root "dist\$d")) {
            throw "RELEASE_FRONTEND_CONTAMINATED ($Phase): dist\$d 不允许出现在官方发布前端"
        }
    }
    foreach ($f in $bannedFiles) {
        if (Test-Path (Join-Path $Root "dist\$f")) {
            throw "RELEASE_FRONTEND_CONTAMINATED ($Phase): dist\$f 不允许出现在官方发布前端"
        }
    }
    $violations = Get-ChildItem (Join-Path $Root 'dist') -Recurse -Force -File -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -like '.env.*' -or $_.Extension -in @('.ts', '.tsx', '.rs', '.map')
    }
    if ($violations) {
        $list = ($violations | Select-Object -First 5 | ForEach-Object { $_.FullName }) -join '; '
        throw "RELEASE_FRONTEND_CONTAMINATED ($Phase): dist 含禁止扩展文件（$list）"
    }
    Write-Host "  dist hygiene ($Phase): clean"
}

# ---------- 1) verify clean worktree ----------
Step "verify clean worktree"
$Dirty = @(git status --porcelain)
if ($Dirty.Count -gt 0 -and -not $AllowDirty) {
    $Dirty | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
    throw "WORKTREE_DIRTY: 先 commit，或人工复核后以 -AllowDirty 构建（RC 场景）"
}
if ($Dirty.Count -gt 0) {
    Write-Host "  [AllowDirty] 未提交变更将包含在本次构建中：" -ForegroundColor Yellow
    $Dirty | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
}
else {
    Write-Host "  worktree clean"
}

# ---------- 2) print HEAD ----------
Step "print HEAD"
$Head = (git rev-parse HEAD).Trim()
Write-Host "  HEAD: $Head"

# ---------- release build environment（§10：save → set → finally restore） ----------
Step "set HIGHER_RELEASE_BUILD=1"
$SavedReleaseBuild = [System.Environment]::GetEnvironmentVariable('HIGHER_RELEASE_BUILD')
$SavedReleaseBuildWasNull = ($null -eq $SavedReleaseBuild)
$env:HIGHER_RELEASE_BUILD = '1'
Write-Host "  HIGHER_RELEASE_BUILD=$env:HIGHER_RELEASE_BUILD（构建结束后恢复原值）"

try {

    # ---------- 3) tsc ----------
    Step "tsc --noEmit"
    npx tsc --noEmit
    if ($LASTEXITCODE -ne 0) { throw "TSC_FAILED" }

    # ---------- 4) frontend build ----------
    Step "frontend build (vite, release mode: no public/)"
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "FRONTEND_BUILD_FAILED" }

    # ---------- 5) dist hygiene gate（首次构建后，§11） ----------
    Step "dist hygiene gate (after first build)"
    Assert-DistHygiene 'after-first-build'

    # ---------- 6) cargo check ----------
    Step "cargo check"
    Push-Location (Join-Path $Root 'src-tauri')
    try {
        cargo check -j 1
        if ($LASTEXITCODE -ne 0) { throw "CARGO_CHECK_FAILED" }

        # ---------- 7) release tests ----------
        Step "release tests (batch0652_release)"
        $env:RUST_TEST_THREADS = '1'
        cargo test --test batch0652_release -j 1
        if ($LASTEXITCODE -ne 0) { throw "RELEASE_TESTS_FAILED" }
    }
    finally {
        Pop-Location
    }

    # ---------- 8) tauri build（beforeBuildCommand 将再次 npm run build，env 已覆盖） ----------
    Step "tauri build (NSIS, release mode)"
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "TAURI_BUILD_FAILED" }

    # ---------- 9) dist hygiene gate（Tauri 二次构建后，§34） ----------
    Step "dist hygiene gate (after tauri build)"
    Assert-DistHygiene 'after-tauri-build'

    # ---------- 10) locate NSIS installer ----------
    Step "locate NSIS installer"
    $Conf = Get-Content (Join-Path $Root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
    $Version = $Conf.version
    $NsisDir = Join-Path $Root 'src-tauri\target\release\bundle\nsis'
    $Original = Get-ChildItem -Path $NsisDir -Filter "*-setup.exe" -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "Higher_${Version}_*" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $Original) {
        throw "NSIS_INSTALLER_NOT_FOUND: 在 $NsisDir 未找到 Higher_${Version}_*-setup.exe"
    }
    Write-Host "  found: $($Original.FullName)"

    # ---------- 11) installer size budget（§14：硬上限 120 MiB） ----------
    Step "installer size budget"
    $SizeBytes = (Get-Item $Original.FullName).Length
    $SizeMiB = [math]::Round($SizeBytes / 1MB, 2)
    Write-Host ("  installer: {0:N0} bytes ({1} MiB)；预算上限 125,829,120 bytes (120 MiB)" -f $SizeBytes, $SizeMiB)
    if ($SizeBytes -gt 125829120) {
        throw "INSTALLER_SIZE_BUDGET_MISS: $SizeBytes bytes > 125,829,120 bytes（报告 actual size / dist size / Higher.exe size / bundle 清单后 STOP）"
    }

    # ---------- 12) copy to release\ ----------
    Step "copy to release\"
    $ReleaseDir = Join-Path $Root 'release'
    New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
    $Dest = Join-Path $ReleaseDir "Higher_${Version}_Setup.exe"
    Copy-Item -Path $Original.FullName -Destination $Dest -Force

    # ---------- 13) SHA256 + 独立复验（§16） ----------
    Step "calculate SHA256"
    $Hash = (Get-FileHash -Path $Dest -Algorithm SHA256).Hash.ToLower()
    $ShaFile = Join-Path $ReleaseDir "Higher_${Version}_SHA256.txt"
    "$Hash  Higher_${Version}_Setup.exe" | Set-Content -Path $ShaFile -Encoding ascii
    $Reverify = (Get-FileHash -Path $Dest -Algorithm SHA256).Hash.ToLower()
    $Recorded = (Get-Content $ShaFile -Raw).Trim().Split(' ')[0]
    if ($Reverify -ne $Recorded) { throw "SHA256_MISMATCH: 复验 $Reverify ≠ 记录 $Recorded" }
    Write-Host "  sha256 re-verified: match"

    # ---------- summary ----------
    $ExeSize = (Get-Item (Join-Path $Root 'src-tauri\target\release\Higher.exe') -ErrorAction SilentlyContinue).Length
    $DistSize = (Get-ChildItem (Join-Path $Root 'dist') -Recurse -Force -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
    Write-Host ""
    Write-Host "================ BUILD OK ================" -ForegroundColor Green
    Write-Host "version:      $Version (dynamic from tauri.conf.json)"
    Write-Host "original:     $($Original.FullName)"
    Write-Host "release copy: $Dest"
    Write-Host "size:         $SizeBytes bytes ($SizeMiB MiB)"
    Write-Host "Higher.exe:   $ExeSize bytes"
    Write-Host "dist total:   $DistSize bytes"
    Write-Host "sha256:       $Hash"
    Write-Host "sha256 file:  $ShaFile"
    Write-Host "=========================================" -ForegroundColor Green
}
finally {
    # §10：finally 恢复 HIGHER_RELEASE_BUILD 原值（原不存在则移除）
    if ($SavedReleaseBuildWasNull) {
        Remove-Item Env:\HIGHER_RELEASE_BUILD -ErrorAction SilentlyContinue
    }
    else {
        $env:HIGHER_RELEASE_BUILD = $SavedReleaseBuild
    }
    Write-Host ""
    Write-Host "==> HIGHER_RELEASE_BUILD restored (was null: $SavedReleaseBuildWasNull)" -ForegroundColor DarkGray
}
