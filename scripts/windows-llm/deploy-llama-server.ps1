#Requires -Version 7.0
<#
.SYNOPSIS
    幂等绿色部署 Windows CUDA 版 llama-server 与 Qwen3.5-9B-Q5_K_M 模型权重。
.DESCRIPTION
    在指定的空闲大容量分区目录（默认 E:\majjj-llm）建立独立绿色运行目录，
    拉取并校验官方预编译 CUDA 版 llama-server、CUDA 运行库与 GGUF 模型权重。
    支持跳过已校验通过的本地文件、HuggingFace 直链解析与 HTTP Range 分块断点续传，
    并在下载完成后校验字节数与 SHA-256。
.EXAMPLE
    pwsh -File .\deploy-llama-server.ps1 -RootDir E:\majjj-llm
.EXAMPLE
    pwsh -File .\deploy-llama-server.ps1 -RootDir D:\majjj-llm -SkipModel
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-llm',
    [string]$LlamaReleaseTag = 'b12760',
    [string]$CudaVersion = '13.5',
    [string]$LlamaZipUrl = '',
    [string]$CudartZipUrl = '',
    [string]$ModelRepo = 'unsloth/Qwen3.5-9B-GGUF',
    [string]$ModelFileName = 'Qwen3.5-9B-Q5_K_M.gguf',
    [string]$ModelUrl = '',
    [string]$ExpectedModelSha256 = 'dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c',
    [long]$ExpectedModelBytes = 6577841376,
    [ValidateRange(4, 256)]
    [int]$ChunkSizeMB = 64,
    [ValidateRange(1, 20)]
    [int]$MaxRetries = 5,
    [switch]$SkipBinary,
    [switch]$SkipModel,
    [switch]$ForceRedownload
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($LlamaZipUrl)) {
    $LlamaZipUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$LlamaReleaseTag/llama-$LlamaReleaseTag-bin-win-cuda-$CudaVersion-x64.zip"
}
if ([string]::IsNullOrWhiteSpace($CudartZipUrl)) {
    $CudartZipUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$LlamaReleaseTag/cudart-llama-bin-win-cuda-$CudaVersion-x64.zip"
}
if ([string]::IsNullOrWhiteSpace($ModelUrl)) {
    $ModelUrl = "https://huggingface.co/$ModelRepo/resolve/main/$ModelFileName"
}

$BinDir = Join-Path $RootDir 'bin'
$ModelsDir = Join-Path $RootDir 'models'
$LogsDir = Join-Path $RootDir 'logs'
$TmpDir = Join-Path $RootDir 'tmp'

foreach ($Dir in @($RootDir, $BinDir, $ModelsDir, $LogsDir, $TmpDir)) {
    if (-not (Test-Path -LiteralPath $Dir)) {
        New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    }
}

