#Requires -Version 7.0
<#
.SYNOPSIS
    一键真机冒烟验证 Windows llama-server（健康检查、6 档情绪 json_schema、零思考链泄漏、5 位牌友角色与 3 路并发）。
.DESCRIPTION
    依次验证：
      1. GET /health 与 /props、/v1/models 就绪状态；
      2. OpenAI 兼容 POST /v1/chat/completions 在 json_schema 严格约束下返回合法 { mood, text } JSON；
      3. mood 严格属于 Smug / Mocking / Tilted / Nervous / Plotting / Deadpan 六档规范枚举；
      4. 严禁出现 <think> 标签或 reasoning_content 思考链泄漏；
      5. 覆盖全部 5 位内置毒舌牌友角色的中文台词生成，统计 TTFT（首字延迟）与整句端到端耗时；
      6. 可选执行 3 路并发推理压测，确认显存与吞吐稳定。
.EXAMPLE
    pwsh -File .\smoke-test.ps1
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -BaseUrl http://192.168.5.50:8080 -Concurrency 3
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -SkipConcurrency
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://127.0.0.1:8080',
    [ValidateRange(5, 300)]
    [int]$TimeoutSec = 60,
    [switch]$SkipWarmup,
    [switch]$SkipConcurrency,
    [ValidateRange(1, 8)]
    [int]$Concurrency = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$NormalizedBaseUrl = $BaseUrl.TrimEnd('/')
$AllowedMoods = @('Smug', 'Mocking', 'Tilted', 'Nervous', 'Plotting', 'Deadpan')

$ResponseFormat = @{
    type        = 'json_schema'
    json_schema = @{
        name   = 'table_banter'
        strict = $true
        schema = @{
            type                 = 'object'
            additionalProperties = $false
            required             = @('mood', 'text')
            properties           = @{
                mood = @{
                    type = 'string'
                    enum = $AllowedMoods
                }
                text = @{
                    type      = 'string'
                    minLength = 2
                    maxLength = 60
                }
            }
        }
    }
}

$CommonSystemRules = @'
你是家庭麻将牌桌上的毒舌牌友，正在边打牌边互怼吐槽。
你必须严格输出 JSON 对象 {"mood":"...","text":"..."}，禁止输出任何思考过程或额外文字。
mood 必须从以下 6 档心理锚点中选一个最贴合当前心境的英文枚举：
- Smug：得意欠揍（胡牌炫耀、占便宜沾沾自喜）
- Mocking：阴阳嘲讽（说反话、笑里藏刀、看别人出糗）
- Tilted：红温破防（被抢胡、点炮、被针对时急眼甩锅）
- Nervous：心虚紧绷（做大牌怕点炮、手牌危险）
- Plotting：阴险憋大招（暗中算计、拱火看热闹）
- Deadpan：无语装淡定（故作冷静嘴硬、面无表情吐槽）
text 必须是 10 到 26 字的鲜活中文口语短句，符合人设嗓音，禁止说教与长篇大论。
'@

$PersonaCases = @(
    @{
        Name       = '急眼快攻哥'
        SystemNote = '人设：急眼快攻哥（快攻牌风）。嘴速手速双快，胡 0 番基本胡也要狂吹，被抢碰立马急眼。'
        UserPrompt = '小局事件：你五巡就自摸 0 番基本胡，快过全场还在做大牌的对手。请吐一句得意台词。'
    },
    @{
        Name       = '阴阳茶艺师'
        SystemNote = '人设：阴阳茶艺师（防守做大牌牌风）。满口反话笑里藏刀，不带脏字但最爱阴阳怪气和翻旧账。'
        UserPrompt = '小局事件：下家小王捏着一张发财死活不肯舍牌，生怕被你碰。请阴阳怪气调侃一句。'
    },
    @{
        Name       = '嘴硬死磕王'
        SystemNote = '人设：嘴硬死磕王（快攻牌风）。死磕大牌不听劝，点炮永远甩锅发牌员和上家，死不认错。'
        UserPrompt = '小局事件：你死磕清一色结果一不小心点炮给了对门，还硬要甩锅。请嘴硬吐槽一句。'
    },
    @{
        Name       = '拱火乐子人'
        SystemNote = '人设：拱火乐子人（均衡牌风）。专业煽风点火挑拨离间，看别人点炮比自家自摸还开心，最爱补刀。'
        UserPrompt = '小局事件：对门刚刚给红温老雀神点炮送了大番，全桌气氛火药味拉满。请煽风点火补一刀。'
    },
    @{
        Name       = '红温老雀神'
        SystemNote = '人设：红温老雀神（防守做大牌牌风）。自封牌桌导师爱摆谱点评他家操作，自家一输当场破防。'
        UserPrompt = '小局事件：你刚教训完新手不懂留牌，下一秒自己补杠就被新手抢杠胡了。请当场破防吐一句。'
    }
)

