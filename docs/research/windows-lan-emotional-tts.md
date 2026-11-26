# RTX 3060 12GB（~4.8 GiB 显存余量）Windows 局域网中文情感 TTS 模型与服务选型调研报告

> **调研范围**：面向家庭局域网 4 人麻将牌局（Majjj）毒舌`牌友角色`实时语音合成，在宿主机 **`BENDER`（Windows 11 / i5-12400F / 16 GB RAM / RTX 3060 12GB，常驻 `llama-server` 已占 ~7.24 GiB 显存，剩余 ~4.8 GiB）** 的真实约束下，基于截至 **2026 年 11 月** 的官方 GitHub 仓库、HuggingFace Model Card、技术论文与引擎官方文档，系统评估 Qwen3-TTS、CosyVoice 3.0 及其他主流中文情感开源 TTS 模型、显存共存机制、Windows 绿色服务栈与 Mac/iPhone Safari 播放链路。

---

## 执行摘要（Executive Summary）

1. **首选方案（Primary）：`Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice`（BF16）+ `faster-qwen3-tts`（PyTorch `StaticCache` + CUDA Graphs + 原生 Windows 支持）**
   - **显存与生态精准命中**：权重约 ~2.5 GB（硬盘），BF16 推理常驻加峰值显存约 **2.6–3.3 GiB**，与已占 `7,237 MiB` 的 `llama-server` 共存后总显存约 **9.8–10.5 GiB / 12.0 GiB**，保留 **1.5–2.2 GiB 安全余量**，无需冒风险量化 1.7B 模型，且全栈 **Apache-2.0** 协议。
   - **天然 1:1 匹配 Majjj 5 位`牌友角色`**：内置 9 位预置音色中恰好包含 **5 位辨识度极高的中文/方言角色**（`Vivian`、`Serena`、`Uncle_Fu`、京片子 `Dylan`、川渝腔 `Eric`），无需每次推理编码参考音频，音色零漂移。
   - **关键实测验证点（已提前标红）**：上游社区反馈 `0.6B-CustomVoice` 对 `instruct` 情绪指令的遵循强度弱于 `1.7B-CustomVoice`（部分旧版封装甚至会忽略 `instruct`）。因此上线前必须在 `BENDER` 上执行 6 档`牌友情绪`盲听验收；若 0.6B `instruct` 情绪张力不足，立即平滑切入下方 Runner-up 或「离线 VoiceDesign 铸造 5×6 情绪参考音 + 0.6B-Base 零样本预缓存」混合路线。
2. **强情感与副语言备选（Runner-up）：`FunAudioLLM/Fun-CosyVoice3-0.5B-2512`（PyTorch FP16，原生 Windows）**
   - **中文情绪与毒舌表现力天花板级**：0.5B 级体量（FP32 总权重 ~3.44 GB，启用 `fp16=True` 后常驻与推理峰值约 **2.5–3.8 GiB**），原生支持 `<|endofprompt|>` 自然语言情绪/方言指令（18 种中国方言）、以及 `[laughter]`、`[breath]`、`<strong>` 等内联副语言/重音标签，**Apache-2.0** 协议。
   - **工程取舍**：原生 Windows 部署需绕过仅提供 Linux wheel 的 `ttsfrd` 与 `vllm`，改走 `pynini`（conda-forge Python 3.10 或社区 wheel）+ `WeTextProcessing` 回退链路；同时需在薄封装服务中规避上游 `inference_instruct2` 与 `zero_shot_spk_id` 混用时的 `prompt_text` 覆盖 Bug。
3. **超轻显存与确定性保底（Fallback）：`GPT-SoVITS v2 / v2Pro`（FP16，MIT）或 `Qwen3-TTS-12Hz-0.6B-Base`（离线 5×6 参考音矩阵）**
   - 无自然语言 `instruct` 漂移风险：可在离线阶段（暂停 `llama-server` 时）用 `Qwen3-TTS-12Hz-1.7B-VoiceDesign` 一次性生成 **5 角色 × 6 情绪 = 30 条 3–5 秒高张力参考音频**，在线服务仅按 `(persona_id, mood)` 命中预缓存的 speaker/prompt 特征，以 ~2.5–3.5 GiB 显存获得 100% 确定性的情绪区分度。
