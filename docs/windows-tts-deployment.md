# Windows 局域网情感 TTS 绿色部署与运维指南

本文档用于指导在家庭局域网 Windows 推理主机（默认 **`BENDER` / `192.168.5.50`**，RTX 3060 12GB 剩约 `4.8 GiB` 显存 / 16 GB RAM，与常驻 `llama-server` 共存）上绿色部署、常驻运行、安全暴露、按需切换引擎以及真机盲听验收 Majjj 实时`台词语音`（`BanterVoice`）服务。

> 所有运维脚本均基于 **PowerShell 7（`pwsh`）** 编写，位于仓库 [`scripts/windows-tts/`](../scripts/windows-tts/)，严禁依赖 WSL2 或 Docker Desktop，全程采用独立 `uv` 虚拟环境与绿色目录，不污染系统盘与全局 Python。

---

## 1. 目录结构（镜像 `E:\majjj-llm\`，推荐置于空闲大容量分区）

默认部署根目录为 `E:\majjj-tts`（所有脚本均支持通过 `-RootDir` 自定义路径）：

```text
E:\majjj-tts\
├── bin\
│   └── uv.exe                                  # 便携版 uv（若 PATH 无 uv 自动落盘于此）
├── .venv\                                      # uv 托管独立 Python 3.11 + PyTorch CUDA + FastAPI
├── cache\                                      # uv 与 HuggingFace 缓存（全部隔离在 E: 盘）
├── models\
│   └── Qwen3-TTS-12Hz-0.6B-CustomVoice\        # 本地固定快照权重（运行时离线加载）
├── voices\                                     # 备选引擎参考音目录（CosyVoice3 5 角色 / 0.6B-Base 5x6 矩阵）
├── server\
│   ├── __main__.py                             # CLI 入口（在导入 torch 前强制设置 PYTORCH_CUDA_ALLOC_CONF）
│   ├── app.py                                  # FastAPI 路由：GET /health, POST /v1/tts/synthesize, POST /v1/audio/speech
│   ├── service.py                              # 单并发串行 GPU 队列 + deadline_ms 入队前/入 GPU 前过期丢弃
│   ├── backends.py                             # 可插拔推理后端（Qwen3-0.6B-CustomVoice / CosyVoice3 / Qwen3-0.6B-Base）
│   ├── vram_guard.py                           # 显存护栏配置与统计
│   ├── personas.py                             # 5 牌友角色 x 6 牌友情绪解析器
│   ├── personas.json                           # 5 角色音色与 6 档情绪中文指令映射表
│   └── audio.py                                # 24 kHz 16-bit 单声道 WAV 封装
├── logs\
│   ├── tts-server.log                          # 服务端运行日志
│   └── tts-server.pid                          # 后台常驻进程 PID
├── deploy-tts-server.ps1                       # 一键绿色部署（uv + 独立虚拟环境 + 模型快照）
├── start-tts-server.ps1                        # 幂等启动（Win32_Process.Create 脱离 SSH，端口 8081）
├── stop-tts-server.ps1                         # 幂等停止
├── setup-firewall.ps1                          # 唯一局域网入站规则 majjj-tts-lan-8081
├── install-autostart.ps1                       # 可选 SYSTEM 开机自启计划任务
└── smoke-test.ps1                              # 5 角色 x 6 情绪盲听采样 + 30 句连续压测 + 显存红线核验
```

---

## 2. 5 道防显存溢出护栏（RTX 3060 12GB 共驻生命线）

Windows WDDM 驱动在物理显存不足时会静默回退至 16 GB 系统共享内存，导致推理骤降 5–10 倍并连带拖慢常驻的 `llama-server`。本服务强制落实 5 道硬护栏：

1. **驱动层禁止静默回退系统共享显存（Fail-Fast）**：
   - 在 `BENDER` 打开 **NVIDIA Control Panel → 管理 3D 设置 → 程序设置**；
   - 添加并选中专用解释器 **`E:\majjj-tts\.venv\Scripts\python.exe`**；
   - 将 **`CUDA - Sysmem Fallback Policy`** 设为 **`Prefer No Sysmem Fallback`** 并应用。
2. **PyTorch 进程级显存硬封顶（`30%` ≈ `3,686 MiB`）**：
   - 后端加载权重前强制调用 `torch.cuda.set_per_process_memory_fraction(0.30, device=0)`（通过 `-VramFraction` 可在 `0.10..0.45` 间微调），从 CUDA 分配器层面物理隔离 `llama-server` 的 `~7.24 GiB` 显存。