function Sync-CompanionScripts {
    param([string]$TargetRoot)
    if ([string]::IsNullOrWhiteSpace($PSScriptRoot)) {
        return
    }
    $SrcResolved = (Resolve-Path -LiteralPath $PSScriptRoot).Path.TrimEnd('\', '/')
    $DstResolved = (Resolve-Path -LiteralPath $TargetRoot).Path.TrimEnd('\', '/')
    if ($SrcResolved -ieq $DstResolved) {
        return
    }
    $ScriptNames = @(
        'deploy-llama-server.ps1',
        'start-llama-server.ps1',
        'stop-llama-server.ps1',
        'setup-firewall.ps1',
        'install-autostart.ps1',
        'smoke-test.ps1'
    )
    foreach ($Name in $ScriptNames) {
        $SrcFile = Join-Path $SrcResolved $Name
        if (Test-Path -LiteralPath $SrcFile) {
            Copy-Item -LiteralPath $SrcFile -Destination (Join-Path $DstResolved $Name) -Force
        }
    }
    Write-Host "[deploy] 已同步运维脚本至 $DstResolved"
}

function Test-ModelFileValid {
    param(
        [string]$Path,
        [long]$ExpectedBytes,
        [string]$ExpectedSha256
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }
    $Item = Get-Item -LiteralPath $Path
    if ($ExpectedBytes -gt 0 -and $Item.Length -ne $ExpectedBytes) {
        Write-Warning "[deploy] 模型文件字节数不匹配：当前 $($Item.Length)，预期 $ExpectedBytes"
        return $false
    }
    if (-not [string]::IsNullOrWhiteSpace($ExpectedSha256)) {
        Write-Host "[deploy] 正在校验模型 SHA-256：$Path ..."
        $ActualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
        $TargetHash = $ExpectedSha256.Trim().ToLowerInvariant()
        if ($ActualHash -ne $TargetHash) {
            Write-Warning "[deploy] 模型 SHA-256 校验失败：实际 $ActualHash，预期 $TargetHash"
            return $false
        }
    }
    return $true
}

function Invoke-DownloadWithRetry {
    param(
        [string]$Uri,
        [string]$OutFile,
        [int]$Retries
    )
    for ($Attempt = 1; $Attempt -le $Retries; $Attempt++) {
        try {
            Write-Host "[deploy] 下载 ($Attempt/$Retries): $Uri"
            Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing
            return
        } catch {
            if ($Attempt -eq $Retries) {
                throw
            }
            $DelaySec = [Math]::Min(30, [int][Math]::Pow(2, $Attempt))
            Write-Warning "[deploy] 下载失败，${DelaySec}s 后重试：$($_.Exception.Message)"
            Start-Sleep -Seconds $DelaySec
        }
    }
}

function Resolve-DirectDownloadInfo {
    param([string]$Uri)
    $Handler = [System.Net.Http.HttpClientHandler]::new()
    $Handler.AllowAutoRedirect = $true
    $Client = [System.Net.Http.HttpClient]::new($Handler)
    $Client.Timeout = [TimeSpan]::FromSeconds(60)
    try {
        # 使用 HEAD 解析 HuggingFace 302 重定向链，拿到直链 CDN 地址与总长度，避免 curl -L -C - 偏移错位
        $Req = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Head, $Uri)
        $Resp = $Client.Send($Req)
        $Resp.EnsureSuccessStatusCode() | Out-Null
        $FinalUri = $Resp.RequestMessage.RequestUri.AbsoluteUri
        $ContentLength = $Resp.Content.Headers.ContentLength
        $SupportsRange = $false
        if ($Resp.Headers.AcceptRanges) {
            foreach ($RangeUnit in $Resp.Headers.AcceptRanges) {
                if ($RangeUnit -ieq 'bytes') {
                    $SupportsRange = $true
                }
            }
        }
        return [pscustomobject]@{
            FinalUri      = $FinalUri
            ContentLength = $ContentLength
            SupportsRange = $SupportsRange
        }
    } finally {
        $Client.Dispose()
        $Handler.Dispose()
    }
}

function Invoke-ChunkedModelDownload {
    param(
        [string]$SourceUrl,
        [string]$DestinationPath,
        [string]$WorkDir,
        [long]$ExpectedBytes,
        [int]$ChunkMB,
        [int]$Retries
    )

    Write-Host "[deploy] 正在解析模型直链 CDN 地址：$SourceUrl"
    $Info = Resolve-DirectDownloadInfo -Uri $SourceUrl
    $DirectUrl = $Info.FinalUri
    $TotalBytes = if ($null -ne $Info.ContentLength -and $Info.ContentLength -gt 0) {
        [long]$Info.ContentLength
    } else {
        $ExpectedBytes
    }

    if ($ExpectedBytes -gt 0 -and $TotalBytes -gt 0 -and $TotalBytes -ne $ExpectedBytes) {
        throw "远端模型字节数 ($TotalBytes) 与预期 ($ExpectedBytes) 不一致，请确认模型 URL 或哈希参数。"
    }

    if (-not $Info.SupportsRange -or $TotalBytes -le 0) {
        Write-Warning '[deploy] 远端未声明 Accept-Ranges: bytes 或未知总长度，回退为单流下载。'
        $TmpSingle = "$DestinationPath.downloading"
        Invoke-DownloadWithRetry -Uri $DirectUrl -OutFile $TmpSingle -Retries $Retries
        Move-Item -LiteralPath $TmpSingle -Destination $DestinationPath -Force
        return
    }

    $ChunkBytes = [long]$ChunkMB * 1024L * 1024L
    $ChunkCount = [int][Math]::Ceiling($TotalBytes / [double]$ChunkBytes)
    $BaseName = [System.IO.Path]::GetFileName($DestinationPath)
    $PartDir = Join-Path $WorkDir "$BaseName.parts"
    if (-not (Test-Path -LiteralPath $PartDir)) {
        New-Item -ItemType Directory -Path $PartDir -Force | Out-Null
    }

    Write-Host "[deploy] 直链 CDN 就绪：总大小 $([Math]::Round($TotalBytes / 1GB, 2)) GiB，分 $ChunkCount 块（每块 ${ChunkMB} MiB，支持断点续传）"

    $Handler = [System.Net.Http.HttpClientHandler]::new()
    $Handler.AllowAutoRedirect = $true
    $Client = [System.Net.Http.HttpClient]::new($Handler)
    $Client.Timeout = [TimeSpan]::FromMinutes(10)

    try {
        for ($Index = 0; $Index -lt $ChunkCount; $Index++) {
            $RangeStart = [long]$Index * $ChunkBytes
            $RangeEnd = [Math]::Min($TotalBytes - 1L, $RangeStart + $ChunkBytes - 1L)
            $ExpectedPartLen = ($RangeEnd - $RangeStart) + 1L
            $PartFile = Join-Path $PartDir ('part_{0:D4}.bin' -f $Index)

            if ((Test-Path -LiteralPath $PartFile) -and ((Get-Item -LiteralPath $PartFile).Length -eq $ExpectedPartLen)) {
                continue
            }

            $Downloaded = $false
            for ($Attempt = 1; $Attempt -le $Retries; $Attempt++) {
                try {
                    $Req = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Get, $DirectUrl)
                    $Req.Headers.Range = [System.Net.Http.Headers.RangeHeaderValue]::new($RangeStart, $RangeEnd)
                    $Resp = $Client.Send($Req, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead)
                    if ([int]$Resp.StatusCode -ne 206 -and [int]$Resp.StatusCode -ne 200) {
                        throw "HTTP $([int]$Resp.StatusCode) ($($Resp.ReasonPhrase))"
                    }

                    $PartTmp = "$PartFile.tmp"
                    $InStream = $Resp.Content.ReadAsStream()
                    try {
                        $OutStream = [System.IO.File]::Create($PartTmp)
                        try {
                            $InStream.CopyTo($OutStream)
                        } finally {
                            $OutStream.Dispose()
                        }
                    } finally {
                        $InStream.Dispose()
                        $Resp.Dispose()
                    }

                    $ActualLen = (Get-Item -LiteralPath $PartTmp).Length
                    if ($ActualLen -ne $ExpectedPartLen) {
                        Remove-Item -LiteralPath $PartTmp -Force -ErrorAction SilentlyContinue
                        throw "分块 $Index 长度异常：实际 $ActualLen，预期 $ExpectedPartLen"
                    }
                    Move-Item -LiteralPath $PartTmp -Destination $PartFile -Force
                    $Downloaded = $true
                    break
                } catch {
                    if ($Attempt -eq $Retries) {
                        throw "分块 $Index ($RangeStart-$RangeEnd) 重试 $Retries 次仍失败：$($_.Exception.Message)"
                    }
                    $DelaySec = [Math]::Min(30, [int][Math]::Pow(2, $Attempt))
                    Write-Warning "[deploy] 分块 $($Index + 1)/$ChunkCount 失败，${DelaySec}s 后重试：$($_.Exception.Message)"
                    Start-Sleep -Seconds $DelaySec
                }
            }

            if ($Downloaded -and (($Index + 1) % 8 -eq 0 -or ($Index + 1) -eq $ChunkCount)) {
                $Pct = [Math]::Round((($Index + 1) / [double]$ChunkCount) * 100, 1)
                Write-Host "[deploy] 模型下载进度：$($Index + 1)/$ChunkCount 块 ($Pct%)"
            }
        }
    } finally {
        $Client.Dispose()
        $Handler.Dispose()
    }

    $MergedTmp = Join-Path $WorkDir "$BaseName.merged"
    Write-Host "[deploy] 正在合并 $ChunkCount 个分块至 $MergedTmp ..."
    $Buffer = New-Object byte[] (4 * 1024 * 1024)
    $OutFileStream = [System.IO.File]::Create($MergedTmp)
    try {
        for ($Index = 0; $Index -lt $ChunkCount; $Index++) {
            $PartFile = Join-Path $PartDir ('part_{0:D4}.bin' -f $Index)
            $InFileStream = [System.IO.File]::OpenRead($PartFile)
            try {
                while (($Read = $InFileStream.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
                    $OutFileStream.Write($Buffer, 0, $Read)
                }
            } finally {
                $InFileStream.Dispose()
            }
        }
    } finally {
        $OutFileStream.Dispose()
    }

    $MergedLen = (Get-Item -LiteralPath $MergedTmp).Length
    if ($MergedLen -ne $TotalBytes) {
        throw "合并后文件字节数 ($MergedLen) 与预期 ($TotalBytes) 不一致。"
    }

    Move-Item -LiteralPath $MergedTmp -Destination $DestinationPath -Force
    Remove-Item -LiteralPath $PartDir -Recurse -Force -ErrorAction SilentlyContinue
}

# 1. 同步运维脚本（若从仓库目录执行）
Sync-CompanionScripts -TargetRoot $RootDir

# 2. 部署官方 CUDA 版 llama-server 与 CUDA 运行库
$LlamaExe = Join-Path $BinDir 'llama-server.exe'
if (-not $SkipBinary) {
    $HasCudart = @(Get-ChildItem -LiteralPath $BinDir -Filter 'cudart64_*.dll' -ErrorAction SilentlyContinue).Count -gt 0
    if (-not $ForceRedownload -and (Test-Path -LiteralPath $LlamaExe) -and $HasCudart) {
        Write-Host "[deploy] 已检测到有效的 llama-server.exe 与 CUDA 运行库，跳过下载（如需覆盖请加 -ForceRedownload）。"
    } else {
        $LlamaZip = Join-Path $TmpDir "llama-$LlamaReleaseTag-cuda-$CudaVersion.zip"
        $CudartZip = Join-Path $TmpDir "cudart-$LlamaReleaseTag-cuda-$CudaVersion.zip"

        Invoke-DownloadWithRetry -Uri $LlamaZipUrl -OutFile $LlamaZip -Retries $MaxRetries
        Invoke-DownloadWithRetry -Uri $CudartZipUrl -OutFile $CudartZip -Retries $MaxRetries

        Write-Host "[deploy] 正在解压 llama-server ($LlamaReleaseTag) 至 $BinDir ..."
        Expand-Archive -LiteralPath $LlamaZip -DestinationPath $BinDir -Force
        Write-Host "[deploy] 正在解压 CUDA $CudaVersion 运行库至 $BinDir ..."
        Expand-Archive -LiteralPath $CudartZip -DestinationPath $BinDir -Force

        if (-not (Test-Path -LiteralPath $LlamaExe)) {
            throw "解压后未找到 $LlamaExe，请检查发布包结构。"
        }
        Remove-Item -LiteralPath $LlamaZip, $CudartZip -Force -ErrorAction SilentlyContinue
    }

    Write-Host '[deploy] 验证 llama-server --version：'
    & $LlamaExe --version
}

# 3. 部署并校验 GGUF 模型权重
$ModelPath = Join-Path $ModelsDir $ModelFileName
if (-not $SkipModel) {
    if (-not $ForceRedownload -and (Test-ModelFileValid -Path $ModelPath -ExpectedBytes $ExpectedModelBytes -ExpectedSha256 $ExpectedModelSha256)) {
        Write-Host "[deploy] 模型已就绪且校验通过，跳过下载：$ModelPath"
    } else {
        Invoke-ChunkedModelDownload `
            -SourceUrl $ModelUrl `
            -DestinationPath $ModelPath `
            -WorkDir $TmpDir `
            -ExpectedBytes $ExpectedModelBytes `
            -ChunkMB $ChunkSizeMB `
            -Retries $MaxRetries

        if (-not (Test-ModelFileValid -Path $ModelPath -ExpectedBytes $ExpectedModelBytes -ExpectedSha256 $ExpectedModelSha256)) {
            throw "模型下载完成但完整性/SHA-256 校验未通过：$ModelPath"
        }
        Write-Host "[deploy] 模型 SHA-256 校验通过：$ModelPath"
    }
}

Write-Host ''
Write-Host '[deploy] 绿色部署完成：'
Write-Host "  根目录 : $RootDir"
Write-Host "  可执行 : $LlamaExe"
Write-Host "  模型   : $ModelPath"
Write-Host "  日志   : $LogsDir"
Write-Host '下一步：'
Write-Host "  1. 管理员权限运行：pwsh -File `"$RootDir\setup-firewall.ps1`""
Write-Host "  2. 启动推理服务 ：pwsh -File `"$RootDir\start-llama-server.ps1`""
Write-Host "  3. 真机冒烟验证 ：pwsh -File `"$RootDir\smoke-test.ps1`""
