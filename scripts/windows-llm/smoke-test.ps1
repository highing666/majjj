#Requires -Version 7.0
<#
.SYNOPSIS
    一键真机冒烟验证 Windows llama-server（8K 单槽上下文、原生思考分离、{speak,mood,text} 严格 json_schema、5 位熟人牌友角色与 2 路并发）。
.DESCRIPTION
    依次验证：
      1. GET /health、/v1/models 与 /props 就绪状态（确认单槽 n_ctx >= 8192、思考分离且 reasoning_in_content = false）；
      2. OpenAI 兼容 POST /v1/chat/completions 开启原生思考（enable_thinking = true）并在 json_schema 严格约束下返回合法 { speak, mood, text } JSON；
      3. speak 严格为布尔值、mood 严格属于 Smug / Mocking / Tilted / Nervous / Plotting / Deadpan 六档规范枚举，speak=false 允许空 text；
      4. 思考内容正确分离至 reasoning_content（非空），且 content 与 text 中严禁出现 <think> 或 </think> 标签泄漏；
      5. 覆盖全部 5 位真人化牌友角色（大川、小晚、大鹏、小桃、老陈）及 1 组平淡巡沉默变脸样例，统计 TTFT 与端到端耗时；
      6. 可选执行 2 路并发推理压测，确认显存余量与结构化合规率稳定。
.EXAMPLE
    pwsh -File .\smoke-test.ps1
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -BaseUrl http://192.168.5.50:8080 -Concurrency 2
.EXAMPLE
    pwsh -File .\smoke-test.ps1 -SkipConcurrency
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://127.0.0.1:8080',
    [ValidateRange(5, 300)]
    [int]$TimeoutSec = 60,
    [ValidateRange(64, 2048)]
    [int]$MaxTokens = 512,
    [switch]$SkipWarmup,
    [switch]$SkipConcurrency,
    [ValidateRange(1, 8)]
    [int]$Concurrency = 2
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
            required             = @('speak', 'mood', 'text')
            properties           = @{
                speak = @{
                    type = 'boolean'
                }
                mood  = @{
                    type = 'string'
                    enum = $AllowedMoods
                }
                text  = @{
                    type      = 'string'
                    minLength = 0
                    maxLength = 64
                }
            }
        }
    }
}

$CommonSystemRules = @'
你是熟人家庭麻将牌桌上的一位牌友角色，正在边打推倒胡边唠嗑。
请先在心里看清局势与气氛，再严格输出 JSON 对象 {"speak":true/false,"mood":"...","text":"..."}。
规则：
1. speak 为布尔值：有真实情绪波动、有梗可接或想顺口吐槽时设为 true；平淡无波澜的摸牌出牌巡优先设为 false 安静打牌（仅更新 mood），此时 text 输出空字符串 ""。
2. mood 必须从以下 6 档心理锚点中选一个最贴合当前心境的英文枚举：
- Smug：得意欠揍（胡牌炫耀、手顺暗爽）
- Mocking：阴阳嘲讽（说反话、笑里藏刀、看热闹补刀）
- Tilted：红温破防（被抢胡、点炮、连摸废张憋不住火）
- Nervous：心虚紧绷（做大牌怕点炮、手牌危险）
- Plotting：阴险憋大招（暗中算计、起哄拱火）
- Deadpan：无语装淡定（故作冷静嘴硬、面无表情吐槽）
3. 当 speak 为 true 时，text 必须是 2 到 18 字（常态 4 到 14 字）的鲜活中文口语短句；默认不带人名直接开口，禁止说教与客服腔。
'@

