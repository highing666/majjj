# IPv6 `远程联机`运维与 Surge Enhanced Mode 排查指南

本文档用于指导宿主机（默认 macOS，兼顾 Linux 无头部署）在家庭宽带公网 IPv6 环境下安全开启 Majjj **`远程联机`**，涵盖三层网络信任模型与明文威胁边界、家庭宽带与路由器 IPv6 防火墙放行、macOS 应用防火墙与 Ad-hoc 签名注意项、外部轻量 DDNS 配置、**Surge Enhanced Mode** 宿主机专项排查与连通性实测，以及二期本机外挂 TLS 反向代理升级 `https/wss` 的衔接要点。

> **隐私与零外部依赖边界（`ADR-0001` / `ADR-0012`）**
> - `mahjong-server` 运行时不内置 DDNS 客户端、不托管 TLS 证书、不主动访问任何第三方公网探测接口；路由器放行、DDNS 同步与外网连通性校验均通过本文档的独立运维步骤完成。
> - 本文所有示例地址与域名均为 RFC 标准文档占位符：公网域名采用 RFC 2606 **`majjj.example.com`**，公网 IPv6 采用 RFC 3849 **`2001:db8::/32`**（实际家庭宽带为运营商分配的 `2000::/3` 公网单播前缀；`mahjong-server` 地址发现会自动过滤文档前缀 `2001:db8::/32`），公网 IPv4 占位采用 RFC 5737 **`203.0.113.x`**。个人真实域名仅保存于本机 `preferences.json` 或启动环境变量中，切勿提交到仓库。

---

## 1. 架构概览、三层安全模型与日常操作习惯

### 1.1 默认双栈监听与按需启动参数

`mahjong-server` 默认通过单 socket 双栈绑定 **`[::]:8789`**（显式设置 `IPV6_V6ONLY = false`），在同一端口同时承接 IPv4（`127.0.0.1`、家庭局域网）与原生 IPv6（`::1`、公网单播）连接。仅当未显式指定 `--bind` / `MAJJJ_BIND` 且宿主机内核禁用 IPv6 时，才会打印告警并回退至 `0.0.0.0:8789`。

与`远程联机`相关的全部 CLI 参数与环境变量如下：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `--port <u16>` | `MAJJJ_PORT` | `8789` | TCP 监听端口 |
| `--bind <IpAddr>` | `MAJJJ_BIND` | `[::]`（双栈） | 显式绑定地址；指定后不会静默回退 IPv4 |
| `--allow-remote` | `MAJJJ_ALLOW_REMOTE` | `false` | 启动时是否立即开启`远程联机`（默认关闭，重启恢复关闭；开启时在标准输出打印 8 位`入桌口令`） |
| `--public-url <origin>` | `MAJJJ_PUBLIC_URL` | 无 | 运行时覆盖自定义公网入口（如 `http://majjj.example.com:8789`），优先级：`CLI/ENV > preferences.json > 首选稳定公网 IPv6` |
| `--headless` | `MAJJJ_HEADLESS` | `false` | 7×24 无头模式（禁用空闲自动退出与 `POST /api/shutdown`） |
| `--data-dir <path>` | `MAJJJ_DATA_DIR` | macOS: `~/Library/Application Support/majjj`<br>Linux: `~/.local/share/majjj` | 持久化数据目录，其中 `preferences.json` 的 `public_url_override` 保存本地自定义公网域名 |

### 1.2 三层信任边界（`peer + Host` 双校验）

服务端在 HTTP 与 WebSocket 路由树最高层统一执行 `peer + Host` 双校验分类器（**完全忽略 `X-Forwarded-For`、`Forwarded`、`X-Real-IP` 等代理头**），分别判定 TCP 对端 IP（经 `to_canonical()` 归一化）与 HTTP `Host` 头，并**取两者中更严格的层级**：

| 信任层级 | TCP `peer` 条件 | HTTP `Host` 条件 | 典型访问方式 |
| --- | --- | --- | --- |
| **`Loopback`**（本机宿主） | `127.0.0.0/8` 或 `::1`（含 `::ffff:127.0.0.1`） | `localhost`、`127.0.0.1`、`[::1]` / `::1` | 宿主机打开 `http://127.0.0.1:8789` |
| **`LAN`**（家庭局域网） | `Loopback`、RFC 1918（`10/8`、`172.16/12`、`192.168/16`）、`169.254/16`、`fe80::/10`、`fc00::/7` | 上述范围或 `*.local`（如 `macbook.local:8789`） | 家人手机扫码连 `http://192.168.5.10:8789` |
| **`Remote`**（公网与非局域网域名） | 一切公网单播 IPv6（`2000::/3`，**含与宿主机同 `/64` 网段**）、公网 IPv4、CGNAT `100.64/10` 或缺失对端 | 公网域名（如 `majjj.example.com:8789`）、公网 IP、缺失/重复/畸形 `Host` | 远程朋友经公网 IPv6 / 自有域名访问，或本机 `127.0.0.1` 反代转发的公网域名请求 |

