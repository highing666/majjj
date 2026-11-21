---
status: accepted
---

# Rust 单二进制权威服务端与浏览器 2D 前端架构

为了在家庭局域网内打造 Mac 优先、兼顾 iPhone 零安装直连且保留未来迁移至 Mac mini / Linux NAS 能力的 4 人推倒胡麻将游戏，我们决定采用 **Rust 单二进制权威服务端（`Axum` + `Tokio` + `Rayon` + `rust-embed`）+ 现代 Web 2D 浏览器前端（`PixiJS v8` 默认 WebGL2 + `Svelte 5` + 原生 `Web Audio API`）**。Mac 端通过本机浏览器或 Safari「添加到程序坞」获得独立无地址栏桌面窗口，iPhone 与另一台 Mac 浏览器扫码/直连；仓库采用单仓 Monorepo（Rust Cargo Workspace `mahjong-core` / `mahjong-ai` / `mahjong-server` + `web/`，发布期经 `rust-embed` 将前端内嵌为单个 Apple Silicon 可执行文件）。

## Considered Options

- **Tauri v2 原生壳 + 内嵌局域网服务**：生产构建从 `tauri://localhost` 等安全上下文向 `ws://127.0.0.1` 建连会被 `WKWebView` 按 W3C Mixed Content 拦截（`NSAllowsLocalNetworking` 无法绕过），迫使本机与局域网通信割裂为两套传输适配；叠加未付费 Apple Developer ID 公证在另一台 Mac 触发 Gatekeeper 拦阻、`wry` 合成器帧冻结风险，以及面向 NAS 常驻时需额外维护独立无头二进制。
- **Bevy 2D / Macroquad（原生 + WASM）**：Bevy WASM 产物即使压缩仍达 10–20+ MB，在 iOS Safari 易触发内存 OOM；Macroquad 缺乏现代 UI 布局与原生态中文字体排版。两者的 CJK 界面与双端维护成本远高于 PixiJS + HTML/CSS DOM 叠层。
- **Rust 核心编译 WASM 共享给前端**：权威麻将牌局的牌墙与他家手牌不可下发前端，前端无独立运行引擎或 AI 的需求；引入 `wasm-bindgen` 只会增加双端构建链路与序列化胶水，无实际收益。
- **Bun + TypeScript 全栈**：`bun build --compile` 单二进制基线达 57–65+ MB，且单线程 JS 事件循环在执行百毫秒级强 AI 搜索时易阻塞心跳，桥接 Rust FFI 又会丧失纯 TS 简洁性。
- **Electron**：项目硬性排除。

## Consequences

- **对等单二进制宿主机与常驻兼容**：两台 Apple Silicon Mac 运行同一份无原生 GUI 的单二进制；默认桌面模式自动打开本机浏览器，同时提供无头参数（如 `--headless --port <p>`）供 Mac mini 或 Linux NAS 零代码分叉常驻部署。
- **纯 Rust 零 C/FFI 发现依赖**：后端明确不引入绑定苹果 `dns_sd.h` FFI 的 `async-dnssd`（避免交叉编译 Linux NAS 时依赖 `libavahi-compat-libdnssd`），局域网接入以零原生依赖的局域网 IP 二维码 + 系统原生 `<主机名>.local` 为基准。
- **8GB Mac 宿主机资源预算**：服务端采用纯 CPU 博弈 AI（零神经网络/零显存，总 RSS 约 30–80 MB），但需将 Rayon 思考池默认设为温和线程上限（如 `min(逻辑核数 / 2, 4)`，可配置）以防满核抢占同机 Safari 60fps 渲染；PixiJS 单张图集与 `cacheAsTexture` 严控 ≤ `2048×2048`，兼顾 8GB 统一内存与 iPhone Safari 上限。
- **iPhone 离线边界**：iPhone 定位为纯浏览器瘦客户端；当家庭局域网内无任何 Mac 或常驻宿主机运行时，v1 不支持 iPhone 独立离线单人打牌。