Write-Host "[smoke] 目标推理服务：$NormalizedBaseUrl"

# 1. /health 检查
$Health = Invoke-RestMethod -Uri "$NormalizedBaseUrl/health" -Method Get -TimeoutSec $TimeoutSec
if ($null -eq $Health -or $Health.status -ne 'ok') {
    throw "/health 检查失败：响应为 $($Health | ConvertTo-Json -Compress)"
}
Write-Host "[smoke] [PASS] /health => status=$($Health.status)"

# 2. /props 与 /v1/models 检查
$ModelsResp = Invoke-RestMethod -Uri "$NormalizedBaseUrl/v1/models" -Method Get -TimeoutSec $TimeoutSec
$ModelIds = @()
if ($null -ne $ModelsResp -and $ModelsResp.PSObject.Properties['data']) {
    $ModelIds = @($ModelsResp.data | ForEach-Object { $_.id })
}
if ($ModelIds.Count -eq 0) {
    throw '/v1/models 未返回任何已加载模型。'
}
$ActiveModel = $ModelIds[0]
Write-Host "[smoke] [PASS] /v1/models => $ActiveModel"

try {
    $Props = Invoke-RestMethod -Uri "$NormalizedBaseUrl/props" -Method Get -TimeoutSec $TimeoutSec
    $Slots = $Props.total_slots
    $NCtx = if ($Props.PSObject.Properties['default_generation_settings']) {
        $Props.default_generation_settings.n_ctx
    } else {
        'unknown'
    }
    $ReasoningFmt = if ($Props.PSObject.Properties['reasoning_format']) { $Props.reasoning_format } else { 'n/a' }
    $ReasoningInContent = if ($Props.PSObject.Properties['reasoning_in_content']) { $Props.reasoning_in_content } else { 'n/a' }
    Write-Host "[smoke] [PASS] /props => total_slots=$Slots, n_ctx=$NCtx, reasoning_format=$ReasoningFmt, reasoning_in_content=$ReasoningInContent"
    if ($ReasoningFmt -ne 'n/a' -and $ReasoningFmt -ne 'none') {
        throw "/props 检测到 reasoning_format='$ReasoningFmt'，预期为 'none'（思考链未彻底关闭）。"
    }
    if ($ReasoningInContent -is [bool] -and $ReasoningInContent) {
        throw '/props 检测到 reasoning_in_content=true，预期为 false。'
    }
} catch {
    Write-Warning "[smoke] /props 检查出现告警：$($_.Exception.Message)"
}

