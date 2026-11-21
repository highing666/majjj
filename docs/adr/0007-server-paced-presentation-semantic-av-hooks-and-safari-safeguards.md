---
status: accepted
---

# 服务端重叠节拍防泄密演出、语义音画总线、双层 SFX 与 Safari 音画保底契约

为了让客厅双人 Mac / iPhone 联机在 2D 漫画牌桌上保持画面与音效同频、杜绝 AI 思考停顿泄露手牌信息、在不改动状态机与协议的前提下平滑迎接 v2 角色配音包与进阶特效，同时规避 iOS Safari 音频静音/打断与 8GB 统一内存渲染陷阱，我们决定锁定 **「服务端主导重叠拟人节奏 + 统一落河防泄密节拍（桌级共享 `pace`）」**、**「服务端单一真相源 + 前端串行 `PresentationQueue`（含`补杠`/`抢杠胡`两段演出与他家`暗杠`四张扣牌投影）」**、**「`SemanticGameEvent` 语义总线 + `SeatInfo.voice_pack_id` 占位 + 双层 `SfxBackend`（单 `.m4a` AudioSprite 真实采样为主 + 程序化合成 1:1 兜底）」**，以及 **「iOS 默认 `playback` / `interrupted` 防御恢复胶囊 + 全端 `maxDpr = 2` 单 `@2x` 图集与分级 Shader 降级」**。

## Considered Options

- **服务端零延迟全速推进、完全交给客户端本地队列控制节奏**：双人同处客厅使用不同性能设备时，画面与音效进度容易错位，且中途接管 AI 座位（`ADR-0005` 的 `seat_generation` 取消窗口）会被瞬间耗尽；同时无法统一把 AI 弃权思考藏进一致的出牌节拍中。
- **将对局节奏 `pace` 设为浏览器本地个人偏好（`localStorage`）**：既然 AI 落子节奏由 `mahjong-server` 权威驱动，单机各自设置会产生架构冲突与双机失步，必须将 `pace` 归属桌级 `TableSession` 与服务端 `preferences.json`，仅把纯单机听感/交互偏好留在 `localStorage`。
- **v1 纯程序化 Web Audio 合成（彻底放弃真实采样 AudioSprite）**：虽然零音频资源体积，但真实骨牌碰撞质感与漫画拍桌压迫感上限明显逊于精选真实采样；改为双层 `SfxBackend` 既保障开发期零阻塞与解码失败兜底，又让正式版拥有真实采样手感。
- **v1 不预留 `SeatInfo.voice_pack_id` 与 VO Manifest/Ducking 契约，等 v2 再改协议**：推迟协议占位与事件总线会导致 v1 逻辑层直接耦合具体音效文件名、v2 接入角色配音时被迫同时改动 `mahjong-server` 协议与前端牌桌代码。
- **iOS 默认 `navigator.audioSession.type = "ambient"`**：虽然不打断后台音乐，但大多数 iPhone 常年开启侧边静音拨片，导致首局打开即彻底无声；改为默认 `"playback"` 并在设置中一键切换 `"ambient"` 更符合预期。
- **为 iPhone Pro 额外构建 `@3x` 牌面图集并放开 `maxDpr = 3`**：3× 渲染缓冲像素填充与显存开销是 2× 的 2.25 倍，且重复膨胀 `rust-embed` 单二进制体积，在 6 寸屏上收益远小于发热与 iOS 标签页 OOM 风险。
- **他家`暗杠`采用日麻式 `2 扣 2 明`全桌立即可见**：偏离常见推倒胡`暗杠`不向对手亮牌、不可被`抢杠胡`的直觉，且提前向对手和强 AI 泄露 4 张牌已耗尽的组合壁信息；改为仅自家可见（`3 扣 1 明`）、对他家投影 `tile: None`（`4 张青玉扣牌`直至`小局`结算公开）与 `ADR-0003`、`ADR-0004` 完全一致。

## Consequences

