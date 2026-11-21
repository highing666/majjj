---
status: accepted
---

# 单活跃牌桌局域网会话、WebSocket 快照同步、暂停优先重连与纯 Rust JSONL 战绩持久化

为了让家庭局域网内的双人/单人推倒胡牌桌在双 Apple Silicon Mac 与 iPhone Safari 间做到零账号零房间号摩擦、断线不毁手牌且保持纯 Rust 零 C/FFI 单二进制（兼顾未来无头 Mac mini / Linux NAS 迁移），我们决定将 `mahjong-server` 的会话与存储架构锁定为 **「单实例单活跃牌桌（`TableSession`）+ 固定端口 `0.0.0.0:8789` 与零 `mdns-sd` 接入」**、**「重连全量 `TableSnapshot` + 在线 `seq` 增量 + `visibilitychange` 主动探活 + `expected_seq` 幂等」**、**「断线默认无限期暂停与局内人机热切换（`seat_generation` 取消防竞态）」**，以及 **「仅持久化已完战绩的纯 Rust 本地 JSONL/JSON 存储」**。

## Considered Options

- **多房间并发与房间号列表**：在 2 人家庭局域网中强行引入建房、输入房间号与空房回收属于典型复杂度过剩，且与 `GLOSSARY.md` 以「对局」而非「房间」建模的领域语言背离；通过将单桌状态封装为独立 `TableSession`，未来若迁至常驻 NAS 确有多桌并发诉求仍可平滑扩展。
- **引入纯 Rust `mdns-sd` 广播自定义 `.local` 或 `_majjj._tcp` 服务**：全部客户端均为 Safari 浏览器（无法浏览 DNS-SD 服务记录），而在 macOS 原生 `mDNSResponder` 之外再运行独立 mDNS 应答器存在端口与记录冲突风险；直接使用系统原生 `<LocalHostName>.local:8789` 配合过滤虚拟网卡后的局域网 IPv4 二维码更稳健。
- **断线超时后自动转为 AI 托管**：家庭局域网已锁定无限时出牌，iPhone 切出去回复消息几十秒若被 AI 擅自拆牌或点炮体验极差；改为默认无限期暂停并仅允许其余在线人类手动触发临时 AI 托管。
- **嵌入式 SQLite（`rusqlite` bundled）与进行中牌局落盘断点续局**：`bundled` SQLite 会引入 C 编译链，削弱 ADR-0001 的纯 Rust 零 C/FFI 跨平台单二进制优势；且家庭数年数千小局仅数 MB，纯 Rust JSONL 启动毫秒级聚合已完全充裕。客户端断线（含 Mac 休眠、iPhone 锁屏、刷新）均由内存态无缝重连恢复，仅为极低频的「强制杀死服务端进程」引入状态机落盘与版本迁移收益过低。
- **按浏览器 `client_id` 聚合战绩**：家庭成员会在自己的 Mac 与 iPhone 间切换设备，按设备随机 UUID 统计会导致同一人的战绩被割裂；改为以规范化昵称作为战绩聚合主键、`client_id` 仅管理会话凭证。

## Consequences

- **零房间号接入与防双开复用**：服务端默认监听 `0.0.0.0:8789`（支持 `--port` 覆盖，绝不静默漂移随机端口以稳定 Safari 程序坞与书签）；排除 `utun`/`bridge`/`awdl`/`llw` 与 `100.64.0.0/10`、`198.18.0.0/15` 等代理/虚拟网段精选真实局域网 IPv4 生成二维码（并提供备选 IP 切换）；提供 `GET /api/health`，桌面模式遇端口被已有 `majjj` 实例占用时直接打开页面并退出。
- **三态生命周期与家庭对等控制**：`TableSession` 沿 `Lobby → InMatch → Finished` 流转，代码不硬锁 2 人上限（自然支持 1–4 人类入座，空位补 AI，默认新开对局随机定座）；无房主特权等级，天然适配无头 NAS，仅在 ≥2 名人类在线时对局间结束`自由桌`或强制解散未完`对局`要求双方确认（对方离线时允许在线方单方确认解锁）。
- **重连、顶号与跨设备接管**：同浏览器凭 `localStorage` 内的 `(table_id, seat_token)` 自动重连；重复标签页采用新连接顶替旧连接（下发 `SessionReplaced` 并禁止旧页自动重连，防互踢死循环）；严禁抢占在线人类座位，但允许新设备经二次确认接管「已离线」人类座位（签发新 `seat_token` 作废旧凭证），支持 Mac 与 iPhone 间无缝切设备继续同一手牌。
- **同步可靠性与人机热切换零竞态**：WS 建连/重连首帧统一发送全量 `TableSnapshot`（跳过陈旧补帧动画），在线期间以 `EventBatch(seq)` 增量驱动并支持断层自愈；以 `10s/25s` 应用层心跳、`visibilitychange` `2s` 探活与 `pagehide` 通知兜底 iOS 僵尸连接；客户端动作须带 `expected_seq` 与 `client_action_id` 严防连点重传。局内支持随时开关 AI 托管及第二人中途立即接管 AI 座位，异步 Rayon 思考绑定 `(match_id, expected_seq, seat_generation)`，人类接管即递增代数丢弃过期结果，杜绝“幽灵落子”。
- **防损坏 JSONL 战绩与双 Mac 独立边界**：默认存于 `~/Library/Application Support/majjj/`（Linux 为 `$XDG_DATA_HOME/majjj/`，支持 `--data-dir` 覆盖），每个`小局`结算瞬间即追加 `hands.jsonl` 并 `sync_data`，每场已结算 ≥1 `小局`的`对局`结束时追加 `matches.jsonl`，配置通过临时文件 + `sync_all` + 原子 `rename` 写入 `preferences.json`，均带 `schema_version` 且启动容忍异常断电导致的截断尾行。v1 双 Mac 各自独立存本地战绩，不做跨机同步。
