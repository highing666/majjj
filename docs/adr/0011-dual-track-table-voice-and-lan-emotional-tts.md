---
status: accepted
---

# 双轨牌桌语音架构（离线角色语音包 + 局域网实时情感台词语音）

> 前置一手调研：分支 `research/windows-lan-emotional-tts`（[`docs/research/windows-lan-emotional-tts.md`](https://github.com/highing666/majjj/blob/research/windows-lan-emotional-tts/docs/research/windows-lan-emotional-tts.md)）。

为了让家庭局域网牌桌获得完整的听觉沉浸感（覆盖 34 张牌面报牌、`碰`/`明杠`/`补杠`/`暗杠`/`自摸`/`点炮`/`抢杠胡` 动作喊词、`快捷互动`短语音，以及 5 位毒舌`牌友角色`按 6 档`牌友情绪`实时演出的`牌桌台词`），同时严格守住 Windows 推理节点 **`BENDER`（RTX 3060 12GB 剩 `~4.8 GiB` 显存 / 16 GB RAM）**、8GB Apple Silicon Mac 单二进制与 Mac/iPhone Safari 播放红线，我们决定：**采用「离线静态`角色语音包`（`VoicePack`）+ 在线局域网实时`台词语音`（`BanterVoice`）」双轨解耦语音架构，正式点亮并演进 `ADR-0007` 预留的 `SeatInfo.voice_pack_id` 契约，并将局域网 TTS 服务定义为默认启用、可优雅降级的可选运行依赖**。

## Considered Options

- **全部语音（报牌、碰杠胡、`快捷互动`、吐槽台词）统一走在线 GPU 实时 TTS**：`舍牌`报牌每 `0.5–1.5 s` 触发一次且`鸣牌`/胡牌喊词必须与漫画冲击帧 `< 20 ms` 瞬发对齐；在线实时合成单句需 `0.5–1.0 s` 且在剩余 `4.8 GiB` 显存下必须严格单并发串行，全走在线不仅会导致报牌严重滞后，还会把毒舌吐槽的 GPU 队列彻底堵死。
- **将局域网 TTS 设为与 `ADR-0010` `llama-server` 相同的启动强门禁必选依赖**：语音合成属于听觉体验增强，不应在 Windows TTS 服务临时关闭、维护或模型切换时阻止整桌开局或打断对局；保留无声漫画气泡与离线静态`角色语音包`降级通道更符合家庭日常使用习惯。
- **等 LLM + TTS 全部合成完成再同步弹出漫画气泡，或首版直接引入 Raw PCM + `AudioWorkletNode` 流式播报**：前者会把高光同步窗口从 `~1.5 s` 拉长到 `~2.0–2.5 s` 拖慢牌局手感；后者在 v1 显著抬升跨端流式状态机与 iOS Safari 边界测试复杂度，而 10–26 字短句（2–4 秒）24 kHz 16-bit 单声道 WAV 仅 `96–192 KB`（局域网传输 `< 5 ms`），先弹气泡再异步拉取整包 WAV 性价比最高。
- **把 WAV 以 Base64 内联塞进 WebSocket JSON 消息，或让浏览器直连 Windows TTS 端口**：Base64 内联会使单帧膨胀至 `200+ KB` 并阻塞同连接上的实时出牌帧；浏览器直连 Windows 端会暴露内网多端口、引发 CORS 且导致双人同桌重复消耗 2 倍 GPU 算力。
- **在 `BENDER` 上使用 WSL2 / Docker Desktop 运行 `vLLM-Omni` 或强行共驻 `Qwen3-TTS-1.7B` / `IndexTTS-2.5`（BF16）**：`BENDER` 仅 `16 GB` 物理内存，WSL2 默认占用 50%（8 GB RAM）；`1.7B` 与 `IndexTTS-2.5` BF16 显存起步 `6–8 GiB`，叠加已占 `7,237 MiB` 的 `llama-server` 必然触发 Windows WDDM Shared GPU Memory 溢出，导致推理骤降 5–10 倍或 CUDA OOM。
- **有声时默认彻底取消`牌桌台词`文字气泡**：在深夜静音、iOS 首次未解锁音频、客厅交谈噪音或 TTS 降级时会导致玩家漏看毒舌台词与谐音梗，且削弱综艺花字视觉质感；改为默认保留并联动播音、同时提供本机可隐藏开关与自动回显兜底更稳妥。

## Consequences

- **双轨解耦架构与音色一致性（演进 `ADR-0007`）**：
  - **静态预录轨（`角色语音包` / `VoicePack`）**：覆盖 34 张标准牌名（`一万`～`九万`、`一条`～`九条`、`一筒`～`九筒`、`东风`、`南风`、`西风`、`北风`、`红中`、`发财`、`白板`）、5 类动作喊词（`碰`、`杠`、`暗杠`、`自摸`、`胡了`、`抢杠胡`）及 6 条`快捷互动`短语。离线在 `BENDER` 暂停 `llama-server` 时用满血 `Qwen3-TTS-12Hz-1.7B-CustomVoice`（5 位内置中文/方言音色：`QuickHothead→Eric`、`TeaSarcasm→Serena`、`StubbornBlamer→Dylan`、`ChaosInstigator→Vivian`、`TiltedMaster→Uncle_Fu`）与 `1.7B-VoiceDesign`（2 套人类可选音色：`human_male` / `human_female`）批量生成并编码为 7 套轻量单声道 `.m4a` AudioSprite + `manifest.json`（总计约 `1.0–1.4 MB`），随仓库提交并经 `rust-embed` 内嵌单二进制，实现 `< 5 ms` 瞬发、离线可用与缺失静默降级。
  - **在线实时轨（`台词语音` / `BanterVoice`）**：覆盖全部动态 AI `牌桌台词`（高光吐槽、局中心理战、AI 互怼链、`快捷互动`回喷、慢思考催牌、结算复盘），按 6 档`牌友情绪`实时情感合成；即便真机盲听后将在线引擎切至 `Fun-CosyVoice3-0.5B-2512` 或零样本克隆路线，也复用同一套离线 1.7B 角色音频作参考音，保持同一位`牌友角色`的静态报牌与实时吐槽音色一致。
- **可选优雅降级依赖与窄边界 `TtsClient`（对齐并区别于 `ADR-0010`）**：
  - `mahjong-server` 将局域网 TTS 封装在窄边界异步 `TtsClient` 之后，默认地址 `http://192.168.5.50:8081`，按 CLI `--tts-url` / `MAJJJ_TTS_URL` > `preferences.json` > 默认值解析，并支持 `--no-tts` 显式关闭；自动化离线测试通过注入本地假 HTTP 服务或确定性 Stub 验证完整契约。
  - 启动与对局中若 TTS 不可达、超时或返回异常，**绝不阻止启动或阻塞牌局推进**，仅向客户端广播轻量状态提示并退回纯漫画气泡（`角色语音包`照常播放）；连续失败进入 `15 s` 静默冷却退避并经后台 `/health` 探活自动恢复。
- **Windows 原生绿色部署、模型切换门禁与显存护栏**：
  - 镜像 `E:\majjj-llm\` 运维体系，在 `BENDER` 以原生 Windows 绿色目录 `E:\majjj-tts\`（`uv` 独立虚拟环境 + FastAPI/uvicorn + PowerShell 7 `Win32_Process.Create` + 端口 `8081` 仅放行家庭局域网网段）提供领域 HTTP 契约 `GET /health` 与 `POST /v1/tts/synthesize`（预留 `stream: true` Raw Int16 PCM 升级口）。
  - 首选后端为 `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice`（BF16 + `faster-qwen3-tts` + `attn_implementation="sdpa"`），以「与 3 并发 `llama-server` 共跑总显存 `≤ 10,800 MiB`、零 WDDM Shared GPU Memory 增量、6 档`牌友情绪`盲听区分度达标」作为首个真机验收门禁；若 0.6B 情绪张力不足，仅替换 Windows 后端至 `Fun-CosyVoice3-0.5B-2512`（FP16 + 5 角色内存声纹缓存）或「离线 1.7B 5×6 参考音矩阵 + 在线 0.6B-Base 预缓存」，`mahjong-server` 零代码改动。
  - 强制落实 5 道防溢出护栏：NVIDIA 驱动 `Prefer No Sysmem Fallback`、`torch.cuda.set_per_process_memory_fraction(0.30)`、`PYTORCH_CUDA_ALLOC_CONF=max_split_size_mb:128,garbage_collection_threshold:0.8`、`asyncio.Semaphore(1)` 严格单并发队列 + `max_new_tokens=120` 硬限，以及携带 `seat_generation` / `deadline_ms`（局中 `2,500 ms`、结算 `5,000 ms`）的入队前过期熔断。
- **单次合成 LRU 中转、双轨防吵仲裁与气泡/本机偏好联动**：
  - `mahjong-server` 产出台词后立即广播 `TableBanter`（附带唯一 `banter_id`），同时异步向 Windows TTS 合成一次 24 kHz 16-bit 单声道 WAV 存入短时 LRU 内存缓存（上限 24 条 / TTL 60 秒 / 总内存 `< 5 MB`），再广播轻量 `TableBanterAudioReady { banter_id, seat, audio_url, duration_ms }`；Mac 与 iPhone 客户端同源拉取 `GET /api/banter-audio/<banter_id>`，经 `AudioContext.decodeAudioData()` 接入现有 `voGain → masterGain → DynamicsCompressorNode` 总线。
  - 双轨仲裁遵循防吵纪律：① 报牌覆盖四家，但绝不排队、绝不打断他人，任何`台词语音`或高优先级动作喊词播放中报牌静默让路；② 动作喊词按 `胡 > 抢杠胡 > 杠 > 碰` 瞬发直出，确保冲击帧零延迟；③ `快捷互动`即点即播短语音；④ `台词语音`实行同座新句立即打断旧句、跨座短队列上限 2 句（超限淘汰队列中最旧普通吐槽）、切入下一`小局`/回大厅/解散立即停播清空，播放期间沿用 `ADR-0007` 的 30 ms `sfxGain` 自动 Ducking（`0.45`）与相对座次轻声像（上家 `-0.35` / 下家 `+0.35` / 自家与对门 `0`）。
  - 气泡默认保留并展示说话脉冲、停留时长覆盖播音结束；前端本机 `DevicePreferences` 新增独立 `voice_volume`、`tile_call_voice_enabled`（报牌语音开关）、`banter_voice_enabled`（台词语音开关）、人类音色选择与 `hide_banter_bubble_with_voice`（有声时隐藏气泡，默认关闭；本机静音或 TTS 降级时强制自动回显）。
