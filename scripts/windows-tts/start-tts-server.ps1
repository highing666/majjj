#Requires -Version 7.0
<#
.SYNOPSIS
    幂等启动 Windows 原生局域网情感 TTS 服务（默认端口 8081，与 llama-server 共驻 RTX 3060 12GB）。
.DESCRIPTION
    落地显存护栏 #2（30% 进程显存硬封顶）、#3（Windows WDDM 适配 PYTORCH_CUDA_ALLOC_CONF，
    禁用 expandable_segments）、#4（单并发串行 GPU 队列 + max_new_tokens=120 硬限）与 #5（deadline_ms 过期丢弃）。
    后台模式通过 Win32_Process.Create 脱离 SSH / 终端会话 Job 对象长期常驻，
    启动后自动轮询 /health 验证就绪状态。
    支持仅通过 -Backend / -ModelPath 在默认引擎与备选引擎之间平滑切换，无需改动 Rust 或前端。
.EXAMPLE
    pwsh -File .\start-tts-server.ps1
.EXAMPLE
    pwsh -File .\start-tts-server.ps1 -Backend cosyvoice3 -Restart
.EXAMPLE
    pwsh -File .\start-tts-server.ps1 -Foreground
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-tts',
    [ValidateSet('qwen3-custom-voice', 'cosyvoice3', 'qwen3-base-clone')]
    [string]$Backend = 'qwen3-custom-voice',
    [string]$ModelPath = '',
    [string]$VoicesDir = '',
    [string]$BindHost = '0.0.0.0',
    [ValidateRange(1, 65535)]
    [int]$Port = 8081,
    [ValidateRange(0.10, 0.45)]
    [double]$VramFraction = 0.30,
    [ValidateRange(16, 120)]
    [int]$MaxNewTokens = 120,
    [ValidateRange(1, 120)]
    [int]$StreamChunkSize = 6,
    [ValidateRange(10, 600)]
    [int]$HealthTimeoutSec = 180,
    [switch]$Restart,
    [switch]$Foreground
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$DefaultModelSubdirs = @{
    'qwen3-custom-voice' = 'models\Qwen3-TTS-12Hz-0.6B-CustomVoice'
    'cosyvoice3'         = 'models\Fun-CosyVoice3-0.5B-2512'
    'qwen3-base-clone'   = 'models\Qwen3-TTS-12Hz-0.6B-Base'
}

$VenvPython = Join-Path $RootDir '.venv\Scripts\python.exe'
$ServerDir = Join-Path $RootDir 'server'
$LogsDir = Join-Path $RootDir 'logs'
$LogFile = Join-Path $LogsDir 'tts-server.log'
$PidFile = Join-Path $LogsDir 'tts-server.pid'
$PersonasPath = Join-Path $ServerDir 'personas.json'

if ([string]::IsNullOrWhiteSpace($ModelPath)) {
    $ModelPath = Join-Path $RootDir $DefaultModelSubdirs[$Backend]
}
if ([string]::IsNullOrWhiteSpace($VoicesDir)) {
    $VoicesDir = Join-Path $RootDir 'voices'
}

if (-not (Test-Path -LiteralPath $VenvPython)) {
    throw "未找到独立虚拟环境解释器：$VenvPython（请先运行 deploy-tts-server.ps1）。"
}
if (-not (Test-Path -LiteralPath $PersonasPath)) {
    throw "未找到角色配置：$PersonasPath（请先运行 deploy-tts-server.ps1）。"
}
if (-not (Test-Path -LiteralPath $LogsDir)) {
    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
}

# 显存护栏 #3：Windows 11 WDDM 专用 CUDA 分配器回收参数（禁止使用 expandable_segments）
$CudaAllocConf = 'max_split_size_mb:128,garbage_collection_threshold:0.8'
$env:PYTORCH_CUDA_ALLOC_CONF = $CudaAllocConf

if (-not $PSBoundParameters.ContainsKey('StreamChunkSize') -and -not [string]::IsNullOrWhiteSpace($env:MAJJJ_TTS_STREAM_CHUNK_SIZE)) {
    $ParsedChunkSize = 0
    if (-not [int]::TryParse($env:MAJJJ_TTS_STREAM_CHUNK_SIZE.Trim(), [ref]$ParsedChunkSize) -or $ParsedChunkSize -lt 1 -or $ParsedChunkSize -gt 120) {
        throw "环境变量 MAJJJ_TTS_STREAM_CHUNK_SIZE='$($env:MAJJJ_TTS_STREAM_CHUNK_SIZE)' 非法，需为 1..120 之间的正整数。"
    }
    $StreamChunkSize = $ParsedChunkSize
}
$env:MAJJJ_TTS_STREAM_CHUNK_SIZE = [string]$StreamChunkSize

$HealthUrl = "http://127.0.0.1:$Port/health"

function Get-TtsHealth {
    param([string]$Url)
    try {
        return Invoke-RestMethod -Uri $Url -Method Get -TimeoutSec 3
    } catch {
        return $null
    }
}

function Show-HealthSummary {
    param([object]$Health)
    if ($null -eq $Health) {
        return
    }
    Write-Host "[start-tts] /health 就绪摘要：backend=$($Health.backend), model=$($Health.model), personas_cached=$($Health.personas_cached), queue_depth=$($Health.queue_depth), vram_allocated=$($Health.vram_allocated_mib) MiB, vram_reserved=$($Health.vram_reserved_mib) MiB, limit_fraction=$($Health.vram_limit_fraction)"
}