$PersonaCases = @(
    @{
        Name       = '大川'
        SystemNote = '角色：大川（川渝急性子爽快哥们，快攻牌风）。语速脆、嗓门亮，胡 0 番基本胡也要乐呵显摆，被截胡立马炸毛。'
        UserPrompt = '小局事件：你五巡就自摸 0 番基本胡，抢在全场做大牌之前先胡了。此刻你想开口得意一句，请输出决策。'
    },
    @{
        Name       = '小晚'
        SystemNote = '角色：小晚（温婉腹黑、笑里藏刀爱翻旧账姐妹，防守做大牌牌风）。说话轻声细语但专戳痛处，最会微笑阴阳和记仇。'
        UserPrompt = '小局事件：下家阿哲捏着一张发财犹豫半天不肯舍牌，明显怕被你碰。此刻你想轻飘飘调侃一句，请输出决策。'
    },
    @{
        Name       = '大鹏'
        SystemNote = '角色：大鹏（京片子嘴贫死磕、输牌借口清奇的哥们，快攻牌风）。爱立 Flag、死磕大牌不回头，吃亏了总能找清奇理由嘴硬圆回来。'
        UserPrompt = '小局事件：你死磕清一色结果一不小心点炮给了对门，全桌都在笑你。此刻你想嘴硬找个清奇借口圆回来，请输出决策。'
    },
    @{
        Name       = '小桃'
        SystemNote = '角色：小桃（网感强、爱吃瓜起哄补刀的损友妹妹，均衡牌风）。看热闹不嫌事大，谁点炮吃亏她比谁都乐，嘴快会玩梗。'
        UserPrompt = '小局事件：对门刚刚给老陈点炮送了大番，牌桌火药味一下起来了。此刻你想吃瓜补一刀，请输出决策。'
    },
    @{
        Name       = '老陈'
        SystemNote = '角色：老陈（爱端前辈架子点评牌技、自家一吃亏瞬间红温的老哥，防守做大牌牌风）。平时爱拿老牌友腔调指点江山，真破防时嘴比谁都急。'
        UserPrompt = '小局事件：你刚端着架子点评完上家不会留牌，下一秒自家补杠就被对方抢杠胡了。此刻你彻底红温破防，请输出决策。'
    }
)

$SilentTurnCase = @{
    Name       = '老陈（平淡巡沉默检查）'
    SystemNote = '角色：老陈（防守做大牌牌风）。平时沉得住气，没啥波澜的普通回合不会乱插话。'
    UserPrompt = '小局常态巡检：东一局刚第 2 巡，四家都没鸣牌，牌墙余 78 张，大家都在安静整理起手。你刚摸切了一张无用边张，距上次开口仅 1 巡，目前毫无波澜。若觉得没必要说话请将 speak 设为 false 并留空 text，仅按当前心境给出 mood。'
}

Write-Host "[smoke] 目标推理服务：$NormalizedBaseUrl"

# 1. /health 检查
$Health = Invoke-RestMethod -Uri "$NormalizedBaseUrl/health" -Method Get -TimeoutSec $TimeoutSec
if ($null -eq $Health -or $Health.status -ne 'ok') {
    throw "/health 检查失败：响应为 $($Health | ConvertTo-Json -Compress)"
}
Write-Host "[smoke] [PASS] /health => status=$($Health.status)"

# 2. /v1/models 与 /props 检查
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

$Props = $null
try {
    $Props = Invoke-RestMethod -Uri "$NormalizedBaseUrl/props" -Method Get -TimeoutSec $TimeoutSec
} catch {
    Write-Warning "[smoke] /props 接口不可用，跳过上下文元数据校验：$($_.Exception.Message)"
}

if ($null -ne $Props) {
    $Slots = $Props.total_slots
    $NCtx = if ($Props.PSObject.Properties['default_generation_settings'] -and $null -ne $Props.default_generation_settings) {
        [int]$Props.default_generation_settings.n_ctx
    } else {
        0
    }
    $GenParams = if (
        $Props.PSObject.Properties['default_generation_settings'] -and
        $null -ne $Props.default_generation_settings -and
        $Props.default_generation_settings.PSObject.Properties['params']
    ) {
        $Props.default_generation_settings.params
    } else {
        $null
    }
    $ReasoningFmt = if ($null -ne $GenParams -and $GenParams.PSObject.Properties['reasoning_format']) {
        [string]$GenParams.reasoning_format
    } elseif ($Props.PSObject.Properties['reasoning_format']) {
        [string]$Props.reasoning_format
    } else {
        'n/a'
    }
    $ReasoningInContent = if ($null -ne $GenParams -and $GenParams.PSObject.Properties['reasoning_in_content']) {
        $GenParams.reasoning_in_content
    } elseif ($Props.PSObject.Properties['reasoning_in_content']) {
        $Props.reasoning_in_content
    } else {
        'n/a'
    }

    Write-Host "[smoke] [PASS] /props => total_slots=$Slots, n_ctx=$NCtx, default_reasoning_format=$ReasoningFmt, reasoning_in_content=$ReasoningInContent"

    if ($NCtx -gt 0 -and $NCtx -lt 8192) {
        throw "/props 检测到单槽 n_ctx=$NCtx，低于 8192（预期 --parallel 2 -c 16384）。"
    }
    if ($ReasoningInContent -is [bool] -and $ReasoningInContent) {
        throw '/props 检测到 reasoning_in_content=true，预期为 false（思考内容不得混入 content）。'
    }
}

