# 2D 漫画厚涂风格渲染、轻量化资产管线与 Safari 音频架构调研报告

> **调研范围（Ticket #5）**：面向家庭局域网 4 人标准推倒胡麻将（Mac Retina 高分屏优先、兼容 iPhone Safari），以「反 3D、类《英雄联盟：双城之战》（Arcane）手绘厚涂漫画质感」与「个人开发者 v1 可控美术/音频工作量」为核心约束，基于一手官方文档、规范与源码进行技术选型调研。

---

## 执行摘要（Executive Summary）

1. **渲染路线推荐：首选 Web 2D 引擎 `PixiJS v8`（默认 WebGL2 + 可选 WebGPU），不建议首版用 SVG+CSS 滤镜或 Rust/Bevy 编译 WASM 跑 iPhone Safari**
   - **视觉表现与性能平衡**：`PixiJS v8` 通过统一的 `Filter`（支持 GLSL `GlProgram` 与 WGSL `GpuProgram`）、`pixi-filters` v6（如 `OutlineFilter`、`DropShadowFilter`）、`pixi.js/advanced-blend-modes` 与 `cacheAsTexture()`，可在单次 GPU Pass 内叠加纸纹/笔触肌理、冷暖聚光暗角、墨线描边与碰杠胡漫画冲击帧（速度线、拟声词角标、微震/闪白），同时原生支持 `resolution: window.devicePixelRatio` 与 `autoDensity: true` 适配 Mac Retina（2x）与 iPhone（2x/3x）。
   - **规避平台短板**：纯 `Canvas 2D` 的 `CanvasRenderingContext2D.filter` 在桌面及 iOS Safari 上至今**不支持**（静默失效），且高 DPR 下逐像素纸纹/模糊容易成为 CPU/软件光栅瓶颈；`SVG + CSS` 的 `<feTurbulence>`/`<feDisplacementMap>` 等复合滤镜链在 Safari 上易退化为 CPU 光栅化，动画时易引发掉帧与发热；`Rust + Bevy/wgpu` 在原生桌面（macOS）能力极强，但编译到 `wasm32-unknown-unknown` 面对 iPhone Safari 时，需处理 WebGL2 的 `DownlevelFlags` 限制（无 `COMPUTE_SHADERS`、16-byte uniform 对齐等）、较大的 WASM 体积、Safari 标签页严格内存上限，以及多 Pass 后处理的移动端兼容性风险。
2. **麻将牌面与牌桌资产管线推荐：「分层组合 + `@2x/@3x` Spritesheet 图集 + Shader/Blend Mode 宏观质感叠加」**
   - **低工作量且保证 34 种牌面一眼清晰**：切勿逐张手绘 34 张牌的多个视角（会膨胀为上百张独立资产）。将牌拆解为「**3–4 张手绘厚涂牌身底图（立牌/倒牌/牌背与斜角）+ 34 张高对比矢量牌面符号（构建时用 SVG/`resvg` 批量导出 `@2x`/`@3x` 图集）**」，共仅约 **38 份源素材**。
   - **风格统一与可读性隔离**：牌桌木框/暗青绒布、冷暖主光源、画布厚涂笔触纹仅作为 **2–3 张可平铺静态底图**，在 GPU 层通过混合模式/自定义 Shader 叠加到桌面与牌身倒角；**牌面正中符号层保持极低纸纹干扰（对比度干扰 <5%–8%）与利落深色墨线描边**，确保 Mac Retina 与 iPhone 小屏上都能在瞬间辨认萬/筒/索/字。
3. **音频架构推荐：Web 端直接使用原生 `Web Audio API`（`AudioBufferSourceNode` + AudioSprite），事件总线解耦预留角色配音（VO）包扩展口**
   - **低延迟并发混音**：`AudioBufferSourceNode` 为轻量级一次性（one-shot）节点，共享已解码的 `AudioBuffer`，天然支持摸牌、拍牌与碰/杠/胡音效毫秒级零阻塞叠加；通过 `start(when, offset, duration)` 即可从单一 AudioSprite 切片播放全部基础音效。
   - **Safari 生死坑位必须显式兜底**：① 必须在 `pointerdown`/`touchend`/`click` 的**同步调用栈**内调用 `audioContext.resume()`（避免跨 `await` 失效或仅监听 `touchstart`）；② iOS Safari 默认 Web Audio 属于 `"ambient"` 会话，**拨动 iPhone 静音开关会彻底无声**，需按需配置 Safari 16.4+ 支持的 `navigator.audioSession.type`（`"ambient"` vs `"playback"`）；③ 接电话、Siri 或切后台会将 `AudioContext.state` 置为 `"interrupted"`，**处于 `"interrupted"` 时直接调用 `resume()` 会返回被拒绝的 Promise**，必须监听 `statechange` 与 `visibilitychange` 并在下次手势恢复。
   - **Rust 音频库定位**：`kira` 在原生桌面端提供极佳的 `TrackHandle` 子混音轨与 `Tween` 淡入淡出能力，而 `rodio` 依赖后台阻塞线程；二者编译到 WASM 仍须经由 `cpal` 映射回 Web Audio API，且 `kira::StreamingSoundData` **不支持 `wasm32`**、`cpal` 的 `audioworklet` 后端强制要求原子特性与 `COOP/COEP` 跨源隔离响应头，因此在 Web/Safari 端直接使用原生 Web Audio API 远比经由 Rust/WASM 桥接更轻量可控。

