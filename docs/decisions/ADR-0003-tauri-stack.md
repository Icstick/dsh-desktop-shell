---
id: ADR-0003
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0003: Tauri 2 + React/TypeScript + Rust Baseline

## 背景

调研中存在 Electron MVP 与 Tauri/Rust 两条路线。Electron 对 Browser/PTY 原型更直接；Tauri/Rust 更适合 native Supervisor、process ownership、权限边界和长期 daemon。

## 决策

P0 使用 Tauri 2、React/TypeScript、Rust。Supervisor 初期在 Tauri Rust backend 内，保持独立 crate boundary。Browser automation 通过独立 provider/CDP，不绑定 system WebView。

## 替代方案

- Electron + Node/TS：原型快、Chromium 一致，但 runtime 更重且 native lifecycle 仍需加固。
- 双路线不决：会让模块与测试计划无法冻结。
- Tauri/Rust：采用。

## 后果

Browser provider 需要额外 PoC，Linux/macOS/Windows WebView 差异需真实 matrix；但核心 native boundary 更清晰。

## 验证门禁

- M1 Tauri WebView/permission PoC。
- M2 process ownership PoC。
- M4 至少两个 Browser provider candidate 通过同一 contract。

## 受影响模块

apps/desktop、全部 crates、browser-provider
