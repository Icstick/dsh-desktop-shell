---
id: ADR-0004
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0004: Unmodified Upstream DSH Web UI

## 背景

DOM、router、CSS 或 renderer patch 会把 Desktop 与快速变化的上游 UI 结构绑定，并扩大 WebView privilege。

## 决策

DSH Web UI 作为 unprivileged remote WebView 原样承载。Activity Rail、overlay、settings 和 native surfaces 位于外层 Shell。禁止 DOM injection、renderer fork 和 unrestricted global bridge。

## 替代方案

- 深度 DOM integration：短期体验更融合，但兼容/安全成本不可接受。
- iframe：会遇到 CSP、OAuth、下载等限制，且不是主要架构。

## 后果

某些深度 UI 集成不可用；换来更强兼容性、清晰 trust boundary 与独立升级。

## 验证门禁

- WebView 无 Tauri command capability。
- 上游 DOM/router 变化不影响 start/health/reconnect。
- External navigation policy 测试。

## 受影响模块

harness-surface、shell-ui、security
