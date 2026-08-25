---
id: ADR-0001
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0001: User-owned External DSH Core

## 背景

DSH 仍在快速演化。若 Desktop 自己打包 Core、Node、pnpm 与插件，就必须承担版本、patch、依赖闭包、供应链和多平台分发。

## 决策

Desktop 只发现、验证、启动或连接用户已有 DSH。用户的安装、Node/pnpm、DSH_HOME、Profile、插件、凭据与升级节奏保持权威。Desktop 自有状态放在平台 AppData/Application Support。

## 替代方案

- Bundled Core：零配置更好，但维护与供应链成本高。
- Attach-only：更薄，但缺少可靠 lifecycle。
- External Core + Supervisor：采用。

## 后果

优点是减少上游耦合、供应链与许可证范围；代价是必须处理多种用户安装布局、版本与命令。

## 验证门禁

- PATH/global/source/custom discovery fixtures。
- Desktop 不写 DSH_HOME 的审计测试。
- DSH Core 改变只触发 compatibility/restart，不触发 Desktop updater。

## 受影响模块

environment-settings、supervisor、adapter-dsh、compatibility