---

## 1. 2D 风格化渲染手法对比与推荐方案

### 1.1 《双城之战》2D 厚涂漫画风拆解与技术映射

为了在 v1 控制个人开发工作量，需将参考风格拆解为 **4 个可工程化的视觉元素**：

1. **厚涂纸纹 / 笔触叠加（Impasto & Paper Grain）**：局部冷暖色块边界带有干燥画笔/帆布纹理，避免矢量色块带来的“塑料感”。技术上对应**无缝平铺（tileable）笔触/纸纹法线或灰度纹理 + `overlay` / `soft-light` / `multiply` 混合，或片段着色器采样叠加**。
2. **利落墨线描边（Crisp Ink Outlines）**：牌体轮廓、高亮出牌边框与人物/UI 角标采用深炭黑或深靛蓝不均匀墨线。技术上优先**内建在牌体美术底图/矢量轮廓中**，仅对**动态高亮态（如听牌、可碰杠牌、飞出动画牌）**施加着色器外描边（如 `OutlineFilter` 或 UV 邻域 Alpha 采样）。
3. **高对比冷暖聚光暗角（Warm/Cool Spotlight & Vignette）**：牌桌中央打一束偏暖琥珀光（聚焦手牌与牌河），桌面四角向冷青/深靛阴影衰减，形成戏剧化舞台感。技术上对应**全屏或牌桌层的径向渐变/聚光暗角 Shader（1 次片段着色器采样或预烘焙低频渐变图叠加）**。
4. **碰 / 杠 / 胡漫画冲击帧（Comic Impact Frames）**：
   - **速度线（Speed Lines）**：极坐标辐射线条（1 张预设 SVG/矢量放射网贴图旋转+缩放动画，或程序化极坐标噪声片段着色器）。
   - **拟声词角标（Onomatopoeia Callout）**：如「碰!」「杠!」「胡!」手绘漫画矢量字块 + 夸张外描边，做 120–180ms 的弹性缩放（overshoot）与抽帧感（stepped easing，模拟 12fps/15fps 动画拍数）。
   - **轻微震屏与反色/闪白（Screen Shake & Flash）**：Root `Container` / 相机位施加衰减正弦位移（4–8px、150ms 内归零），叠加 1–2 帧（16–33ms）低透明度暖白/冷青闪光覆盖层。

---

### 1.2 五种 2D 渲染技术路线一手能力与性能对比