4. **明确否决项（因显存/内存/许可证硬冲突）**
   - **否决 `Qwen3-TTS-1.7B`（原生 BF16 ~6 GiB）、`IndexTTS-2 / 2.5`（FP16/BF16 起步 ~6–8 GiB）、`VoxCPM2`（2B 需 ~8–24 GiB）、`Step-Audio-TTS/EditX`（~10–18 GiB）作为与 `llama-server` 共驻的默认引擎**：均会突破 4.8 GiB 剩余显存，触发 Windows WDDM Shared GPU Memory 溢出至 16 GB 系统内存，导致推理骤降 5–10 倍或直接 OOM。
   - **否决 WSL2 / Docker Desktop 部署**：`BENDER` 仅 16 GB 物理内存，WSL2 默认占用 50% 宿主机内存（8 GB），叠加 Windows 11 与 `llama-server` 极易触发系统换页抖动；推荐与 `feat/spec-63-lan-llm-table-personas` 分支中 `docs/windows-llm-deployment.md` 的 `E:\majjj-llm` 保持一致的 **原生 Windows 独立绿色目录 `E:\majjj-tts\`（`uv` 便携 Python + FastAPI/uvicorn + PowerShell 7 脱离会话启动）**。
   - **音频链路**：v1 针对 10–26 字短句（2–4 秒语音，24 kHz 16-bit 单声道 WAV 仅 ~100–190 KB，局域网传输 <5 ms），直接由 Windows 返回 **WAV** 给 Mac `mahjong-server`，前端 Mac/iPhone Safari 走 `AudioContext.decodeAudioData()` 接入现有 Web Audio `voGain` + 自动 Ducking + `DynamicsCompressorNode` 管线；v1.5 可按需升级为 **Raw Int16 PCM + `AudioWorkletNode`** 真流式首包播报。

---

## 1. 硬件、显存与业务约束基线

### 1.1 `BENDER` 宿主机实测基线

根据仓库 `feat/spec-63-lan-llm-table-personas` 分支实测验收文档 `docs/windows-llm-deployment.md` §5，家庭局域网 Windows 推理节点 **`BENDER`（`192.168.5.50`）** 的硬软件基线与已冻结 LLM 占用如下：

| 维度 | 实测基线 | 对 TTS 选型的硬约束 |
| --- | --- | --- |
| 操作系统与运维栈 | Windows 11 / PowerShell `7.6.10` / 绿色目录 `E:\majjj-llm\` | 必须绿色免污染、支持 PowerShell 7 后台常驻（`Win32_Process.Create` 脱离 SSH Job） |
| CPU / 系统内存 | Intel i5-12400F / **16 GB RAM** | **严禁引入重型虚拟机开销**（WSL2/Docker Desktop 默认吃掉 8 GB RAM） |
| GPU 与驱动 | **NVIDIA GeForce RTX 3060 12GB**（Ampere SM 8.6，12,288 MiB，驱动 `621.40`，CUDA `13.5`） | 支持硬件 BF16/FP16 与 PyTorch SDPA FlashAttention 内核；但 CUDA 13.5 下手动编译第三方 C++ 扩展（如 `flash-attn`）风险高 |
| 桌面基线显存 | `1,046 MiB`（Windows DWM / 显示驱动） | 不可回收基线 |
| `llama-server` 占用 | `Qwen3.5-9B-Q5_K_M.gguf`（`--parallel 3 -c 12288`）：常驻总占用 `7,229 MiB`，3 路并发峰值 **`7,237 MiB`** | `llama-server` 自身约占 `6,191 MiB`，需长期稳定常驻，不可因 TTS 争抢显存而崩溃 |
| **可用显存余量** | **`5,051 MiB`（~4.93 GiB，实际安全空间 ~4.8 GiB）** | **TTS 常驻 + 推理峰值必须严格控制在 `≤ 3.5–4.0 GiB`**（保留 ≥1.0–1.5 GiB 防 WDDM 溢出与碎片余量） |

### 1.2 Majjj 麻将毒舌`牌友角色`业务特征

结合 `docs/adr/0010-lan-llm-table-personas.md`（Spec #63）与 `docs/windows-llm-deployment.md` §5：

- **短文本高频中文口语**：每句`牌桌台词`长度严格控制在 **10–26 字**（合成后约 **2.0–4.0 秒**语音），高频出现「啧、哼、哎哟、哈哈哈、行行行、就这？」等口语叹词、反问语调与短停顿，对**中文字正腔圆之外的戏剧化韵律**要求极高。
- **5 位固定`牌友角色`（`PersonaId`）**：急眼快攻哥、阴阳茶艺师、嘴硬死磕王、拱火乐子人、红温老雀神 —— 需要 **5 种一听就能辨认的差异化音色**（性别、年龄感、地域口音/语速基调区分明显），且 `SeatInfo` 已预留 `voice_pack_id` 占位字段。
- **6 档离散`牌友情绪`（`PersonaMood`）**：`Smug`（得意欠揍）、`Mocking`（阴阳嘲讽）、`Tilted`（红温破防）、`Nervous`（紧张心虚）、`Plotting`（阴险憋大招）、`Deadpan`（面无表情冷吐槽）—— 核心刚需是**单句级动态切换情绪语气，而不改变角色音色**。
- **端到端时延预算**：
  - LLM 单句平均端到端 **`860 ms`**（TTFT `156 ms`），3 路并发单句平均 **`1,262 ms`**（整批 `1.22–1.56 s`）。
  - ADR-0010 为`鸣牌`、`抢杠胡`、`小局`结算、`快捷互动`等高光事件设定了约 `1.5 s` 的短同步协同展示窗口；音频可紧随漫画气泡与冲击帧播报（允许比气泡晚 `0.3–1.0 s` 开声），因此要求 TTS **单句整句耗时 ≤ 1.0–1.2 s** 或 **流式首包延迟（TTFA）≤ 350–450 ms**。
- **跨端播放约束**：Mac 上的 Rust `mahjong-server` 作为中转调用 `192.168.5.50` 局域网 TTS 接口，下发给 **Mac Safari 与 iPhone Safari** 播放，必须兼容 iOS Safari 的自动播放策略、Web Audio `decodeAudioData` 编解码限制及与牌桌 SFX 的自动闪避（Ducking）。

---

## 2. 2025–2026 中文情感开源 TTS 候选一手横评表与深度剖析

> **RTF 口径声明**：语音学界与论文中标准实时率定义为 $\text{RTF} = \text{推理耗时} / \text{音频时长}$（**越小越快**，`< 1.0` 表示快于实时）；而部分开源推理框架（如 `faster-qwen3-tts`）将加速比写成 $\text{音频时长} / \text{推理耗时}$（如 `5.53x` 表示快于实时 5.53 倍，即标准 $\text{RTF} \approx 0.18$）。下文所有表格均统一折算并注明口径。

### 2.1 一手核心参数横评总表

| 模型与版本 | 参数量 / 核心架构 | 磁盘权重体积 | 推理峰值显存（实测/官方） | 中文情绪控制方式 | 音色供给方式 | 流式能力 & 延迟参考 | Windows 原生部署 | 开源协议 | 4.8 GiB 余量适配结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Qwen3-TTS-12Hz-0.6B-CustomVoice** [1][2][3] | 0.6B（28L Talker + 5L Code Predictor + 12.5 Hz 16 码本 ConvNet） | ~2.5 GB（BF16 safetensors） | **~2.6–3.3 GiB**（BF16 + `faster-qwen3-tts`）[4][5] | 自然语言 `instruct`（⚠️ 0.6B 遵循度弱于 1.7B，需实测核验）[2][6] | 内置 9 音色（含 **5 位中文/京/川音色**）[1] | 架构双轨流式理论 97 ms [3]；`faster-qwen3-tts` 4090 TTFA ~154 ms / 5.53× 实时 [5] | **优**（PyTorch SDPA + `faster-qwen3-tts` 原生支持）[5] | **Apache-2.0** [3] | **✅ 首选（Primary）** |
| **Qwen3-TTS-12Hz-1.7B-CustomVoice / VoiceDesign** [1][3] | 1.7B（同 12.5 Hz 双轨多码本架构） | ~3.9–4.5 GB | **~6.0–8.0 GiB**（BF16）[4][5] | **极强**自然语言 `instruct`；VoiceDesign 支持纯文本造声 [1] | 9 预置音色 / 纯文本造声 / 3s 克隆（Base）[1] | 同上；`faster-qwen3-tts` 4090 TTFA ~171 ms / 4.78× 实时 [5] | **优**（同上） | **Apache-2.0** [3] | **❌ 共驻超显存**（✅ 适合离线生成 5×6 参考音频素材） |
| **Fun-CosyVoice3-0.5B-2512** [7][8][9] | 0.5B Qwen2 LLM + DiT CFM Flow + CausalHiFT 24kHz | ~3.44 GB（FP32：`llm.pt` 2.02G + `flow.pt` 1.33G + `hift.pt` 83M）[9][10] | **~2.5–3.8 GiB**（`fp16=True`，不含 vLLM）[11] | **极强**：`<|endofprompt|>` 指令（情绪/18 方言）+ `[laughter]`/`[breath]`/`<strong>` [7][8] | 3–10s 零样本克隆 + `add_zero_shot_spk` 缓存（⚠️ `instruct2` 联用有坑）[12] | 原生双向流式，优化栈首包 ~150 ms [7][8] | **中（有坑）**：`ttsfrd` 与 `vllm` 仅 Linux；需 `pynini` + `WeTextProcessing` [13] | **Apache-2.0** [9] | **✅ 强情感备选（Runner-up）** |
| **GPT-SoVITS v2 / v2Pro** [14] | AR GPT + SoVITS（v2Pro 加 `eres2netv2` 声纹编码器；v4 改 CFM 48kHz） | ~1.5–2.5 GB | **~2.5–4.0 GiB**（FP16；v2 更轻，v2Pro 略高）[14] | **仅靠切换参考音频**（无自然语言 `instruct`） | 3–10s 零样本/少样本克隆（5×6 参考音频矩阵） | 支持分块流式输出，短句整句极快 | **极优**（最成熟 Windows 一键/便携生态）[14] | **MIT** [14] | **✅ 超轻保底（Fallback）** |
| **OpenAudio S1-mini (0.5B)** (`fish-speech`) [15][16] | 0.5B DualAR + VQ-GAN/DAC（S1 4B 蒸馏版） | ~1.8–2.5 GB | ~3.5–4.5 GiB（BF16/FP16）[15][16] | 丰富内联标记：`(angry)` `(laughing)` `(sighing)` `(whispering)` 等 [16] | 零样本参考音频克隆 | API 支持流式，但原生非 vLLM/SGLang 栈首包延迟偏高（Issue #1000）[17] | **良**（Python 3.11/3.12 + CUDA）[15] | **❌ CC-BY-NC-SA-4.0 / Research** [16] | **⚠️ 条件备选（非商用条款受限 + 首包延迟待优化）** |
| **IndexTTS-2 / IndexTTS-2.5** [18][19][20] | GPT + 25 Hz 语义码本 + Zipformer S2M（2.5）+ `w2v-bert-2.0` + `qwen0.6bemo4-merge` + BigVGAN-v2 | ~5–6 GB（含 `qwen0.6bemo4-merge` 1.21 GB）[19] | **~6.0–8.0 GiB 起步**（推荐 10–12 GiB）[18][19] | **业界标杆**：音色与情绪完全解耦（8 维情绪向量 / 情绪参考音 / 文本软指令）[18][20] | 零样本克隆 + 独立情绪源 | 2.5 版 RTF 提速 2.28× [20]；原生非低延迟流式（流式需 Linux `vllm-omni`）[21] | **良**（官方 `uv` + Python 3.10–3.11）[18] | **bilibili 自定义商用协议**（≤1 亿 MAU / 10 亿营收免费）[19] | **❌ 共驻超显存**（离线配音极佳，不适合 4.8 GiB 共驻） |
| **VoxCPM2 (2B) / VoxCPM-0.5B** [22][23] | 无分词器连续潜变量扩散自回归（2B / 0.5B） | 2B ~4+ GB；0.5B ~1.5 GB | **2B：8–24 GiB**；**0.5B：~5 GiB** [22][23] | 2B 支持自然语言 Voice Design 与风格引导；0.5B 侧重上下文韵律（缺显式情绪指令） | 零样本克隆 / 2B 纯文本造声 | `StreamingVAEDecoder` 原生流式；2B 4090 RTF ~0.3（vLLM ~0.13）[22] | **良** | **Apache-2.0** [22] | **❌ 2B 严重超显存；0.5B 紧贴红线且缺显式指令** |
| **Step-Audio-TTS / EditX** [24][25] | 音频多模态大模型 + 迭代编辑 | ~6+ GB | **~10–18 GiB**（BF16）[24] | 极细粒度情绪/副语言编辑与强度调节 [25] | 零样本克隆 + 二次编辑 | 偏重离线/重算力编辑 | 中 | **Apache-2.0** [24] | **❌ 严重超显存淘汰** |
| **FireRedTTS-2 / Spark-TTS / MegaTTS3** [26][27][28] | 12.5 Hz 双 Transformer / 0.5B BiCodec / 0.45B DiT+WaveVAE | ~1.5–3 GB | ~4–8 GiB / ~4 GiB / ~3–4 GiB | FireRed 侧重播客对话；Spark 仅粗粒度音高语速；MegaTTS3 缺细粒度情绪指令 | 零样本克隆（MegaTTS3 官方隐匿 WaveVAE 编码器）[28] | FireRed 流式友好；Spark 非双轨低延迟 | 一般（社区 Windows 运维薄弱） | **Apache-2.0** | **❌ 情感可控性或权重完整度不及首选/备选，淘汰** |

---

### 2.2 重点候选深度拆解

#### 1) Qwen3-TTS（`QwenLM/Qwen3-TTS`，`arXiv:2601.15621`）

- **开源矩阵与核心架构**：阿里 Qwen 团队开源了基于自研 **`Qwen3-TTS-Tokenizer-12Hz`**（12.5 Hz 超低帧率、16 层残差多码本 + 轻量因果 ConvNet 解码器，非 DiT 扩散结构）的完整家族，全系采用 **Apache-2.0** 协议 [1][3]：
  - **`Qwen3-TTS-12Hz-0.6B-CustomVoice`** / **`1.7B-CustomVoice`**：内置 9 位预置音色（其中 5 位为中文/中国方言音色：`Vivian`、`Serena`、`Uncle_Fu`、北京话 `Dylan`、成都四川话 `Eric`）[1]。
  - **`Qwen3-TTS-12Hz-0.6B-Base`** / **`1.7B-Base`**：专注 3 秒快速零样本声音克隆，提供 `create_voice_clone_prompt(ref_audio, ref_text, x_vector_only_mode=False)` 缓存复用的 prompt 对象，避免每次重复编码参考音频 [1]。
  - **`Qwen3-TTS-12Hz-1.7B-VoiceDesign`**：纯自然语言描述创造全新音色（仅 1.7B 提供）[1]。
  - **分层解码加速原理**：主干 `Talker`（28 层 Transformer）以 12.5 Hz 预测第 0 层语义主码本，轻量 `Code Predictor`（5 层 Transformer）自回归补齐其余 15 层声学残差码本，最后由因果 ConvNet 直接重建 24 kHz 波形，双轨流式架构下理论首包发射延迟可低至 **97 ms** [3]。
- **必须正视的 3 个一线工程坑位**：
  1. **官方 `qwen-tts` Python SDK 既非流式又慢**：上游 `QwenLM/Qwen3-TTS` 标准 Python 推理循环每步触发大量零碎 CUDA kernel 启动，且默认整句生成后才返回波形，实际首包/整句耗时高达数秒 [5]。**解法**：生产服务不直接用原生 `generate_custom_voice()` 裸循环，而是采用开源加速器 **`andimarafioti/faster-qwen3-tts`**（基于 PyTorch `StaticCache` + `torch.cuda.CUDAGraph` 整步捕获，附带原生 Windows 安装脚本 `setup_windows.bat`），在 RTX 4090 上将 `0.6B-CustomVoice` 推至 **5.53× 实时（标准 RTF ≈ 0.18）、TTFA ~154 ms**，`1.7B-CustomVoice` 推至 **4.78× 实时、TTFA ~171 ms**，且原生支持 chunk 级实时流式输出 [5]。社区亦存在 `qwentts.cpp` / GGML C++ 轻量后端与 `vLLM-Omni`（Linux 生产高并发服务）[5][29]。
  2. **`0.6B-CustomVoice` 的 `instruct` 遵循度弱于 `1.7B`**：尽管 `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice` HuggingFace 模型卡与 `qwen-tts` 代码示例均支持传入 `instruct="用特别愤怒的语气说"` [2]，但官方表格与 GitHub Issues（如 #14、#231 及 `qwen_tts 0.1.1` 相关反馈）显示：0.6B 模型对复杂情绪指令的响应力度明显弱于 1.7B，且部分第三方向低比特移植版直接裁剪了 0.6B 的 `instruct` 通路 [6]。同时，`Base` 克隆模型默认不支持 `instruct` 情绪叠加 [6]。**工程对策**：在 `BENDER` 真机验收中将「6 档情绪盲听区分度」设为强制卡点；一旦 0.6B-CustomVoice 对 `Mocking` / `Tilted` 表现平淡，即切换至 **CosyVoice 3.0** 或 **「离线 1.7B-VoiceDesign 造 5×6 情绪参考音 + 在线 0.6B-Base 预缓存克隆」**。
  3. **Windows 11 + RTX 3060（SM 8.6）的 Attention 选择**：RTX 3060 为 Ampere SM 8.6，硬件完整支持 `bfloat16` 与 FlashAttention-2；但 `flash-attn` 官方不发布 Windows 预编译 wheel，在 CUDA `13.5` + MSVC 环境下源码编译极易失败。务必显式配置 **`attn_implementation="sdpa"`**，直接调用 PyTorch 2.x 内置的 FlashAttention / Memory-Efficient SDPA CUDA 内核，在 Windows 上零编译即可获得近似加速且不踩 T2060 类老卡的 FP16 数值溢出坑（Ampere 直接用 `torch.bfloat16`）[1][6]。

#### 2) CosyVoice 3.0（`FunAudioLLM/Fun-CosyVoice3-0.5B-2512`，`arXiv:2505.17589`）

- **架构、体积与显存优势**：仓库位于 `FunAudioLLM/CosyVoice`（亦映射至 `QwenAudio/CosyVoice`），模型卡 `FunAudioLLM/Fun-CosyVoice3-0.5B-2512`，全系 **Apache-2.0** 协议 [7][8][9]。核心权重拆分为：
  - `llm.pt`（~2.02 GB FP32，基于 Qwen2-0.5B 的 `CosyVoice3LM`，另附强化学习微调版 **`llm.rl.pt`** 可直接替换以提升表现力与稳定性）[9][10]；
  - `flow.pt`（~1.33 GB FP32，条件流匹配 DiT 声学解码器）[9][10]；
  - `hift.pt`（~83.2 MB，24 kHz 因果 HiFT 神经声码器）[9][10]；
  - 前端辅助模型 `campplus.onnx`（声纹提取）与 `speech_tokenizer_v3.onnx`（语音分词）[9]。
  - **显存核算**：FP32 总权重 ~3.44 GB，显存需 6–8 GiB；在初始化时开启 **`fp16=True`（且不开启仅限 Linux 的 `load_vllm=True`）**，LLM 与 Flow 权重减半至 ~1.7 GB，实测常驻与推理峰值显存收敛在 **~2.5–3.8 GiB** [11]，正好落在 `BENDER` 的 4.8 GiB 余量内！
- **最契合毒舌牌桌的情绪与副语言能力**：
  - CosyVoice 3 经 100 万小时多语种数据与后训练强化，原生支持 **9 种语言 + 18 种中国方言**（川渝、粤语、东北、上海、天津等），且文本规范化内生能力显著增强 [7][8]。
  - 通过 `inference_instruct2()` 传入格式 `You are a helpful assistant. <中文情绪/方言指令><|endofprompt|>` 即可在保持克隆音色不变的前提下强力切换情绪 [11]，同时原生支持在待合成台词中直接嵌入 **`[laughter]`（笑声）、`[breath]`（叹气/吸气）、`<strong>词</strong>`（重音强调）** [7][30] —— 这对「哈哈哈、啧、就你这牌也敢碰？」类毒舌台词简直是降维打击！
  - 原生支持**文本入、音频出的双向流式（Bi-Streaming）**，优化栈首包延迟低至 ~150 ms [7][8]。
- **原生 Windows 部署的两大深坑与绕过策略**：
  1. **前端依赖坑（`ttsfrd` / `pynini` / `vllm`）**：
     - 阿里官方高性能前端 `CosyVoice-ttsfrd` 仅提供 **`cp310-linux_x86_64`** Linux wheel，Windows 无法安装 [13]；所幸 CosyVoice 3 内置自动回退至开源 `WeTextProcessing`（`wetext`），且 3.0 自身具备较强的免前端文本归一化能力 [7][13]。
     - `WeTextProcessing` 强依赖 OpenFST 封装 **`pynini`**，PyPI 无官方 Windows wheel [13]。**Windows 实操方案**：通过独立的便携 Miniforge/Micromamba（Python 3.10，`conda install -c conda-forge pynini==2.1.5`）或社区编译的 Windows `pynini` cp310 wheel 安装，避免污染系统环境 [13]。
     - `load_vllm=True` 在原生 Windows 不可用（vLLM 官方仅支持 Linux CUDA），Windows 原生路线使用 PyTorch `fp16=True`（后续可选配 TensorRT 引擎加速 `flow.pt`）。
  2. **`inference_instruct2` + `zero_shot_spk_id` 上游 Bug**：
     - 上游 `CosyVoice` GitHub Issue 记录：当向 `inference_instruct2` 同时传入已缓存的 `zero_shot_spk_id` 时，`frontend_instruct2` 内部可能错误混入或被该 speaker 原先绑定的 `prompt_text` 覆盖，导致情绪指令失效或语音串音 [12]。
     - **工程修补方案**：在我们的 FastAPI 服务端初始化阶段，为 5 位固定角色分别预提取并驻留内存字典 `SPK_CACHE[persona_id] = { llm_embedding, flow_embedding, flow_prompt_speech_token, prompt_speech_feat }`（仅约数 MB），每次推理将 `prompt_text` 动态拼接为 `f"You are a helpful assistant. {mood_instruct}<|endofprompt|>"` 注入 LLM 前端，彻底绕过 `zero_shot_spk_id` 的文本覆盖缺陷，且省去每句重复运行 `campplus.onnx` 与 `speech_tokenizer_v3.onnx` 的耗时。

#### 3) 其他 2025–2026 候选的真实定位与淘汰依据

- **`GPT-SoVITS v2 / v2Pro`（`RVC-Boss/GPT-SoVITS`，MIT）— 保底方案**：
  - FP16 推理显存约 **2.5–4.0 GiB**（v2 极轻，v2Pro 引入 `eres2netv2` 提升音色还原但略增显存；v4 改为 CFM + 48 kHz HiFiGAN 显存与耗时均上升，不推荐用于紧显存低延迟场景）[14]。Windows 一键/绿色生态最成熟、MIT 协议零顾虑。
  - 短板是**不支持自然语言 `instruct` 情绪控制**，必须通过切换情绪参考音频控制语气，但对于 Majjj 仅有的 **5×6 = 30 种固定组合**，预制 30 条情绪参考音频反而是最稳妥、绝不翻车的工程兜底。
- **`OpenAudio S1-mini (0.5B)`（`fishaudio/fish-speech`）— 许可证受限**：
  - 0.5B 蒸馏模型在显存（~3.5–4.5 GiB）与内联情绪标签（`(angry)`、`(laughing)`、`(sighing)`、`(whispering)`、`(amused)`）上颇具吸引力 [15][16]；其 2026 旗舰 `S2-Pro`（4B）则需 12–24 GiB 显存（4-bit 量化仍超预算）直接出局 [15]。
  - 未列为首选的两大硬伤：① **权重采用非商业许可（CC-BY-NC-SA-4.0 / Fish Audio Research License）**，相比 Apache-2.0 的 Qwen3-TTS 与 CosyVoice 3 存在不必要的合规包袱 [16]；② 社区反馈（GitHub Issue #1000）在无 Linux vLLM/SGLang 加速时流式首包延迟偏大 [17]。
- **`IndexTTS-2` & `IndexTTS-2.5`（`index-tts/index-tts`）— 强情感标杆但共驻超显存**：
  - Bilibili IndexTeam 出品，**情感与音色解耦能力堪称 2025–2026 开源中文标杆**（支持 8 维情感向量、独立情感参考音频以及内置 `qwen0.6bemo4-merge` 文本到情感模块）；2026 年升级的 **IndexTTS-2.5** 将语义码本帧率从 50 Hz 压至 25 Hz 并以 Zipformer 替换 U-DiT S2M，官方报告 **RTF 提速 2.28×** [18][19][20]。
  - **为何淘汰（共驻场景）**：完整推理管线串联 GPT、S2M、w2v-bert-2.0、BigVGAN-v2 以及 `qwen0.6bemo4-merge`（单情绪模块即占 1.21 GB 磁盘），官方与实测均指出 **FP16/BF16 最低需 6–8 GiB 显存、流畅运行推荐 10–12 GiB** [18][19]；在 `BENDER` 仅剩 4.8 GiB 的余量下必然溢出至系统内存。此外其流式服务依赖 Linux `vllm-omni` [21]，且权重为自定义 `bilibili Model Use License Agreement` [19]。*（注：若未来离线批量制作角色固定快捷语音包，IndexTTS-2.5 是极佳的离线工具。）*
- **`VoxCPM` 系列（`OpenBMB/VoxCPM`，Apache-2.0）**：
  - 旗舰 **VoxCPM2（2B）** 采用无分词器连续潜变量扩散自回归架构，支持自然语言 Voice Design、风格引导克隆与 `StreamingVAEDecoder` 流式，但显存门槛高达 **8–24 GiB** [22][23]，彻底超出 4.8 GiB 预算；早期 **VoxCPM-0.5B** 仍需约 **~5 GiB** 显存且缺少 VoxCPM2 引入的显式情绪/风格指令控制 [23]，故不推荐。
- **其余淘汰模型速查**：
  - **`Step-Audio-TTS / Step-Audio-EditX`**（Apache-2.0）：情绪与副语言迭代编辑极强，但 BF16 推理需 **10–18 GiB** 显存，严重超预算 [24][25]。
  - **`FireRedTTS-2 / FireRedTTS3`**（Apache-2.0）：偏重长对话播客双人交替，显存 4–8 GiB 且 Windows 原生部署资料稀缺 [26]。
  - **`Spark-TTS`**（0.5B Qwen2.5 + BiCodec，Apache-2.0）：显存约 4 GiB 勉强塞入，但可控维度停留在性别/音高/语速数值滑块，缺少针对中文毒舌台词的细粒度情绪指令（如阴阳怪气、破防怒吼、冷淡吐槽）[27]。
  - **`MegaTTS3`**（0.45B DiT + WaveVAE，Apache-2.0）：体量极小且 8 步 PeRFlow 推理极快，但字节跳动官方出于安全考量**未公开发布 WaveVAE 编码器**（仅靠预提取 `.npy` 潜变量或非官方社区权重绕过），且缺乏细粒度中文情绪指令控制 [28]。

---

## 3. RTX 3060 12GB（~4.8 GiB 余量）+ 16 GB RAM 共存与 Windows 部署架构

### 3.1 精确显存与系统内存账本

在 `BENDER`（12,288 MiB 物理显存 + 16,384 MiB 物理内存）上，各方案与常驻 `llama-server` 共存的账本核算如下：

| 占用项 / 候选方案 | GPU 显存占用（MiB） | 与 LLM 共存后总显存（MiB / 12,288） | 剩余安全余量（MiB） | 宿主系统 RAM 增量 | 显存共存等级 |
| --- | --- | --- | --- | --- | --- |
| **固定基线**：Windows 11 DWM 桌面 | `1,046` | — | — | ~4.5–5.5 GB（OS） | 不可回收 |
| **固定基线**：`llama-server`（Qwen3.5-9B Q5_K_M, `--parallel 3 -c 12288` 峰值） | `6,191` | **`7,237`** | `5,051` | ~1.0–1.5 GB | 已实测冻结 |
| **方案 A（Primary）**：`Qwen3-TTS-12Hz-0.6B-CustomVoice`（BF16 + `faster-qwen3-tts`）<br>*(权重 ~2,000 + CUDA 上下文 ~400 + StaticCache/Graph/激活 ~600)* | **`~2,600–3,300`** | **`9,837–10,537`** | **`1,751–2,451`** | ~1.2–1.8 GB（原生 Python） | **🟢 原生 BF16 稳跑** |
| **方案 B（Runner-up）**：`Fun-CosyVoice3-0.5B-2512`（FP16，无 vLLM）<br>*(LLM+Flow FP16 ~1,750 + HiFT/ONNX ~250 + CUDA 上下文与激活 ~900–1,600)* | **`~2,500–3,800`** | **`9,737–11,037`** | **`1,251–2,551`** | ~1.5–2.2 GB（原生 Python） | **🟢 原生 FP16 可稳跑（需限并发=1）** |
| **方案 C（Fallback）**：`GPT-SoVITS v2 / v2Pro`（FP16） | **`~2,500–4,000`** | **`9,737–11,237`** | **`1,051–2,551`** | ~1.5–2.0 GB（原生 Python） | **🟢 原生 FP16 稳跑（v2 更宽裕）** |
| **风险方案**：`Qwen3-TTS-1.7B`（社区 INT4/GGUF 量化） | `~2,500–3,500` | `~9,737–10,737` | `~1,551–2,551` | ~1.5–2.0 GB | **🟡 必须量化**（社区量化易损音质或裁剪 `instruct`） |
| **超标方案**：`Qwen3-TTS-1.7B` BF16（~6,100） / `IndexTTS-2.5` BF16（~6,500+） / `VoxCPM2`（~8,000+） | **`6,100–10,000+`** | **`13,337+`（溢出！）** | **`< 0`** | 触发 WDDM 共享显存吞噬 RAM | **🔴 必爆或触发 WDDM 降速 5–10×** |

### 3.2 防范 Windows WDDM Shared GPU Memory 溢出与双进程显存争抢的 5 项硬护栏

Windows WDDM 驱动在物理显存耗尽时，默认将 CUDA 内存透明迁移至「共享 GPU 内存（系统 RAM）」。在 16 GB RAM 的 `BENDER` 上，一旦发生溢出不仅推理耗时从 0.8 s 暴跌至 6–10 s，还会连带拖慢 `llama-server` [31]。必须在工程上部署以下 5 道硬护栏：

1. **驱动层禁止静默溢出（Fail-Fast 原则）**：
   - 在 `BENDER` 打开 **NVIDIA Control Panel → 管理 3D 设置 → 程序设置**，选中 TTS 专用解释器 `E:\majjj-tts\.venv\Scripts\python.exe`，将 **`CUDA - Sysmem Fallback Policy`（CUDA - 系统内存回退策略）** 设为 **`Prefer No Sysmem Fallback`**（NVIDIA R536.40+ 驱动提供，`BENDER` 驱动为 `621.40`，完全支持）[31]。
   - 效果：调参或压测一旦超额立即抛出标准 `torch.OutOfMemoryError` 暴露问题，绝不让服务在不知情下掉入 PCIe 系统内存泥潭。
2. **PyTorch 进程级显存硬封顶**：
   - TTS 进程启动加载模型前立即执行：
     ```python
     import torch
     # 12,288 MiB * 0.30 = 3,686 MiB 硬上限，物理隔离保护 llama-server
     torch.cuda.set_per_process_memory_fraction(0.30, device=0)
     ```
   - 依据 PyTorch 官方文档 [32]，这能从 CUDA Caching Allocator 层面硬性阻止 TTS 进程触碰 `llama-server` 的 7.24 GiB 领地。
3. **Windows 专用 PyTorch Allocator 调优（避开 `expandable_segments` 坑）**：
   - 注意：Linux 常用的 `PYTORCH_CUDA_ALLOC_CONF=expandable_segments:True` 在多个 Windows WDDM PyTorch 构建中不受支持或会触发警告并静默回退 [32]。
   - Windows 11 下应使用可靠的参数组合：
     ```powershell
     $env:PYTORCH_CUDA_ALLOC_CONF = "max_split_size_mb:128,garbage_collection_threshold:0.8"
     ```
   - 配合 `faster-qwen3-tts` 的固定尺寸 `StaticCache`（显存一次性分配、零动态重分配碎片），可彻底消除长时间运行后的显存碎片爬升 [5]。
4. **单并发串行 GPU 队列 + `max_new_tokens` 硬限 + 过期代数丢弃**：
   - `llama-server` 可同时吐出 3 位牌友的结算互喷台词，但 TTS 服务端**严禁多线程并发打 GPU**，必须以 `asyncio.Semaphore(1)` 严格串行合成（`batch_size=1`），避免激活与 KV Cache 峰值翻倍。
   - 鉴于 Majjj 台词上限 26 字（约 3.5–4.5 秒），将 Qwen3-TTS（12.5 Hz）的 `max_new_tokens` 封顶在 **`120` 帧（≈ 9.6 秒）**，防止偶发幻觉长停顿吃满 KV Cache。
   - 结合 ADR-0010 的座次单调代数守卫，请求体携带 `generation_id` 与 `deadline_ms`：排队中若已超期或被同一座次新事件取代，**在进入 GPU 前直接丢弃**，杜绝积压雪崩。
5. **常驻优先，避免频繁冷启动抖动**：
   - 当 TTS 峰值严格锁死在 `≤ 3.3 GiB` 时，**保持常驻**远优于「按需加载/卸载」：模型磁盘冷加载（2.5 GB safetensors + CUDA Graph 首次捕获）耗时约 6–15 秒，会彻底毁掉对局交互感；仅在需要离线跑 `1.7B-VoiceDesign` 生成素材时临时停掉 `llama-server` 与在线 TTS。

### 3.3 Windows 11 局域网部署栈选型：原生绿色 Python vs C++/ONNX vs WSL2/Docker

| 部署栈 | 16 GB RAM 宿主机开销 | 与 `E:\majjj-llm` 体系一致性 | 生态成熟度（Qwen3-TTS / CosyVoice3） | 结论 |
| --- | --- | --- | --- | --- |
| **原生 Windows + 独立 `uv` 绿色目录 (`E:\majjj-tts\`) + FastAPI/uvicorn + PowerShell 7** | **低（~1.2–2.0 GB RAM）** | **极高**：完全复用 `deploy/start/stop/setup-firewall/install-autostart/smoke-test.ps1` 与 `Win32_Process.Create` 模式，不污染系统盘与全局 Python | **极高**：`faster-qwen3-tts` 原生提供 Windows 支持；CosyVoice3 可用独立便携 Miniforge Python 3.10 装 `pynini` | **✅ 强烈推荐** |
| **纯 C++ / GGML / ONNX 运行时**（如 `qwentts.cpp` / ONNX） | **极低（~0.5–1.0 GB RAM）** | 高（单 exe + dll，类似 `llama-server.exe`） | **偏低/不成熟**：截至 2026 年底，Qwen3-TTS C++/GGML 生态仍处社区早期，对 `CustomVoice` + `instruct` 与流式支持不如 PyTorch `faster-qwen3-tts` 完整 [5][6] | **🟡 持续观望（v2 降本备选）** |
| **WSL2 + Docker Desktop（vLLM-Omni）** | **极高（WSL2 默认划走 50% = 8 GB RAM [33] + Docker 开销）** | **差**：引入 Hyper-V/WSL2 虚拟网卡端口转发、系统盘 VHDX 膨胀与开机自启复杂度 | 虽可解锁 `vLLM-Omni` / `ttsfrd`，但在仅 1 路并发的家庭 4 人桌场景下大炮打蚊子 | **❌ 明确否决**（在 16 GB RAM 机器上得不偿失） |

推荐 `E:\majjj-tts\` 绿色目录结构（镜像现有 `E:\majjj-llm\`）：
```text
E:\majjj-tts\
├── .venv\                      # uv 托管独立 Python 3.11 + PyTorch CUDA + faster-qwen3-tts + FastAPI
├── models\                     # 本地固定快照权重（禁止运行时隐式连网拉 HF）
│   └── Qwen3-TTS-12Hz-0.6B-CustomVoice\
├── voices\                     # （若走克隆路线）预生成的 5×6 参考音频与预计算 prompt cache (.pt)
├── server\
│   ├── main.py                 # FastAPI + Semaphore(1) + 显存上限 + /v1/tts/synthesize + /health
│   └── personas.json           # 5 PersonaId × 6 PersonaMood → speaker / instruct 映射表
├── logs\                       # tts-server.log / tts-server.pid
├── deploy-tts-server.ps1       # 幂等绿色部署（装 uv、拉模型、校验）
├── start-tts-server.ps1        # Win32_Process.Create 后台常驻（端口 8081）
├── stop-tts-server.ps1
├── setup-firewall.ps1          # 唯一规则 majjj-tts-lan-8081，仅放行 192.168.5.0/24
└── smoke-test-tts.ps1          # 5×6 音色情绪 + 与 llama-server 双开压测脚本
```

---

## 4. Majjj 5×6 `牌友角色`情感落地、接口契约与 Safari 音频链路

### 4.1 5 位`牌友角色` × 6 档`牌友情绪` 精准映射方案

#### 方案一：首选在线直驱（`Qwen3-TTS-12Hz-0.6B-CustomVoice`）

`Qwen3-TTS-12Hz-0.6B-CustomVoice` 内置的 5 位中文/方言预置音色与 Majjj 5 位`牌友角色`天然形成极高辨识度的戏剧组合 [1]：

| Majjj `PersonaId` | 对应 Qwen3-TTS 内置 Speaker | 音色特质（官方定义）[1] | 角色贴合度说明 |
| --- | --- | --- | --- |
| **急眼快攻哥** | **`Eric`** | Lively Chengdu male voice, slightly husky brightness（活泼微哑川渝男声） | 川渝麻将文化拉满，语速偏快、急躁催牌味极正 |
| **阴阳茶艺师** | **`Serena`** *(或 `Vivian`)* | Warm, gentle young female voice（温婉女声；`Vivian` 更锐利带刺） | 温柔客气腔配合刻薄台词形成极强反差「茶艺感」 |
| **嘴硬死磕王** | **`Dylan`** | Youthful Beijing male voice, clear natural timbre（京片子青年男声） | 京腔自带贫嘴、嘴硬不服输的胡同牌桌味 |
| **拱火乐子人** | **`Vivian`** *(或 `Dylan` 调高语速)* | Bright, slightly edgy young female voice（明亮带锋芒年轻女声） | 清脆跳脱、自带看热闹不嫌事大的拱火挑事感 |
| **红温老雀神** | **`Uncle_Fu`** | Seasoned male voice, low mellow timbre（沉稳低沉老成男声） | 平时端着老江湖架子，一旦 `Tilted` 红温破防反差爆炸 |

6 档 `PersonaMood` 到中文 `instruct` 映射模板（同时兼容 Qwen3-TTS `instruct` 与 CosyVoice 3 `You are a helpful assistant. <指令><|endofprompt|>`）：

| `PersonaMood` | 中文情绪锚点 | 推荐 `instruct` 指令文本 | CosyVoice 3 可选内联副语言增强（由服务端按需注入） |
| --- | --- | --- | --- |
| `Smug` | 得意欠揍 | `用得意洋洋、挑衅欠揍、带着轻笑的语气说，语速稍快。` | 句首/句中可自然保留 `[laughter]` |
| `Mocking` | 阴阳嘲讽 | `用阴阳怪气、假装客气实则拉长尾音讽刺挖苦的语气说。` | 关键词包 `<strong>...</strong>` |
| `Tilted` | 红温破防 | `用气急败坏、情绪激动、音量提高且语速急促的愤怒语气说。` | 句首加 `[breath]` 表现气急喘气 |
| `Nervous` | 紧张心虚 | `用紧张心虚、略带犹豫磕绊、声音压低且不自信的语气说。` | 句首加 `[breath]` |
| `Plotting` | 阴险憋大招 | `用压低声音、阴险狡黠、慢条斯理藏着算计的神秘语气说。` | 慢速低沉重读关键词 |
| `Deadpan` | 面无表情冷吐槽 | `用面无表情、语调平淡毫无起伏、冷漠敷衍的吐槽语气说。` | 零笑声、干脆收尾 |

#### 方案二：离线「1.7B VoiceDesign 铸模」+ 在线零样本预缓存（备选/保底）

若在 `BENDER` 实测中发现 `0.6B-CustomVoice` 对 6 档 `instruct` 区分度不够鲜明，采用**零在线显存增量的混合管线**：
1. **离线制作期（仅一次，暂停 `llama-server` 释放全部 12 GiB 显存）**：
   - 运行 `Qwen3-TTS-12Hz-1.7B-VoiceDesign`（或 `1.7B-CustomVoice`），为每位角色先设计出理想基准声线，再批量生成 **5 角色 × 6 情绪 = 30 条 4–5 秒高质量情绪参考音频**（`voices/<persona>_<mood>.wav` + 对应文本）。
2. **在线服务期（与 `llama-server` 共驻）**：
   - **路线 2a（CosyVoice 3.0 FP16）**：仅需 5 条角色中性基准音频（预缓存声纹/flow 特征），每句按 `PersonaMood` 动态下发 `<|endofprompt|>` 情绪指令；
   - **路线 2b（Qwen3-TTS-0.6B-Base 或 GPT-SoVITS v2）**：启动时一次性把 30 条 `(persona, mood)` 参考音频预计算为内存 `voice_clone_prompt`（`create_voice_clone_prompt(..., x_vector_only_mode=False)` [1]），请求到达时 `O(1)` 查表命中情绪+音色绑定缓存，**0 ms 编码耗时、100% 情绪稳定**！

### 4.2 局域网 HTTP / Streaming API 契约

在 `BENDER`（`192.168.5.50:8081`）上，建议以**专用领域端点为主、OpenAI 兼容端点为辅**：

- **`GET /health`**：返回 `{ "status": "ok", "model": "...", "queue_depth": 0, "vram_allocated_mib": 2410, "vram_reserved_mib": 2860, "personas_cached": 5 }`，供 Mac `mahjong-server` 启动检查与健康探活。
- **`POST /v1/tts/synthesize`**（Majjj 专用主契约，避免把情绪指令硬塞进 OpenAI `voice` 字段）：
  ```json
  {
    "request_id": "seat2-gen41",
    "persona_id": "quick_rush",
    "mood": "Tilted",
    "text": "啧，你这也能碰？手气全让你一个人摸光了吧！",
    "instruct_override": null,
    "format": "wav",
    "stream": false,
    "deadline_ms": 1800
  }
  ```
  - 当 `stream: false`：返回 `Content-Type: audio/wav`（24 kHz, 16-bit signed PCM, mono），响应头附带 `X-TTS-Duration-Ms`、`X-TTS-Synth-Ms`、`X-TTS-Queue-Wait-Ms`。
  - 当 `stream: true`（v1.5 升级口）：返回 `Content-Type: application/octet-stream`（`X-Audio-Sample-Rate: 24000`, `X-Audio-Channels: 1`, `X-Audio-Format: pcm_s16le`），以 HTTP Chunked Transfer 逐块推送 160–240 ms 原始 PCM 帧。
- **可选 `POST /v1/audio/speech`**：标准 OpenAI TTS 兼容层，便于运维期用 `curl` 快速冒烟。

### 4.3 Mac Safari 与 iPhone Safari 跨端音频播放链路

结合本仓库已验证的 Safari 音频架构（`origin/research/comic-2d-visual-audio:docs/research/comic-2d-visual-audio.md` §3）与 MDN / WebKit 官方规范 [34][35][36]：

1. **为什么 v1 短句首选「24 kHz 16-bit Mono WAV（整包返回）」而非 Opus / MP3 / AAC？**
   - **极小体积与零转码耗时**：Majjj 台词为 10–26 字（约 2–4 秒），24 kHz 16-bit 单声道 WAV 仅 **`96–192 KB`**，家庭局域网内传输耗时 **< 5 ms**，且 Windows 端与 Mac `mahjong-server` 均**无需安装 FFmpeg 或运行压缩编码器**（省去 20–50 ms 编码耗时与依赖维护）。
   - **Safari 100% 零坑解码**：MDN 与 WebKit 行为表明，桌面及 iOS Safari 的 `AudioContext.decodeAudioData()` 对完整 **WAV** 与 **MP3** 支持最稳健，而 Ogg/Opus 容器在 iOS Safari 历史上存在长期兼容性限制 [34]；WAV 无有损编码的 encoder delay/padding，150 KB 内存解码仅约 1–2 ms。
   - **无缝复用既有 Web Audio 总线**：解码出的 `AudioBuffer` 直接通过 `AudioBufferSourceNode` 接入已设计的 `voGain → DynamicsCompressorNode → destination` 链路，天然享有：① 同座新台词 `oldSource.stop()` 互斥打断；② `sfxGain` 瞬时自动闪避（Ducking）；③ iOS Safari `AudioContext` `"interrupted"` 状态恢复与 `navigator.audioSession` 管理。
2. **为什么 v1.5 真流式必须走「Raw Int16 PCM + `AudioWorkletNode`」而非 MSE / `decodeAudioData` 分片？**
   - **`decodeAudioData()` 不支持任意分片流**：MDN 明确指出 `decodeAudioData()` 需要完整文件或严格帧对齐数据，传入不完整 WAV/压缩分片会被 WebKit 拒绝（报错 `EncodingError` 或 `null`）[34]。
   - **iOS Safari 缺乏标准音频 MSE**：iOS 17.1+ 虽引入 `ManagedMediaSource`，但需绑定 `<video>`/`<audio>` 媒体元素且强制要求处理 AirPlay/禁用 `disableRemotePlayback` [35]，无法像 Web Audio 那样做毫秒级低延迟并发混音与 SFX Ducking。
   - **正确流式方案**：服务端直吐 24 kHz `pcm_s16le` 二进制流，浏览器端通过 `fetch().body.getReader()` 读取分块，转换为 `Float32Array` 写入 **`AudioWorkletNode`**（Safari 14.1+ 全平台支持 [36]）环形缓冲区播放，零容器封装、零解码延迟、直接进入 `voGain` 混音总线！

---

## 5. 最终选型结论与 `BENDER` 真机实测验收清单

### 5.1 三梯队落地决策树

```text
                       [上线首轮 BENDER 真机实测]
                                   │
         ┌─────────────────────────┴─────────────────────────┐
         ▼                                                   ▼
  Primary：Qwen3-TTS-12Hz-0.6B-CustomVoice            若 0.6B instruct 盲听
  + faster-qwen3-tts (BF16 + SDPA + CUDA Graphs)      6 档情绪区分度不足
         │                                                   │
         │ 实测通过（峰值 ≤ 3.5 GiB，TTFA ≤ 450 ms，          ▼
         │ 整句 ≤ 1.2 s，6 档情绪清晰可辨）          Runner-up：Fun-CosyVoice3-0.5B-2512
         ▼                                            (PyTorch FP16 + 5 角色内存缓存 +
    直接冻结为 v1 生产引擎                             <|endofprompt|> 情绪指令 + 副语言标签)
                                                             │
                                                             │ 若 Windows pynini 依赖或
                                                             │ 峰值显存波动仍不满意
                                                             ▼
                                                  Fallback：离线 1.7B-VoiceDesign 生成
                                                  5×6=30 条高张力情绪参考音频
                                                  + 在线 Qwen3-TTS-0.6B-Base（或 GPT-SoVITS v2）
                                                  预缓存零样本直出（100% 确定性）