三层权限与安全防护矩阵：

| 能力 / 端点 | `Loopback` | `LAN` | `Remote`（未开启） | `Remote`（开启·未认证） | `Remote`（开启·有效 Cookie） |
| --- | --- | --- | --- | --- | --- |
| 宿主控制面：`GET /api/remote-access`、`POST /api/remote-access/{enable,disable,reset-code,public-url}`、`POST /api/shutdown`（非 headless） | 允许 | `403` | `403` | `403` | `403` |
| 家庭数据与管理：`GET /api/health`、`GET /api/stats`、`GET /api/preferences`、`GET /api/history/export`、`POST /api/history/clear`、`GET /api/persona-memory`、`POST /api/persona-memory/clear` | 允许 | 允许（零口令） | `403` | `403` | `403` |
| 门禁入口 `GET /`（含 `/?code=...`） | SPA | SPA | 极简 `403`（零指纹） | 内联中文门禁页 HTML（`no-store`，零主包） | SPA（自动清除地址栏 `?code=`） |
| 口令换票 `POST /api/remote-auth` | 允许（开启时） | 允许（开启时） | `403` | 允许（`application/json` + 双层防爆破） | 允许 |
| 静态资源、`/ws`、`/api/banter-audio/{banter_id}` | 允许 | 允许 | `403` | `403` | 允许（`TableSnapshot` 清空 `lan_urls`） |

附加安全机制：

- **动态`入桌口令`与内存态会话**：每次开启自动由系统密码学随机源生成 8 位去混淆字母数字`入桌口令`（字符集 `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`，约 40 bit 熵；校验时忽略首尾空白、大小写不敏感、常量时间比对），通过后签发 256-bit 随机 `majjj_remote_session`（`HttpOnly; SameSite=Lax; Path=/`）。口令与会话令牌纯内存保存，绝不落盘。
- **双层防爆破限流**：单来源桶（IPv6 按 **`/64` 前缀**归桶、IPv4 按 `/32` 归桶）5 分钟内连续失败 5 次锁定 5 分钟；全局桶 1 分钟内累计失败 20 次触发 60 秒全局冷却，触发后返回 `429 Too Many Requests` 与 `Retry-After`；校验成功清除该来源桶计数。
- **跨站与牌桌边界**：`GET /ws` 强制同源 `Origin` 与 `Host` 一致（防 CSWSH）；变更类 `POST /api/*` 强制 `Content-Type: application/json`（防简单请求 CSRF）；远程连接可中途接管空余 AI 座次，但**严禁接管任何人类离线座次**（断线仅凭自身 `seat_token` 自动重连），且对远程`快捷互动`执行 4 秒冷却以保护局域网 GPU 推理队列。

### 1.3 公网明文 `http/ws` 威胁边界与推荐日常操作习惯

一期默认采用纯 `http://` + `ws://` 零证书直连，这意味着**数据在公网传输链路中为明文且无传输层服务端身份认证**。请清晰认知以下威胁边界并养成日常操作习惯：

- **明文边界**：若远程牌友身处不可信公共 Wi-Fi，或中间网络链路被窃听/劫持，请求中的 `?code=...`、`POST /api/remote-auth` 请求体、`majjj_remote_session` 会话 Cookie 及 WebSocket 牌局明文帧存在被嗅探或篡改的风险。若对传输加密有硬要求，请按第 5 节外挂本机 TLS 反向代理升级 `https/wss`。
- **打完即关（一键熔断）**：日常仅在约局时从宿主机本机界面（大厅右侧栏「远程联机」卡片，或`对局`进行中顶部工具栏「远程联机」按钮）点击「一键开启远程联机」；散场后立即点击**「关闭远程联机」**（或关闭服务端进程）。关闭操作会瞬间清空口令与全部远程 Cookie，并向所有在线远程 WebSocket 推送断连通知后立即断开。
- **口令不复用，泄漏即轮换**：不要长期把同一`入桌口令`留在公开群聊或固定便签中。若担心邀请链接被转发给无关人员、但又不想打断正在同桌打牌的朋友，点击**「重置口令」**即可作废旧口令并生成新 8 位口令，已通过认证的远程牌友可继续完成当前`对局`。
- **外出零暴露**：携带笔记本到咖啡馆或公共 Wi-Fi 时保持`远程联机`关闭（默认态）；即使忘记关闭，只要重启服务即自动恢复关闭。

