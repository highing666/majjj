# Windows `llama-server` 局域网绿色部署与运维指南

本文档用于指导在家庭局域网 Windows 主机（NVIDIA RTX 3060 12GB 及以上显卡）上绿色部署、常驻运行、安全暴露与真机冒烟验证 Majjj 熟人牌友推理服务。

> 所有脚本均基于 **PowerShell 7（`pwsh`）** 编写，位于仓库 [`scripts/windows-llm/`](../scripts/windows-llm/)，支持重复幂等执行，无需在系统盘安装 Python 或污染全局环境变量。

---

## 1. 目录结构（推荐置于空闲大容量分区）

默认部署根目录为 `E:\majjj-llm`（所有脚本均支持通过 `-RootDir` 自定义路径）：

```text
E:\majjj-llm\
├── bin\
│   ├── llama-server.exe               # 官方预编译 CUDA 版 llama.cpp（默认 b12760）
│   └── cudart64_*.dll / cublas64_*.dll# 对应 CUDA 13.5 运行库
├── models\
│   └── Qwen3.5-9B-Q5_K_M.gguf         # 默认权重（6,577,841,376 bytes，SHA-256 校验）
├── logs\
│   ├── llama-server.log               # 服务端运行日志
│   └── llama-server.pid               # 后台常驻进程 PID
├── tmp\                               # 分块断点续传缓存（校验合并后自动清理）
├── deploy-llama-server.ps1            # 一键绿色部署与校验
├── start-llama-server.ps1             # 幂等启动（支持 -ModelPath / -ReasoningBudget / -Restart / -Foreground）
├── stop-llama-server.ps1              # 幂等停止
├── setup-firewall.ps1                 # 幂等局域网防火墙入站规则
├── install-autostart.ps1              # 可选开机自启计划任务（SYSTEM 权限）
└── smoke-test.ps1                     # 8K 单槽 + 原生思考分离 + {speak,mood,text} + 5 位牌友角色 + 2 并发冒烟
```

---

## 2. 一键部署与日常运维

### 2.1 一键绿色部署（幂等，自动跳过已校验文件）

```powershell
pwsh -File .\scripts\windows-llm\deploy-llama-server.ps1 -RootDir E:\majjj-llm
```

- 自动下载官方 `ggml-org/llama.cpp` `b12760`（CUDA 13.5）可执行包与 `cudart` 运行库并解压至 `bin\`。
- 自动解析 HuggingFace 302 重定向后的直链 CDN，按 64 MiB 分块（HTTP `Range`）断点续传下载 `unsloth/Qwen3.5-9B-GGUF` 的 `Qwen3.5-9B-Q5_K_M.gguf`，合并后严格校验字节数（`6,577,841,376`）与 SHA-256（`dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c`）。
- 若目标目录已存在校验通过的可执行文件与模型权重，再次执行将直接跳过下载；同时自动把 6 个 `.ps1` 脚本同步至 `E:\majjj-llm\`，便于脱离仓库目录直接运维。

### 2.2 配置局域网防火墙（管理员权限，幂等）

```powershell
pwsh -File E:\majjj-llm\setup-firewall.ps1 -LanSubnet 192.168.5.0/255.255.255.0 -Port 8080
```

- 创建唯一入站规则 **`majjj-llama-server-lan-8080`**（`Inbound` / `Allow` / `TCP 8080` / `Profile: Any`），仅放行指定家庭局域网网段，拒绝 `Any`、`0.0.0.0/0`、`*` 等通配来源。
- 重复执行会先清理同名旧规则，确保始终保持唯一 1 条规则；如需移除规则，执行：

```powershell
pwsh -File E:\majjj-llm\setup-firewall.ps1 -Remove
```

### 2.3 启动、调优与停止服务（幂等，SSH 断开不掉线）

```powershell
# 后台常驻启动（默认 --parallel 2 -c 16384，原生思考预算 256 tokens；脱离 SSH/终端会话 Job）
pwsh -File E:\majjj-llm\start-llama-server.ps1

# 调整原生思考预算并重启（例如调快至 192 tokens，或设为 0 临时关闭原生思考）
pwsh -File E:\majjj-llm\start-llama-server.ps1 -ReasoningBudget 192 -Restart

# 前台调试启动（设置 $PSNativeCommandArgumentPassing = 'Standard'）
pwsh -File E:\majjj-llm\start-llama-server.ps1 -Foreground