| 评估维度 | **PixiJS v8 (`Filter`/Shader)** | **原生 WebGL2 / WebGPU** | **Canvas 2D** | **SVG + CSS** | **Rust 2D (`Bevy` / `wgpu`)** |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **渲染后端与 Safari 支持** | **双后端统一架构**：默认 WebGL2，设置 `preference: 'webgpu'` 可启用 WebGPU 并自动回退 `'webgl'`；Mac/iPhone Safari 全覆盖 | **WebGPU** 于 **Safari 26**（macOS 26 / iOS 26）起默认开启（基于 Metal/WGSL）；旧版 iOS 17/18 依赖 **WebGL2**（Metal 转换层） | 全版本 Safari 支持基础 2D API，但 **`CanvasRenderingContext2D.filter` 在桌面及 iOS Safari 上均不支持**（赋值静默无效） | 全版本 Safari 支持基础 SVG 与 CSS `mix-blend-mode`，但复杂 SVG 滤镜硬件加速不稳定 | 原生 macOS 走 Metal 极快；Web 端编译为 `wasm32` 后通过 `wgpu` 走 WebGPU 或回退 WebGL2（受 `DownlevelFlags` 约束） |
| **厚涂纸纹 / 混合模式** | 原生支持 `normal`/`add`/`multiply`/`screen`；引入 `pixi.js/advanced-blend-modes` 并设 `useBackBuffer: true` 支持 `overlay`/`color-burn` 等 | Fragment Shader 直接采样纹理做任意数学混合（GLSL/WGSL），零额外状态开销 | `globalCompositeOperation` 在 Safari 上支持 `multiply`/`overlay`/`soft-light` 等，但每次全屏合成像素填充率压力大 | CSS `mix-blend-mode` + `<feTurbulence>` / `<feBlend>`；但程序噪声计算昂贵 | 自定义 `Material2d`（WGSL `fragment_shader()`）实现单 Pass 叠加或读取屏幕纹理 |
| **墨线描边与冲击帧** | `pixi-filters` v6（适配 PixiJS v8，支持 GLSL+WGSL）提供 `OutlineFilter`、`DropShadowFilter`、`ShockwaveFilter`、`ZoomBlurFilter`、`AdjustmentFilter`；静态组件可 `cacheAsTexture()` | 需自行编写 Sprite Batcher、UV 打包、Filter 离屏 Framebuffer Ping-Pong 管线 | 缺乏 GPU Shader，`shadowBlur` 与逐像素描边在 Retina 高分辨率下极慢；无法低成本做径向速度线着色器 | `<feMorphology>`/`<feDropShadow>` 可做静态描边，但做动画、震屏与上百张牌 DOM 重绘易卡顿 | 需自行在 WGSL 编写描边与速度线着色器，或参考 Bevy `custom_post_processing.rs` 适配 `Core2d` 节点 |
| **Mac Retina & iPhone DPR 适配** | `app.init({ resolution: window.devicePixelRatio \|\| 1, autoDensity: true })` 自动分离 CSS 逻辑尺寸与物理像素缓冲；注意高级混合模式需设 `Filter.defaultOptions.resolution = 'inherit'` 防裁切 | 需手动同步 `canvas.width = cssW * dpr`。注意 **Safari 不支持 `ResizeObserverEntry.devicePixelContentBoxSize`**，须用 `contentBoxSize * dpr` | 手动设置 canvas 物理宽高与 `ctx.scale(dpr, dpr)`；受限于 Safari 单 Canvas 内存上限与软件滤镜缺失 | 矢量天然匹配任意 Retina DPR，但大面积位图化滤镜缓冲易吃满移动端内存 | 原生 `WindowResolution` 自动读取 OS `scale_factor()`（也可 `set_scale_factor_override`）；Web 端需自行同步 canvas DPR |
| **iPhone Safari 实操风险** | 单张 `cacheAsTexture` 不可超 4096×4096；需控制同时激活的多 Pass `Filter` 数量以防移动 GPU 带宽吃紧 | 研发工作量极大；WebGL2 在 Safari 上存在 uniform buffer 模拟开销与内存敏感问题 | 无法使用 `ctx.filter` 做模糊/调色/阴影加速；高 DPR 全屏叠加肌理易掉到 30fps 以下 | SVG 复合滤镜（如 `feTurbulence`）易落入 **CPU 光栅化**，震屏或大量节点联动会导致掉帧与手机发烫 | **WASM 体积大**（通常数 MB～十数 MB）；iOS Safari 标签页**内存上限严格**，超限会被系统直接强杀静默重载；WebGL2 后端无 Compute Shader |
| **solo v1 综合工程性价比** | **★★★★★（极高）**：兼顾美术表达、双端一致性与低开发成本 | **★★☆☆☆（低）**：重复造轮子成本远超个人开发收益 | **★★☆☆☆（低）**：受限于 Safari `ctx.filter` 缺失与高 DPR 性能墙 | **★★☆☆☆（低）**：适合静态 UI 壳层，不适合高张力牌桌特效 | **★★★☆☆（中）**：纯桌面原生强，但受「必须兼容 iPhone Safari」约束拖累明显 |

---

### 1.3 推荐渲染技术方案与落地细节

**核心结论**：采用 **`PixiJS v8`（WebGL2 默认基准 + 可选 `GlProgram`/`GpuProgram` 双着色器）作为牌桌核心渲染器**，外围房间大厅/设置面板/语音指示等表单 UI 可叠放轻量 HTML/CSS 层。

#### 关键工程约束与官方 API 实践要点

1. **Retina 与 iPhone 高 DPR 精确配置**
   - 初始化 `Application` 时开启 `autoDensity: true`，建议对 `resolution` 做上限夹紧（例如 `Math.min(window.devicePixelRatio || 1, 3)`，Mac Retina 为 `2`，iPhone Pro 为 `3`），避免外接 4K/高倍屏无意义撑爆显存：
     ```ts
     import { Application, Filter } from 'pixi.js';
     import 'pixi.js/advanced-blend-modes';

     // 防止高 DPR 下高级混合模式（如 overlay / soft-light）出现裁剪错位
     Filter.defaultOptions.resolution = 'inherit';

     const app = new Application();
     await app.init({
       resizeTo: window,
       resolution: Math.min(window.devicePixelRatio || 1, 3),
       autoDensity: true,
       useBackBuffer: true, // 开启高级混合模式必需
       // v1 稳妥起见默认 WebGL2（兼容性最好），待目标设备均升级 Safari 26+ 后可改为 ['webgpu', 'webgl']
       preference: 'webgl',
     });
     ```
   - **关于 WebGPU 的理性取舍**：虽然 **Safari 26** 已默认开启 WebGPU，但考虑到家庭设备中可能存在未升级至 iOS 26/macOS 26 的 iPhone，且 WebGL2 在 `PixiJS v8` 中已极其成熟，v1 推荐以 `WebGL2` 为默认基准（仅编写 `GlProgram` GLSL 着色器即可全端运行）；若未来开启 `preference: 'webgpu'`，需同步为自定义 `Filter` 补充 WGSL `GpuProgram`（PixiJS v8 会跳过缺少对应后端程序的 Filter）。