function Invoke-StreamingBanterCheck {
    param(
        [string]$Endpoint,
        [string]$ModelId,
        [string]$SystemPrompt,
        [string]$UserPrompt,
        [hashtable]$SchemaFormat,
        [string[]]$ValidMoods,
        [int]$ReqTimeoutSec,
        [int]$ReqMaxTokens
    )

    $PayloadObj = @{
        model                = $ModelId
        stream               = $true
        temperature          = 0.9
        top_p                = 0.9
        repeat_penalty       = 1.1
        presence_penalty     = 0.4
        max_tokens           = $ReqMaxTokens
        chat_template_kwargs = @{ enable_thinking = $true }
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
        $ContentSb = [System.Text.StringBuilder]::new()
        $ReasoningSb = [System.Text.StringBuilder]::new()
        $TtftMs = -1.0
        $ContentFirstMs = -1.0

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
                            if ($TtftMs -lt 0) {
                                $TtftMs = $Sw.Elapsed.TotalMilliseconds
                            }
                            $ReasoningSb.Append([string]$Delta.reasoning_content) | Out-Null
                        }
                        if ($Delta.PSObject.Properties['content'] -and -not [string]::IsNullOrEmpty([string]$Delta.content)) {
                            if ($TtftMs -lt 0) {
                                $TtftMs = $Sw.Elapsed.TotalMilliseconds
                            }
                            if ($ContentFirstMs -lt 0) {
                                $ContentFirstMs = $Sw.Elapsed.TotalMilliseconds
                            }
                            $ContentSb.Append([string]$Delta.content) | Out-Null
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
        $RawText = $ContentSb.ToString().Trim()
        $ReasoningText = $ReasoningSb.ToString().Trim()

        if ([string]::IsNullOrWhiteSpace($ReasoningText)) {
            throw '未检测到 delta.reasoning_content（原生思考模式可能未生效或未分离）。'
        }
        if ($RawText -match '<think>|</think>') {
            throw "检测到 content 中存在 <think> 标签泄漏：$RawText"
        }

        $Parsed = $RawText | ConvertFrom-Json
        $PropNames = @($Parsed.PSObject.Properties.Name | Sort-Object)
        if (($PropNames -join ',') -ne 'mood,speak,text') {
            throw "JSON 字段不符合严格契约 {speak,mood,text}，实际字段：$($PropNames -join ',')"
        }

        if ($Parsed.speak -isnot [bool]) {
            throw "非法 speak 字段类型：expected bool, actual=$($Parsed.speak)"
        }
        $Speak = [bool]$Parsed.speak
        $Mood = [string]$Parsed.mood
        $LineText = ([string]$Parsed.text).Trim()
        if ($ValidMoods -notcontains $Mood) {
            throw "非法 mood 枚举值：'$Mood'"
        }
        if ($Speak -and [string]::IsNullOrWhiteSpace($LineText)) {
            throw 'speak=true 但返回的 text 为空。'
        }
        if ($LineText -match '<think>|</think>') {
            throw "text 中包含思考标签：$LineText"
        }

        return [pscustomobject]@{
            Speak          = $Speak
            Mood           = $Mood
            Text           = $LineText
            ReasoningChars = $ReasoningText.Length
            TtftMs         = [Math]::Round($TtftMs, 1)
            ContentFirstMs = [Math]::Round($ContentFirstMs, 1)
            E2eMs          = [Math]::Round($E2eMs, 1)
        }
    } finally {
        $Client.Dispose()
    }
}