# 重启服务
pwsh -File E:\majjj-llm\start-llama-server.ps1 -Restart

# 停止服务
pwsh -File E:\majjj-llm\stop-llama-server.ps1
```

核心启动参数（经 RTX 3060 12GB 与局域网 TTS 同卡共驻实测验证）：

```powershell
llama-server.exe `
  -m E:\majjj-llm\models\Qwen3.5-9B-Q5_K_M.gguf `
  --host 0.0.0.0 --port 8080 `
  -ngl 999 -fa on --parallel 2 -c 16384 `
  --jinja --chat-template-kwargs '{"enable_thinking": true}' `
  --reasoning-format deepseek --reasoning-budget 256 `
  --log-file E:\majjj-llm\logs\llama-server.log
```

> **为什么默认采用 `--parallel 2 -c 16384`？**
> Qwen3.5 训练上下文为 `262144`，若不限制 `-c` 会直接撑爆 12 GB 显存；而整场对局 6 块上下文与跨小局`牌桌对话流`需要更充裕的单槽容量。采用 `--parallel 2 -c 16384` 后，每个并发槽位获得 **`n_ctx = 8192` tokens**（较旧版 `4096` 翻倍），且与 `Qwen3-TTS` 同驻 RTX 3060 12GB 时整卡显存仅约 `10.1 GiB`（仍保留约 `1.9 GiB` 安全余量）。

> **`-ReasoningBudget` 原生思考预算调优建议：**
> - **`256`（默认甜点）**：兼顾思考深度、结构化合规率（`100%`）与端到端延迟（单次约 `6.3s`，2 路并发约 `7.6s`），与请求侧 `max_tokens = 512` 预留充裕余量。
> - **`192`（偏快档）**：单次耗时降至约 `5.1s`，适合更在意响应速度的场景，但较长上下文下偶发提前截断思考导致保守沉默。
> - **`384`（深思考档）**：单次耗时升至约 `9.1s`；若调高至此档位，务必同步将调用侧 `max_tokens` 放宽至 `640+`，避免思考耗尽 token 挤压 JSON 正文。
> - **`0`（应急关闭）**：`start-llama-server.ps1` 会自动切换 `{"enable_thinking": false}` 与 `--reasoning-budget 0`，用于临时回退无思考极速模式。

### 2.4 可选开机自启（管理员权限，幂等）

```powershell
# 注册 SYSTEM 权限开机自启计划任务（默认同步转发 -ParallelSlots 2 -ContextSize 16384 -ReasoningBudget 256）
pwsh -File E:\majjj-llm\install-autostart.ps1 -RootDir E:\majjj-llm

# 自定义思考预算并更新计划任务
pwsh -File E:\majjj-llm\install-autostart.ps1 -RootDir E:\majjj-llm -ReasoningBudget 256

# 卸载计划任务
pwsh -File E:\majjj-llm\install-autostart.ps1 -Uninstall
```

### 2.5 真机冒烟验证

在 Windows 本机或同一局域网内的 Mac（若安装了 `pwsh`）运行：

```powershell
pwsh -File E:\majjj-llm\smoke-test.ps1 -BaseUrl http://127.0.0.1:8080
```

脚本会依次检查：
1. `/health`、`/v1/models` 与 `/props`（确认 `total_slots = 2`、单槽 `n_ctx >= 8192` 且 `reasoning_in_content = false`）；
2. OpenAI 兼容 `/v1/chat/completions` 在开启原生思考（`chat_template_kwargs.enable_thinking = true`）与 `json_schema` 严约束下返回合法 `{ speak, mood, text }` JSON：`speak` 为布尔值，`mood` 严格属于 `Smug | Mocking | Tilted | Nervous | Plotting | Deadpan` 6 档枚举，`speak = false` 允许空 `text`；
3. 思考内容正确分离至 `reasoning_content`（非空），且 `content` 与 `text` 中零 `<think>` / `</think>` 标签泄漏；
4. 覆盖全部 5 位真人化`牌友角色`（**大川、小晚、大鹏、小桃、老陈**）及 1 组平淡巡沉默变脸样例，输出每句实测 TTFT、JSON 首字与端到端耗时；
5. 默认执行 2 路并发验证（可加 `-SkipConcurrency` 跳过）。

从 Mac 宿主机也可直接用 `curl` 快速验证局域网可达性：

```bash
curl -s http://192.168.5.50:8080/health
curl -s http://192.168.5.50:8080/v1/models
```

---

## 3. 通过 `-ModelPath` 一键切换模型权重

服务端与脚本仅依赖标准 OpenAI 兼容端点与 `json_schema` 契约，切换其他 GGUF 权重无需修改脚本主逻辑或服务端代码：

```powershell
# 1. 将新 GGUF 权重放入 E:\majjj-llm\models\
# 2. 仅通过 -ModelPath 与 -Restart 切换并重启
pwsh -File E:\majjj-llm\start-llama-server.ps1 `
  -ModelPath E:\majjj-llm\models\Huihui-Qwen3.5-9B-abliterated.Q5_K_M.gguf `
  -Restart