- **服务端重叠拟人节奏与防泄密节拍**：`mahjong-server` 将 Rayon AI 思考耗时（硬上限 `180 ms`）与拟人停顿窗口**重叠**而非叠加。每次`舍牌`后服务端统一保持 `~200 ms` 落河节拍再推进下家摸牌，AI 响应评估并行于该节拍内完成，AI 选择「过」时零额外延迟静默放行，彻底避免由停顿反推 AI 曾考虑`碰`或`点炮`胡（双人局另一人类无限时思考不伪造延迟，延续 `ADR-0003` 不广播等待态红线）。标准档 AI 落子总窗口：`摸切` `380–580 ms`、`手切` `520–850 ms`、`碰`/`明杠`/`补杠`/`暗杠` `420–680 ms`、`自摸`/`点炮`/`抢杠胡` `350–550 ms`；`HandStarted` 后保留 `~650 ms` 理牌缓冲，`MeldDeclared` 后 AI 紧随动作停顿下限对齐至至少覆盖 `~700 ms` 冲击帧；桌级 `pace: Standard | Fast`（快捷档压缩约 40%，同步作用于前端动画倍率）由 `TableSession` 共享并持久化至服务端 `preferences.json`，AI 托管座位沿用同一节奏。
- **前端串行演出队列、两段`补杠`与`暗杠`投影**：局域网 `< 5 ms` RTT 下客户端点击出牌立即锁输入（`expected_seq`）并给轻抬反馈，由服务端回包作为单一真相源驱动串行 `PresentationQueue`（摸牌 `~140 ms`、`舍牌`落河 `~180 ms`、鸣牌冲击帧 `~700 ms`、胡牌冲击帧 `~1100 ms` 可点按提前展开结算、`荒庄` `~600 ms`、`一炮多响`单帧合印章同步高亮多座）。队列常规积压 `≥ 3` 自动以 `1.6×` 加速消化；收到重连全量 `TableSnapshot` 立即清空队列硬切最新态。收到 `AddedKongAttempted` 时仅做悬停靠拢与金属悬念音、不播「杠！」大冲击帧也不跳杠分，待被`抢杠胡`直接引爆或全部点过后由 `MeldDeclared(AddedKong)` 完整播「补杠！」；`PlayerView`/`PlayerEvent` 对他家`暗杠`统一投影为 `tile: None`（前端绘 4 张扣牌，结算公开），自家按 `3 扣 1 明` 绘制。
- **语义音画总线、双层 `SfxBackend` 与 v2 VO/特效扩展口**：前端经单一映射层将 `PlayerEvent` 与本地 UI 动作转为 `SemanticGameEvent`，同时驱动 `AudioEventRouter` 与独立的 `PresentationEffectsBus`（v1 挂皮城冲击帧，v2 直接订阅拓展角色 Cut-in 与大番全屏演出）。`SeatInfo` 在 v1 即预留可选透传字段 `voice_pack_id: Option<String>`（v1 默认为空）。音频总线统一经 `sfxGain` + `voGain`（v1 置空）+ `masterGain` → `DynamicsCompressorNode` 软限幅防破音；v1 `SfxBackend` 主通道加载单个嵌入 AAC `.m4a` AudioSprite（总长 `< 15 s`、体积 `≤ 300 KB`、`sfx-sprite.json` 定义约 14 个语义槽位，常用牌声配 2–3 变体与 `±4%` 音高微抖），缺失或解码异常无缝降级至 1:1 槽位映射的原型程序化合成器。公开信息驱动的听感分层锁定为：`摸切`轻放 / `手切`与残局（`牌墙`余张 `≤ 24` 张）重拍、胡牌按 `0–1 / 2–3 / 4 番封顶` 分 3 档气势、相对座次克制声像（上家 `-0.35` / 下家 `+0.35` / 自家与对家 `0`）、`UiActionPrompted` 严格仅本机发声。v2 VO Pack 契约固化为 `/assets/vo/<pack_id>/manifest.json`（单声道 `.m4a`、iOS LRU 缓存上限 4 包）、同座新句打断旧句、跨座优先级 `Hu > RobKong > Kong > Pon > Chi > Discard` 与 VO 播发时 `sfxGain` 30 ms 平滑 Ducking 至 `0.45`。
- **Safari 音频会话与 8GB Mac / iPhone 渲染保底**：iOS 默认 `navigator.audioSession.type = "playback"`，设置内提供切换 `"ambient"` 开关；严格遵循 W3C 规范，在 `AudioContext.state === "interrupted"` 时禁止直接调 `resume()`，仅于 `suspended` 时在可见回调或 `pointerdown`/`touchend`/`click`/`keydown` 同步调用栈内伴随 1-sample 静音缓冲解锁，非主动静音且未运行态在角落展示不挡牌的「🔇 点按恢复音效」胶囊；音量、静音、`audioSession`、减弱动效/省电与出牌二次确认等纯单机偏好按设备独立存于 `localStorage`。渲染侧全端锁定 `PixiJS v8` `resolution = Math.min(dpr, 2)` 复用单一 `@2x` 矢量导出图集（单张 `≤ 2048×2048`），`document.hidden` 时自动 `app.ticker.stop()` 停帧，遵循 `prefers-reduced-motion`、手动省电开关及连续 3 秒 `< 30 fps` 自动关闭全屏纸纹/暗角单 Pass Shader 与震屏闪白（始终保留预烘焙底图与 34 张牌面锐度），并监听 `webglcontextlost`/`webglcontextrestored` 自动恢复（失败提示刷新经 `TableSnapshot` 无损续局）。
