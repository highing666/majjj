#Requires -Version 7.0
<#
.SYNOPSIS
    幂等绿色部署 Windows 原生局域网情感 TTS 服务（默认 E:\majjj-tts，独立 uv 虚拟环境）。
.DESCRIPTION
    在指定的空闲大容量分区目录（默认 E:\majjj-tts）建立独立绿色运行目录，
    不依赖 WSL2 或 Docker Desktop，不污染系统盘与全局 Python：
      1. 同步运维脚本与 Python FastAPI 服务端代码至目标根目录；
      2. 在 $RootDir\bin 与 $RootDir\.venv 下通过 uv 建立独立 Python 3.11 虚拟环境；
      3. 安装 PyTorch CUDA（使用原生 SDPA，零 flash-attn 编译依赖）与所选 TTS 后端依赖；
      4. 拉取本地固定 HuggingFace 模型快照至 $RootDir\models\（幂等跳过已存在权重）；
      5. 提示并输出第 1 道显存护栏（NVIDIA Control Panel 禁止回退系统共享显存）配置路径。
.EXAMPLE
    pwsh -File .\deploy-tts-server.ps1 -RootDir E:\majjj-tts
.EXAMPLE
    pwsh -File .\deploy-tts-server.ps1 -RootDir E:\majjj-tts -Backend cosyvoice3
.EXAMPLE
    pwsh -File .\deploy-tts-server.ps1 -RootDir E:\majjj-tts -SkipModel
