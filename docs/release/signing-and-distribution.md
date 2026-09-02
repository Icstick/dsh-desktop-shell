# 发布与签名说明（M8-C，v0.1.0）

## Windows

**签名策略（maintainer 2026-08-30 拍板）：自签（DV 级）**

- 首选工具：`signtool`（Windows SDK）或 `osslsigncode`（跨平台）。
- **本机实测（2026-08-31，无 Windows SDK）：PowerShell 内置 cmdlet 等效完成**
  （Authenticode 签名本质相同）：
  ```powershell
  # 1) 生成自签代码签名证书（CurrentUser\My，5 年）
  New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=DSH Desktop Shell" `
    -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(5) `
    -KeyExportPolicy Exportable

  # 2) 签名（SHA256 + RFC 3161 时间戳）
  $cert = Get-ChildItem Cert:\CurrentUser\My\<THUMBPRINT>
  Set-AuthenticodeSignature -FilePath target\debug\dsh-desktop-shell.exe `
    -Certificate $cert -HashAlgorithm SHA256 `
    -TimestampServer "http://timestamp.digicert.com"

  # 3) 验证
  Get-AuthenticodeSignature target\debug\dsh-desktop-shell.exe
  ```
- 实测结果（2026-08-31，thumbprint 1B6A576C...）：签名与时间戳写入成功、
  签名后 exe 正常运行；`Status=UnknownError` 是**自签根不受信任的预期结果**
  （StatusMessage: 证书链在不受信任的根中终止）。**不要**把自签证书加入
  Trusted Root（机器级信任自签者，安全边界外）；信任由用户首次运行时显式授予。
- **SmartScreen 预期**：自签证书首次运行时显示 "Windows 已保护你的电脑"
  （未知发布者）。用户需点 "更多信息" → "仍要运行"。文档与发布说明中明确告知。
- 若以后装了 Windows SDK：`signtool sign /fd SHA256 /sm /s My /n "DSH Desktop Shell"
  /tr http://timestamp.digicert.com /td SHA256 <exe>` 等效（签名对象：shell exe +
  NSIS setup exe）。
- EV/OV 证书留待分发需求出现时再评估（成本/收益）。

## macOS

- **无开发者账号**（maintainer 2026-08-30）：CI 构建无签名 dmg；Developer ID +
  notarization 留待账号就绪。
- 无签名 dmg 的 Gatekeeper 预期：首次打开需右键 → 打开。

## Linux

- **先做 deb**（maintainer 2026-08-30）：`tauri build` 产出 .deb（依赖
  libwebkit2gtk-4.1/gtk3/appindicator，已配置）。
- AppImage 暂不做，按需评估。
- deb 签名（仓库 GPG）暂不做；发布说明中给校验和。

## 构建安装包

- tauri.conf.json `bundle.active=false`（开发构建不带 bundle），发布时必须
  显式指定：`pnpm exec tauri build --bundles nsis,msi`（Windows）或
  `pnpm exec tauri build --bundles deb`（Linux）；macOS dmg 同理
  （`--bundles dmg`）。

## SBOM 与 provenance（M8-D）

- `cargo deny`（licenses 审计）+ `cargo cyclonedx` 生成 SBOM（见 scripts/release/）
- GitHub Actions attestation（release 时启用）