```

### 5.2 `BENDER` 真机实测验收清单（Smoke Test Checklist）

在正式为 `mahjong-server` 编写 TTS 客户端集成代码前，必须在 **`BENDER`（`192.168.5.50`，RTX 3060 12GB + 16 GB RAM）** 与 Mac/iPhone 实机上依次通过以下 **6 项硬指标验收**：

- [ ] **验收项 1：环境隔离与驱动防溢出配置**
  - 确认 `E:\majjj-tts\` 独立目录运行，未修改系统全局 Python；
  - 确认 NVIDIA Control Panel 已对 `E:\majjj-tts\.venv\Scripts\python.exe` 设置 `CUDA - Sysmem Fallback Policy = Prefer No Sysmem Fallback`；
  - 确认防火墙仅放行 `192.168.5.0/24` 到 `TCP 8081`（`majjj-tts-lan-8081`），SSH 断开后 `curl http://192.168.5.50:8081/health` 持续 `200 OK`。
- [ ] **验收项 2：双服务共跑显存与系统内存峰值（核心生死线）**
  - 保持 `E:\majjj-llm` `llama-server`（`--parallel 3 -c 12288`）运行，同时启动 TTS 服务并完成 CUDA Graph 热身；
  - 记录 `nvidia-smi` 指标：
    - `llama-server` + TTS 常驻总显存 **`≤ 10,800 MiB / 12,288 MiB`**；
    - TTS 单进程显存峰值 **`≤ 3,500 MiB`**（且 `torch.cuda.set_per_process_memory_fraction(0.30)` 生效无 OOM）；
    - Windows 任务管理器 **Shared GPU Memory（共享 GPU 内存）用量在压测期间增量为 `0 MB`**；
    - 物理系统内存（16 GB RAM）可用余量 **`≥ 3.0 GB`**，无磁盘页面文件剧烈换页。