2. **用单 Pass 自定义 `Filter` 替代多 Pass 滤镜串联**
   - `pixi-filters` v6 提供了开箱即用的 `OutlineFilter`、`DropShadowFilter`、`ShockwaveFilter` 等，但串联多个现成滤镜会引发多次离屏 RenderTarget 切换（对 iPhone 移动 GPU 带宽开销大）。
   - **推荐做法**：对**静态牌桌底景**使用 `container.cacheAsTexture(true)` 烘焙（注意单容器不超过 4096×4096，且有滤镜的对象应将 `cacheAsTexture` 放在其父容器上）；对**聚光暗角 + 纸纹叠加 + 碰杠胡短时闪白/色差**，合并编写 **1 个全屏/牌桌级单 Pass 自定义 `Filter`**（读取 `uPaperNoiseTexture`、聚光半径与中心坐标 `uSpotlight`、冲击强度 `uImpactFlash`），单帧仅增加 1 次全屏采样，确保 Mac 与 iPhone 稳定 60fps。
3. **非抗锯齿缓冲区的矢量平滑**
   - 若为了省带宽关闭 MSAA 或将矢量绘入 `RenderTexture`（`RenderTexture` 不自动享有多重采样抗锯齿），PixiJS v8 提供了 `SmoothGraphics`（在着色器内动态计算边缘抗锯齿）以保住手绘墨线的锐利度。

---

## 2. 麻将牌面与牌桌资产管线建议

### 2.1 三种低工作量资产管线对比

在麻将游戏中，共有 **34 种基础牌面**（万子 1–9、筒子 1–9、索子 1–9、东南西北中发白 7 张字牌），且牌桌存在 4 家视角（自家正视手牌、对家/左右家立牌背面、各家河牌/吃碰杠倒牌）。

| 方案 | 运行/构建机制（依据一手文档） | Retina 锐利度 | 美术工作量（Solo v1） | 运行期性能与风险 |
| :--- | :--- | :--- | :--- | :--- |
| **方案 A：分层矢量 SVG + 运行时/构建时纹理合成** | **PixiJS v8** 支持两种路径：① `Assets.load('x.svg')` 加载时按设定分辨率光栅化为纹理；② `Graphics.svg()` 解析为矢量几何体并通过 `GraphicsContext` 多实例复用。<br>**Bevy** 核心不原生解析 SVG，需借助 `resvg` 在构建时或加载时转成位图。 | **极高**：矢量无损缩放。但 PixiJS `Graphics.svg()` 官方明确提示**复杂遮罩（clip paths/masks）与 `<text>` 元素可能被跳过并报警告**。 | **中等**：需要熟练用 Figma/Illustrator 规范化导出干净的 `<path>` SVG，厚涂笔触质感难以直接用纯扁平矢量表达。 | **运行时直接解析大量 SVG 几何有风险**；而若在**构建期**用 `resvg` / Node 将 SVG 光栅化为 `@2x`/`@3x` 位图，则安全且高效。 |
| **方案 B：高 DPI Texture Atlas 图集** | **PixiJS v8** 通过 `Spritesheet` JSON 读取 `meta.scale`（如 `"2"` 对应 `@2x.png`/`.webp`），配合 `Assets.init({ preferences: { resolution } })` 自动匹配设备倍率，单批可合并多达 **16 张纹理**；可选支持 `pixi.js/ktx2`（显存省 4–8 倍）与 WebP 回退。<br>**Bevy** 使用 `TextureAtlasLayout::from_grid` 配合 `Sprite`。 | **高**：只要导出 `@2x`（Mac Retina）与 `@3x`（iPhone）图集即可像素级锐利；需注意避免非整数倍剧烈缩放。 | **视拆分策略而定**：若整张牌画死（34×多视角），工作量爆炸；若「底块图集 + 牌面图集」分离，则极低。 | **最优运行时性能**：零运行时光栅化开销，全部牌面合批（Batching）渲染，GPU Draw Calls 极少。 |
| **方案 C：少量静态底图 + Shader/Blend Mode 质感叠加** | 仅制作 2–3 张高质感厚涂静态贴图（桌布、木/铜边框、无缝手绘纸纹/笔触灰阶图），在运行期利用混合模式或着色器叠加到矢量/图集元件上。 | **依赖前景图层精度**，肌理层本身对绝对分辨率不敏感（可低分辨率无缝平铺）。 | **极低**：只需重点精修几张氛围底图，无需反复给 34 张牌重复刷笔触。 | **轻量且统一**：将“《双城之战》氛围感”集中在少量像素通道中复用；但**切忌把强纸纹叠在牌面符号中心**，否则影响认牌速度。 |

