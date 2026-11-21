---
status: accepted
---

# 同源 WebSocket 与 UTF-8 JSON 实时通信协议

尽管 Safari 26.4 已上线 WebTransport，家庭局域网麻将仍决定锁定 **同源 `http://` 下的原生 `ws://` WebSocket + UTF-8 JSON（`serde_json`）+ Rust 权威结构体自动导出 TypeScript 类型（如 `ts-rs`）** 作为前后端唯一实时通信协议与类型契约。

## Considered Options

- **WebTransport（QUIC）**：W3C 规范将其限定在 `[SecureContext]`，家庭局域网纯 `http://` 页面直接不可用；若强上 HTTPS，则需在家庭设备手动信任自签根证书或维护 ≤14 天有效期的 `serverCertificateHashes` 证书轮换。此外 WebKit 存在连接级流控窗口未回补导致长连接永久卡死的已知缺陷（WebKit Bug #317084 / #319818），且旧版 iOS/macOS 不支持，对低频有序回合制麻将属于协议语义过剩。
- **二进制线上传输格式（MessagePack / CBOR / Protobuf）**：麻将每秒仅产生 0 至数条小数据包、单次状态快照约 1–2 KB，二进制编码在局域网节省的字节微不足道，却失去在 Safari DevTools 中直接明文检视协议帧的调试优势，并增加编解码/Schema 维护成本。
- **双端手写协议类型**：Rust 状态机枚举与字段演进时容易与前端 TypeScript 漂移。

## Consequences

- **零证书局域网直连**：同源 `http://` 页面发起 `ws://` 连接完全符合 W3C Mixed Content 规范，在 macOS 与 iOS Safari 均无需证书配置即可直连。
- **iOS 僵尸连接必须在会话层显式兜底**：iOS Safari 切后台或锁屏断连后 `WebSocket.readyState` 常假性维持 `OPEN` 且不立即触发 `onclose`，因此前端与服务端必须在会话协议中落实 `visibilitychange` 主动 Ping 探活、`pagehide` 清理及 Seat Token 全量权威状态快照重同步。
- **单一事实源契约**：所有前后端消息类型仅在 Rust 侧定义，并通过编译/测试流程校验导出的 TypeScript 定义与源码同步。