- [ ] **验收项 3：短句合成时延（15 字 / 26 字中文麻将台词）**
  - 热身 3 次后，从 Mac 跨局域网连续请求 20 条 10–26 字真实毒舌台词：
    - 非流式整句端到端（含局域网 WAV 传输）：avg **`≤ 1,000 ms`**，p95 **`≤ 1,350 ms`**（等效标准 RTF `≤ 0.40`）；
    - 若启用流式模式：TTFA（首包 PCM 到达 Mac）avg **`≤ 350 ms`**，p95 **`≤ 500 ms`**。
- [ ] **验收项 4：5 位`牌友角色` × 6 档`牌友情绪` 盲听验收（决定 Primary 是否留任）**
  - 同一句测试台词（如「行啊，这手牌打得真有你的！」）分别用 `Smug` / `Mocking` / `Tilted` / `Nervous` / `Plotting` / `Deadpan` 6 档情绪合成；
  - 人工盲听确认：① 5 位角色音色一眼/一耳区分且全程无串音；② `Tilted`（高亢急促）、`Mocking`（阴阳拖音）、`Deadpan`（平淡冷漠）、`Plotting`（低沉压声）之间**存在明显听感差异**；③ 无吞字、无尾部鬼畜杂音、常见麻将术语（如「三万、九筒、红中、白板、抢杠胡、点炮、自摸」）发音准确。
  - *判定线*：若 `Qwen3-TTS-0.6B-CustomVoice` 的 `instruct` 盲听区分度不及格，立即按 §5.1 切换验证 `Fun-CosyVoice3-0.5B-2512`（FP16）或「离线 1.7B 铸造 5×6 参考音 + 0.6B-Base 缓存」。
