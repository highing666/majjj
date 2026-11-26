#Requires -Version 7.0
<#
.SYNOPSIS
    幂等停止运行中的 llama-server 推理进程。
.DESCRIPTION
    同时检查 PID 文件、监听指定端口的进程以及目标根目录下的 llama-server.exe 进程，
    执行干净停止并等待退出；若服务未运行则直接成功返回。
.EXAMPLE
    pwsh -File .\stop-llama-server.ps1
.EXAMPLE
    pwsh -File .\stop-llama-server.ps1 -RootDir E:\majjj-llm -Port 8080
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-llm',
    [ValidateRange(1, 65535)]
    [int]$Port = 8080,
    [ValidateRange(1, 120)]
    [int]$WaitTimeoutSec = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$PidFile = Join-Path $RootDir 'logs\llama-server.pid'
$TargetPids = [System.Collections.Generic.HashSet[int]]::new()

# 1. 从 PID 文件读取
if (Test-Path -LiteralPath $PidFile) {
    $RawPid = (Get-Content -LiteralPath $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
    $ParsedPid = 0
    if ($null -ne $RawPid -and [int]::TryParse($RawPid.Trim(), [ref]$ParsedPid) -and $ParsedPid -gt 0) {
        $Proc = Get-Process -Id $ParsedPid -ErrorAction SilentlyContinue
        if ($null -ne $Proc -and $Proc.ProcessName -ieq 'llama-server') {
            $TargetPids.Add($ParsedPid) | Out-Null
        }
    }
}

# 2. 按命令行匹配目标目录或目标端口的 llama-server.exe
$NormalizedRoot = $RootDir.TrimEnd('\', '/')
$CimProcs = Get-CimInstance -ClassName Win32_Process -Filter "Name = 'llama-server.exe'" -ErrorAction SilentlyContinue
foreach ($P in @($CimProcs)) {
    if ($null -eq $P) { continue }
    $Cmd = [string]$P.CommandLine
    $Exe = [string]$P.ExecutablePath
    $MatchesRoot = ($Exe -and $Exe.StartsWith($NormalizedRoot, [System.StringComparison]::OrdinalIgnoreCase)) -or
                   ($Cmd -and $Cmd.IndexOf($NormalizedRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0)
    $MatchesPort = ($Cmd -and ($Cmd -match "(?:--port|-p)\s+$Port(?:\s|$)"))
    if ($MatchesRoot -or $MatchesPort) {
        $TargetPids.Add([int]$P.ProcessId) | Out-Null
    }
}

# 3. 兜底检查监听目标端口的 llama-server 进程
try {
    $Listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    foreach ($Conn in @($Listeners)) {
        if ($null -eq $Conn) { continue }
        $OwnerProc = Get-Process -Id $Conn.OwningProcess -ErrorAction SilentlyContinue
        if ($null -ne $OwnerProc -and $OwnerProc.ProcessName -ieq 'llama-server') {
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
    Write-Host "[stop] 未发现运行中的 llama-server（Port=$Port），幂等退出。"
    exit 0
}

$PidList = @($TargetPids)
Write-Host "[stop] 正在停止 llama-server 进程：PID=$($PidList -join ', ')"
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
    throw "以下 llama-server 进程在 ${WaitTimeoutSec}s 内未能退出：$($StillAlive -join ', ')"
}

if (Test-Path -LiteralPath $PidFile) {
    Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
}

Write-Host '[stop] llama-server 已干净停止。'
