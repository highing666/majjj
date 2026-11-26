#Requires -Version 7.0
<#
.SYNOPSIS
    幂等启动局域网 llama-server 推理服务（默认适配 RTX 3060 12GB + Qwen3.5-9B-Q5_K_M）。
.DESCRIPTION
    使用经过真机验证的启动参数（全层 GPU 卸载、Flash Attention、3 路并发、-c 12288、
    开启 Jinja 模板并彻底关闭思考链）。
    后台模式通过 Win32_Process.Create 脱离 SSH / 终端会话 Job 对象长期常驻，
    启动后自动轮询 /health 与 /props 验证就绪状态。
    支持仅通过 -ModelPath 切换其他 GGUF 模型权重。
.EXAMPLE
    pwsh -File .\start-llama-server.ps1
.EXAMPLE
    pwsh -File .\start-llama-server.ps1 -ModelPath E:\majjj-llm\models\Huihui-Qwen3.5-9B-abliterated.Q5_K_M.gguf -Restart
.EXAMPLE
    pwsh -File .\start-llama-server.ps1 -Foreground
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-llm',
    [string]$ModelPath = '',
    [string]$BindHost = '0.0.0.0',
    [ValidateRange(1, 65535)]
    [int]$Port = 8080,
    [ValidateRange(0, 9999)]
    [int]$GpuLayers = 999,
    [ValidateRange(1, 16)]
    [int]$ParallelSlots = 3,
    [ValidateRange(1024, 131072)]
    [int]$ContextSize = 12288,
    [ValidateRange(10, 600)]
    [int]$HealthTimeoutSec = 180,
    [switch]$Restart,
    [switch]$Foreground
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$BinDir = Join-Path $RootDir 'bin'
$LogsDir = Join-Path $RootDir 'logs'
$LlamaServerExe = Join-Path $BinDir 'llama-server.exe'
$LogFile = Join-Path $LogsDir 'llama-server.log'
$PidFile = Join-Path $LogsDir 'llama-server.pid'

if ([string]::IsNullOrWhiteSpace($ModelPath)) {
    $ModelPath = Join-Path $RootDir 'models\Qwen3.5-9B-Q5_K_M.gguf'
}

if (-not (Test-Path -LiteralPath $LlamaServerExe)) {
    throw "未找到 llama-server.exe：$LlamaServerExe（请先运行 deploy-llama-server.ps1）。"
}
if (-not (Test-Path -LiteralPath $ModelPath)) {
    throw "未找到模型文件：$ModelPath（请确认 -ModelPath 或先运行 deploy-llama-server.ps1）。"
}
if (-not (Test-Path -LiteralPath $LogsDir)) {
    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
}

$HealthUrl = "http://127.0.0.1:$Port/health"
$PropsUrl = "http://127.0.0.1:$Port/props"

function Test-ServerHealthy {
    param([string]$Url)
    try {
        $Resp = Invoke-RestMethod -Uri $Url -Method Get -TimeoutSec 3
        return ($null -ne $Resp -and $Resp.status -eq 'ok')
    } catch {
        return $false
    }
}

function Show-ServerPropsSummary {
    param([string]$Url)
    try {
        $Props = Invoke-RestMethod -Uri $Url -Method Get -TimeoutSec 5
        $Slots = $Props.total_slots
        $NCtx = if ($Props.PSObject.Properties['default_generation_settings']) {
            $Props.default_generation_settings.n_ctx
        } else {
            'unknown'
        }
        $ReasoningFormat = if ($Props.PSObject.Properties['reasoning_format']) {
            $Props.reasoning_format
        } else {
            'n/a'
        }
        $ReasoningInContent = if ($Props.PSObject.Properties['reasoning_in_content']) {
            $Props.reasoning_in_content
        } else {
            'n/a'
        }
        $LoadedModel = if ($Props.PSObject.Properties['model_path']) {
            $Props.model_path
        } else {
            $ModelPath
        }
        Write-Host "[start] /props 就绪摘要：total_slots=$Slots, n_ctx=$NCtx, reasoning_format=$ReasoningFormat, reasoning_in_content=$ReasoningInContent"
        Write-Host "[start] 已加载模型：$LoadedModel"
        return $Props
    } catch {
        Write-Warning "[start] /props 查询失败（/health 已就绪）：$($_.Exception.Message)"
        return $null
    }
}

# 幂等检查：服务已健康运行且未要求重启时直接返回
if (Test-ServerHealthy -Url $HealthUrl) {
    if (-not $Restart) {
        Write-Host "[start] llama-server 已在端口 $Port 正常运行（幂等跳过；如需切换模型或重启请加 -Restart）。"
        $Props = Show-ServerPropsSummary -Url $PropsUrl
        if ($null -ne $Props -and $Props.PSObject.Properties['model_path']) {
            $LoadedPath = [string]$Props.model_path
            if (-not [string]::IsNullOrWhiteSpace($LoadedPath) -and ($LoadedPath -ine $ModelPath)) {
                Write-Warning "[start] 当前加载模型 ($LoadedPath) 与参数 -ModelPath ($ModelPath) 不同，请使用 -Restart 生效。"
            }
        }
        exit 0
    }
}