- [ ] **验收项 5：3 路 LLM 并发 + TTS 连续混合压测零 OOM**
  - 在 Mac 端并行触发 `smoke-test.ps1`（3 路并发 LLM 生成）同时向 `8081` 连续投递 30 条 TTS 请求（模拟连续`小局`结算 3 位牌友接连互喷）；
  - 要求：**0 例 CUDA OOM**、**0 例进程崩溃**、`llama-server` 3 路并发耗时退化 `< 15%`（证明无 GPU 显存抖动与 PCIe 回退）。
- [ ] **验收项 6：Mac Safari & iPhone Safari 真机播放链路验证**
  - 桌面 Safari 与 iPhone Safari（含静音拨片开启/关闭场景）通过 `AudioContext.decodeAudioData()` 播放 `24 kHz 16-bit Mono WAV`；
  - 验证快速连续触发同座新台词时旧语音立即干净截断无爆音、播放期间牌桌拍牌 SFX 自动 Ducking 且结束后恢复、锁屏切回后 `AudioContext` 可自动恢复播放。

---

## 6. 完整一手参考文献列表（Primary Sources）

### Qwen3-TTS 与加速生态
- `[1]` Qwen Team — **Qwen3-TTS Official GitHub Repository**（模型矩阵、9 位 CustomVoice 预置音色、`create_voice_clone_prompt` 与 `qwen-tts` SDK）: https://github.com/QwenLM/Qwen3-TTS
- `[2]` Qwen Team — **Qwen3-TTS-12Hz-0.6B-CustomVoice HuggingFace Model Card**（`instruct` 用法、权重规格与 Apache-2.0 协议）: https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice
- `[3]` Qwen Team — **Qwen3-TTS Technical Report**（12.5 Hz 16 码本多码本 Tokenizer、Talker + Code Predictor、双轨流式 97 ms 首包延迟、Apache-2.0）: https://arxiv.org/abs/2601.15621
- `[4]` Qwen Team — **Qwen3-TTS Official HuggingFace Collection**（`0.6B` / `1.7B` Base、CustomVoice、VoiceDesign 与 `Qwen3-TTS-Tokenizer-12Hz`）: https://huggingface.co/collections/Qwen/qwen3-tts
- `[5]` Andres Marafioti — **`faster-qwen3-tts` Official GitHub Repository**（PyTorch `StaticCache` + CUDA Graphs 加速、RTF/TTFA 基准、流式支持、`setup_windows.bat` 与 `qwentts.cpp`/GGML 后端）: https://github.com/andimarafioti/faster-qwen3-tts
- `[6]` QwenLM/Qwen3-TTS GitHub Issues（`0.6B-CustomVoice` `instruct` 遵循度、Base 与 instruct 组合讨论 #14/#231、旧卡 FP16 数值问题 #43）: https://github.com/QwenLM/Qwen3-TTS/issues
- `[29]` vLLM Project — **vLLM-Omni Official Documentation**（Qwen3-TTS 与多模态语音服务端集成）: https://docs.vllm.ai/projects/vllm-omni/

