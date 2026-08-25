# Signing, SBOM and Provenance

稳定发布前：

- Windows installer/binary 使用已批准 signing identity。
- macOS app 完成 signing、hardened runtime 与 notarization。
- Linux package 生成 checksums 和适用签名。
- 每个平台生成 SBOM、dependency/license inventory。
- Artifact provenance 绑定 repository、workflow、commit/tag。
- 发布页列出 checksums、支持平台、已测 DSH matrix 和已知限制。

Signing identity 与账号当前未指定，作为 M7 外部前置条件记录，不能用测试证书冒充稳定发布证据。