---

## 2. 家庭宽带实操清单

### 2.1 确认公网 IPv6 `/64` 与区分稳定 / 临时 / 废弃地址

在宿主机 Mac 终端检查实际联网物理网卡（Wi-Fi 或有线网卡通常为 `en0` / `en1`，可用 `networksetup -listallhardwareports` 确认）：

```bash
# 查看物理网卡 IPv6 地址与标记
ifconfig en0 | grep -E 'inet6|status:'

# 确认 mahjong-server 已在双栈端口监听（需先启动服务）
lsof -nP -iTCP:8789 -sTCP:LISTEN
```

典型输出解读（以下地址均以 RFC 3849 `2001:db8::/32` 演示）：

```text
inet6 fe80::1234:56ff:fe78:9abc%en0 prefixlen 64 secured scopeid 0x4
inet6 2001:db8:abcd:1234:a1b2:c3d4:e5f6:0010 prefixlen 64 autoconf secured
inet6 2001:db8:abcd:1234:9876:5432:10fe:dcba prefixlen 64 autoconf temporary
inet6 2001:db8:0000:9999:a1b2:c3d4:e5f6:0010 prefixlen 64 deprecated autoconf secured
```

| 地址特征 | 类型 | 是否可用于远程入站 | `mahjong-server` 处理策略 |
| --- | --- | --- | --- |
| `fe80::...` | 链路本地（Link-Local） | 否（仅同局域网链路有效） | 自动排除 |
| `fc00::/7`（常见 `fd...`） | 唯一本地地址（ULA） | 否（内网私有） | 自动排除 |
| `2000::/3` + `autoconf secured`（无 `temporary`、`deprecated`） | **稳定公网单播 IPv6** | **推荐首选**（后缀稳定，适合路由器放行与 DDNS） | 通过 BSD `SIOCGIFAFLAG_IN6` 识别并排在首位 |
| `2000::/3` + `autoconf temporary` | macOS 隐私扩展临时地址 | 仅作临时兜底（会定期轮换） | 仅在无稳定地址时降级兜底 |
| 含 `deprecated` | 旧宽带前缀废弃地址 | 否（ISP 已切换新前缀） | 自动排除 |

> **未看到 `2000::/3` 公网 IPv6 怎么办？**
> 1. 登录光猫与主路由器管理后台，确认宽带拨号已开启 IPv6（PPPoE 双栈 / DHCPv6-PD 获取 `/56` 或 `/60` 前缀），且 LAN 侧已开启 SLAAC（RA）或 DHCPv6 分配 `/64`。
> 2. 确认 macOS「系统设置 → 网络 → 当前网络 → 详细信息 → TCP/IP → 配置 IPv6」为**「自动」**。

### 2.2 路由器 IPv6 防火墙入站放行原则

IPv6 没有 IPv4 的 NAT 端口映射概念，家庭内每台设备都直接持有公网 IPv6 地址，由主路由器（以及未改桥接的光猫）的**状态化 IPv6 防火墙**拦截公网主动入站流量。

配置入站放行时，必须坚守**最小暴露原则**：

1. **绝不关闭全局 IPv6 防火墙**：关闭全局防火墙会瞬间将家庭局域网内所有设备的 SSH、SMB、NAS、智能家居以及局域网 GPU 推理节点（如 Windows 主机 `8080` LLM / `8081` TTS）全部暴露到公网！
2. **仅放行宿主机单机 + 目标 TCP 端口 `8789`**：
   - 方向：`WAN → LAN`（转发 / Forward 入站）
   - 协议：`TCP`
   - 源地址：任意（`::/0`）
   - 目标地址：宿主机 Mac 的**稳定公网 IPv6**（例如 `2001:db8:abcd:1234:a1b2:c3d4:e5f6:0010/128`）
   - 目标端口：`8789`
   - 动作：`ALLOW / ACCEPT`