### CosyVoice 3.0（FunAudioLLM）
- `[7]` FunAudioLLM / QwenAudio — **CosyVoice Official GitHub Repository**（架构、双向流式、18 方言、`[laughter]`/`[breath]`/`<strong>` 副语言标签、Apache-2.0）: https://github.com/FunAudioLLM/CosyVoice
- `[8]` Du et al. (FunAudioLLM) — **CosyVoice 3: Towards In-the-wild Speech Generation via Scaling-up and Post-training**（论文原文）: https://arxiv.org/abs/2505.17589
- `[9]` FunAudioLLM — **`Fun-CosyVoice3-0.5B-2512` HuggingFace Model Card & Files**（`llm.pt`、`llm.rl.pt`、`flow.pt`、`hift.pt`、`campplus.onnx`、`speech_tokenizer_v3.onnx`、Apache-2.0）: https://huggingface.co/FunAudioLLM/Fun-CosyVoice3-0.5B-2512
- `[10]` Fun-CosyVoice3-0.5B-2512 权重文件清单核对（`llm.pt` ~2.02 GB、`flow.pt` ~1.33 GB、`hift.pt` ~83.2 MB）: https://huggingface.co/FunAudioLLM/Fun-CosyVoice3-0.5B-2512/tree/main
- `[11]` CosyVoice 3 `inference_instruct2` 与 FP16 推理用法参考（官方示例 `example.py` 与 `<|endofprompt|>` 格式）: https://github.com/FunAudioLLM/CosyVoice/blob/main/example.py
- `[12]` CosyVoice GitHub Issues（`inference_instruct2` 与 `zero_shot_spk_id` 混用时的 `prompt_text` 前端处理问题讨论）: https://github.com/FunAudioLLM/CosyVoice/issues
- `[13]` CosyVoice Windows 前端依赖（`ttsfrd` 仅提供 Linux x86_64 wheel、`WeTextProcessing` 与 `pynini==2.1.5` conda-forge 回退说明）: https://github.com/FunAudioLLM/CosyVoice#install
- `[30]` CosyVoice 3 Paralinguistic & Instruct Demos（`FunAudioLLM/CosyVoice` 官方演示与文档）: https://funaudiollm.github.io/cosyvoice3/