function Invoke-StreamingBanterCheck {
    param(
        [string]$Endpoint,
        [string]$ModelId,
        [string]$SystemPrompt,
        [string]$UserPrompt,
        [hashtable]$SchemaFormat,
        [string[]]$ValidMoods,
        [int]$ReqTimeoutSec
    )

    $PayloadObj = @{
        model                = $ModelId
        stream               = $true
        temperature          = 0.9
        top_p                = 0.9
        repeat_penalty       = 1.1
        presence_penalty     = 0.4
        max_tokens           = 96
        chat_template_kwargs = @{ enable_thinking = $false }
        response_format      = $SchemaFormat
        messages             = @(
            @{ role = 'system'; content = $SystemPrompt },
            @{ role = 'user'; content = $UserPrompt }
        )
    }
    $JsonBody = $PayloadObj | ConvertTo-Json -Depth 12 -Compress

    $Client = [System.Net.Http.HttpClient]::new()
    $Client.Timeout = [TimeSpan]::FromSeconds($ReqTimeoutSec)
    try {
        $Content = [System.Net.Http.StringContent]::new($JsonBody, [System.Text.Encoding]::UTF8, 'application/json')
        $Request = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Post, "$Endpoint/v1/chat/completions")
        $Request.Content = $Content

        $Sw = [System.Diagnostics.Stopwatch]::StartNew()
        $Resp = $Client.Send($Request, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead)
        if (-not $Resp.IsSuccessStatusCode) {
            $ErrBody = $Resp.Content.ReadAsStringAsync().GetAwaiter().GetResult()
            throw "HTTP $([int]$Resp.StatusCode): $ErrBody"
        }

        $Stream = $Resp.Content.ReadAsStream()
        $Reader = [System.IO.StreamReader]::new($Stream, [System.Text.Encoding]::UTF8)
        $Sb = [System.Text.StringBuilder]::new()
        $TtftMs = -1.0
        $ReasoningLeak = $false

        try {
            while (-not $Reader.EndOfStream) {
                $Line = $Reader.ReadLine()
                if ([string]::IsNullOrWhiteSpace($Line)) { continue }
                if (-not $Line.StartsWith('data:')) { continue }
                $Data = $Line.Substring(5).Trim()
                if ($Data -eq '[DONE]') { break }

                $Chunk = $Data | ConvertFrom-Json
                if ($null -ne $Chunk -and $Chunk.PSObject.Properties['choices'] -and $Chunk.choices.Count -gt 0) {
                    $Delta = $Chunk.choices[0].delta
                    if ($null -ne $Delta) {
                        if ($Delta.PSObject.Properties['reasoning_content'] -and -not [string]::IsNullOrEmpty([string]$Delta.reasoning_content)) {
                            $ReasoningLeak = $true
                        }
                        if ($Delta.PSObject.Properties['content'] -and -not [string]::IsNullOrEmpty([string]$Delta.content)) {
                            if ($TtftMs -lt 0) {
                                $TtftMs = $Sw.Elapsed.TotalMilliseconds
                            }
                            $Sb.Append([string]$Delta.content) | Out-Null
                        }
                    }
                }
            }
        } finally {
            $Reader.Dispose()
            $Stream.Dispose()
            $Resp.Dispose()
        }

        $Sw.Stop()
        $E2eMs = $Sw.Elapsed.TotalMilliseconds
        $RawText = $Sb.ToString().Trim()

        if ($ReasoningLeak) {
            throw '检测到 delta.reasoning_content 思考链泄漏。'
        }
        if ($RawText -match '<think>|</think>') {
            throw "检测到 <think> 标签泄漏：$RawText"
        }

        $Parsed = $RawText | ConvertFrom-Json
        $PropNames = @($Parsed.PSObject.Properties.Name | Sort-Object)
        if (($PropNames -join ',') -ne 'mood,text') {
            throw "JSON 字段不符合严格契约 {mood,text}，实际字段：$($PropNames -join ',')"
        }

        $Mood = [string]$Parsed.mood
        $LineText = ([string]$Parsed.text).Trim()
        if ($ValidMoods -notcontains $Mood) {
            throw "非法 mood 枚举值：'$Mood'"
        }
        if ([string]::IsNullOrWhiteSpace($LineText)) {
            throw '返回的 text 为空。'
        }
        if ($LineText -match '<think>|</think>') {
            throw "text 中包含思考标签：$LineText"
        }

        return [pscustomobject]@{
            Mood   = $Mood
            Text   = $LineText
            TtftMs = [Math]::Round($TtftMs, 1)
            E2eMs  = [Math]::Round($E2eMs, 1)
        }
    } finally {
        $Client.Dispose()
    }
}