---

### 2.2 推荐管线：「分层组合（牌体×牌面）+ 构建期图集 + 运行期 Shader 肌理」

综合 solo 开发者产能与 Mac Retina 可读性，推荐**混合方案 B + C（辅以构建期 SVG 光栅化）**：

#### 资产拆分清单（v1 总计仅约 ~45 个源文件）
1. **层 1 — 手绘厚涂牌桌与环境氛围层（约 3–4 张静态图）**
   - `table_base.webp`（1 份，1920×1080 或按比例铺底）：深青灰带暖边高光的厚涂桌面，边缘预留暗角底色。
   - `impasto_grain_tile.png`（1 份，512×512 无缝平铺灰阶纹理）：干画笔刷痕 + 水彩纸纹，供自定义 Shader 或半透明 `overlay` 叠加在牌桌、阴影与牌背/侧边厚切面上。
   - `impact_burst_atlas`（1 套，含 1 张放射速度线、`碰/杠/胡` 漫画拟声词贴纸）：用于吃碰杠胡瞬间爆发。
2. **层 2 — 共享牌体与投影基底（约 4–6 张手绘厚涂底图）**
   - 仅手绘 **几种有限的麻将立体块体**（含厚实深色墨线描边与斜切高光笔触）：
     - 自家正面立牌底块（1 张）
     - 河牌/明牌平放底块（1 张，可选正放与横放“碰/杠”旋转复用）
     - 他家立牌背面底块（1 张，通过左右镜像或色温微调复用三家）
     - 暗杠牌背底块（1 张）
     - 软投影贴图（1 张，斜向冷色阴影，比实时 `DropShadowFilter` 省电 10 倍）
   - *优势*：彻底集中美术精力把 **“这 4 块麻将的厚涂笔触与墨线描边”** 画到《双城之战》水准，136 张牌在桌面上立显手绘质感。
3. **层 3 — 34 种牌面符号图集（矢量导出 `@2x` / `@3x` 图集）**
   - **可读性核心原则**：在 Figma / Illustrator 中用干净矢量绘制 34 种标准牌面符号（**高饱和度却略微做旧的朱红、钴蓝、孔雀绿 + 深炭黑墨线**，避免纯 `#FFFFFF` 刺眼白底，改用暖象牙白 `#F4EDE0` 底块提升冷暖对比），**不叠加高频纸纹噪点**。
   - 构建脚本在打包期使用 Node SVG 工具或 Rust `resvg` 导出两套 `Spritesheet`（`tiles@2x.png/json` 面向桌面及 Mac Retina，`tiles@3x.png/json` 按需面向 iPhone，或直接全端复用 `@2x` 配合 WebP 有损/无损压缩）。
   - **渲染合批**：在 PixiJS 中每张桌面牌仅是一个轻量 `Container`（底块 `Sprite` + 牌面符号 `Sprite`），同一张 `Spritesheet` 图集让全场 100+ 张牌在极少 Draw Call 内完成合批；仅在**选中/听牌提示/最新打出**的 1–2 张牌上临时启用 `OutlineFilter` 或程序化发光层。

> **关于 KTX2 压缩纹理的取舍**：v1 资产总尺寸很小（1–2 张 `2048×2048` 牌面与牌体 WebP/PNG 图集 + 几张底图，总加载量通常在 2–5 MB 内），优先使用 **WebP + PNG 兜底**即可；无需在 v1 引入 KTX2/Basis 转码器复杂度（可留作后续加入大量高清角色立绘时的优化项）。

---

## 3. 轻量音频播放、Safari 踩坑与未来配音扩展设计

### 3.1 Web Audio API 并发架构 vs Rust 音频库（`kira` / `rodio`）

| 对比维度 | **原生 Web Audio API（浏览器 / TypeScript）** | **Rust `kira`（经 `cpal`）** | **Rust `rodio`（经 `cpal`）** |
| :--- | :--- | :--- | :--- |
| **适用运行环境** | 所有浏览器（Mac/iPhone Safari 原生一等公民） | 原生桌面（macOS）极佳；`wasm32` 仅限静态内存解码 | 原生桌面简单播放；`wasm32` 集成难度高 |
| **核心并发/混音模型** | `AudioBuffer`（解码后常驻内存）+ 每次播放创建轻量 `AudioBufferSourceNode` 连接 `GainNode` 混音总线；天然零上限并发叠音 | `AudioManager` + `StaticSoundData`（可廉价 `clone` 共享内存）+ `TrackHandle` 子混音轨 + `Tween` 平滑调制 | `OutputStream` / `MixerDeviceSink` + `Sink` + `Source`；偏向单流/队列式播放，缺乏内建游戏向 `Track` 与 `Tween` |
| **WASM / iPhone Safari 局限** | 无跨层桥接开销；可直接精确操控 `AudioContext.state`、`onstatechange` 与 `navigator.audioSession` | 官方文档明确标注 **`StreamingSoundData` 不支持 `wasm32`**（依赖后台文件/线程流式读取）；全量音效需预加载内存 | 重度依赖后台阻塞线程（如 `sleep_until_end`）与 OS 调度，易与浏览器单线程事件循环冲突；若仅用其解码需关闭 default-features |
| **低延迟音频工作线程要求** | 原生主线程触发 `source.start(0)`，底层混音在浏览器原生音频线程完成，**无需任何特殊 HTTP 响应头** | 走 `cpal` 默认 `webaudio` 后端或可选 `audioworklet` 后端；**开启 `audioworklet` 需 Rust `-Zbuild-std` atomics 且强制要求服务器下发 `COOP` + `COEP` 响应头开启 `SharedArrayBuffer`** | 同左，依赖 `cpal` 的 WebAudio/AudioWorklet 后端，且缺少开箱即用的 Safari 用户手势解锁封装 |