3. **Windows 专用 CUDA 分配器回收参数（禁用 `expandable_segments`）**：
   - `start-tts-server.ps1` 与 `server/__main__.py` 在导入 `torch` 前强制写入：
     ```text
     PYTORCH_CUDA_ALLOC_CONF=max_split_size_mb:128,garbage_collection_threshold:0.8
     ```
   - 严格拒绝 Windows WDDM 不兼容的 `expandable_segments:True`。
4. **严格单并发串行 GPU 队列 + `max_new_tokens` 硬封顶**：
   - `uvicorn` 固定 `workers=1`，`service.py` 通过 `asyncio.Semaphore(1)` 保证任意时刻最多 `1` 条请求占用 GPU；
   - 针对 10–26 字麻将短句将 `max_new_tokens` 封顶在 `120` 帧（12.5 Hz 约 `9.6 s`），防止偶发长停顿撑爆 KV Cache。
5. **入队前与入 GPU 前 `deadline_ms` 过期丢弃**：
   - 请求携带 `deadline_ms`（局中 `2500 ms`、结算 `5000 ms`）：`deadline_ms <= 0` 在入队前直接返回 `HTTP 408`；排队等待后若 `queue_wait_ms >= deadline_ms`，在进入 GPU 前直接丢弃并返回 `HTTP 408`（附带 `X-TTS-Dropped-Reason: deadline_exceeded`）。

---

## 3. 一键部署与日常运维

### 3.1 一键绿色部署（默认主引擎 `Qwen3-TTS-12Hz-0.6B-CustomVoice`）

```powershell
pwsh -File .\scripts\windows-tts\deploy-tts-server.ps1 -RootDir E:\majjj-tts
```

- 自动同步所有 `.ps1` 脚本与 `server\` 源码至 `E:\majjj-tts\`；
- 使用便携 `uv` 在 `E:\majjj-tts\.venv` 安装 Python 3.11、CUDA 版 PyTorch（Ampere SM 8.6 使用原生 `attn_implementation="sdpa"` + `bfloat16`，零 `flash-attn` 编译依赖）、`faster-qwen3-tts`、`qwen-tts`、`fastapi` 与 `uvicorn`；
- 拉取 `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice` 快照至 `E:\majjj-tts\models\`（已存在时幂等跳过）。
- 部署完毕后请按第 2 节完成 NVIDIA Control Panel 的 `Prefer No Sysmem Fallback` 配置。

### 3.2 配置局域网防火墙（管理员权限，幂等）

```powershell
pwsh -File E:\majjj-tts\setup-firewall.ps1 -LanSubnet 192.168.5.0/255.255.255.0 -Port 8081
```

- 创建唯一入站规则 **`majjj-tts-lan-8081`**（`Inbound` / `Allow` / `TCP 8081` / `Profile: Any`），仅放行家庭局域网网段，拒绝 `Any`、`0.0.0.0/0`、`*` 等通配地址。
- 移除规则：

```powershell
pwsh -File E:\majjj-tts\setup-firewall.ps1 -Remove
```

### 3.3 启动与停止服务（幂等，SSH 断开不掉线）

```powershell
# 后台常驻启动（Win32_Process.Create 脱离 SSH/终端会话 Job，自动轮询 /health）
pwsh -File E:\majjj-tts\start-tts-server.ps1

# 前台调试启动（设置 $PSNativeCommandArgumentPassing = 'Standard'）
pwsh -File E:\majjj-tts\start-tts-server.ps1 -Foreground

# 重启服务
pwsh -File E:\majjj-tts\start-tts-server.ps1 -Restart

# 停止服务
pwsh -File E:\majjj-tts\stop-tts-server.ps1
```

### 3.4 可选开机自启（管理员权限，幂等）

```powershell
# 注册 SYSTEM 权限开机自启计划任务（无需登录桌面即可拉起）
pwsh -File E:\majjj-tts\install-autostart.ps1 -RootDir E:\majjj-tts