# 3. 可选热身
if (-not $SkipWarmup) {
    Write-Host '[smoke] 正在执行 1 次热身请求...'
    $WarmSystem = "$CommonSystemRules`n$($PersonaCases[0].SystemNote)"
    $Warm = Invoke-StreamingBanterCheck `
        -Endpoint $NormalizedBaseUrl `
        -ModelId $ActiveModel `
        -SystemPrompt $WarmSystem `
        -UserPrompt $PersonaCases[0].UserPrompt `
        -SchemaFormat $ResponseFormat `
        -ValidMoods $AllowedMoods `
        -ReqTimeoutSec $TimeoutSec
    Write-Host "[smoke] 热身完成：TTFT=$($Warm.TtftMs)ms, E2E=$($Warm.E2eMs)ms"
}

# 4. 5 位毒舌牌友角色顺序流式冒烟
Write-Host ''
Write-Host '[smoke] === 5 位牌友角色结构化流式冒烟 ==='
$PersonaResults = [System.Collections.Generic.List[object]]::new()
$Failures = 0

foreach ($Case in $PersonaCases) {
    $SysPrompt = "$CommonSystemRules`n$($Case.SystemNote)"
    try {
        $Res = Invoke-StreamingBanterCheck `
            -Endpoint $NormalizedBaseUrl `
            -ModelId $ActiveModel `
            -SystemPrompt $SysPrompt `
            -UserPrompt $Case.UserPrompt `
            -SchemaFormat $ResponseFormat `
            -ValidMoods $AllowedMoods `
            -ReqTimeoutSec $TimeoutSec
        $PersonaResults.Add([pscustomobject]@{
            Persona = $Case.Name
            Mood    = $Res.Mood
            Text    = $Res.Text
            TtftMs  = $Res.TtftMs
            E2eMs   = $Res.E2eMs
        })
        Write-Host ("[smoke] [PASS] {0} | [{1}] 「{2}」 (TTFT={3}ms, E2E={4}ms)" -f $Case.Name, $Res.Mood, $Res.Text, $Res.TtftMs, $Res.E2eMs)
    } catch {
        $Failures++
        Write-Error "[smoke] [FAIL] $($Case.Name): $($_.Exception.Message)" -ErrorAction Continue
    }
}

# 5. 可选 3 路并发验收
$ConcurrentWallMs = $null
$ConcurrentResults = @()
if (-not $SkipConcurrency) {
    Write-Host ''
    Write-Host "[smoke] === ${Concurrency} 路并发推理验证 ==="
    $BatchCases = @($PersonaCases | Select-Object -First $Concurrency)
    $WallSw = [System.Diagnostics.Stopwatch]::StartNew()

    $ConcurrentResults = @(
        $BatchCases | ForEach-Object -ThrottleLimit $Concurrency -Parallel {
            $CaseItem = $_
            $Endpoint = $using:NormalizedBaseUrl
            $ModelId = $using:ActiveModel
            $CommonRules = $using:CommonSystemRules
            $SchemaFormat = $using:ResponseFormat
            $ValidMoods = $using:AllowedMoods
            $ReqTimeoutSec = $using:TimeoutSec

            $BodyObj = @{
                model                = $ModelId
                stream               = $false
                temperature          = 0.9
                top_p                = 0.9
                repeat_penalty       = 1.1
                presence_penalty     = 0.4
                max_tokens           = 96
                chat_template_kwargs = @{ enable_thinking = $false }
                response_format      = $SchemaFormat
                messages             = @(
                    @{ role = 'system'; content = "$CommonRules`n$($CaseItem.SystemNote)" },
                    @{ role = 'user'; content = $CaseItem.UserPrompt }
                )
            }
            $JsonBody = $BodyObj | ConvertTo-Json -Depth 12 -Compress
            $Sw = [System.Diagnostics.Stopwatch]::StartNew()
            try {
                $Resp = Invoke-RestMethod `
                    -Uri "$Endpoint/v1/chat/completions" `
                    -Method Post `
                    -ContentType 'application/json; charset=utf-8' `
                    -Body ([System.Text.Encoding]::UTF8.GetBytes($JsonBody)) `
                    -TimeoutSec $ReqTimeoutSec
                $Sw.Stop()

                $Msg = $Resp.choices[0].message
                if ($Msg.PSObject.Properties['reasoning_content'] -and -not [string]::IsNullOrEmpty([string]$Msg.reasoning_content)) {
                    throw '检测到 reasoning_content 思考链泄漏。'
                }
                $Raw = ([string]$Msg.content).Trim()
                if ($Raw -match '<think>|</think>') {
                    throw "检测到 <think> 泄漏：$Raw"
                }
                $Parsed = $Raw | ConvertFrom-Json
                if ($ValidMoods -notcontains [string]$Parsed.mood) {
                    throw "非法 mood：$($Parsed.mood)"
                }
                if ([string]::IsNullOrWhiteSpace([string]$Parsed.text)) {
                    throw 'text 为空'
                }

                [pscustomobject]@{
                    Ok      = $true
                    Persona = $CaseItem.Name
                    Mood    = [string]$Parsed.mood
                    Text    = ([string]$Parsed.text).Trim()
                    E2eMs   = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
                    Error   = ''
                }
            } catch {
                $Sw.Stop()
                [pscustomobject]@{
                    Ok      = $false
                    Persona = $CaseItem.Name
                    Mood    = ''
                    Text    = ''
                    E2eMs   = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
                    Error   = $_.Exception.Message
                }
            }
        }
    )

    $WallSw.Stop()
    $ConcurrentWallMs = [Math]::Round($WallSw.Elapsed.TotalMilliseconds, 1)

    foreach ($Item in $ConcurrentResults) {
        if ($Item.Ok) {
            Write-Host ("[smoke] [PASS] 并发 | {0} | [{1}] 「{2}」 (E2E={3}ms)" -f $Item.Persona, $Item.Mood, $Item.Text, $Item.E2eMs)
        } else {
            $Failures++
            Write-Error "[smoke] [FAIL] 并发 | $($Item.Persona): $($Item.Error)" -ErrorAction Continue
        }
    }
}

# 6. 汇总报告
Write-Host ''
Write-Host '[smoke] === 验收汇总 ==='
if ($PersonaResults.Count -gt 0) {
    $AvgTtft = [Math]::Round(($PersonaResults | Measure-Object -Property TtftMs -Average).Average, 1)
    $MaxTtft = [Math]::Round(($PersonaResults | Measure-Object -Property TtftMs -Maximum).Maximum, 1)
    $AvgE2e = [Math]::Round(($PersonaResults | Measure-Object -Property E2eMs -Average).Average, 1)
    $MaxE2e = [Math]::Round(($PersonaResults | Measure-Object -Property E2eMs -Maximum).Maximum, 1)
    Write-Host "  5 角色顺序流式 : $($PersonaResults.Count)/$($PersonaCases.Count) 通过 | TTFT avg=${AvgTtft}ms (max=${MaxTtft}ms) | E2E avg=${AvgE2e}ms (max=${MaxE2e}ms)"
}
if ($null -ne $ConcurrentWallMs) {
    $ConcPass = @($ConcurrentResults | Where-Object { $_.Ok }).Count
    Write-Host "  ${Concurrency} 路并发验证   : $ConcPass/$($ConcurrentResults.Count) 通过 | 整批 wall-clock=${ConcurrentWallMs}ms"
}

if ($Failures -gt 0) {
    throw "冒烟验证存在 $Failures 项失败，请排查上述错误日志。"
}

Write-Host '[smoke] 全部真机冒烟验证通过（100% 合规 JSON、零思考链泄漏）。'
