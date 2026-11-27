---
status: accepted
---

# Mac 极薄解耦启动器、双入口局域网接入与无隔离 SSH 推送分发

为了让非终端用户在 8GB Apple Silicon Mac 与 iPhone 上实现零门槛、零付费公证、零 Gatekeeper 拦阻的家庭局域网推倒胡游玩与版本更新体验，我们决定采用 **「用户级解耦极薄 `.app` 启动器（随开随关 + 分状态空闲自动退出）」**、**「本机单人与局域网双人双入口分工（统一 `http://127.0.0.1:8789` 与 `<LocalHostName>.local:8789` / 二维码）」**，以及 **「首次一次性初始化 + 开发机局域网 SSH 密钥一键推送原子替换」** 作为 v1 的 Mac 双机分发与启停方案。

## Considered Options

- **`launchd` 用户级后台常驻 + Safari「添加到程序坞」直连**：虽然能消除启停概念，但后台持续监听 `0.0.0.0:8789` 会在携带 Mac 外出接入公司或咖啡馆公共 Wi-Fi 时意外暴露牌桌与战绩，且与 `ADR-0005` 已锁定的桌面受控关服契约产生张力。
- **将纯 Rust `majjj` 二进制直接打包为 `Majjj.app/Contents/MacOS/majjj`**：纯 Rust 服务端不含 AppKit/Cocoa 事件循环，经 macOS LaunchServices 直接启动会触发程序坞弹跳与「应用程序无响应」状态，且用户关闭浏览器窗口后再次点击图标时系统仅发送重开事件而无法重新唤起牌桌；放在 `/Applications` 亦增加权限与整包覆盖摩擦。
- **增加原生 macOS 菜单栏状态图标（`NSStatusItem`）管理启停**：需要引入 Swift/ObjC 原生胶水代码与独立签名打包链路，违背纯 Rust 极简架构，而「前端受控关服 + 分状态空闲自动退出」已完整兜底直接关闭窗口的使用习惯。
- **对象 Mac 仅留单一本机入口、大厅自动探测开发机并跳转**：每次加入开发机开桌仍需先拉起对象 8GB Mac 的闲置本地服务端，并额外引入跨机探活状态处理。
- **AirDrop/浏览器下载分发、`majjj` 内置局域网自更新或 GitHub Releases 拉取**：在 macOS 15+ 环境下，经 AirDrop/浏览器传入的未公证二进制会被打上 `com.apple.quarantine` 且「右键 → 打开」已失效；`majjj` 内置自更新需引入 HTTP 客户端/TLS 与自替换重启复杂度；GitHub Releases 受国内网络波动与私有仓鉴权影响，对家庭双机场景属于过度设计。

## Consequences

- **用户级免 `sudo` 解耦布局与防失效启动**：启动器固定安装于 `~/Applications/Majjj.app`（仅含 `LSUIElement = true` 的 `Info.plist`、漫画风 `.icns` 图标与极薄 POSIX 启动脚本），独立单二进制安装于 `~/Library/Application Support/majjj/bin/majjj`（与 `ADR-0005` 战绩数据目录同域收口，双 Mac 完全对称）。点击 `Majjj.app` 时先探 `http://127.0.0.1:8789/api/health`，未运行则后台拉起 `majjj` 并等待就绪，随后立即退出启动器本身，确保重复点击随时重新唤起牌桌且永不触发 LaunchServices 无响应；启动失败或端口被非 `majjj` 占用时经 `osascript` 弹出简明中文原生告警并记录至 `~/Library/Logs/majjj/server.log`（启动截断/轮转）。
- **规范本地源与双入口零冗余进程**：本机单人访问统一锁定 **`http://127.0.0.1:8789`**（避免与 `localhost:8789` 混用割裂 Safari `localStorage` 内的昵称与 `seat_token`）；`Majjj.app` 优先经 `open -a` 唤起预装本机 Safari Web App（`~/Applications/Majjj桌面.app`，无地址栏且重复点击置顶复用、防触发 `SessionReplaced` 顶号），缺失时自动降级 `open http://127.0.0.1:8789`。双人联机时对象 Mac 通过 Safari「添加到程序坞」独立图标直连 `http://<宿主LocalHostName>.local:8789`（零本机 Rust 进程），对象 iPhone 以扫码为主、主屏幕快捷方式为辅；双 Mac 地位对等。
- **回环受控关服与分状态安全空闲退出**：桌面模式前端仅对本机回环连接显示「关闭本机服务」按钮，服务端 `POST /api/shutdown` 强制校验来源为 `127.0.0.1`/`::1`（`--headless` 模式禁用该接口）。桌面模式默认开启分状态空闲自动退出：`Lobby`/`Finished` 态且 `0` 活跃 WS 连接持续 **`15 分钟`** 自动优雅退服；`InMatch` 态为防锁屏/离桌误杀手牌，延长至 `0` 连接持续 **`4 小时`** 保底退出（`--headless` 无头常驻模式彻底禁用空闲退出）。
- **前端缓存一致性契约**：`rust-embed` 静态服务强制 `index.html` 与 `manifest` 返回 `Cache-Control: no-cache`、Vite 哈希产物返回 `public, max-age=31536000, immutable`、`/api/*` 返回 `no-store`，大厅页脚展示服务端版本号，确保更新后 Safari Web App 与 iPhone 绝不命中陈旧前端资源。
- **零隔离一键推送、防打断与快回滚脚本**：仓库提供 `scripts/install-local.sh`（双机共用本机安装）、`scripts/uninstall-local.sh`（清理应用但默认保留 `*.jsonl` 战绩）与 `scripts/deploy-partner.sh`（对象机地址与用户名存于 `.gitignore` 忽略的 `.deploy.local`，支持 `--host`/`--force`/`--rollback`/`--sync-app`）。推送前先探活对象机 `GET /api/health`，若检测到 `InMatch` 或仍有在线人类默认中止更新；空闲态经 SSH 优雅停服后用 `scp` 上传同目录临时文件（天然不写 `com.apple.quarantine`）、校验 SHA-256、执行 `chmod +x` 与 `codesign --force --sign - --identifier com.majjj.launcher`（锁定稳定 Bundle ID 以兼容 macOS 本地网络隐私，避免继承临时文件名；`--sync-app` 亦在对象机本地重签 `Majjj.app`）、备份旧版为 `majjj.prev` 并原子 `mv` 生效。v1 维持零成本 Ad-hoc 签名（macOS 防火墙默认关闭；若未来启用防火墙产生复弹窗，再按需切本机自签代码签名证书）。
