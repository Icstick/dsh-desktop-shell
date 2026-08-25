---
id: ADR-0010
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0010: Apache-2.0 and Clean-room Provenance

## 背景

研究输入指出部分同类 Desktop 仓库存在额外商业限制；参考架构与复制代码的许可后果不同。项目希望允许商业和社区使用并明确专利授权。

## 决策

项目采用 Apache-2.0，顶层 LICENSE/NOTICE。第三方参考必须登记；默认 architecture-reference-only、code_copied=false。受限或不明确来源不得复制源码、资产、文案或实质实现。

## 替代方案

- MIT：更短但专利条款简略。
- License pending：阻碍贡献和复用。
- Apache-2.0：采用。

## 后果

需要 NOTICE、source register、第三方审查、SBOM 与 release provenance；许可证不消除引入代码原有义务。

## 验证门禁

- LICENSE 与 ASF 官方全文一致。
- 所有第三方引入有 pinned revision/license/reviewer。
- Release 前 source register 与 SBOM review。

## 受影响模块

全仓、compliance、release
