# Release and CI/CD Plan

M0 不创建可执行 workflow。未来 CI：

- Frontend lint/type/test。
- Rust fmt/clippy/test 三平台 matrix。
- Schema/contract/compatibility。
- Security/redaction/negative tests。
- Tauri build、signing/notarization。
- SBOM、checksums、artifact provenance。

Channels：nightly、beta、stable。Release 必须从 tag、通过 required checks、生成可验证 artifact，并将 Shell update 与 DSH update 分离。