3. **应对运营商前缀轮换（推荐后缀掩码匹配）**：家庭宽带重拨后前 64 位前缀可能变化，但 macOS 的稳定地址（`autoconf secured`）后 64 位接口标识符（IID）在同一网络下保持不变。若路由器固件支持**负掩码 / 后缀匹配**（如 OpenWrt `::a1b2:c3d4:e5f6:0010/::ffff:ffff:ffff:ffff`、RouterOS、部分爱快/梅林/UniFi 固件），优先按**后 64 位 IID + TCP `8789`** 编写规则，前缀变动后无需手工改防火墙。
4. **排查光猫双层拦截**：若光猫处于路由模式（而非桥接由主路由拨号），光猫自身往往带有不可见或独立的 IPv6 防火墙，会直接丢弃公网入站 SYN 包。建议将光猫改为桥接模式由主路由统一拨号，或在光猫侧同样配置仅放行 `TCP 8789` 的精细规则（切勿对整台主机开 DMZ 或关光猫全局 IPv6 防火墙）。

### 2.3 macOS 应用防火墙与 Ad-hoc 签名注意项

使用 `./scripts/install-local.sh` 安装后，真实监听端口的二进制位于：

- 安装态二进制：`~/Library/Application Support/majjj/bin/majjj`（由 `~/Applications/Majjj.app` 后台拉起）
- 开发态直跑二进制：`target/release/mahjong-server` 或 `target/debug/mahjong-server`

需要注意 **Ad-hoc 代码签名**对 macOS 应用防火墙的影响：

- `scripts/install-local.sh` 使用 `codesign --force --sign -` 对 `majjj` 执行 **Ad-hoc 签名**（无 Apple Developer ID 证书）。
- 每次重新编译或重新安装（`./scripts/install-local.sh --build`），二进制的 CDHash 都会变化。若系统启用了 macOS 应用防火墙（**系统设置 → 网络 → 防火墙**），系统可能重新弹窗询问「是否允许应用程序 `majjj` 接受传入网络连接？」，或者旧防火墙条目仍绑定旧 CDHash，导致**本机 `127.0.0.1:8789` 正常但局域网与公网 IPv6 入站一律超时**。

使用系统自带工具核查与刷新放行规则（需管理员权限）：

```bash
# 1. 查看 macOS 应用防火墙状态与已登记应用列表
/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
/usr/libexec/ApplicationFirewall/socketfilterfw --listapps

# 2. 若已开启防火墙且重编后入站被拒，删除旧条目并重新放行当前二进制
MAJJJ_BIN="$HOME/Library/Application Support/majjj/bin/majjj"
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --remove "$MAJJJ_BIN"
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add "$MAJJJ_BIN"
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp "$MAJJJ_BIN"
```

> 若你是通过终端直接运行 `cargo run` 或 `target/release/mahjong-server` 进行联调，防火墙放行路径需指向对应可执行文件本身，而不是 `Majjj.app` 启动器外壳。

### 2.4 使用外部轻量 DDNS 对接自有子域名（以 `majjj.example.com` 占位）

裸 IPv6 邀请链接（`http://[2001:db8:abcd:1234::10]:8789/?code=...`）虽可直接使用，但有两个现实痛点：微信等聊天工具经常无法完整识别带方括号的 IPv6 超链接导致断链；运营商轮换 `/64` 前缀后，远端浏览器 Origin 改变会丢失已保存的昵称与重连 `seat_token`。配置自有子域名即可彻底解决。

操作步骤：

1. **在 DNS 服务商创建纯 `AAAA` 记录**：
   - 主机记录：`majjj`（完整域名占位 `majjj.example.com`）
   - 记录类型：**仅创建 `AAAA`**，指向宿主机稳定公网 IPv6（如 `2001:db8:abcd:1234:a1b2:c3d4:e5f6:0010`）
   - **不要添加不可达的 `A` 记录**：若没有公网 IPv4 映射却保留了旧 `A` 记录，部分客户端浏览器可能先尝试 IPv4 或在连接异常时产生额外等待。
   - TTL 建议设为 `60`–`300` 秒，便于前缀轮换后快速生效。
2. **使用外部轻量 DDNS 工具自动更新 `AAAA`**：
   - 可在宿主机 Mac（如 `ddns-go`、轻量定时脚本调用 DNS API）或支持指定内网主机 MAC/后缀的路由器插件上运行 DDNS，**切勿把云厂商 API 密钥写入 Majjj 仓库**。
   - **关键点一（选对宿主机）**：若 DDNS 跑在路由器上，请确保更新的是**宿主机 Mac 的 IPv6 地址**，而不是路由器 WAN 口自身的 IPv6 地址！
   - **关键点二（选对网卡与稳定地址）**：在宿主机运行 `ddns-go` 等工具时，优先选择「从物理网卡获取（如 `en0`）」，避免选中 `utun*`、`fe80::`、`temporary` 或 `deprecated` 地址。
