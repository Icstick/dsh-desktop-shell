---
id: ADR-0005
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0005: Independently Versioned Capability Contracts

## 背景

单一 DesktopBridge 或全局协议版本会让 Runtime、Browser、Terminal 等不相关领域一起升级，并鼓励万能 RPC。

## 决策

每项 capability 使用独立 apiVersion+kind、requires/supports、typed invocation 和 error model。Capability lease 绑定 participant/activation/session/scope/owner。私有初版使用明确实验命名空间。

## 替代方案

- EverythingDesktopCanDo：拒绝。
- DesktopProtocolVersion=N：拒绝。
- 独立 capability：采用。

## 后果

Schema、fixture 和 migration 数量增加；但模块可独立演进、降级和映射未来标准。

## 验证门禁

- 所有 public operation 有 Schema、owner、tracking 和 fixture plan。
- 未协商 capability 返回 UNAVAILABLE/UNSUPPORTED_VERSION。
- unload/disconnect 可撤销 lease。

## 受影响模块

capability-contracts、local-transport、全部 provider/adapter