#>
[CmdletBinding()]
param(
    [string]$RootDir = 'E:\majjj-tts',
    [ValidateSet('qwen3-custom-voice', 'cosyvoice3', 'qwen3-base-clone')]
    [string]$Backend = 'qwen3-custom-voice',
    [string]$ModelRepo = '',
    [string]$PythonVersion = '3.11',
    [string]$TorchIndexUrl = 'https://download.pytorch.org/whl/cu124',
    [string]$UvZipUrl = 'https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip',
    [switch]$SkipVenv,
    [switch]$SkipModel,
    [switch]$ForceRedownload
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$DefaultModelRepos = @{
    'qwen3-custom-voice' = 'Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice'
    'cosyvoice3'         = 'FunAudioLLM/Fun-CosyVoice3-0.5B-2512'
    'qwen3-base-clone'   = 'Qwen/Qwen3-TTS-12Hz-0.6B-Base'
}

if ([string]::IsNullOrWhiteSpace($ModelRepo)) {
    $ModelRepo = $DefaultModelRepos[$Backend]
}

$ModelDirName = ($ModelRepo -split '/')[-1]
$BinDir = Join-Path $RootDir 'bin'
$CacheDir = Join-Path $RootDir 'cache'
$UvCacheDir = Join-Path $CacheDir 'uv'
$UvPythonDir = Join-Path $RootDir 'python'
$VenvDir = Join-Path $RootDir '.venv'
$ModelsDir = Join-Path $RootDir 'models'
$VoicesDir = Join-Path $RootDir 'voices'
$ServerDir = Join-Path $RootDir 'server'
$LogsDir = Join-Path $RootDir 'logs'
$TmpDir = Join-Path $RootDir 'tmp'
$ModelLocalDir = Join-Path $ModelsDir $ModelDirName

foreach ($Dir in @($RootDir, $BinDir, $CacheDir, $UvCacheDir, $UvPythonDir, $ModelsDir, $VoicesDir, $ServerDir, $LogsDir, $TmpDir)) {
    if (-not (Test-Path -LiteralPath $Dir)) {
        New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    }
}

# 将 uv 缓存与受管 Python 全部限制在 $RootDir 内，不污染系统盘 C:\
$env:UV_CACHE_DIR = $UvCacheDir
$env:UV_PYTHON_INSTALL_DIR = $UvPythonDir
$env:HF_HOME = Join-Path $CacheDir 'huggingface'

function Sync-CompanionFiles {
    param([string]$TargetRoot)
    if ([string]::IsNullOrWhiteSpace($PSScriptRoot)) {
        return
    }
    $SrcResolved = (Resolve-Path -LiteralPath $PSScriptRoot).Path.TrimEnd('\', '/')
    $DstResolved = (Resolve-Path -LiteralPath $TargetRoot).Path.TrimEnd('\', '/')
    if ($SrcResolved -ieq $DstResolved) {
        return
    }

    $RootFiles = @(
        'deploy-tts-server.ps1',
        'start-tts-server.ps1',
        'stop-tts-server.ps1',
        'setup-firewall.ps1',
        'install-autostart.ps1',
        'smoke-test.ps1',
        'pyproject.toml'
    )
    foreach ($Name in $RootFiles) {
        $SrcFile = Join-Path $SrcResolved $Name
        if (Test-Path -LiteralPath $SrcFile) {
            Copy-Item -LiteralPath $SrcFile -Destination (Join-Path $DstResolved $Name) -Force
        }
    }

    $SrcServerDir = Join-Path $SrcResolved 'server'
    $DstServerDir = Join-Path $DstResolved 'server'
    if (Test-Path -LiteralPath $SrcServerDir) {
        $ServerFiles = @(
            '__init__.py',
            '__main__.py',
            'app.py',
            'audio.py',
            'backends.py',
            'personas.json',
            'personas.py',
            'service.py',
            'vram_guard.py'
        )
        foreach ($FileName in $ServerFiles) {
            $SrcItem = Join-Path $SrcServerDir $FileName
            if (Test-Path -LiteralPath $SrcItem) {
                Copy-Item -LiteralPath $SrcItem -Destination (Join-Path $DstServerDir $FileName) -Force
            }
        }
    }
    Write-Host "[deploy-tts] 已同步运维脚本与服务端源码至 $DstResolved"
}

function Resolve-UvExecutable {
    $LocalUv = Join-Path $BinDir 'uv.exe'
    if (Test-Path -LiteralPath $LocalUv) {
        return $LocalUv
    }
    $GlobalUv = Get-Command uv.exe -ErrorAction SilentlyContinue
    if ($null -ne $GlobalUv) {
        return $GlobalUv.Source
    }

    Write-Host "[deploy-tts] 未检测到 uv.exe，正在绿色下载便携版至 $BinDir ..."
    $ZipPath = Join-Path $TmpDir 'uv-win64.zip'
    Invoke-WebRequest -Uri $UvZipUrl -OutFile $ZipPath -UseBasicParsing
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $BinDir -Force
    Remove-Item -LiteralPath $ZipPath -Force -ErrorAction SilentlyContinue

    if (-not (Test-Path -LiteralPath $LocalUv)) {
        $Found = Get-ChildItem -LiteralPath $BinDir -Recurse -Filter 'uv.exe' | Select-Object -First 1
        if ($null -eq $Found) {
            throw "便携版 uv 解压后未找到 uv.exe：$BinDir"
        }
        Copy-Item -LiteralPath $Found.FullName -Destination $LocalUv -Force
    }
    return $LocalUv
}

# 1. 同步脚本与 Python 服务端代码
Sync-CompanionFiles -TargetRoot $RootDir

# 2. 准备独立绿色 uv 与 Python 虚拟环境
$UvExe = Resolve-UvExecutable
$VenvPython = Join-Path $VenvDir 'Scripts\python.exe'

if (-not $SkipVenv) {
    if ($ForceRedownload -or -not (Test-Path -LiteralPath $VenvPython)) {
        Write-Host "[deploy-tts] 正在使用 uv 创建独立 Python $PythonVersion 虚拟环境：$VenvDir"
        & $UvExe venv --python $PythonVersion $VenvDir
        if ($LASTEXITCODE -ne 0) {
            throw "uv venv 创建失败（exit=$LASTEXITCODE）。"
        }
    } else {
        Write-Host "[deploy-tts] 已检测到独立虚拟环境：$VenvPython"
    }

    Write-Host '[deploy-tts] 安装 CUDA 版 PyTorch（Ampere SM 8.6 使用原生 SDPA + BF16，无需编译 flash-attn）...'
    & $UvExe pip install --python $VenvPython --index-url $TorchIndexUrl torch torchaudio
    if ($LASTEXITCODE -ne 0) {
        throw "安装 PyTorch CUDA 失败（exit=$LASTEXITCODE）。"
    }

    Write-Host '[deploy-tts] 安装核心服务端依赖（FastAPI / uvicorn / pydantic / huggingface-hub）...'
    & $UvExe pip install --python $VenvPython fastapi uvicorn pydantic numpy soundfile huggingface-hub
    if ($LASTEXITCODE -ne 0) {
        throw "安装核心服务端依赖失败（exit=$LASTEXITCODE）。"
    }

    if ($Backend -eq 'qwen3-custom-voice' -or $Backend -eq 'qwen3-base-clone') {
        Write-Host "[deploy-tts] 安装 Qwen3-TTS 后端依赖（faster-qwen3-tts + qwen-tts，零 flash-attn 编译）..."
        & $UvExe pip install --python $VenvPython faster-qwen3-tts qwen-tts
        if ($LASTEXITCODE -ne 0) {
            throw "安装 Qwen3-TTS 依赖失败（exit=$LASTEXITCODE）。"
        }
    } elseif ($Backend -eq 'cosyvoice3') {
        Write-Host '[deploy-tts] 安装 Fun-CosyVoice3-0.5B-2512 Windows 依赖（onnxruntime-gpu + wetext）...'
        & $UvExe pip install --python $VenvPython onnxruntime-gpu wetext
        if ($LASTEXITCODE -ne 0) {
            throw "安装 CosyVoice3 基础依赖失败（exit=$LASTEXITCODE）。"
        }
        Write-Warning '[deploy-tts] CosyVoice3 备选后端需在独立环境中准备 CosyVoice 源码与 Windows pynini，详见 docs/windows-tts-deployment.md。'
    }
}

# 3. 下载本地模型快照（运行时离线加载，禁止每次启动隐式连网）
if (-not $SkipModel) {
    $HasWeights = (Test-Path -LiteralPath $ModelLocalDir) -and (@(Get-ChildItem -LiteralPath $ModelLocalDir -File -ErrorAction SilentlyContinue).Count -gt 0)
    if (-not $ForceRedownload -and $HasWeights) {
        Write-Host "[deploy-tts] 模型目录已存在且非空，跳过下载：$ModelLocalDir"
    } else {
        if (-not (Test-Path -LiteralPath $VenvPython)) {
            throw "未找到虚拟环境解释器：$VenvPython（请勿同时跳过 venv 且未预装环境）。"
        }
        Write-Host "[deploy-tts] 正在拉取模型快照 $ModelRepo 至 $ModelLocalDir ..."
        $DownloadCode = @"
from huggingface_hub import snapshot_download
snapshot_download(repo_id=r'$ModelRepo', local_dir=r'$ModelLocalDir', local_dir_use_symlinks=False)
print('Snapshot download complete')
"@
        & $VenvPython -c $DownloadCode
        if ($LASTEXITCODE -ne 0) {
            throw "模型快照下载失败（repo=$ModelRepo, exit=$LASTEXITCODE）。"
        }
    }
}

Write-Host ''
Write-Host '[deploy-tts] 绿色部署完成：'
Write-Host "  根目录   : $RootDir"
Write-Host "  后端引擎 : $Backend"
Write-Host "  虚拟环境 : $VenvPython"
Write-Host "  模型快照 : $ModelLocalDir"
Write-Host "  日志目录 : $LogsDir"
Write-Host ''
Write-Host '[重要] 请务必完成第 1 道显存护栏（NVIDIA 驱动层防静默溢出至系统共享内存）：'
Write-Host '  1. 打开 NVIDIA Control Panel -> 管理 3D 设置 -> 程序设置'
Write-Host "  2. 添加并选中专用解释器：$VenvPython"
Write-Host '  3. 将 "CUDA - Sysmem Fallback Policy" 设为 "Prefer No Sysmem Fallback" 并应用'
Write-Host ''
Write-Host '下一步：'
Write-Host "  1. 管理员权限运行：pwsh -File `"$RootDir\setup-firewall.ps1`""
Write-Host "  2. 启动 TTS 服务 ：pwsh -File `"$RootDir\start-tts-server.ps1`" -Backend $Backend"
Write-Host "  3. 真机冒烟验收 ：pwsh -File `"$RootDir\smoke-test.ps1`""