3. **校验 DNS 解析结果**：
   ```bash
   # 应返回宿主机 en0 的稳定公网 IPv6
   dig +short AAAA majjj.example.com
   # 若无公网 IPv4 入口，应为空
   dig +short A majjj.example.com
   ```
4. **在 Majjj 中绑定自定义公网入口（两种方式任选其一）**：
   - **界面持久化（推荐）**：在宿主机打开 `http://127.0.0.1:8789` → 「远程联机」面板 → 展开**「自定义公网域名配置」** → 输入 `http://majjj.example.com:8789` → 点击**「保存域名」**。服务端会严格校验格式（仅允许 `http://` 或 `https://` + 主机 + 可选端口，自动去尾部 `/`）并原子写入本地 `preferences.json`。
   - **CLI / 环境变量覆盖（适合无头或脚本启动，优先级高于本地配置）**：
     ```bash
     MAJJJ_PUBLIC_URL="http://majjj.example.com:8789" mahjong-server --allow-remote
     # 或显式传参
     mahjong-server --public-url "http://majjj.example.com:8789" --allow-remote
     ```
5. **宽带前缀刚变更、DDNS 缓存未刷新的应急方案**：在宿主「远程联机」面板展开**「备选裸 IPv6 地址」**，临时下拉切换到最新的稳定裸 IPv6 地址，邀请链接与二维码会立即切换为可用裸 IPv6 救急，待 DNS 生效后点击「切回自定义公网域名」即可。

---

## 3. Surge Enhanced Mode 宿主机专项核查清单

若宿主机 Mac 常驻运行 **Surge（Enhanced Mode）**，排查本机联机与外网入站时优先使用 `surge-cli`（若未加入 `PATH`，路径为 `/Applications/Surge.app/Contents/Applications/surge-cli`）。

### 3.1 确认 `ipv6-vif` 未劫持 IPv6 入站

先做被动快照检查，确认 Surge 当前配置、主网卡与 IPv6 设置：

```bash
# 1. 查看当前运行状态与网络摘要（被动查询，不改动配置）
surge-cli status
surge-cli dump summary

# 2. 检查当前配置 [General] 中与 IPv6 / VIF 相关的项
surge-cli profile section get General | grep -iE 'ipv6|vif|enhanced'
```

核查要点：

- **`ipv6 = true`**：确保 Surge 允许 IPv6 正常通信（避免本机外发或 DNS `AAAA` 解析被意外禁用）。
- **`ipv6-vif` 建议保持 `disabled`（默认）**：Surge Enhanced Mode 默认仅通过虚拟网卡（`utun*`）接管 IPv4 流量，不接管 IPv6 VIF，因此公网直连宿主机物理网卡（`en0`）的 IPv6 入站 TCP 包会直接由 macOS 内核交付给监听在 `[::]:8789` 的 `mahjong-server`，完全不经过 Surge 虚拟网卡。若你此前手动将 `ipv6-vif` 设为了 `always` 或 `auto`，请排查其是否对入站或回程路由产生干扰，必要时改回 `ipv6-vif = disabled` 并重载配置。
- **如开启了 Surge Mac 网关模式（Gateway Mode / VMNET）**：可用 `surge-cli vmnet status` 与 `surge-cli vmnet ra` 核查 IPv6 路由通告（RA）接管状态，避免影响局域网设备获取正常前缀与路由。

### 3.2 为何 `utun` 虚拟网卡已被 `mahjong-server` 排除

开启 Surge Enhanced Mode（或同时使用 VPN / Tailscale / WireGuard）后，macOS 会多出若干 `utun*` 虚拟接口，其上往往挂载着 `198.18.0.1` 等 Fake-IP 地址或隧道专属 IPv6 地址。

`mahjong-server` 的网络发现逻辑（`crates/mahjong-server/src/lan.rs`）已内建排除以下虚拟与本地接口前缀：

- **`utun`**（Surge Enhanced Mode / VPN / 隧道虚拟网卡）
- **`bridge`**、**`awdl`**、**`llw`**、**`lo`**

同时会排除链路本地（`fe80::/10`）、ULA（`fc00::/7`）、多播、文档前缀（`2001:db8::/32`）、IPv4 映射及 `deprecated` 地址，并将非 `temporary` 的稳定公网单播 IPv6 排在最前。因此即使 Surge 常驻运行，「远程联机」面板展示的公网 IPv6 候选地址也只会来自宿主机真实物理网卡，绝不会误拼出 `utun` 虚拟地址。