# 3. 如启用了开机自启，同步更新计划任务的默认模型路径
pwsh -File E:\majjj-llm\install-autostart.ps1 `
  -RootDir E:\majjj-llm `
  -ModelPath E:\majjj-llm\models\Huihui-Qwen3.5-9B-abliterated.Q5_K_M.gguf
```

> 若后续希望嘴臭尺度更放飞、零道德顾虑，可无缝替换为 `mradermacher/Huihui-Qwen3.5-9B-abliterated-GGUF`（`Q5_K_M`，体积约 6.1 GiB，与 12 GB 显存预算及当前协议完全兼容）。

---

## 4. 常见故障排查

| 现象 | 常见原因 | 排查与解决 |
| --- | --- | --- |
| 启动报显存不足（CUDA OOM） | 未限制 `-c` 导致默认 `262144` 上下文撑爆显存，或同卡其他程序占用过高 | 运行 `nvidia-smi` 检查空闲显存；确认使用 `--parallel 2 -c 16384`，若与 TTS 同卡共驻显存接近 `10,800 MiB` 红线，可先关闭高显存应用或微调 `-ContextSize` |
| 本机 `127.0.0.1:8080` 可通，Mac 局域网超时 | 防火墙未放行或局域网网段不匹配 | 以管理员重新运行 `setup-firewall.ps1 -LanSubnet <实际网段>`，并确认 Windows 网络配置文件与 Mac 处于同一网段 |
| 通过 SSH 启动后一断开会话进程就退出 | 使用了 `Start-Process` 绑定在 SSH Job 对象内 | 始终使用 `start-llama-server.ps1` 默认后台模式（内部通过 `Win32_Process.Create` 脱离会话 Job） |
| `content` 中出现 `<think>` 标签或未返回 `reasoning_content` | 未传 `--reasoning-format deepseek`，或 PowerShell 引号剥离导致 `--chat-template-kwargs` 未正确送达 | 检查 `logs\llama-server.log`；始终通过本仓库 `start-llama-server.ps1` 启动（前台设 `$PSNativeCommandArgumentPassing = 'Standard'`，后台显式转义双引号）并运行 `smoke-test.ps1` 核验分离效果 |
| 调高 `-ReasoningBudget` 后出现 `speak=true` 但 `text` 为空或 JSON 截断 | 思考 token 预算加上 JSON 正文超出请求 `max_tokens` 上限 | 保持默认 `-ReasoningBudget 256`（配 `max_tokens = 512`），或将 `-ReasoningBudget 384` 与调用侧 `max_tokens >= 640` 同步调整 |
| 模型下载后 SHA-256 校验失败 | `curl -L -C -` 在 HuggingFace 302 重定向链下易发生续传字节错位 | 使用 `deploy-llama-server.ps1`（先 HEAD 解析直链 CDN 再按 64 MiB HTTP Range 分块校验合并） |

---

## 5. `BENDER` 真机实测验收基准汇总

以下数据实测于家庭局域网 Windows 主机 **`BENDER`**（`192.168.5.50`）：

- **硬件与软件基线**：Windows 11 / PowerShell `7.6.10` / Intel i5-12400F / 16 GB RAM / **NVIDIA GeForce RTX 3060 12GB**（驱动 `621.40`，CUDA `13.5`）/ `llama.cpp b12760` / `Qwen3.5-9B-Q5_K_M.gguf`（与 `E:\majjj-tts\` 的 `Qwen3-TTS-12Hz-0.6B-CustomVoice` 同卡共驻运行）

### 5.1 新版默认配置基线（`--parallel 2 -c 16384 --reasoning-format deepseek --reasoning-budget 256`）

- **显存占用（`nvidia-smi`，LLM + TTS + Windows 桌面整卡总计）**：
  - 升级前共驻基线（`--parallel 3 -c 12288`）：`10,283 MiB / 12,288 MiB`
  - 升级后常驻（`--parallel 2 -c 16384`，单槽 `n_ctx = 8192`）：**`10,341–10,351 MiB / 12,288 MiB`**（仅增 `+66 MiB`）
  - 2 路并发原生思考峰值：**`10,349 MiB / 12,288 MiB`**（GPU 利用率 `92%`，仍留约 `1.9 GiB` 安全余量，零 OOM）
- **结构化合规与思考分离**：
  - 默认 `-ReasoningBudget 256` 全套冒烟 **9/9（`100%`）通过**（1 次热身 + 5 位真人化`牌友角色`顺序流式 + 1 次平淡巡沉默检查 + 2 路并发）：严格符合 `{ speak, mood, text }` `json_schema` 契约，**`100%`** 正常分离 `reasoning_content`（均长约 `867` 字），**`0` 例** `<think>` / `</think>` 泄漏至 `content` 或 `text`；平淡巡正确输出 `speak = false` 与空 `text`。
- **延迟表现（`-ReasoningBudget 256`，`max_tokens = 512`）**：
  - 流式首字延迟（TTFT，5 角色）：avg **`286.4 ms`**（max `362.1 ms`）
  - JSON 正文首字（思考结束开始吐 JSON）：`5,410–5,723 ms`
  - 单次顺序端到端耗时（5 角色）：avg **`6,323.2 ms`**（max `6,530.8 ms`）；平淡巡沉默决策：**`6,250.1 ms`**
  - 2 路并发端到端耗时：单请求 `7,447.6 ms` / `7,535.3 ms`，整批 wall-clock **`7,615.9 ms`**（远低于服务端 `20s` 异步超时预算）

### 5.2 原生思考预算甜点对比实测（`--parallel 2 -c 16384`，`max_tokens = 512`）

| `--reasoning-budget` | 冒烟合规率 | 5 角色顺序 E2E 均值（最大） | 思考内容均长 | 实测结论 |
| --- | --- | --- | --- | --- |
| `192` | `6/6`（`100%`） | `5,097.9 ms`（`5,272.2 ms`） | `680.2` 字 | 延迟更低，但思考截断偏早，部分情绪高点被偏保守判为 `speak=false` |
| **`256`（默认）** | **`9/9`（`100%`）** | **`6,323.2 ms`（`6,530.8 ms`）** | **`867.0` 字** | **最佳甜点**：兼顾判断准确度、2 路并发 `7,615.9 ms` 延迟与 `max_tokens = 512` 安全余量 |
| `384` | `5/6`（`83.3%`） | `9,059.5 ms`（`9,310.3 ms`） | `1,359.5` 字 | 单次接近 `9.3s`，且在 `max_tokens = 512` 下出现 1 例正文余量不足导致 `speak=true` 空文本；如需使用须将 `max_tokens` 提至 `640+` |

### 5.3 历史参考基线与调优要点

- **历史无思考基线（v1，`--parallel 3 -c 12288 --reasoning-budget 0`）**：单卡独立部署显存 `7,229 MiB`（3 并发峰值 `7,237 MiB`），TTFT avg `155.7 ms`，无思考单次 E2E avg `860.3 ms`，3 并发 wall-clock `1.22s–1.56s`。
- **Prompt 与协议调优要点**：
  1. 在 system prompt 中为 6 档 `mood` 枚举提供中文心理锚点（如 `Smug=得意欠揍`、`Tilted=红温破防`、`Mocking=阴阳嘲讽`），配合原生思考可显著提升 `speak` 与 `mood` 一致性；
  2. 引导 `2–18` 字（常态 `4–14` 字）鲜活中文短句、默认不带人名直接开口、禁止模板化套路，并允许 `speak = false` 沉默更新`牌友情绪`；
  3. 仅允许引用输入手牌事实与公开`牌河`中明确出现的牌名，未给出具体牌名时用泛指，避免小模型脑补暗牌；
  4. `llama.cpp b12760` 的 `/props` 中 `default_generation_settings.params.reasoning_format` 反映的是原始 `/completion` 默认值（仍显示 `none`），`/v1/chat/completions` 的思考分离是否生效以 `smoke-test.ps1` 实测 `reasoning_content` 非空且 `content` 零 `<think>` 泄漏为准。