**结论**：只要客户端运行在浏览器（包括 iPhone Safari 与 Mac 浏览器），**应直接使用浏览器原生 Web Audio API**（即便游戏逻辑用 Rust 写，音频层也建议通过薄 JS/TS 桥或 `web-sys` 直接驱动 Web Audio，而非通过 `cpal` + `rodio`/`kira` 编译到 WASM）。若未来做独立 macOS 原生客户端（如 Tauri 原生插件或 Bevy 原生桌面端），则首选 **`kira`**。

---

### 3.2 macOS Safari 与 iOS Safari 一手规范坑点与防御方案

依据 W3C Web Audio API 规范、W3C Audio Session 规范及 WebKit/Apple 官方文档，Safari 上存在 **4 个高频致命坑点**：

#### 坑点 1：自动播放拦截与手势调用栈失效
- **机制**：WebKit 对有声媒体与 Web Audio 强制要求**瞬态用户激活（Transient User Activation）**。页面加载时新建的 `AudioContext` 处于 `"suspended"` 状态，未经手势调用 `start()` 会静默排队不发声。
- **关键坑点**：
  1. 必须在 `click`、`touchend`、`pointerdown` 或 `keydown` 回调的**同步执行栈**内立刻触发 `audioContext.resume()`（以及在首次交互时播放 1 个静音 1-sample `AudioBuffer` 稳妥解锁）。
  2. **禁止先 `await fetch(...)` 再调用 `resume()`**——跨过微任务/Promise 边界后，Safari 会判定用户激活已过期，`resume()` 无法切换至 `"running"`。
  3. 在 iOS Safari 上，尽量避免**仅**绑定 `touchstart` 来解锁（手指按下的发起阶段在不同版本 WebKit 上判定不稳定），推荐统一绑定 `pointerdown` 或 `touchend` + `click`。

#### 坑点 2：iPhone 物理静音键（Ring/Silent Switch）导致 Web Audio 彻底无声
- **机制**：依据 W3C Audio Session 规范与 WebKit 实现，Safari 默认将 Web Audio 归类为 **`"ambient"`** 会话类型，在 iOS 上映射为原生 `AVAudioSessionCategoryAmbient`——这意味着一旦玩家手机侧边拨片处于**静音模式**，所有打牌/胡牌音效会**全部被系统静音**（而 `<audio>` 标签默认反而常能出声）。
- **解决方案（Safari 16.4+）**：
  - WebKit 自 Safari 16.4 起支持 `navigator.audioSession`（Audio Session Web API 子集）：
    ```ts
    if ('audioSession' in navigator && navigator.audioSession) {
      // "playback"：无视 iPhone 物理静音开关，但在开启时会压低/暂停手机后台其他音乐 App
      // "ambient"：遵循静音开关，但允许与用户自己的背景音乐（如 Apple Music）混播
      navigator.audioSession.type = 'playback';
    }
    ```
  - **家庭麻将体验建议**：若两人一边打牌一边用手机放歌，`"playback"` 会暂停背景音乐；因此推荐默认或在设置项提供开关（例如默认 `"playback"` 保证刚进房能听见牌声，同时提供「遵循静音键/允许后台音乐混音」切换回 `"ambient"`）。需注意必须在启动 `AudioContext` 之前设置该属性。

