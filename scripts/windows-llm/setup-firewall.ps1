#Requires -Version 7.0
<#
.SYNOPSIS
    幂等配置 Windows 防火墙入站规则，仅放行家庭局域网网段访问 llama-server 端口。
.DESCRIPTION
    创建或更新名为 majjj-llama-server-lan-8080 的唯一 Inbound Allow 规则，
    严格限制 RemoteAddress 为指定的家庭局域网网段（默认 192.168.5.0/255.255.255.0），
    拒绝通配地址（Any / 0.0.0.0/0 / *），防止推理端口暴露到非局域网来源。
    重复执行始终保持唯一 1 条规则；传入 -Remove 可干净移除规则。
.EXAMPLE
    pwsh -File .\setup-firewall.ps1
.EXAMPLE
    pwsh -File .\setup-firewall.ps1 -LanSubnet 192.168.1.0/255.255.255.0 -Port 8080
.EXAMPLE
    pwsh -File .\setup-firewall.ps1 -Remove
#>
[CmdletBinding()]
param(
    [string]$RuleName = 'majjj-llama-server-lan-8080',
    [ValidateRange(1, 65535)]
    [int]$Port = 8080,
    [string]$LanSubnet = '192.168.5.0/255.255.255.0',
    [switch]$Remove
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-IsAdministrator {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $Principal = [Security.Principal.WindowsPrincipal]::new($Identity)
    if (-not $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw '配置 Windows 防火墙需要管理员权限，请以“以管理员身份运行”打开 PowerShell 7 后重试。'
    }
}

function Assert-ValidLanSubnet {
    param([string]$Subnet)
    $Trimmed = if ($null -eq $Subnet) { '' } else { $Subnet.Trim() }
    if ([string]::IsNullOrWhiteSpace($Trimmed)) {
        throw '-LanSubnet 不能为空，必须显式指定家庭局域网网段（例如 192.168.5.0/255.255.255.0 或 192.168.5.0/24）。'
    }
    $ForbiddenWildcards = @('Any', '*', '0.0.0.0/0', '0.0.0.0/0.0.0.0', '::/0', 'Internet')
    foreach ($Bad in $ForbiddenWildcards) {
        if ($Trimmed -ieq $Bad) {
            throw "出于局域网安全约束，禁止将 -LanSubnet 设为通配值 '$Trimmed'；请使用私有局域网网段（如 192.168.5.0/255.255.255.0）。"
        }
    }
    if ($Trimmed -notmatch '^(10\.|172\.(1[6-9]|2[0-9]|3[0-1])\.|192\.168\.)\d{1,3}\.\d{1,3}(\/(\d{1,2}|\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}))?$' -and
        $Trimmed -ine 'LocalSubnet') {
        throw "非法或非私有局域网网段 '$Trimmed'；仅允许 RFC1918 私有网段（10.x / 172.16-31.x / 192.168.x）或 LocalSubnet。"
    }
}

Assert-IsAdministrator

# 先清理全部同名旧规则，保证无论重复执行多少次都绝不残留重复项
$Existing = @(Get-NetFirewallRule -DisplayName $RuleName -ErrorAction SilentlyContinue)
if ($Existing.Count -gt 0) {
    Write-Host "[firewall] 清理既有同名规则（共 $($Existing.Count) 条）：$RuleName"
    $Existing | Remove-NetFirewallRule
}

if ($Remove) {
    Write-Host "[firewall] 已移除入站规则：$RuleName"
    exit 0
}

Assert-ValidLanSubnet -Subnet $LanSubnet

$Rule = New-NetFirewallRule `
    -DisplayName $RuleName `
    -Description "Allow LAN-only access from $LanSubnet to Majjj llama-server TCP/$Port" `
    -Direction Inbound `
    -Action Allow `
    -Protocol TCP `
    -LocalPort $Port `
    -RemoteAddress $LanSubnet `
    -Profile Any `
    -Enabled True

$Verified = @(Get-NetFirewallRule -DisplayName $RuleName -ErrorAction SilentlyContinue)
if ($Verified.Count -ne 1) {
    throw "防火墙规则校验失败：预期唯一 1 条规则 '$RuleName'，实际发现 $($Verified.Count) 条。"
}

$AddrFilter = $Rule | Get-NetFirewallAddressFilter
$PortFilter = $Rule | Get-NetFirewallPortFilter

Write-Host '[firewall] 局域网入站防火墙规则已幂等生效：'
Write-Host "  规则名称 : $($Rule.DisplayName)"
Write-Host "  方向动作 : $($Rule.Direction) / $($Rule.Action) (Profile=$($Rule.Profile))"
Write-Host "  协议端口 : $($PortFilter.Protocol) / $($PortFilter.LocalPort)"
Write-Host "  来源网段 : $($AddrFilter.RemoteAddress -join ', ')"