### 其他 2025–2026 中文情感开源 TTS 候选
- `[14]` RVC-Boss — **GPT-SoVITS Official GitHub Repository**（v2 / v2Pro `eres2netv2` / v4 CFM 48 kHz、FP16 显存与 MIT 协议）: https://github.com/RVC-Boss/GPT-SoVITS
- `[15]` Fish Audio — **`fish-speech` Official GitHub Repository**（OpenAudio S1 / S1-mini / S2-Pro 架构与 Windows 依赖）: https://github.com/fishaudio/fish-speech
- `[16]` Fish Audio — **`fishaudio/s1-mini`（OpenAudio S1-mini 0.5B）HuggingFace Model Card**（内联情绪/副语言标记与 CC-BY-NC-SA-4.0 / Research License 非商业条款）: https://huggingface.co/fishaudio/s1-mini
- `[17]` Fish Audio GitHub Issue #1000（S1 实时流式首包延迟与服务化讨论）: https://github.com/fishaudio/fish-speech/issues/1000
- `[18]` Bilibili IndexTeam — **`index-tts/index-tts` Official GitHub Repository**（IndexTTS-2 / 2.5 情感-音色解耦架构、`uv` 部署与显存建议）: https://github.com/index-tts/index-tts
- `[19]` Bilibili IndexTeam — **`IndexTeam/IndexTTS-2` & `IndexTeam/IndexTTS-2.5` HuggingFace Model Cards**（`qwen0.6bemo4-merge` 1.21 GB、`w2v-bert-2.0`、BigVGAN-v2 与 bilibili Model Use License Agreement）: https://huggingface.co/IndexTeam/IndexTTS-2.5
- `[20]` IndexTeam — **IndexTTS-2 / 2.5 Technical Paper**（25 Hz 语义码本、Zipformer S2M 与 2.28× RTF 提升）: https://arxiv.org/abs/2506.21619
- `[21]` vLLM-Omni — **IndexTTS-2.5 Serving Recipe**（两阶段显存分配与 `/v1/audio/speech` 接口）: https://github.com/vllm-project/vllm-omni
- `[22]` OpenBMB — **VoxCPM Official GitHub Repository & `VoxCPM2` HuggingFace Model Card**（2B 连续潜变量扩散自回归、`StreamingVAEDecoder`、Voice Design、显存门槛与 Apache-2.0）: https://github.com/OpenBMB/VoxCPM
- `[23]` OpenBMB — **VoxCPM HuggingFace Collection（`VoxCPM-0.5B` / `VoxCPM1.5` / `VoxCPM2`）**: https://huggingface.co/openbmb
- `[24]` StepFun AI — **Step-Audio / Step-Audio-EditX Official GitHub Repository**（情绪与副语言迭代编辑、显存需求与 Apache-2.0）: https://github.com/stepfun-ai/Step-Audio
- `[25]` StepFun AI — **Step-Audio-EditX HuggingFace Model Card**: https://huggingface.co/stepfun-ai
- `[26]` Xiaohongshu FireRedTeam — **FireRedTTS-2 Official Repository & Model Card**（12.5 Hz 双 Transformer 播客对话与 Apache-2.0）: https://huggingface.co/FireRedTeam/FireRedTTS2
- `[27]` SparkAudio — **Spark-TTS Official Repository & Technical Paper**（0.5B Qwen2.5 + BiCodec、音高语速控制与 Apache-2.0）: https://github.com/SparkAudio/Spark-TTS
- `[28]` ByteDance — **MegaTTS3 Official Repository & Model Card**（0.45B DiT + WaveVAE 编码器未公开限制与 Apache-2.0）: https://github.com/bytedance/MegaTTS3

