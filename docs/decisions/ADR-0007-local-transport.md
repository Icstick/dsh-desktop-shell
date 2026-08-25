---
id: ADR-0007
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0007: Native Local Transport with Loopback Fallback

## 背景

Desktop 与 DSH Adapter 需要本地 carrier。业务协议不能等待尚未稳定的标准 wire；loopback 方便但本地暴露面更大。

## 决策

Windows 优先 Named Pipe，macOS/Linux 优先 Unix Domain Socket；平台不可用或 PoC 未完成时允许 127.0.0.1 随机端口 fallback。每次实例使用随机 credential、instance identity、generation；绝不监听 0.0.0.0。

## 替代方案

- 固定 localhost 端口和全局 token：拒绝。
- 只用 native IPC：可能阻塞早期兼容。
- native-first + loopback fallback：采用。

## 后果

需要多 carrier contract tests；authentication、framing 和 semantics 分层，未来可替换标准 wire。

## 验证门禁

- Named-pipe ACL / UDS mode negative tests。
- replay/stale generation/malformed message。
- fallback 仅 loopback，credential 不写业务 body。

## 受影响模块

local-transport、supervisor、adapter-dsh
