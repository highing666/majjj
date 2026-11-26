# Windows `llama-server` 局域网绿色部署与运维指南

本文档用于指导在家庭局域网 Windows 主机（NVIDIA RTX 3060 12GB 及以上显卡）上绿色部署、常驻运行、安全暴露与真机冒烟验证 Majjj 毒舌牌友推理服务。

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
├── start-llama-server.ps1             # 幂等启动（支持 -ModelPath 换模型 / -Restart / -Foreground）
├── stop-llama-server.ps1              # 幂等停止
├── setup-firewall.ps1                 # 幂等局域网防火墙入站规则
├── install-autostart.ps1              # 可选开机自启计划任务（SYSTEM 权限）
└── smoke-test.ps1                     # 5 位牌友角色 + 6 档情绪 + 3 并发真机冒烟验证
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

### 2.3 启动与停止服务（幂等，SSH 断开不掉线）

```powershell
# 后台常驻启动（通过 Win32_Process.Create 脱离 SSH/终端会话 Job，并自动轮询 /health 与 /props）
pwsh -File E:\majjj-llm\start-llama-server.ps1

# 前台调试启动（设置 $PSNativeCommandArgumentPassing = 'Standard'）
pwsh -File E:\majjj-llm\start-llama-server.ps1 -Foreground

# 重启服务
pwsh -File E:\majjj-llm\start-llama-server.ps1 -Restart

# 停止服务
pwsh -File E:\majjj-llm\stop-llama-server.ps1
```

核心启动参数（经 RTX 3060 12GB 实测冻结）：

```powershell
llama-server.exe `
  -m E:\majjj-llm\models\Qwen3.5-9B-Q5_K_M.gguf `
  --host 0.0.0.0 --port 8080 `
  -ngl 999 -fa on --parallel 3 -c 12288 `
  --jinja --chat-template-kwargs '{"enable_thinking": false}' --reasoning-budget 0 `
  --log-file E:\majjj-llm\logs\llama-server.log
```

> **为什么必须显式传入 `-c 12288`？**
> Qwen3.5 训练上下文为 `262144`，若不限制总上下文长度，配合 `--parallel 3` 会直接撑爆 12 GB 显存；显式指定 `-c 12288` 后每个并发槽位分配 `n_ctx = 4096`，实测常驻显存仅约 7.1 GiB，同时 `/props` 确认 `total_slots = 3`、`reasoning_format = "none"`、`reasoning_in_content = false`。

### 2.4 可选开机自启（管理员权限，幂等）

```powershell
# 注册 SYSTEM 权限开机自启计划任务（无需登录桌面即可拉起）
pwsh -File E:\majjj-llm\install-autostart.ps1 -RootDir E:\majjj-llm