### 3.3 在个人本地 Surge 模块中为自有子域名配置 `DIRECT` 规则

当宿主机（或同局域网安装了 Surge 的手机/电脑）访问 `http://majjj.example.com:8789` 进行自测时，如果规则集未命中直连，请求会被发往外部代理节点再绕回家庭宽带（Proxy Hairpin）：不仅增加延迟，若默认代理节点不支持 IPv6 出站还会直接连接失败；若全局开启了 MITM / 脚本改写，还可能干扰 WebSocket 升级。

推荐在**宿主机本地个人 Surge 模块**中添加一条精确直连规则（**切勿把包含真实域名的 `.sgmodule` 提交到本仓库**）：

```ini
#!name=Majjj Local Direct
#!desc=Route personal Majjj IPv6 domain directly without proxy hairpin
#!system=mac,ios

[Rule]
DOMAIN,majjj.example.com,DIRECT
```

使用 `surge-cli` 验证规则与 DNS 解析是否生效（调试期也可先用临时规则验证，重启 Surge 自动失效）：

```bash
# 可选：先加一条内存临时规则验证（最高优先级，无需先改文件）
surge-cli rule temp add "DOMAIN,majjj.example.com,DIRECT"

# 1. 离线解释路由决策，确认最终策略为 DIRECT 且来源符合预期
surge-cli rule explain "http://majjj.example.com:8789"

# 2. 检查 Surge DNS 解析出的 AAAA 记录是否为宿主机最新稳定公网 IPv6
surge-cli dns lookup majjj.example.com

# 若刚更新过 DDNS 但命中旧缓存，刷新 Surge DNS 缓存
surge-cli flush dns
```

> **本机自测门禁页小贴士**：在宿主机直接访问 `http://127.0.0.1:8789` 属于 `Loopback`，不会弹出`入桌口令`门禁页；而当 `majjj.example.com` 命中 `DIRECT` 后，在宿主机浏览器访问 `http://majjj.example.com:8789` 时，由于 HTTP `Host` 头为公网域名，服务端会严格按 **`Remote`** 处理，正好可以真实演练门禁页、口令校验与远程视图裁剪。

### 3.4 借助 `surge-cli` 或外网节点实测 `[公网IPv6]:8789` 真实入站连通性

在本机直连测试不会经过运营商公网与路由器 IPv6 入站防火墙。要确认远程朋友真的能连进来，必须从**公网外部路径**发起探测（例如用手机关闭 Wi-Fi 走 5G 蜂窝网络，或通过一台**支持 IPv6 出站的外部 Surge 代理节点**，下例以策略名 `"My-IPv6-Node"` 占位）。

#### 方式 A：使用 `surge-cli http probe` 快速校验 TCP 与防火墙连通性

> **注意**：`surge-cli http probe` 发送的是 **`HTTP HEAD`** 请求。根据 `mahjong-server` 的零指纹门禁策略，未认证公网访问仅对 `GET /` 返回门禁页 HTML，对 `HEAD /` 无论`远程联机`开启与否均返回 **`HTTP 403`**（附 `Cache-Control: no-store`）。因此只要收到 **`403`**（而非 `Connection refused` / `Timeout` / `No route to host`），就证明「外网节点 → 运营商 IPv6 → 路由器入站放行 → macOS 防火墙 → `mahjong-server`」全链路已打通！

```bash
# 1. 先在「远程联机关闭」状态探测裸 IPv6 与自定义域名（期望均快速返回 HTTP 403）
surge-cli http probe "http://[2001:db8:abcd:1234:a1b2:c3d4:e5f6:0010]:8789/" "My-IPv6-Node"
surge-cli http probe "http://majjj.example.com:8789/" "My-IPv6-Node"

# 2. 确认受保护管理端点在外网同样被严密拦截（即使开启远程联机也恒为 HTTP 403）
surge-cli http probe "http://majjj.example.com:8789/api/health" "My-IPv6-Node"
```

#### 方式 B：用外网环境（手机蜂窝网络 / 外部 IPv6 主机）实测完整 `GET /` 门禁页与打牌链路

在宿主界面点击「一键开启远程联机」后，使用**未连家庭 Wi-Fi 的手机蜂窝网络**（或外部 IPv6 主机 `curl -6`）验证：

