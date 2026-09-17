# HIGHER COGNITIVE CORE V1.2 §3 — Overnight Resource Guard
#
# 目的：让机器活着。本脚本只做「采样 + 状态判定 + 建议」，不杀用户进程。
#
# 状态（锁定，与任务书 §3 / §29 生产阈值一致）：
#   NORMAL         RAM < 78%  committed 内存 < 78%  持续 CPU < 75%
#   CONSTRAINED    RAM >= 78% OR committed >= 78% OR 持续 CPU >= 75%
#   HIGH_PRESSURE  RAM >= 84% OR committed >= 84% OR 持续 CPU >= 82%
#   CRITICAL       RAM >= 89% OR committed >= 89% OR 持续 CPU >= 90%
#
# 策略：
#   NORMAL         源码工作 + 一次一个轻量 build/check
#   CONSTRAINED    源码工作继续；不启动新的 compile/test/model 进程
#   HIGH_PRESSURE  不做 Cargo/npm 编译；只做源码/文档/轻量检查
#   CRITICAL       不启动任何昂贵子进程；只安全终止**本任务创建的**子进程；
#                  保留编辑与 ledger；**绝不终止无关的用户应用**
#
# 采样间隔（生产 Device Resource Governor）：8 秒
# 滞回：恶化需连续 2 个样本；恢复需连续 3 个样本。CRITICAL 进入同样是 2 个样本，
#       不因单个样本抖动。
#
# 用法：
#   powershell -File .higher/scripts/overnight_resource_guard.ps1                # 单次采样
#   powershell -File .higher/scripts/overnight_resource_guard.ps1 -Watch         # 持续采样
#   powershell -File .higher/scripts/overnight_resource_guard.ps1 -Watch -Seconds 8

[CmdletBinding()]
param(
    [switch]$Watch,
    [int]$Seconds = 8,
    [int]$Samples = 0
)

$ErrorActionPreference = 'Stop'

function Get-ResourceSample {
    $os = Get-CimInstance -ClassName Win32_OperatingSystem
    $totalKB = [double]$os.TotalVisibleMemorySize
    $freeKB = [double]$os.FreePhysicalMemory
    $ramPercent = if ($totalKB -gt 0) { [math]::Round((($totalKB - $freeKB) / $totalKB) * 100, 2) } else { 0 }

    # committed memory = 提交内存（Windows 上比 physical 更早暴露压力）
    $committed = Get-Counter '\Memory\% Committed Bytes In Use' -ErrorAction SilentlyContinue
    $committedPercent = if ($committed) {
        [math]::Round($committed.CounterSamples[0].CookedValue, 2)
    } else { 0 }

    $cpu = Get-Counter '\Processor(_Total)\% Processor Time' -ErrorAction SilentlyContinue
    $cpuPercent = if ($cpu) { [math]::Round($cpu.CounterSamples[0].CookedValue, 2) } else { 0 }

    [pscustomobject]@{
        sampled_at         = (Get-Date).ToString('s')
        ram_percent        = $ramPercent
        committed_percent  = $committedPercent
        cpu_percent        = $cpuPercent
    }
}

function Get-ResourceState {
    param($Sample)
    if ($Sample.ram_percent -ge 89 -or $Sample.committed_percent -ge 89 -or $Sample.cpu_percent -ge 90) {
        return 'CRITICAL'
    }
    if ($Sample.ram_percent -ge 84 -or $Sample.committed_percent -ge 84 -or $Sample.cpu_percent -ge 82) {
        return 'HIGH_PRESSURE'
    }
    if ($Sample.ram_percent -ge 78 -or $Sample.committed_percent -ge 78 -or $Sample.cpu_percent -ge 75) {
        return 'CONSTRAINED'
    }
    return 'NORMAL'
}

function Get-PolicyAction {
    param([string]$State)
    switch ($State) {
        'NORMAL'        { return '源码工作 + 一次一个轻量 build/check' }
        'CONSTRAINED'   { return '源码工作继续；不要启动新的 compile/test/model 进程' }
        'HIGH_PRESSURE' { return '不做 Cargo/npm 编译；只做源码/文档/轻量检查' }
        'CRITICAL'      { return '不启动任何昂贵子进程；只安全终止本任务创建的子进程；保留编辑与 ledger；绝不终止无关用户应用' }
        default         { return '未知状态' }
    }
}

# ---- 滞回：恶化需连续 2 个样本，恢复需连续 3 个样本 ----

$script:currentState = 'NORMAL'
$script:worseStreak = 0
$script:betterStreak = 0

function Update-Hysteresis {
    param([string]$Observed)
    $rank = @{ 'NORMAL' = 0; 'CONSTRAINED' = 1; 'HIGH_PRESSURE' = 2; 'CRITICAL' = 3 }
    $cur = $rank[$script:currentState]
    $obs = $rank[$Observed]

    if ($obs -gt $cur) {
        $script:worseStreak++
        $script:betterStreak = 0
        if ($script:worseStreak -ge 2) {
            $script:currentState = $Observed
            $script:worseStreak = 0
        }
    }
    elseif ($obs -lt $cur) {
        $script:betterStreak++
        $script:worseStreak = 0
        if ($script:betterStreak -ge 3) {
            $script:currentState = $Observed
            $script:betterStreak = 0
        }
    }
    else {
        $script:worseStreak = 0
        $script:betterStreak = 0
    }
    return $script:currentState
}

function Invoke-OneSample {
    $s = Get-ResourceSample
    $observed = Get-ResourceState -Sample $s
    $effective = Update-Hysteresis -Observed $observed
    $policy = Get-PolicyAction -State $effective
    Write-Output ("[{0}] observed={1} effective={2} ram={3}% committed={4}% cpu={5}% | {6}" -f `
        $s.sampled_at, $observed, $effective, $s.ram_percent, $s.committed_percent, $s.cpu_percent, $policy)
    return $effective
}

if ($Watch) {
    $i = 0
    while ($true) {
        Invoke-OneSample | Out-Null
        $i++
        if ($Samples -gt 0 -and $i -ge $Samples) { break }
        Start-Sleep -Seconds $Seconds
    }
}
else {
    Invoke-OneSample | Out-Null
}
