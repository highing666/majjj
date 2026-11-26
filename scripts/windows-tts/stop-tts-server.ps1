#Requires -Version 7.0
<#
.SYNOPSIS
    幂等停止运行中的 Majjj Windows 局域网情感 TTS 服务进程。
.DESCRIPTION
    同时检查 PID 文件（logs\tts-server.pid）、目标根目录与端口命令行匹配进程，
    以及监听指定端口的 Python 进程，执行干净停止并等待退出；若服务未运行则直接成功返回。
.EXAMPLE
    pwsh -File .\stop-tts-server.ps1
.EXAMPLE
    pwsh -File .\stop-tts-server.ps1 -RootDir E:\majjj-tts -Port 8081
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-tts',
    [ValidateRange(1, 65535)]
    [int]$Port = 8081,
    [ValidateRange(1, 120)]
    [int]$WaitTimeoutSec = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$PidFile = Join-Path $RootDir 'logs\tts-server.pid'
$TargetPids = [System.Collections.Generic.HashSet[int]]::new()

# 1. 从 PID 文件读取
if (Test-Path -LiteralPath $PidFile) {
    $RawPid = (Get-Content -LiteralPath $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
    $ParsedPid = 0
    if ($null -ne $RawPid -and [int]::TryParse($RawPid.Trim(), [ref]$ParsedPid) -and $ParsedPid -gt 0) {
        $Proc = Get-Process -Id $ParsedPid -ErrorAction SilentlyContinue
        if ($null -ne $Proc -and $Proc.ProcessName -match '^python') {
            $TargetPids.Add($ParsedPid) | Out-Null
        }
    }
}

# 2. 按命令行匹配目标目录或目标端口的 Python TTS 进程
$NormalizedRoot = $RootDir.TrimEnd('\', '/')
$CimProcs = Get-CimInstance -ClassName Win32_Process -Filter "Name = 'python.exe' OR Name = 'pythonw.exe'" -ErrorAction SilentlyContinue
foreach ($P in @($CimProcs)) {
    if ($null -eq $P) { continue }
    $Cmd = [string]$P.CommandLine
    $Exe = [string]$P.ExecutablePath
    $MatchesRoot = ($Exe -and $Exe.StartsWith($NormalizedRoot, [System.StringComparison]::OrdinalIgnoreCase)) -or
                   ($Cmd -and $Cmd.IndexOf($NormalizedRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0)
    $MatchesServer = ($Cmd -and ($Cmd -match '-m\s+server\b') -and ($Cmd -match "--port\s+$Port(?:\s|$)"))
    if ($MatchesRoot -or $MatchesServer) {
        $TargetPids.Add([int]$P.ProcessId) | Out-Null
    }
}

# 3. 兜底检查监听目标端口的 Python 进程
try {
    $Listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    foreach ($Conn in @($Listeners)) {
        if ($null -eq $Conn) { continue }
        $OwnerProc = Get-Process -Id $Conn.OwningProcess -ErrorAction SilentlyContinue
        if ($null -ne $OwnerProc -and $OwnerProc.ProcessName -match '^python') {
            $TargetPids.Add([int]$OwnerProc.Id) | Out-Null
        }
    }
} catch {
    # 非管理员或极简环境跳过 NetTCPConnection 探测
}

if ($TargetPids.Count -eq 0) {
    if (Test-Path -LiteralPath $PidFile) {
        Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
    }
    Write-Host "[stop-tts] 未发现运行中的 TTS 服务（Port=$Port），幂等退出。"
    exit 0
}

$PidList = @($TargetPids)
Write-Host "[stop-tts] 正在停止 TTS 服务进程：PID=$($PidList -join ', ')"
foreach ($TargetPid in $PidList) {
    Stop-Process -Id $TargetPid -Force -ErrorAction SilentlyContinue
}

$Deadline = (Get-Date).AddSeconds($WaitTimeoutSec)
while ((Get-Date) -lt $Deadline) {
    $Alive = @($PidList | Where-Object { $null -ne (Get-Process -Id $_ -ErrorAction SilentlyContinue) })
    if ($Alive.Count -eq 0) {
        break
    }
    Start-Sleep -Milliseconds 300
}

$StillAlive = @($PidList | Where-Object { $null -ne (Get-Process -Id $_ -ErrorAction SilentlyContinue) })
if ($StillAlive.Count -gt 0) {
    throw "以下 TTS 进程在 ${WaitTimeoutSec}s 内未能退出：$($StillAlive -join ', ')"
}

if (Test-Path -LiteralPath $PidFile) {
    Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
}

Write-Host '[stop-tts] TTS 服务已干净停止。'