#### 坑点 3：`AudioContext.state === "interrupted"` 与切后台恢复
- **机制**：W3C Web Audio API 规范定义了四种 `AudioContextState`：`"suspended"`、`"running"`、`"interrupted"`、`"closed"`。WebKit 将系统音频打断（来电、Siri 唤醒、闹钟、控制中心切换、iOS 锁屏或切换 App）直接映射为 **`"interrupted"`** 状态。
- **规范级致命坑点**：W3C 规范明确指出——**当 `AudioContext` 处于 `"interrupted"`（或 `"closed"`）状态时，调用 `audioContext.resume()` 会直接返回一个 Rejected Promise**！很多开发者在 `visibilitychange` 里无脑调用 `resume()` 导致报错后音频永久卡死。
- **防御恢复状态机**：
  1. 监听 `audioCtx.onstatechange` 与 `document.addEventListener('visibilitychange', ...)`。
  2. 仅当页面重新可见且 `audioCtx.state === 'suspended'` 时尝试 `resume()`；若处于 `'interrupted'`，则等待系统打断结束自动恢复或转为 `'suspended'` 状态后再恢复。
  3. 保留一个轻量全局一次性的 `pointerdown` 兜底钩子：只要检测到 `audioCtx.state !== 'running'`，在下一次任意牌桌点击/出牌操作时同步调用 `audioCtx.resume()`；若页面长期无音频唤醒，显示一个不遮挡牌面的角标提示「🔇 点击恢复音效」。

#### 坑点 4：iOS 多标签页切换与并发峰值爆音 / 内存控制
- **AudioSprite vs 独立短音**：v1 仅需摸牌、抓牌微响、轻放、重拍、碰、杠、胡、倒计时滴答等 ~15–20 个短音效（总时长 < 15 秒）。打包为 **1–2 个 `.mp3`/`.m4a`/`.opus`（注意：旧版 Safari 对 `.ogg/.opus` 支持曾有局限，桌面+iOS 最稳兼容格式为 **AAC `.m4a` 或 MP3**，高音质低体积）AudioSprite**，只需 **1 次 `fetch` + 1 次 `decodeAudioData`**，通过 `source.start(0, offset, duration)` 精准切片播放，避免初始化时并发几十个网络请求。
- **防削波（Anti-Clipping）**：4 人连环出牌或同时触发拍牌+碰牌+特效音时，线性叠加极易突破 `0 dBFS` 导致 iOS 扬声器破音。在总线末端串联 `DynamicsCompressorNode` 作为软限幅器，保障听感饱满不破音。

---

### 3.3 面向未来的「事件驱动 + 配音包（VO Pack）」扩展架构

v1 仅实现基础牌桌 SFX，但音频管线在代码层应一步到位拆成 **4 层解耦架构**，确保后续加入角色语音（如四川话/粤语/二次元角色喊「碰！」「杠！」「自摸！」）无需重构游戏主逻辑：

```text
[游戏状态机 / 网络事件]
   │  emit SemanticGameEvent { type, seat, actorId, characterId, tile, combo }
   ▼
[AudioEventRouter（规则与冷却分发器）]
   ├──► SFX 通道：tile_slam / tile_draw / call_pon_stinger ──► [sfxGain] ──┐
   └──► VO  通道（v2 启用）：查找 characterId 对应 Manifest      [voGain]  ──┼──► [DynamicsCompressor] ──► destination
            │  (同角色互斥打断 + 优先级 Hu > Kong > Pon + 触发 SFX Ducking) ──┘
```

#### 关键扩展口设计规范
1. **事件语义化，严禁在逻辑层硬编码文件名**
   - 牌局逻辑只派发领域事件（例如 `GameAudioEvent::Meld { kind: 'pon' | 'kong', seat: 0..3, characterId: 'jinx_style' }`），音频模块内部决定它对应哪些音效/语音。
2. **Manifest 驱动的配音包规范（`/assets/vo/<pack_id>/manifest.json`）**
   - 每个语音包自带 JSON 清单：声明 `id`、`displayName`、事件映射池（支持同一事件配置 2–3 条随机变体，防止听腻）、以及音频切片或独立文件 URL：
     ```json
     {
       "packId": "arcane_tactician_zh",
       "events": {
         "pon": ["pon_01.m4a", "pon_02.m4a"],
         "kong": ["kong_01.m4a"],
         "hu_zimo": ["zimo_01.m4a", "zimo_02.m4a"],
         "hu_dianpao": ["ron_01.m4a"]
       }
     }
     ```
3. **动态懒加载与 iOS 内存有界缓存（LRU Cache）**
   - v1 启动只加载 ~150–300 KB 的核心 SFX AudioSprite；未来选定某位角色配音包时才按需 `fetch` + `decodeAudioData`（记住 44.1kHz 单声道 1 分钟未压缩 PCM 约占 ~5.3 MB 内存，立体声翻倍，因此配音包建议使用 **24kHz/44.1kHz 单声道 AAC `.m4a`**，并在内存池中用 LRU 上限如 4 个激活角色包淘汰未选角色缓冲，避免触发 iOS Safari 标签页内存回收）。
4. **VO 冲突仲裁与自动闪避（Ducking）**
   - **同座互斥 + 优先级裁决**：同一玩家短时间连续动作时，新 VO 自动 `oldSource.stop()` 停止上一句；不同玩家同时触发时，高优先级事件（`Hu > Kong > Pon`）可压低并发音效。
   - **自动化 Ducking**：播放高张力「碰/杠/胡」VO 时，利用 Web Audio 原生参数曲线 `sfxGain.gain.setTargetAtTime(0.45, now, 0.03)` 短暂压低底噪/普通打牌音效，VO 结束后在 `onended` 回调平滑拉回 `1.0`。