```bash
# 1. 不带口令访问根路径：应返回 HTTP 200 与极简中文「请输入入桌口令」门禁页（Cache-Control: no-store）
curl -6 -i --max-time 5 "http://[2001:db8:abcd:1234:a1b2:c3d4:e5f6:0010]:8789/"
curl -6 -i --max-time 5 "http://majjj.example.com:8789/"

# 2. 未认证访问受限路径：应返回极简 HTTP 403
curl -6 -i --max-time 5 "http://majjj.example.com:8789/api/stats"
```

随后在手机（5G 蜂窝网络）微信或浏览器中点开宿主复制的带码邀请链接 `http://majjj.example.com:8789/?code=XXXXXXXX`，确认：

1. 自动通过口令验证并跳回 `/`（地址栏 `?code=` 已抹除）；
2. 大厅不显示局域网扫码卡片、不显示历史战绩与`牌友记忆`管理入口、不显示关服按钮；
3. 入座后能正常听见`角色语音包`与`台词语音`、发送`快捷互动`；
4. 宿主机点击「关闭远程联机」后，手机端立即断开并展示明确提示，不再无限重连。

---

## 4. 常见故障排查

| 现象 | 常见原因 | 排查与解决 |
| --- | --- | --- |
| 本机 `127.0.0.1:8789` 正常，外网探测一直超时（Timeout） | 路由器或光猫 IPv6 防火墙未放行 `TCP 8789`；或 ISP 前缀轮换导致静态全地址规则失效 | 检查 `ifconfig en0` 最新稳定 IPv6 是否与路由器规则一致；确认光猫是否处于路由模式二次拦截；优先改用路由器后缀（IID）匹配规则 |
| 局域网与公网入站同时超时，但路由器规则正确 | macOS 应用防火墙拦截了重新编译后 Ad-hoc 签名的 `majjj` 二进制 | 执行第 2.3 节 `socketfilterfw --remove / --add / --unblockapp` 刷新规则 |
| 外网 `http://[2001:db8:...]:8789/` 可通，但域名 `majjj.example.com:8789` 超时或连错设备 | DDNS 更新成了路由器 WAN IPv6、选中了 `temporary`/`deprecated` 地址，或存在不可达 `A` 记录 | 对比 `dig +short AAAA majjj.example.com` 与宿主机 `ifconfig en0` 稳定 IPv6；删除多余 `A` 记录；执行 `surge-cli flush dns` |
| 宿主机或家人用域名自测很慢或失败 | Surge 将 `majjj.example.com` 路由到了代理节点（Proxy Hairpin）或触发了 MITM | 运行 `surge-cli rule explain "http://majjj.example.com:8789"` 确认命中的策略，并按第 3.3 节添加 `DOMAIN,majjj.example.com,DIRECT` |
| 外网探测报 `Connection refused` | `mahjong-server` 未启动，或显式传了 `--bind 127.0.0.1` / `--bind 0.0.0.0` 仅监听了 IPv4 | 检查 `lsof -nP -iTCP:8789 -sTCP:LISTEN` 确认处于双栈 `[::]:8789` 监听，去掉限制性 `--bind` 参数 |
| 「远程联机」面板提示未检测到公网 IPv6 | 宽带未分配 `2000::/3` 公网前缀，或只有 `fe80::`/`fc00::`/`deprecated` 地址 | 检查 `ifconfig en0`；若仅有临时或无公网 IPv6，检查路由器 DHCPv6-PD / SLAAC 设置 |
| 输错几次口令后返回 `429` | 触发了 IPv6 `/64` 同网段防爆破锁定（5 分钟内错 5 次锁 5 分钟）或全局冷却（1 分钟错 20 次锁 60 秒） | 等待响应头 `Retry-After` 提示的冷却时间结束，核对宿主面板当前 8 位口令后重试 |
| 远程朋友无法接管离线人类座次 | 安全策略强制禁止远程连接接管任何人类离线座次（防偷看暗牌） | 原离线人类玩家用原浏览器自动凭 `seat_token` 重连即可；远程朋友仅允许在中途接管空余 AI 座次 |
| 部署脚本 `scripts/deploy-partner.sh` 传入裸 IPv6 地址 | 需按标准格式传递主机地址 | `scripts/deploy-partner.sh` 已内置裸 IPv6 括号拼接兼容（自动组装 `http://[<ipv6>]:8789/api/health`），直接传入 IPv6 地址即可 |

---

## 5. 二期演进：外挂本机 TLS 反向代理升级 `https/wss`

若未来希望在公共 Wi-Fi 等不可信网络中为公网联机补充完整的传输层加密（`https://` + `wss://`），无需修改 `mahjong-server` 源码，只需在宿主机本机外挂 **Caddy** 或 **Nginx** 反向代理至 `http://127.0.0.1:8789` 即可。