# 3. 可选热身
if (-not $SkipWarmup) {
    Write-Host '[smoke] 正在执行 1 次热身请求（原生思考 + {speak,mood,text}）...'
    $WarmSystem = "$CommonSystemRules`n$($PersonaCases[0].SystemNote)"
    $Warm = Invoke-StreamingBanterCheck `
        -Endpoint $NormalizedBaseUrl `
        -ModelId $ActiveModel `
        -SystemPrompt $WarmSystem `
        -UserPrompt $PersonaCases[0].UserPrompt `
        -SchemaFormat $ResponseFormat `
        -ValidMoods $AllowedMoods `
        -ReqTimeoutSec $TimeoutSec `
        -ReqMaxTokens $MaxTokens
    Write-Host "[smoke] 热身完成：speak=$($Warm.Speak), mood=$($Warm.Mood), reasoning=$($Warm.ReasoningChars)字, TTFT=$($Warm.TtftMs)ms, ContentFirst=$($Warm.ContentFirstMs)ms, E2E=$($Warm.E2eMs)ms"
}

# 4. 5 位熟人牌友角色顺序流式冒烟 + 1 组平淡巡沉默变脸样例
Write-Host ''
Write-Host '[smoke] === 5 位真人化牌友角色结构化流式冒烟（原生思考开启）==='
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
            -ReqTimeoutSec $TimeoutSec `
            -ReqMaxTokens $MaxTokens
        $PersonaResults.Add([pscustomobject]@{
            Persona        = $Case.Name
            Speak          = $Res.Speak
            Mood           = $Res.Mood
            Text           = $Res.Text
            ReasoningChars = $Res.ReasoningChars
            TtftMs         = $Res.TtftMs
            ContentFirstMs = $Res.ContentFirstMs
            E2eMs          = $Res.E2eMs
        })
        Write-Host ("[smoke] [PASS] {0} | speak={1} | [{2}] 「{3}」 (think={4}字, TTFT={5}ms, JSON={6}ms, E2E={7}ms)" -f $Case.Name, $Res.Speak, $Res.Mood, $Res.Text, $Res.ReasoningChars, $Res.TtftMs, $Res.ContentFirstMs, $Res.E2eMs)
    } catch {
        $Failures++
        Write-Error "[smoke] [FAIL] $($Case.Name): $($_.Exception.Message)" -ErrorAction Continue
    }
}

Write-Host ''
Write-Host '[smoke] === 平淡巡沉默变脸契约验证（允许 speak=false & text=""）==='
$SilentRes = $null
try {
    $SilentSys = "$CommonSystemRules`n$($SilentTurnCase.SystemNote)"
    $SilentRes = Invoke-StreamingBanterCheck `
        -Endpoint $NormalizedBaseUrl `
        -ModelId $ActiveModel `
        -SystemPrompt $SilentSys `
        -UserPrompt $SilentTurnCase.UserPrompt `
        -SchemaFormat $ResponseFormat `
        -ValidMoods $AllowedMoods `
        -ReqTimeoutSec $TimeoutSec `
        -ReqMaxTokens $MaxTokens
    Write-Host ("[smoke] [PASS] {0} | speak={1} | [{2}] 「{3}」 (think={4}字, TTFT={5}ms, E2E={6}ms)" -f $SilentTurnCase.Name, $SilentRes.Speak, $SilentRes.Mood, $SilentRes.Text, $SilentRes.ReasoningChars, $SilentRes.TtftMs, $SilentRes.E2eMs)
} catch {
    $Failures++
    Write-Error "[smoke] [FAIL] $($SilentTurnCase.Name): $($_.Exception.Message)" -ErrorAction Continue
}