---

## 4. 一手来源索引（Primary Sources）

### Web 2D 渲染与资产管线（PixiJS v8 / MDN / WebKit）
1. PixiJS v8 Application & Renderer Options (`preference`, `resolution`, `autoDensity`, `useBackBuffer`): https://pixijs.com/8.x/guides/components/application
2. PixiJS v8 Filters (`Filter`, `GlProgram`, `GpuProgram`, 内置滤镜与分辨率): https://pixijs.com/8.x/guides/components/filters
3. PixiJS v8 Blend Modes (`pixi.js/advanced-blend-modes`): https://pixijs.com/8.x/guides/components/blend-modes
4. PixiJS v8 Graphics & SVG (`GraphicsContext`, `.svg()`, `SmoothGraphics`): https://pixijs.com/8.x/guides/components/graphics
5. PixiJS v8 Assets & Multi-resolution: https://pixijs.com/8.x/guides/components/assets
6. PixiJS v8 Spritesheets (`meta.scale`, `@2x` 与图集批处理): https://pixijs.com/8.x/guides/components/spritesheets
7. PixiJS v8 Cache as Texture (`cacheAsTexture()`, `updateCacheTexture()`, 4096px 限制): https://pixijs.com/8.x/guides/components/cache-as-texture
8. PixiJS Official Filters v6 (`pixi-filters` v6 适配 PixiJS v8 / WGSL): https://github.com/pixijs/filters
9. WebKit Blog — WebKit Features in Safari 26.0 (WebGPU 默认启用与 WebGPU HDR Canvas): https://webkit.org/blog/16812/webkit-features-in-safari-26-0/
10. WebKit Blog — WebGPU now available for testing in Safari Technology Preview: https://webkit.org/blog/14917/webgpu-now-available-for-testing-in-safari-technology-preview/
11. Apple Developer — Safari Release Notes (WebGPU, CanvasRenderingContext2D, AudioSession 演进): https://developer.apple.com/documentation/safari-release-notes
12. MDN — `CanvasRenderingContext2D` & `CanvasRenderingContext2D.filter` (Safari 兼容状态): https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/filter
13. MDN — `ResizeObserverEntry` (`devicePixelContentBoxSize` Safari 兼容状态): https://developer.mozilla.org/en-US/docs/Web/API/ResizeObserverEntry
14. MDN — SVG `<feTurbulence>` 滤镜参考: https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Element/feTurbulence

### Rust 2D / wgpu / SVG 生态
15. Bevy Official Repository (`Material2d`, `TextureAtlasLayout`, `custom_post_processing` 示例): https://github.com/bevyengine/bevy
16. Bevy API Documentation (`docs.rs`): https://docs.rs/bevy
17. `wgpu` Documentation (`DownlevelFlags` 与 WebGL2 限制): https://docs.rs/wgpu
18. `resvg` Documentation (SVG 到像素光栅化): https://docs.rs/resvg

### 音频规范、Safari 行为与 Rust 音频库
19. W3C Web Audio API Specification (`AudioContextState` `"interrupted"`, `resume()`, `AudioBufferSourceNode`): https://www.w3.org/TR/webaudio/
20. W3C Audio Session Specification (`AudioSessionType`: `"auto"`, `"playback"`, `"ambient"`, `"transient"`): https://w3c.github.io/audio-session/
21. WebKit Blog — WebKit Features in Safari 16.4 (AudioSession Web API 支持): https://webkit.org/blog/13962/webkit-features-in-safari-16-4/
22. WebKit Blog — Auto-Play Policy Changes for macOS: https://webkit.org/blog/7736/auto-play-policy-changes-for-macos/
23. WebKit Blog — New Video Policies for iOS: https://webkit.org/blog/6784/new-video-policies-for-ios/
24. WebKit Blog — The User Activation API: https://webkit.org/blog/13760/the-user-activation-api/
25. MDN — `AudioContext`, `AudioBufferSourceNode`, `AudioSession`:
    - https://developer.mozilla.org/en-US/docs/Web/API/AudioContext
    - https://developer.mozilla.org/en-US/docs/Web/API/AudioBufferSourceNode
    - https://developer.mozilla.org/en-US/docs/Web/API/AudioSession
26. `kira` Documentation (`StaticSoundData`, `StreamingSoundData` 不支持 wasm32, `TrackHandle`, `Tween`): https://docs.rs/kira
27. `rodio` Documentation (`OutputStream`, `Sink`, `Decoder`, `Source`): https://docs.rs/rodio
28. `cpal` Documentation (`webaudio` 与 `audioworklet` WASM 后端约束): https://docs.rs/cpal
