#Requires -Version 7.0
<#
.SYNOPSIS
    可选幂等安装或卸载 llama-server 开机自启 Windows 计划任务（SYSTEM 权限）。
.DESCRIPTION
    注册在系统启动时自动调用 start-llama-server.ps1 的计划任务（默认 MajjjLlamaServer），
    使用 SYSTEM 账号最高权限后台常驻运行，无需人工登录桌面。
    重复运行会以 -Force 幂等更新任务；传入 -Uninstall 可幂等卸载。
.EXAMPLE
    pwsh -File .\install-autostart.ps1
.EXAMPLE
    pwsh -File .\install-autostart.ps1 -RootDir E:\majjj-llm -ModelPath E:\majjj-llm\models\Qwen3.5-9B-Q5_K_M.gguf
.EXAMPLE
    pwsh -File .\install-autostart.ps1 -Uninstall
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'MajjjLlamaServer',
    [string]$RootDir = 'E:\majjj-llm',
    [string]$ModelPath = '',
    [string]$BindHost = '0.0.0.0',
    [ValidateRange(1, 65535)]
    [int]$Port = 8080,
    [ValidateRange(1, 16)]
    [int]$ParallelSlots = 3,
    [ValidateRange(1024, 131072)]
    [int]$ContextSize = 12288,
    [switch]$Uninstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-IsAdministrator {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $Principal = [Security.Principal.WindowsPrincipal]::new($Identity)
    if (-not $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw '管理开机自启计划任务需要管理员权限，请以“以管理员身份运行”打开 PowerShell 7 后重试。'
    }
}

Assert-IsAdministrator

if ($Uninstall) {
    $ExistingTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($null -eq $ExistingTask) {
        Write-Host "[autostart] 计划任务 '$TaskName' 不存在，幂等跳过卸载。"
        exit 0
    }
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Host "[autostart] 已卸载开机自启计划任务：$TaskName"
    exit 0
}

$StartScript = Join-Path $RootDir 'start-llama-server.ps1'
if (-not (Test-Path -LiteralPath $StartScript)) {
    throw "未找到启动脚本：$StartScript（请先运行 deploy-llama-server.ps1 将脚本部署到 $RootDir）。"
}

$PwshCmd = Get-Command pwsh.exe -ErrorAction SilentlyContinue
if ($null -eq $PwshCmd) {
    throw '未在 PATH 中找到 pwsh.exe（PowerShell 7）。'
}
$PwshExe = $PwshCmd.Source

$ArgParts = @(
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy', 'Bypass',
    '-File', "`"$StartScript`"",
    '-RootDir', "`"$RootDir`"",
    '-BindHost', $BindHost,
    '-Port', [string]$Port,
    '-ParallelSlots', [string]$ParallelSlots,
    '-ContextSize', [string]$ContextSize
)
if (-not [string]::IsNullOrWhiteSpace($ModelPath)) {
    $ArgParts += @('-ModelPath', "`"$ModelPath`"")
}
$TaskArguments = $ArgParts -join ' '

$Action = New-ScheduledTaskAction -Execute $PwshExe -Argument $TaskArguments -WorkingDirectory $RootDir
$Trigger = New-ScheduledTaskTrigger -AtStartup
$Principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
$Settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -StartWhenAvailable `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 10) `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $Action `
    -Trigger $Trigger `
    -Principal $Principal `
    -Settings $Settings `
    -Description 'Majjj 局域网毒舌牌友 llama-server 推理服务开机自启任务' `
    -Force | Out-Null

Write-Host '[autostart] 开机自启计划任务已幂等注册：'
Write-Host "  任务名称 : $TaskName"
Write-Host "  运行账号 : SYSTEM (Highest)"
Write-Host "  触发条件 : 系统启动时 (AtStartup)"
Write-Host "  执行命令 : $PwshExe $TaskArguments"