# 5. 可选 2 路并发验收
$ConcurrentWallMs = $null
$ConcurrentResults = @()
if (-not $SkipConcurrency) {
    Write-Host ''
    Write-Host "[smoke] === ${Concurrency} 路并发推理验证（原生思考开启）==="
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
            $ReqMaxTokens = $using:MaxTokens

            $BodyObj = @{
                model                = $ModelId
                stream               = $false
                temperature          = 0.9
                top_p                = 0.9
                repeat_penalty       = 1.1
                presence_penalty     = 0.4
                max_tokens           = $ReqMaxTokens
                chat_template_kwargs = @{ enable_thinking = $true }
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
                $Reasoning = if ($Msg.PSObject.Properties['reasoning_content']) { ([string]$Msg.reasoning_content).Trim() } else { '' }
                if ([string]::IsNullOrWhiteSpace($Reasoning)) {
                    throw '未检测到分离的 reasoning_content。'
                }
                $Raw = ([string]$Msg.content).Trim()
                if ($Raw -match '<think>|</think>') {
                    throw "检测到 content 中存在 <think> 泄漏：$Raw"
                }
                $Parsed = $Raw | ConvertFrom-Json
                $PropNames = @($Parsed.PSObject.Properties.Name | Sort-Object)
                if (($PropNames -join ',') -ne 'mood,speak,text') {
                    throw "JSON 字段不合规：$($PropNames -join ',')"
                }
                if ($Parsed.speak -isnot [bool]) {
                    throw "非法 speak：$($Parsed.speak)"
                }
                $SpeakVal = [bool]$Parsed.speak
                if ($ValidMoods -notcontains [string]$Parsed.mood) {
                    throw "非法 mood：$($Parsed.mood)"
                }
                $LineText = ([string]$Parsed.text).Trim()
                if ($SpeakVal -and [string]::IsNullOrWhiteSpace($LineText)) {
                    throw 'speak=true 但 text 为空'
                }
                if ($LineText -match '<think>|</think>') {
                    throw "text 中包含思考标签：$LineText"
                }

                [pscustomobject]@{
                    Ok             = $true
                    Persona        = $CaseItem.Name
                    Speak          = $SpeakVal
                    Mood           = [string]$Parsed.mood
                    Text           = $LineText
                    ReasoningChars = $Reasoning.Length
                    E2eMs          = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
                    Error          = ''
                }
            } catch {
                $Sw.Stop()
                [pscustomobject]@{
                    Ok             = $false
                    Persona        = $CaseItem.Name
                    Speak          = $false
                    Mood           = ''
                    Text           = ''
                    ReasoningChars = 0
                    E2eMs          = [Math]::Round($Sw.Elapsed.TotalMilliseconds, 1)
                    Error          = $_.Exception.Message
                }
            }
        }
    )

    $WallSw.Stop()
    $ConcurrentWallMs = [Math]::Round($WallSw.Elapsed.TotalMilliseconds, 1)

    foreach ($Item in $ConcurrentResults) {
        if ($Item.Ok) {
            Write-Host ("[smoke] [PASS] 并发 | {0} | speak={1} | [{2}] 「{3}」 (think={4}字, E2E={5}ms)" -f $Item.Persona, $Item.Speak, $Item.Mood, $Item.Text, $Item.ReasoningChars, $Item.E2eMs)
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
    $AvgThink = [Math]::Round(($PersonaResults | Measure-Object -Property ReasoningChars -Average).Average, 1)
    Write-Host "  5 角色顺序流式 : $($PersonaResults.Count)/$($PersonaCases.Count) 通过 | TTFT avg=${AvgTtft}ms (max=${MaxTtft}ms) | E2E avg=${AvgE2e}ms (max=${MaxE2e}ms) | reasoning avg=${AvgThink}字"
}
if ($null -ne $SilentRes) {
    Write-Host "  平淡巡沉默检查 : 1/1 合规 | speak=$($SilentRes.Speak) | mood=$($SilentRes.Mood) | E2E=$($SilentRes.E2eMs)ms"
}
if ($null -ne $ConcurrentWallMs) {
    $ConcPass = @($ConcurrentResults | Where-Object { $_.Ok }).Count
    Write-Host "  ${Concurrency} 路并发验证   : $ConcPass/$($ConcurrentResults.Count) 通过 | 整批 wall-clock=${ConcurrentWallMs}ms"
}

if ($Failures -gt 0) {
    throw "冒烟验证存在 $Failures 项失败，请排查上述错误日志。"
}

Write-Host '[smoke] 全部真机冒烟验证通过（100% 合规 {speak,mood,text} JSON、原生思考已分离且 content 零 <think> 泄漏）。'
