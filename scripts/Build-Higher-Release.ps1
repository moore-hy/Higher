# ============================================================
# Build-Higher-Release.ps1 — DEV-0065.2R §21
# Higher 0.3.0 Windows NSIS 安装包构建脚本
#
# 步骤（§21）：
#   verify clean worktree → print HEAD → tsc → frontend build
#   → cargo check → release tests → tauri build
#   → locate NSIS installer → copy to release\ → SHA256
#
# 产物：
#   release\Higher_<version>_Setup.exe
#   release\Higher_<version>_SHA256.txt
#
# 安全（§7/§8/§52）：
#   - 本脚本不打包、不复制任何开发/测试个人数据
#   - 不触碰用户生产 AppLocalData
#   - -AllowDirty 仅用于 DEV 进行中（§53 Commit: NO）的人工复核构建
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

# ---------- 1) verify clean worktree ----------
Step "verify clean worktree"
$Dirty = @(git status --porcelain)
if ($Dirty.Count -gt 0 -and -not $AllowDirty) {
    $Dirty | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
    throw "WORKTREE_DIRTY: 先 commit，或人工复核后以 -AllowDirty 构建（§53 Commit: NO 的 DEV 场景）"
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

# ---------- 3) tsc ----------
Step "tsc --noEmit"
npx tsc --noEmit
if ($LASTEXITCODE -ne 0) { throw "TSC_FAILED" }

# ---------- 4) frontend build ----------
Step "frontend build (vite)"
npm run build
if ($LASTEXITCODE -ne 0) { throw "FRONTEND_BUILD_FAILED" }

# ---------- 5) cargo check ----------
Step "cargo check"
Push-Location (Join-Path $Root 'src-tauri')
try {
    cargo check -j 1
    if ($LASTEXITCODE -ne 0) { throw "CARGO_CHECK_FAILED" }

    # ---------- 6) release tests ----------
    Step "release tests (batch0652_release)"
    $env:RUST_TEST_THREADS = '1'
    cargo test --test batch0652_release -j 1
    if ($LASTEXITCODE -ne 0) { throw "RELEASE_TESTS_FAILED" }
}
finally {
    Pop-Location
}

# ---------- 7) tauri build ----------
Step "tauri build (NSIS)"
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "TAURI_BUILD_FAILED" }

# ---------- 8) locate NSIS installer ----------
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

# ---------- 9) copy to release\ ----------
Step "copy to release\"
$ReleaseDir = Join-Path $Root 'release'
New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
$Dest = Join-Path $ReleaseDir "Higher_${Version}_Setup.exe"
Copy-Item -Path $Original.FullName -Destination $Dest -Force

# ---------- 10) SHA256 ----------
Step "calculate SHA256"
$Hash = (Get-FileHash -Path $Dest -Algorithm SHA256).Hash.ToLower()
$ShaFile = Join-Path $ReleaseDir "Higher_${Version}_SHA256.txt"
"$Hash  Higher_${Version}_Setup.exe" | Set-Content -Path $ShaFile -Encoding ascii

# ---------- summary ----------
$Size = (Get-Item $Dest).Length
Write-Host ""
Write-Host "================ BUILD OK ================" -ForegroundColor Green
Write-Host "original:     $($Original.FullName)"
Write-Host "release copy: $Dest"
Write-Host "size:         $Size"
Write-Host "sha256:       $Hash"
Write-Host "sha256 file:  $ShaFile"
Write-Host "=========================================" -ForegroundColor Green