if ($Restart) {
    $StopScript = Join-Path $PSScriptRoot 'stop-llama-server.ps1'
    if (-not (Test-Path -LiteralPath $StopScript)) {
        $StopScript = Join-Path $RootDir 'stop-llama-server.ps1'
    }
    if (Test-Path -LiteralPath $StopScript) {
        & $StopScript -RootDir $RootDir -Port $Port
    } else {
        Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
    }
}

$ChatTemplateKwargs = '{"enable_thinking": false}'

if ($Foreground) {
    Write-Host "[start] 前台模式启动 llama-server（Bind=${BindHost}:${Port}, Slots=$ParallelSlots, Context=$ContextSize）"
    Write-Host "[start] 模型：$ModelPath"
    # PowerShell 7 原生参数传递设为 Standard，确保 '{"enable_thinking": false}' 引号完整送达 llama-server
    $PSNativeCommandArgumentPassing = 'Standard'
    $ServerArgs = @(
        '-m', $ModelPath,
        '--host', $BindHost,
        '--port', [string]$Port,
        '-ngl', [string]$GpuLayers,
        '-fa', 'on',
        '--parallel', [string]$ParallelSlots,
        '-c', [string]$ContextSize,
        '--jinja',
        '--chat-template-kwargs', $ChatTemplateKwargs,
        '--reasoning-budget', '0',
        '--log-file', $LogFile
    )
    & $LlamaServerExe @ServerArgs
    exit $LASTEXITCODE
}

# 后台常驻模式：通过 Win32_Process.Create 脱离 SSH / PowerShell 会话 Job
# 显式转义 JSON 内部双引号，确保 C 运行库 argv 解析出精确的 {"enable_thinking": false}
$EscapedTemplateKwargs = $ChatTemplateKwargs.Replace('"', '\"')
$CommandLine = @(
    "`"$LlamaServerExe`"",
    '-m', "`"$ModelPath`"",
    '--host', $BindHost,
    '--port', [string]$Port,
    '-ngl', [string]$GpuLayers,
    '-fa', 'on',
    '--parallel', [string]$ParallelSlots,
    '-c', [string]$ContextSize,
    '--jinja',
    '--chat-template-kwargs', "`"$EscapedTemplateKwargs`"",
    '--reasoning-budget', '0',
    '--log-file', "`"$LogFile`""
) -join ' '

Write-Host "[start] 后台常驻启动 llama-server（Bind=${BindHost}:${Port}, Slots=$ParallelSlots, Context=$ContextSize）..."
$CreateResult = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
    CommandLine      = $CommandLine
    CurrentDirectory = $BinDir
}

if ($CreateResult.ReturnValue -ne 0 -or -not $CreateResult.ProcessId) {
    throw "Win32_Process.Create 启动 llama-server 失败（ReturnValue=$($CreateResult.ReturnValue)）。"
}

$ServerPid = [int]$CreateResult.ProcessId
Set-Content -LiteralPath $PidFile -Value $ServerPid -Encoding Ascii
Write-Host "[start] 进程已脱离会话启动（PID=$ServerPid），正在等待 $HealthUrl 就绪（超时 ${HealthTimeoutSec}s）..."

$Deadline = (Get-Date).AddSeconds($HealthTimeoutSec)
$Ready = $false
while ((Get-Date) -lt $Deadline) {
    $Proc = Get-Process -Id $ServerPid -ErrorAction SilentlyContinue
    if ($null -eq $Proc) {
        Write-Warning "[start] llama-server 进程 (PID=$ServerPid) 在就绪前意外退出。"
        if (Test-Path -LiteralPath $LogFile) {
            Write-Host '--- llama-server.log (最后 40 行) ---'
            Get-Content -LiteralPath $LogFile -Tail 40 | ForEach-Object { Write-Host $_ }
        }
        throw "llama-server 启动失败（PID=$ServerPid 已退出），请检查日志：$LogFile"
    }

    if (Test-ServerHealthy -Url $HealthUrl) {
        $Ready = $true
        break
    }
    Start-Sleep -Seconds 1
}

if (-not $Ready) {
    throw "等待 $HealthUrl 就绪超时（${HealthTimeoutSec}s），请检查日志：$LogFile"
}

Write-Host "[start] llama-server 启动成功并已通过 /health 检查（PID=$ServerPid，监听 ${BindHost}:${Port}）。"
Show-ServerPropsSummary -Url $PropsUrl | Out-Null