### Windows 显存管理、WSL2 内存规范与 Safari Web Audio 规范
- `[31]` NVIDIA Support / Driver Notes — **System Memory Fallback for Stable Diffusion / CUDA Applications**（R536.40+ `CUDA - Sysmem Fallback Policy` → `Prefer No Sysmem Fallback`）: https://nvidia.custhelp.com/app/answers/detail/a_id/5490/
- `[32]` PyTorch Official Documentation — **CUDA Semantics & Memory Management**（`PYTORCH_CUDA_ALLOC_CONF`、`expandable_segments` 平台限制、`torch.cuda.set_per_process_memory_fraction`）: https://docs.pytorch.org/docs/stable/notes/cuda.html
- `[33]` Microsoft Learn — **Advanced settings configuration in WSL (`.wslconfig`)**（WSL2 VM 默认分配 50% 宿主机物理内存规范）: https://learn.microsoft.com/en-us/windows/wsl/wsl-config
- `[34]` MDN Web Docs — **`BaseAudioContext.decodeAudioData()`**（Safari/iOS 格式兼容与完整音频帧要求）: https://developer.mozilla.org/en-US/docs/Web/API/BaseAudioContext/decodeAudioData
- `[35]` WebKit Blog — **Introducing Managed Media Source**（iOS 17.1+ `ManagedMediaSource` 与 `disableRemotePlayback` 约束）: https://webkit.org/blog/14735/introducing-managed-media-source/
- `[36]` MDN Web Docs & WebKit — **`AudioWorkletNode`**（Safari 14.1+ 跨平台低延迟 PCM 流式渲染）: https://developer.mozilla.org/en-US/docs/Web/API/AudioWorkletNode

### 本仓库基线文档
- `[37]` `docs/windows-llm-deployment.md`（`feat/spec-63-lan-llm-table-personas`：`BENDER` RTX 3060 12GB 实测显存 `7,237 MiB`、TTFT `155.7 ms`、3 并发耗时 `1,261.5 ms` 与 PowerShell 7 绿色运维体系）
- `[38]` `docs/adr/0010-lan-llm-table-personas.md`（`feat/spec-63-lan-llm-table-personas`：5 位`牌友角色`、6 档`牌友情绪`、`voice_pack_id` 预留字段与 `~1.5 s` 高光短同步约束）
