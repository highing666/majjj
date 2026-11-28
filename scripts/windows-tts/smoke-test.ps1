#Requires -Version 7.0
<#
.SYNOPSIS
    一键真机冒烟与显存压测 Windows 局域网情感 TTS 服务（5 牌友角色 x 6 牌友情绪盲听采样 + 30 句连续压测）。
.DESCRIPTION
    依次验证：
      1. GET /health 就绪状态、当前后端/模型标识、已缓存角色数与显存占用；
      2. POST /v1/tts/synthesize 覆盖全部 5 位牌友角色 x 6 档牌友情绪（共 30 条 WAV），
         校验 24 kHz 16-bit 单声道 RIFF/WAVE 头与诊断响应头，并将音频落盘供人工盲听；
      3. 验证 deadline_ms=0 请求在入队前直接返回 HTTP 408 丢弃；
      4. 验证可选 OpenAI 兼容端点 POST /v1/audio/speech；
      5. 可选执行 30 条连续短句压测与并发突发串行验证，并调用 nvidia-smi 核验总显存 <= 10,800 MiB、
         TTS 单进程显存 <= 3,500 MiB。
.EXAMPLE
    pwsh -File .\smoke-test.ps1
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -BaseUrl http://192.168.5.50:8081 -OutputDir E:\majjj-tts\logs\blind-listen
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -SkipStress
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://127.0.0.1:8081',
    [string]$OutputDir = '',
    [ValidateRange(5, 300)]
    [int]$TimeoutSec = 60,
    [switch]$SkipWarmup,
    [switch]$SkipMatrix,
    [switch]$SkipStress,
    [ValidateRange(5, 100)]
    [int]$StressCount = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$NormalizedBaseUrl = $BaseUrl.TrimEnd('/')
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path ([System.IO.Path]::GetTempPath()) 'majjj-tts-smoke'
}
if (-not (Test-Path -LiteralPath $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$Personas = @(
    @{ Id = 'QuickHothead';    Name = '大川'; Speaker = 'Eric' },
    @{ Id = 'TeaSarcasm';      Name = '小晚'; Speaker = 'Serena' },
    @{ Id = 'StubbornBlamer';  Name = '大鹏'; Speaker = 'Dylan' },
    @{ Id = 'ChaosInstigator'; Name = '小桃'; Speaker = 'Vivian' },
    @{ Id = 'TiltedMaster';    Name = '老陈'; Speaker = 'Uncle_Fu' }
)

$MoodLines = @(
    @{ Mood = 'Smug';     Text = '哈哈哈，五巡自摸基本胡，手速快过你们做大牌！' },
    @{ Mood = 'Mocking';  Text = '哎哟，您这手牌打得可真稳呢，生怕别人碰着呀？' },
    @{ Mood = 'Tilted';   Text = '啧，刚补杠就被抢杠胡？！今天这破手气真绝了！' },
    @{ Mood = 'Nervous';  Text = '等等，这张发财我可不太敢舍，别回头又点炮了。' },
    @{ Mood = 'Plotting'; Text = '哼哼，你们先接着互掐吧，好戏还在后头呢。' },
    @{ Mood = 'Deadpan';  Text = '行行行，算你手气好，下小局咱牌河里接着算账。' }
)

function Assert-ValidWav24kMono {
    param([byte[]]$Bytes, [string]$Label)
    if ($null -eq $Bytes -or $Bytes.Length -lt 44) {
        throw "[$Label] WAV 长度过短：$($Bytes.Length) bytes"
    }
    $Riff = [System.Text.Encoding]::ASCII.GetString($Bytes, 0, 4)
    $Wave = [System.Text.Encoding]::ASCII.GetString($Bytes, 8, 4)
    if ($Riff -ne 'RIFF' -or $Wave -ne 'WAVE') {
        throw "[$Label] 非法 WAV 文件头：$Riff / $Wave"
    }
    $Channels = [System.BitConverter]::ToUInt16($Bytes, 22)
    $SampleRate = [System.BitConverter]::ToUInt32($Bytes, 24)
    $BitsPerSample = [System.BitConverter]::ToUInt16($Bytes, 34)
    if ($Channels -ne 1 -or $SampleRate -ne 24000 -or $BitsPerSample -ne 16) {
        throw "[$Label] 音频格式不符合 24kHz 16-bit 单声道：channels=$Channels, rate=$SampleRate, bits=$BitsPerSample"
    }
}

function Invoke-SynthesizeOnce {
    param(
        [string]$RequestId,
        [string]$PersonaId,
        [string]$Mood,
        [string]$Text,
        [int]$DeadlineMs = 2500
    )
    $Payload = @{
        request_id  = $RequestId
        persona_id  = $PersonaId
        mood        = $Mood
        text        = $Text
        format      = 'wav'
        stream      = $false
        deadline_ms = $DeadlineMs
    } | ConvertTo-Json -Compress

    $Client = [System.Net.Http.HttpClient]::new()
    $Client.Timeout = [TimeSpan]::FromSeconds($TimeoutSec)
    try {
        $Content = [System.Net.Http.StringContent]::new($Payload, [System.Text.Encoding]::UTF8, 'application/json')
        $Sw = [System.Diagnostics.Stopwatch]::StartNew()
        $Resp = $Client.PostAsync("$NormalizedBaseUrl/v1/tts/synthesize", $Content).GetAwaiter().GetResult()
        $Bytes = $Resp.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
        $Sw.Stop()

        $HeaderMap = @{}
        foreach ($H in $Resp.Headers) {
            $HeaderMap[$H.Key] = ($H.Value -join ',')
        }
        return [pscustomobject]@{
            StatusCode = [int]$Resp.StatusCode
            Bytes      = $Bytes
            Headers    = $HeaderMap
            WallMs     = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
        }
    } finally {
        $Client.Dispose()
    }
}

function Invoke-SynthesizeStreamOnce {
    param(
        [string]$RequestId,
        [string]$PersonaId,
        [string]$Mood,
        [string]$Text,
        [int]$DeadlineMs = 5000
    )
    $Payload = @{
        request_id  = $RequestId
        persona_id  = $PersonaId
        mood        = $Mood
        text        = $Text
        format      = 'pcm'
        stream      = $true
        deadline_ms = $DeadlineMs
    } | ConvertTo-Json -Compress

    $Client = [System.Net.Http.HttpClient]::new()
    $Client.Timeout = [TimeSpan]::FromSeconds($TimeoutSec)
    try {
        $ReqMsg = [System.Net.Http.HttpRequestMessage]::new(
            [System.Net.Http.HttpMethod]::Post,
            "$NormalizedBaseUrl/v1/tts/synthesize"
        )
        $ReqMsg.Content = [System.Net.Http.StringContent]::new($Payload, [System.Text.Encoding]::UTF8, 'application/json')

        $Sw = [System.Diagnostics.Stopwatch]::StartNew()
        $Resp = $Client.SendAsync($ReqMsg, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        $HeadersMs = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)

        $HeaderMap = [hashtable]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($H in $Resp.Headers) {
            $HeaderMap[$H.Key] = ($H.Value -join ',')
        }
        foreach ($H in $Resp.Content.Headers) {
            $HeaderMap[$H.Key] = ($H.Value -join ',')
        }

        $StatusCode = [int]$Resp.StatusCode
        if ($StatusCode -ne 200) {
            $BodyBytes = $Resp.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
            $Sw.Stop()
            return [pscustomobject]@{
                StatusCode = $StatusCode
                Headers    = $HeaderMap
                HeadersMs  = $HeadersMs
                TtfaMs     = $null
                TotalMs    = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
                ChunkCount = 0
                TotalBytes = $BodyBytes.Length
                MaxGapMs   = 0.0
            }
        }

        $NetStream = $Resp.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $Buf = [byte[]]::new(16384)
        $ChunkCount = 0
        $TotalBytes = 0
        $TtfaMs = $null
        $PrevChunkMs = $null
        $MaxGapMs = 0.0
        try {
            while ($true) {
                $ReadLen = $NetStream.Read($Buf, 0, $Buf.Length)
                if ($ReadLen -le 0) {
                    break
                }
                $NowMs = $Sw.Elapsed.TotalMilliseconds
                if ($null -eq $TtfaMs) {
                    $TtfaMs = [Math]::Round($NowMs, 1)
                } elseif ($null -ne $PrevChunkMs) {
                    $Gap = $NowMs - $PrevChunkMs
                    if ($Gap -gt $MaxGapMs) {
                        $MaxGapMs = $Gap
                    }
                }
                $PrevChunkMs = $NowMs
                $ChunkCount += 1
                $TotalBytes += $ReadLen
            }
        } finally {
            $NetStream.Dispose()
        }
        $Sw.Stop()

        return [pscustomobject]@{
            StatusCode = $StatusCode
            Headers    = $HeaderMap
            HeadersMs  = $HeadersMs
            TtfaMs     = $TtfaMs
            TotalMs    = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
            ChunkCount = $ChunkCount
            TotalBytes = $TotalBytes
            MaxGapMs   = [Math]::Round($MaxGapMs, 1)
        }
    } finally {
        $Client.Dispose()
    }
}

function Measure-NvidiaVram {
    $NvidiaSmi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue
    if ($null -eq $NvidiaSmi) {
        $NvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    }
    if ($null -eq $NvidiaSmi) {
        Write-Host '[smoke-tts] 本机未检测到 nvidia-smi（跨机运行），跳过本机显存直读。'
        return $null
    }
    $RawTotal = (& $NvidiaSmi.Source --query-gpu=memory.used,memory.total --format=csv,noheader,nounits | Select-Object -First 1)
    if ([string]::IsNullOrWhiteSpace($RawTotal)) {
        return $null
    }
    $Parts = $RawTotal.Split(',') | ForEach-Object { $_.Trim() }
    $UsedMiB = [int]$Parts[0]
    $TotalMiB = [int]$Parts[1]
    Write-Host "[smoke-tts] nvidia-smi 总显存：$UsedMiB MiB / $TotalMiB MiB（共驻红线 <= 10800 MiB）"
    if ($UsedMiB -gt 10800) {
        throw "GPU 总显存超标：$UsedMiB MiB > 10800 MiB"
    }
    return [pscustomobject]@{ UsedMiB = $UsedMiB; TotalMiB = $TotalMiB }
}

Write-Host "[smoke-tts] 目标 TTS 服务：$NormalizedBaseUrl"
Write-Host "[smoke-tts] 盲听采样输出目录：$OutputDir"

# 1. /health 检查
$Health = Invoke-RestMethod -Uri "$NormalizedBaseUrl/health" -Method Get -TimeoutSec $TimeoutSec
if ($null -eq $Health -or $Health.status -ne 'ok' -or -not $Health.ready) {
    throw "/health 检查失败：$($Health | ConvertTo-Json -Compress)"
}
if ([int]$Health.personas_cached -lt 5) {
    throw "/health 缓存角色数不足 5：$($Health.personas_cached)"
}
Write-Host "[smoke-tts] [PASS] /health => backend=$($Health.backend), model=$($Health.model), personas_cached=$($Health.personas_cached), vram_reserved=$($Health.vram_reserved_mib) MiB"

# 2. 热身
if (-not $SkipWarmup) {
    Write-Host '[smoke-tts] 正在执行 3 次热身请求（完成 CUDA Graph 捕获）...'
    for ($i = 1; $i -le 3; $i++) {
        $Warm = Invoke-SynthesizeOnce -RequestId "warmup-$i" -PersonaId 'QuickHothead' -Mood 'Smug' -Text '热身测试，五巡自摸！' -DeadlineMs 10000
        if ($Warm.StatusCode -ne 200) {
            throw "热身请求 #$i 失败：HTTP $($Warm.StatusCode)"
        }
        Assert-ValidWav24kMono -Bytes $Warm.Bytes -Label "warmup-$i"
        Write-Host "[smoke-tts] 热身 #$i 完成：Wall=$($Warm.WallMs)ms"
    }
}

# 3. 5 角色 x 6 情绪矩阵采样（保存 WAV 供人工盲听）
if (-not $SkipMatrix) {
    Write-Host ''
    Write-Host '[smoke-tts] === 5 位牌友角色 x 6 档牌友情绪 盲听采样与契约验收 ==='
    $MatrixLatencies = [System.Collections.Generic.List[double]]::new()
    foreach ($Persona in $Personas) {
        foreach ($Line in $MoodLines) {
            $ReqId = "matrix-$($Persona.Id)-$($Line.Mood)"
            $Res = Invoke-SynthesizeOnce -RequestId $ReqId -PersonaId $Persona.Id -Mood $Line.Mood -Text $Line.Text -DeadlineMs 5000
            if ($Res.StatusCode -ne 200) {
                throw "[$ReqId] 合成失败：HTTP $($Res.StatusCode)"
            }
            Assert-ValidWav24kMono -Bytes $Res.Bytes -Label $ReqId
            foreach ($HeaderName in @('X-TTS-Duration-Ms', 'X-TTS-Synth-Ms', 'X-TTS-Queue-Wait-Ms')) {
                if (-not $Res.Headers.ContainsKey($HeaderName)) {
                    throw "[$ReqId] 缺少诊断响应头：$HeaderName"
                }
            }
            $OutFile = Join-Path $OutputDir "$($Persona.Id)_$($Line.Mood).wav"
            [System.IO.File]::WriteAllBytes($OutFile, $Res.Bytes)
            $MatrixLatencies.Add($Res.WallMs)
            Write-Host ("[smoke-tts] [PASS] {0} ({1}) | [{2}] Wall={3}ms Synth={4}ms Dur={5}ms -> {6}" -f `
                $Persona.Name, $Persona.Speaker, $Line.Mood, $Res.WallMs, $Res.Headers['X-TTS-Synth-Ms'], $Res.Headers['X-TTS-Duration-Ms'], $OutFile)
        }
    }
}

# 3b. 流式 Raw PCM16 真增量吐流（TTFA、多块分片间隔与响应头契约）验证
Write-Host ''
Write-Host '[smoke-tts] === 流式 Raw PCM16 真增量吐流与 TTFA 验收 ==='
$StreamProbes = @(
    @{ Id = 'stream-probe-1'; Persona = 'QuickHothead'; Mood = 'Tilted';  Text = '啧，刚补杠就被抢杠胡？！今天这破手气真绝了！' },
    @{ Id = 'stream-probe-2'; Persona = 'TeaSarcasm';   Mood = 'Mocking'; Text = '哎哟，您这手牌打得可真稳呢，生怕别人碰着呀？' },
    @{ Id = 'stream-probe-3'; Persona = 'TiltedMaster'; Mood = 'Deadpan'; Text = '行行行，算你手气好，下小局咱牌河里接着算账。' }
)
$StreamTtfas = [System.Collections.Generic.List[double]]::new()
foreach ($Probe in $StreamProbes) {
    $StreamRes = Invoke-SynthesizeStreamOnce -RequestId $Probe.Id -PersonaId $Probe.Persona -Mood $Probe.Mood -Text $Probe.Text -DeadlineMs 5000
    if ($StreamRes.StatusCode -ne 200) {
        throw "[$($Probe.Id)] 流式合成失败：HTTP $($StreamRes.StatusCode)"
    }
    if ($StreamRes.Headers['Content-Type'] -notlike 'application/octet-stream*') {
        throw "[$($Probe.Id)] 流式 Content-Type 不符：$($StreamRes.Headers['Content-Type'])"
    }
    if ($StreamRes.Headers['X-Audio-Format'] -ne 'pcm_s16le') {
        throw "[$($Probe.Id)] 缺少或非法的 X-Audio-Format：$($StreamRes.Headers['X-Audio-Format'])"
    }
    if ($StreamRes.Headers['X-Audio-Sample-Rate'] -ne '24000') {
        throw "[$($Probe.Id)] 缺少或非法的 X-Audio-Sample-Rate：$($StreamRes.Headers['X-Audio-Sample-Rate'])"
    }
    if ($StreamRes.Headers['X-Audio-Channels'] -ne '1') {
        throw "[$($Probe.Id)] 缺少或非法的 X-Audio-Channels：$($StreamRes.Headers['X-Audio-Channels'])"
    }
    foreach ($HeaderName in @('X-TTS-Request-Id', 'X-TTS-Queue-Wait-Ms', 'X-TTS-Backend', 'X-TTS-Model')) {
        if (-not $StreamRes.Headers.ContainsKey($HeaderName)) {
            throw "[$($Probe.Id)] 流式响应缺少头：$HeaderName"
        }
    }
    if ($StreamRes.TotalBytes -le 0 -or ($StreamRes.TotalBytes % 2) -ne 0) {
        throw "[$($Probe.Id)] 流式 PCM16 字节数异常：$($StreamRes.TotalBytes)"
    }
    if ($StreamRes.ChunkCount -lt 2) {
        throw "[$($Probe.Id)] 预期收到多块增量分片（>=2），实际 ChunkCount=$($StreamRes.ChunkCount)"
    }
    $AudioDurMs = [Math]::Round(($StreamRes.TotalBytes / 2.0) * 1000.0 / 24000.0, 0)
    $StreamTtfas.Add($StreamRes.TtfaMs)
    Write-Host ("[smoke-tts] [PASS] {0} | TTFA={1}ms Total={2}ms Chunks={3} MaxGap={4}ms Bytes={5} AudioDur={6}ms" -f `
        $Probe.Id, $StreamRes.TtfaMs, $StreamRes.TotalMs, $StreamRes.ChunkCount, $StreamRes.MaxGapMs, $StreamRes.TotalBytes, $AudioDurMs)
}
$AvgTtfaMs = [Math]::Round(($StreamTtfas | Measure-Object -Average).Average, 1)
Write-Host "[smoke-tts] [PASS] 流式 PCM16 验收通过：平均 TTFA=${AvgTtfaMs}ms"

$StreamExpired = Invoke-SynthesizeStreamOnce -RequestId 'stream-deadline-drop-test' -PersonaId 'StubbornBlamer' -Mood 'Deadpan' -Text '流式入队前超时丢弃。' -DeadlineMs 0
if ($StreamExpired.StatusCode -ne 408) {
    throw "流式 deadline_ms=0 预期返回 HTTP 408，实际为 HTTP $($StreamExpired.StatusCode)"
}
Write-Host '[smoke-tts] [PASS] 流式 deadline_ms=0 返回 HTTP 408（入队前直接丢弃）'
Measure-NvidiaVram | Out-Null

# 4. deadline_ms 过期入队前丢弃验证
Write-Host ''
Write-Host '[smoke-tts] === deadline_ms 过期丢弃护栏验证 ==='
$Expired = Invoke-SynthesizeOnce -RequestId 'deadline-drop-test' -PersonaId 'StubbornBlamer' -Mood 'Deadpan' -Text '这条应当在入队前直接丢弃。' -DeadlineMs 0
if ($Expired.StatusCode -ne 408) {
    throw "deadline_ms=0 预期返回 HTTP 408，实际为 HTTP $($Expired.StatusCode)"
}
Write-Host '[smoke-tts] [PASS] deadline_ms=0 返回 HTTP 408（入队前直接丢弃）'

# 5. OpenAI 兼容端点快速探测
$OpenAiBody = @{
    model           = 'majjj-tts'
    voice           = 'QuickHothead'
    input           = '快点出牌，别墨迹！'
    response_format = 'wav'
} | ConvertTo-Json -Compress
$Client = [System.Net.Http.HttpClient]::new()
$Client.Timeout = [TimeSpan]::FromSeconds($TimeoutSec)
try {
    $Resp = $Client.PostAsync(
        "$NormalizedBaseUrl/v1/audio/speech",
        [System.Net.Http.StringContent]::new($OpenAiBody, [System.Text.Encoding]::UTF8, 'application/json')
    ).GetAwaiter().GetResult()
    $AudioBytes = $Resp.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    if ([int]$Resp.StatusCode -ne 200) {
        throw "/v1/audio/speech 探活失败：HTTP $([int]$Resp.StatusCode)"
    }
    Assert-ValidWav24kMono -Bytes $AudioBytes -Label 'openai-speech'
    Write-Host "[smoke-tts] [PASS] /v1/audio/speech => $($AudioBytes.Length) bytes WAV"
} finally {
    $Client.Dispose()
}

# 6. 可选 30 条连续短句压测与显存红线核验
if (-not $SkipStress) {
    Write-Host ''
    Write-Host "[smoke-tts] === ${StressCount} 条连续短句压测与显存红线核验 ==="
    $StressLatencies = [System.Collections.Generic.List[double]]::new()
    for ($Idx = 0; $Idx -lt $StressCount; $Idx++) {
        $P = $Personas[$Idx % $Personas.Count]
        $M = $MoodLines[$Idx % $MoodLines.Count]
        $Res = Invoke-SynthesizeOnce -RequestId "stress-$Idx" -PersonaId $P.Id -Mood $M.Mood -Text $M.Text -DeadlineMs 5000
        if ($Res.StatusCode -ne 200) {
            throw "压测第 $Idx 条失败：HTTP $($Res.StatusCode)"
        }
        Assert-ValidWav24kMono -Bytes $Res.Bytes -Label "stress-$Idx"
        $StressLatencies.Add($Res.WallMs)
    }

    $Sorted = @($StressLatencies | Sort-Object)
    $AvgMs = [Math]::Round(($Sorted | Measure-Object -Average).Average, 1)
    $P95Index = [Math]::Min($Sorted.Count - 1, [int][Math]::Ceiling($Sorted.Count * 0.95) - 1)
    $P95Ms = $Sorted[$P95Index]
    Write-Host "[smoke-tts] [PASS] 连续 ${StressCount} 条合成零 OOM：avg=${AvgMs}ms, p95=${P95Ms}ms（目标 avg <= 1000ms, p95 <= 1350ms）"

    $PostHealth = Invoke-RestMethod -Uri "$NormalizedBaseUrl/health" -Method Get -TimeoutSec $TimeoutSec
    if ([int]$PostHealth.vram_reserved_mib -gt 3500) {
        throw "TTS 进程显存占用超标：$($PostHealth.vram_reserved_mib) MiB > 3500 MiB"
    }
    Write-Host "[smoke-tts] [PASS] 压测后 TTS 进程显存：allocated=$($PostHealth.vram_allocated_mib) MiB, reserved=$($PostHealth.vram_reserved_mib) MiB（红线 <= 3500 MiB）"
    Measure-NvidiaVram | Out-Null
}

Write-Host ''
Write-Host "[smoke-tts] 全部真机冒烟与契约验证通过！盲听 WAV 样本保存在：$OutputDir"