# 卸载计划任务
pwsh -File E:\majjj-llm\install-autostart.ps1 -Uninstall
```

### 2.5 真机冒烟验证

在 Windows 本机或同一局域网内的 Mac（若安装了 `pwsh`）运行：

```powershell
pwsh -File E:\majjj-llm\smoke-test.ps1 -BaseUrl http://127.0.0.1:8080
```

脚本会依次检查：
1. `/health`、`/v1/models` 与 `/props`（确认 `reasoning_format = none`）；
2. OpenAI 兼容 `/v1/chat/completions` 在 `json_schema` 约束下返回严格 `{ mood, text }` JSON，`mood` 严格属于 `Smug | Mocking | Tilted | Nervous | Plotting | Deadpan` 6 档枚举，且无 `<think>` 或 `reasoning_content` 思考链泄漏；
3. 覆盖全部 5 位毒舌牌友角色（急眼快攻哥、阴阳茶艺师、嘴硬死磕王、拱火乐子人、红温老雀神），输出每句实测 TTFT 与整句端到端耗时；
4. 默认执行 3 路并发验证（可加 `-SkipConcurrency` 跳过）。

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
| 启动报显存不足（CUDA OOM） | 未限制 `-c` 导致默认 `262144` 上下文撑爆显存，或其他程序占用大量显存 | 运行 `nvidia-smi` 检查空闲显存；确认使用 `-c 12288 --parallel 3`，必要时先关闭高显存占用应用或将 `-ParallelSlots` 降为 `2` |
| 本机 `127.0.0.1:8080` 可通，Mac 局域网超时 | 防火墙未放行或局域网网段不匹配 | 以管理员重新运行 `setup-firewall.ps1 -LanSubnet <实际网段>`，并确认 Windows 网络配置文件与 Mac 处于同一网段 |
| 通过 SSH 启动后一断开会话进程就退出 | 使用了 `Start-Process` 绑定在 SSH Job 对象内 | 始终使用 `start-llama-server.ps1` 默认后台模式（内部通过 `Win32_Process.Create` 脱离会话 Job） |
| 响应中出现 `<think>` 或 `reasoning_content` | PowerShell 引号剥离导致 `--chat-template-kwargs` 未正确传入 JSON | 检查 `logs\llama-server.log` 启动参数；使用本仓库 `start-llama-server.ps1`（前台设 `$PSNativeCommandArgumentPassing = 'Standard'`，后台转义双引号）并用 `/props` 确认 `reasoning_format = none` |
| 模型下载后 SHA-256 校验失败 | `curl -L -C -` 在 HuggingFace 302 重定向链下易发生续传字节错位 | 使用 `deploy-llama-server.ps1`（先 HEAD 解析直链 CDN 再按 64 MiB HTTP Range 分块校验合并） |

---

## 5. `BENDER` 真机实测验收基准汇总

以下数据实测于家庭局域网 Windows 主机 **`BENDER`**（`192.168.5.50`），由 Mac 宿主机跨局域网发起请求测得：

- **硬件与软件基线**：Windows 11 / PowerShell `7.6.10` / Intel i5-12400F / 16 GB RAM / **NVIDIA GeForce RTX 3060 12GB**（驱动 `621.40`，CUDA `13.5`）/ `llama.cpp b12760` / `Qwen3.5-9B-Q5_K_M.gguf`
- **显存占用（`nvidia-smi`）**：
  - 部署前桌面基线：`1,046 MiB` 已用（`11,242 MiB` 空闲）
  - 模型常驻 + `--parallel 3 -c 12288`：**`7,229 MiB` 已用 / `12,288 MiB`**
  - 3 路并发推理峰值：**`7,237 MiB` 已用**（余量约 `4.8 GiB`，零 OOM）
- **结构化合规与思考链抑制**：
  - 累计实测 **40+ 次请求**（含流式、顺序、3 路并发）：**`100%`** 合法 `{ mood, text }` JSON，6 档枚举（`Smug / Mocking / Tilted / Nervous / Plotting / Deadpan`）100% 合规，**`0` 例** `<think>` 或 `reasoning_content` 泄漏。
- **延迟与吞吐表现**：
  - 流式首字延迟（TTFT，热身后 5 次）：avg **`155.7 ms`**（p50 `176.5 ms`，max `183.4 ms`）
  - 生成吞吐：约 **`42–46.6 tok/s`**
  - 单次端到端耗时（Prompt v2，10–26 字中文口语）：avg **`860.3 ms`**（范围 `715–1,320 ms`）
  - 3 路并发端到端耗时（3 轮 × 3 并发）：单请求 avg **`1,261.5 ms`**（max `1,543.8 ms`），整批 wall-clock **`1.22s – 1.56s`**，满足高光事件 `~1.5s` 短同步预算。
- **Prompt 调优要点**：
  1. 在 system prompt 中为 6 档 `mood` 枚举提供中文心理锚点（如 `Smug=得意欠揍`、`Tilted=红温破防`、`Mocking=阴阳嘲讽`），可显著提升情绪选择准确率；
  2. 通过角色嗓音约束 + 字数上限（`10–26 字`）+ 采样参数（`temperature=0.9, top_p=0.9, repeat_penalty=1.1, presence_penalty=0.4`）去模板化；
  3. 仅允许引用输入手牌事实中明确出现的牌名，未给出具体牌名时用泛指，避免小模型脑补牌面。
