# PLAN-M8: Stable Candidate（三平台 + 发布就绪）

> M8 规划（2026-08-30，maintainer 确认方向：Stable Candidate——三平台加固、
> CI 矩阵、签名/SBOM、遗留收尾）。状态：草案，待 maintainer 确认后开工。

## 背景

M1-M7 全部在 Windows 验证（本机 + 单平台门禁）。Stable Candidate 的目标：
**三平台可构建可测试、发布链路（签名/SBOM/安装包）就绪、已知遗留项决策关闭**。
不是功能里程碑——功能增强（B2 并发多 profile、browser 状态上报、handover）留 M9+。

## 现状盘点（2026-08-30 调研）

| 面 | Windows | macOS/Linux | 差距 |
|---|---|---|---|
| managed-runtime 进程树 | Job Object ✓ | **进程组已实现**（cfg(unix) ×3） | 无实现差距；需 CI 验证 |
| daemon 单实例 | claim 端口 + lockfile ✓ | 同实现（平台无关） | named mutex 未做（评审 MEDIUM，M8 决策） |
| terminal-provider PTY | ConPTY ✓ | **无 Unix PTY 实现** | 最大缺口（openpty + termios） |
| browser（WebView2 权限拦截） | webview2-com ✓ | 无 | 非 Windows 降级策略（wry 默认 webview，权限拦截标注 degrade）或禁用 |
| Shell（tauri） | ✓ | ✓（libc dep 已就位） | 需 CI 构建验证 |
| CI | 无 | 无 | **完全空白（GitHub Actions 矩阵）** |
| 签名/SBOM/安装包 | 未做 | 未做 | 发布链路全空白 |

## 切片

### M8-A 平台补齐
- terminal-provider：Unix PTY（openpty + termios + ioctl TIOCSWINSZ），
  reader 轮询模式复用（PeekNamedPipe → poll/read）；平台 gate 测试
- daemon singleton：评估并决策 named mutex（补 Windows 实现 or 正式声明
  claim 端口+lockfile 为最终方案，ADR-0019 决策 4 修订定稿）
- browser-provider：非 Windows 降级（wry 默认 webview + 能力标注
  degraded/permission-intercept-unavailable）；编译 gate（windows-only 模块隔离）
- 全平台 cargo test/clippy 通过（CI 承担）

### M8-B CI 矩阵（GitHub Actions）
- 矩阵：windows-latest / macos-latest / ubuntu-latest
- 每平台：cargo test --workspace（串行）、clippy -D warnings、fmt、vitest、
  validate-acl、tauri build --debug --no-bundle
- Windows 额外：live QA（daemon + CDP 冒烟）
- 缓存：cargo registry + pnpm store（action 缓存）
- 门禁与本地一致（串行测试，已知 flaky 零容忍）

### M8-C 签名与安装包
- Windows：tauri bundle（NSIS/MSI）；签名策略待 maintainer 决策：
  A) EV/OV 代码签名证书（花钱，SmartScreen 友好）
  B) 自签 + SmartScreen 说明（免费，首次运行警告）
- macOS：Developer ID + notarization（需 $99/年 开发者账号 + 钥匙串 CI 配置）
  —— 若用户无账号，标注「发布前需账号」，构建先行
- Linux：deb/rpm/AppImage（签名可选，仓库 GPG）

### M8-D SBOM 与 provenance
- cargo-deny：依赖 license 审计 + 已知漏洞（cargo audit 等价）
- SBOM 生成：cyclonedx（cargo + npm 双源）→ 发布 artifact
- GitHub Actions attestation（release provenance）

### M8-E 收尾与发布
- 真实 DSH 端到端验证：dsh-surface bootstrap token 路径（真实 DSH + 真实
  session cookie 验证 binding）——本机有真实 DSH，Windows 上先验证
- v0.1.0 发布：tag + release notes（中文）+ 证据账本（门禁/QA/评审/测试矩阵）
- CURRENT.md/project.yaml 收尾

## 决策点（maintainer 2026-08-30 拍板）

1. **Windows 签名**：**自签（DV 级）**——免费，SmartScreen 首次运行说明；
   签名工具（signtool / osslsigncode）与说明文档进发布流程。EV/OV 留待
   分发需求出现时再评估。
2. **macOS**：**无开发者账号**——CI 构建先行（无签名构建），Developer ID +
   notarization 留待账号就绪。
3. **非 Windows browser**：**降级**（wry 默认 webview，能力标注 degraded：
   permission-intercept 与 capture 增强在非 Windows 不可用）。
4. **Linux 发行包**：**先做 deb**（AppImage 暂不做，后续按需评估）。

## 依赖与顺序

- M8-A（平台补齐）→ M8-B（CI 验证矩阵）并行推进；M8-C/D（发布链路）依赖 A/B
  绿线；M8-E 最后
- 本机 Windows 开发不变；macOS/Linux 改动靠 CI 验证（无本地环境）

## 门禁

- 三平台 CI 全绿（矩阵）
- 本地 Windows 全量门禁（与 M1-M7 一致）
- 发布 artifact 可安装（Windows 冒烟安装）
- v0.1.0 release + SBOM + attestation