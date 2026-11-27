# Majjj

面向家庭局域网与受邀远程联机的 4 人推倒胡麻将牌局（Rust 单二进制内嵌 Svelte 5 + PixiJS 2D 前端）。

## 启动前依赖

- **局域网推理服务（必需）**：服务端启动时探测 OpenAI 兼容 `llama-server`（默认 `http://192.168.5.50:8080`，可用 `--llm-url` 或 `MAJJJ_LLM_URL` 覆盖）。部署与运维见 [docs/windows-llm-deployment.md](docs/windows-llm-deployment.md)。
- **局域网台词语音服务（可选）**：默认连接 `http://192.168.5.50:8081`（不可达时自动降级，也可用 `--tts-url` 或 `--no-tts` 控制）。部署见 [docs/windows-tts-deployment.md](docs/windows-tts-deployment.md)。
- **远程联机（可选）**：支持双栈 IPv6 与入桌口令准入，详情见 [docs/ipv6-remote-deployment.md](docs/ipv6-remote-deployment.md)。

## 快速启动

### 1. 构建并安装本机桌面启动器（macOS 推荐）

```bash
./scripts/install-local.sh --build
open ~/Applications/Majjj.app
```

首次启动若系统弹出「本地网络」权限确认框，点**允许**后重新打开即可。

### 2. 命令行构建并运行单二进制

```bash
make dist-mac
./target/release/mahjong-server
```

默认监听 `[::]:8789` 并自动打开 `http://127.0.0.1:8789`（常用参数如 `--allow-remote`、`--no-open`、`--port` 可通过 `--help` 查看）。

### 3. 前后端开发热重载模式

```bash
# 终端 1：后端
cargo run -p mahjong-server -- --no-open

# 终端 2：前端（自动代理 /api 与 /ws 到 8789）
npm --prefix web run dev
```

浏览器访问 `http://127.0.0.1:5173`。

## 验证与检查

```bash
./scripts/check.sh        # 完整校验（等同 make check）
```