# 卸载计划任务
pwsh -File E:\majjj-tts\install-autostart.ps1 -Uninstall
```

### 3.5 真机冒烟、盲听采样与显存压测

```powershell
pwsh -File E:\majjj-tts\smoke-test.ps1 -BaseUrl http://127.0.0.1:8081 -OutputDir E:\majjj-tts\logs\blind-listen
```

脚本会依次核验 `/health`、执行 3 次热身、合成并落盘 **5 位`牌友角色` × 6 档`牌友情绪` = 30 条 24 kHz 单声道 WAV**、验证 `deadline_ms=0` 返回 `408`、探活 `/v1/audio/speech`，并执行 30 条连续短句压测与 `nvidia-smi` 显存红线校验（总显存 `≤ 10,800 MiB`，TTS 单进程 `≤ 3,500 MiB`）。

从 Mac 宿主机快速探活：

```bash
curl -s http://192.168.5.50:8081/health
```

---

## 4. 可插拔后端平滑切换（Mac 服务端与前端零改动）

所有后端共享完全一致的 HTTP 契约（`GET /health`、`POST /v1/tts/synthesize`、`POST /v1/audio/speech`）以及 `personas.json` 中的 5 角色 × 6 情绪映射：

| 梯队 | `-Backend` 参数 | 模型快照 | 特点与适用场景 |
| --- | --- | --- | --- |
| 首选（Primary） | `qwen3-custom-voice` | `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice` | BF16 + `faster-qwen3-tts` + PyTorch SDPA，内置 5 位中文/方言音色（`Eric`/`Serena`/`Dylan`/`Vivian`/`Uncle_Fu`），零参考音编码耗时 |
| 强情感备选（Runner-up） | `cosyvoice3` | `FunAudioLLM/Fun-CosyVoice3-0.5B-2512` | FP16 + 5 角色内存声纹特征缓存（规避上游 `zero_shot_spk_id` 提示词覆盖缺陷）+ `<\|endofprompt\|>` 情绪指令与副语言标签 |
| 确定性保底（Fallback） | `qwen3-base-clone` | `Qwen/Qwen3-TTS-12Hz-0.6B-Base` | 预缓存离线 1.7B 生成的 5×6=30 条高张力情绪参考音频（`voices/<voice_pack_id>_<mood>.wav`），零自然语言指令漂移 |

若 `BENDER` 盲听验收发现 0.6B-CustomVoice 情绪张力不及格，仅在 Windows 端执行切换即可（`mahjong-server` 与前端零代码改动）：

```powershell
# 切至 Fun-CosyVoice3-0.5B-2512（需先在 E:\majjj-tts\voices\ 放置 5 角色中性参考音频）
pwsh -File E:\majjj-tts\deploy-tts-server.ps1 -RootDir E:\majjj-tts -Backend cosyvoice3
pwsh -File E:\majjj-tts\start-tts-server.ps1 -RootDir E:\majjj-tts -Backend cosyvoice3 -Restart