### 5.1 拓扑与证书要点（DNS-01 + 非标端口）

家庭宽带普遍封禁入站 TCP `80` / `443`，因此必须采用 **ACME DNS-01** 自动签发 `majjj.example.com` 的 TLS 证书，并在非标端口（例如 **`8443`**）终止 TLS：

```text
远程浏览器 (https://majjj.example.com:8443 / wss://.../ws)
    │  公网 IPv6 TCP 8443（路由器改为仅放行 TCP 8443，关闭外网 8789）
    ▼
宿主机本机 Caddy / Nginx ([::]:8443, DNS-01 TLS 证书)
    │  本机回环 HTTP/WS (127.0.0.1:8789，原样保留 Host: majjj.example.com:8443)
    ▼
mahjong-server ([::]:8789)
```

配套调整：

1. 路由器 IPv6 入站规则改为**仅放行宿主机 `TCP 8443`**，并**移除外网 `TCP 8789` 放行规则**（避免远程流量绕过 TLS 直连明文端口；家庭局域网设备仍继续直连 `http://192.168.x.x:8789` 零口令免摩擦）。
2. 在宿主「远程联机」面板（或 `--public-url` / `MAJJJ_PUBLIC_URL`）将自定义公网入口更新为 **`https://majjj.example.com:8443`**。

### 5.2 最小反代配置参考

**Caddy 示例（推荐，默认自动透传客户端原始 `Host` 头并原生支持 WebSocket）**：

```caddyfile
https://majjj.example.com:8443 {
    tls {
        # 需使用带对应 DNS 插件的 Caddy 构建，凭据通过环境变量注入
        dns cloudflare {env.DNS_API_TOKEN}
    }
    reverse_proxy 127.0.0.1:8789
}
```

**Nginx 示例（配合 `acme.sh` DNS-01 证书）**：

```nginx
server {
    listen [::]:8443 ssl http2;
    server_name majjj.example.com;

    ssl_certificate     /path/to/fullchain.pem;
    ssl_certificate_key /path/to/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8789;
        proxy_http_version 1.1;
        # 必须原样保留含端口的原始 Host 头！切勿改写为 127.0.0.1 或 localhost
        proxy_set_header Host $http_host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
    }
}
```

### 5.3 `peer + Host` 双校验如何从架构上防止反代权限逃逸

传统仅看 TCP 来源 IP 的服务端在套上本机反向代理后存在严重漏洞：因为反向代理从 `127.0.0.1` 连向后端，服务端容易把所有公网请求误判为本机 `Loopback` 管理员。

Majjj 的 `peer + Host` 双校验机制在设计上彻底封堵了这一逃逸路径：

1. **防越权判定**：当本机 Caddy/Nginx 将公网访问 `https://majjj.example.com:8443` 的请求转发至 `127.0.0.1:8789` 时，虽然 TCP `peer` 为 `127.0.0.1`（`Loopback`），但保留后的 HTTP `Host` 头为 `majjj.example.com:8443`（`Remote`）。双校验取两者更严格者，最终仍严格判定为 **`Remote`**——所有公网访客依然必须经过`入桌口令`门禁，依然无法访问 `/api/stats`、`/api/preferences`、`/api/history/*`、`/api/persona-memory*`、`/api/health`、`/api/shutdown` 与 `/api/remote-access*`，`TableSnapshot` 依然清空 `lan_urls`。
2. **WS 同源校验兼容**：`GET /ws` 会比对浏览器发送的 `Origin`（`https://majjj.example.com:8443`）与请求 `Host`（`majjj.example.com:8443`），因此原样保留带端口的 `$http_host` 同时也是 `wss://` 握手通过的必要条件。
3. **两条反代配置红线与二期演进备注**：
   - **严禁在反代层把 `Host` 头覆盖为 `127.0.0.1:8789` 或 `localhost:8789`**，否则会同时满足 `peer == Loopback` 与 `Host == Loopback`，人为破坏安全边界。
   - **防爆破源桶说明**：由于一期服务端有意忽略 `X-Forwarded-For` / `Forwarded` 代理头，所有经本机 `127.0.0.1` 反代转发的失败口令尝试会共用同一个 `127.0.0.1` 限流桶（单桶 5 分钟错 5 次触发 5 分钟锁定）。若二期高频使用本机 TLS 反代，可在反代侧叠加限流，或按需扩展显式 opt-in 的本机受信任反代透传与 `Secure` Cookie 标志。