# 幂等检查：服务已健康运行且未要求重启时直接返回
$ExistingHealth = Get-TtsHealth -Url $HealthUrl
if ($null -ne $ExistingHealth -and $ExistingHealth.status -eq 'ok' -and $ExistingHealth.ready) {
    if (-not $Restart) {
        Write-Host "[start-tts] TTS 服务已在端口 $Port 正常运行（幂等跳过；如需切换后端或重启请加 -Restart）。"
        Show-HealthSummary -Health $ExistingHealth
        if ($ExistingHealth.backend -ine $Backend) {
            Write-Warning "[start-tts] 当前运行后端 ($($ExistingHealth.backend)) 与参数 -Backend ($Backend) 不同，请使用 -Restart 生效。"
        }
        exit 0
    }
}

if ($Restart) {
    $StopScript = Join-Path $PSScriptRoot 'stop-tts-server.ps1'
    if (-not (Test-Path -LiteralPath $StopScript)) {
        $StopScript = Join-Path $RootDir 'stop-tts-server.ps1'
    }
    if (Test-Path -LiteralPath $StopScript) {
        & $StopScript -RootDir $RootDir -Port $Port
    } else {
        Start-Sleep -Seconds 1
    }
}

$VramFractionInvariant = $VramFraction.ToString('0.00', [System.Globalization.CultureInfo]::InvariantCulture)

if ($Foreground) {
    Write-Host "[start-tts] 前台模式启动 TTS 服务（Bind=${BindHost}:${Port}, Backend=$Backend, VramFraction=$VramFractionInvariant, MaxNewTokens=$MaxNewTokens, StreamChunkSize=$StreamChunkSize）"
    Write-Host "[start-tts] 模型：$ModelPath"
    $PSNativeCommandArgumentPassing = 'Standard'
    $ServerArgs = @(
        '-m', 'server',
        '--host', $BindHost,
        '--port', [string]$Port,
        '--backend', $Backend,
        '--model-path', $ModelPath,
        '--voices-dir', $VoicesDir,
        '--personas-path', $PersonasPath,
        '--vram-fraction', $VramFractionInvariant,
        '--max-new-tokens', [string]$MaxNewTokens,
        '--stream-chunk-size', [string]$StreamChunkSize,
        '--cuda-alloc-conf', $CudaAllocConf,
        '--log-file', $LogFile
    )
    Push-Location -LiteralPath $RootDir
    try {
        & $VenvPython @ServerArgs
        exit $LASTEXITCODE
    } finally {
        Pop-Location
    }
}

# 后台常驻模式：通过 Win32_Process.Create 脱离 SSH / PowerShell 会话 Job
# server/__main__.py 会在导入 torch 前先行写入 PYTORCH_CUDA_ALLOC_CONF，确保 WMI 启动路径同样生效
$CommandLine = @(
    "`"$VenvPython`"",
    '-m', 'server',
    '--host', $BindHost,
    '--port', [string]$Port,
    '--backend', $Backend,
    '--model-path', "`"$ModelPath`"",
    '--voices-dir', "`"$VoicesDir`"",
    '--personas-path', "`"$PersonasPath`"",
    '--vram-fraction', $VramFractionInvariant,
    '--max-new-tokens', [string]$MaxNewTokens,
    '--stream-chunk-size', [string]$StreamChunkSize,
    '--cuda-alloc-conf', "`"$CudaAllocConf`"",
    '--log-file', "`"$LogFile`""
) -join ' '

Write-Host "[start-tts] 后台常驻启动 TTS 服务（Bind=${BindHost}:${Port}, Backend=$Backend, VramFraction=$VramFractionInvariant, MaxNewTokens=$MaxNewTokens, StreamChunkSize=$StreamChunkSize）..."
$CreateResult = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
    CommandLine      = $CommandLine
    CurrentDirectory = $RootDir
}

if ($CreateResult.ReturnValue -ne 0 -or -not $CreateResult.ProcessId) {
    throw "Win32_Process.Create 启动 TTS 服务失败（ReturnValue=$($CreateResult.ReturnValue)）。"
}

$ServerPid = [int]$CreateResult.ProcessId
Set-Content -LiteralPath $PidFile -Value $ServerPid -Encoding Ascii
Write-Host "[start-tts] 进程已脱离会话启动（PID=$ServerPid），正在等待 $HealthUrl 就绪（超时 ${HealthTimeoutSec}s）..."

$Deadline = (Get-Date).AddSeconds($HealthTimeoutSec)
$ReadyHealth = $null
while ((Get-Date) -lt $Deadline) {
    $Proc = Get-Process -Id $ServerPid -ErrorAction SilentlyContinue
    if ($null -eq $Proc) {
        Write-Warning "[start-tts] TTS 进程 (PID=$ServerPid) 在就绪前意外退出。"
        if (Test-Path -LiteralPath $LogFile) {
            Write-Host '--- tts-server.log (最后 40 行) ---'
            Get-Content -LiteralPath $LogFile -Tail 40 | ForEach-Object { Write-Host $_ }
        }
        throw "TTS 服务启动失败（PID=$ServerPid 已退出），请检查日志：$LogFile"
    }

    $Probe = Get-TtsHealth -Url $HealthUrl
    if ($null -ne $Probe -and $Probe.status -eq 'ok' -and $Probe.ready) {
        $ReadyHealth = $Probe
        break
    }
    Start-Sleep -Seconds 1
}

if ($null -eq $ReadyHealth) {
    throw "等待 $HealthUrl 就绪超时（${HealthTimeoutSec}s），请检查日志：$LogFile"
}

Write-Host "[start-tts] TTS 服务启动成功并已通过 /health 检查（PID=$ServerPid，监听 ${BindHost}:${Port}）。"
Show-HealthSummary -Health $ReadyHealth