# 或切至 Qwen3-TTS-12Hz-0.6B-Base 5x6 参考音矩阵（需先放置 30 条参考音频）
pwsh -File E:\majjj-tts\deploy-tts-server.ps1 -RootDir E:\majjj-tts -Backend qwen3-base-clone
pwsh -File E:\majjj-tts\start-tts-server.ps1 -RootDir E:\majjj-tts -Backend qwen3-base-clone -Restart
```

---

## 5. HTTP 领域契约与 5×6 音色情绪映射

### 5.1 端点摘要

- **`GET /health`**：返回 `status`、`ready`、`backend`、`model`、`queue_depth`、`vram_allocated_mib`、`vram_reserved_mib`、`vram_limit_fraction`、`personas_cached`、`max_new_tokens`、`sample_rate`。
- **`POST /v1/tts/synthesize`**：
  - 请求体：`request_id`、`persona_id`（支持 `QuickHothead` 等规范枚举或 `quick_hothead` 别名）、`mood`（6 档规范枚举）、`text`、可选 `instruct_override`、`format`（默认 `wav`）、`stream`（v1 默认 `false`；设为 `true` 预留 24 kHz `pcm_s16le` 原始流式分块扩展口）、`deadline_ms`（默认 `2500`）。
  - 响应体：24 kHz 16-bit 单声道 `audio/wav`，附带 `X-TTS-Request-Id`、`X-TTS-Duration-Ms`、`X-TTS-Synth-Ms`、`X-TTS-Queue-Wait-Ms`、`X-TTS-Backend`、`X-TTS-Model`；超期返回 `408` 与 `X-TTS-Dropped-Reason: deadline_exceeded`。
- **`POST /v1/audio/speech`**：轻量 OpenAI TTS 兼容探活端点。

### 5.2 5 位`牌友角色`音色映射

| `PersonaId` | 昵称 | 内置 Speaker | `voice_pack_id` |
| --- | --- | --- | --- |
| `QuickHothead` | 急眼快攻哥 | `Eric` | `quick_hothead` |
| `TeaSarcasm` | 阴阳茶艺师 | `Serena` | `tea_sarcasm` |
| `StubbornBlamer` | 嘴硬死磕王 | `Dylan` | `stubborn_blamer` |
| `ChaosInstigator` | 拱火乐子人 | `Vivian` | `chaos_instigator` |
| `TiltedMaster` | 红温老雀神 | `Uncle_Fu` | `tilted_master` |

---

## 6. 常见故障排查

| 现象 | 常见原因 | 排查与解决 |
| --- | --- | --- |
| 启动或推理报 `torch.OutOfMemoryError` | `llama-server` 显存超预期或本机桌面占显存偏高 | 运行 `nvidia-smi` 核验空余显存；确认 `llama-server` 使用 `-c 12288 --parallel 3`（约 `7.24 GiB`），并将 TTS `-VramFraction` 保持在 `0.30` |
| 推理骤慢至 5–10 秒且系统内存暴涨 | 未设置驱动防回退，触发 WDDM Shared GPU Memory 溢出 | 打开 NVIDIA Control Panel，对 `E:\majjj-tts\.venv\Scripts\python.exe` 设置 `Prefer No Sysmem Fallback` 后重启服务 |
| 出现 `expandable_segments` 警告或崩溃 | 外部环境变量污染了 `PYTORCH_CUDA_ALLOC_CONF` | 使用 `start-tts-server.ps1` 启动，强制覆盖为 `max_split_size_mb:128,garbage_collection_threshold:0.8` |
| 本机 `127.0.0.1:8081` 可通，Mac 局域网超时 | 防火墙未放行 `TCP 8081` 或网段不匹配 | 以管理员重新运行 `setup-firewall.ps1 -LanSubnet <实际网段> -Port 8081` |
| SSH 断开后服务退出 | 未通过脱离会话方式启动 | 始终使用 `start-tts-server.ps1` 默认后台模式（内部通过 `Win32_Process.Create` 脱离会话 Job） |

---

## 7. `BENDER` 真机验收与盲听基准记录模板

> 当前为真机部署前模板；在 `BENDER`（`192.168.5.50`，RTX 3060 12GB / 16 GB RAM）完成与 3 路并发 `llama-server` 双开实测及 6 档`牌友情绪`人工盲听后，将实测数字回填至下表并锁定最终在线后端。

- **验收日期**：`TBD`
- **最终锁定在线后端**：`TBD`（候选：`qwen3-custom-voice` / `cosyvoice3` / `qwen3-base-clone`）
- **硬件与软件基线**：Windows 11 / PowerShell `7.6.x` / Intel i5-12400F / 16 GB RAM / NVIDIA GeForce RTX 3060 12GB / 与 `E:\majjj-llm` `llama-server`（`--parallel 3 -c 12288`）双开

| 验收项 | 硬指标红线 | 实测结果 | 结论 |
| --- | --- | --- | --- |
| 驱动防回退策略 | `E:\majjj-tts\.venv\Scripts\python.exe` 已设为 `Prefer No Sysmem Fallback` | `TBD` | `[ ]` |
| 局域网防火墙与 SSH 脱离常驻 | 仅放行 `192.168.5.0/24 -> TCP 8081`，SSH 断开后 `/health` 持续 `200 OK` | `TBD` | `[ ]` |
| 双服务共驻总显存（`nvidia-smi`） | `≤ 10,800 MiB / 12,288 MiB` | `TBD MiB` | `[ ]` |
| TTS 单进程显存峰值 | `≤ 3,500 MiB`（`set_per_process_memory_fraction(0.30)` 零 OOM） | `TBD MiB` | `[ ]` |
| Windows Shared GPU Memory 增量 | 压测期间增量为 `0 MB` | `TBD MB` | `[ ]` |
| 30 条连续短句 + 3 路 LLM 并发压测 | 零 OOM、零拖慢 `llama-server` | `TBD` | `[ ]` |
| 10–26 字中文台词端到端耗时 | avg `≤ 1,000 ms` / p95 `≤ 1,350 ms` | `avg TBD ms / p95 TBD ms` | `[ ]` |
| 5 位`牌友角色`音色辨识度盲听 | 5 角色一耳可辨、音色无漂移 | `TBD` | `[ ]` |
| 6 档`牌友情绪`盲听区分度 | `Smug / Mocking / Tilted / Nervous / Plotting / Deadpan` 张力达标 | `TBD` | `[ ]` |
