# Supply-chain Policy

本项目不通过 Desktop 安装 DSH、Node 或插件。未来依赖引入：

1. 固定版本/commit 与官方来源。
2. 登记 license、hash、用途、native/build script。
3. 默认拒绝未知 install script；不得使用 dangerouslyAllowAllBuilds。
4. Release 生成 SBOM、checksums 和 provenance。
5. Shell update 与 DSH update 分离。
6. 第三方代码/资产必须通过 clean-room/source register gate。

Architecture reference 不是 dependency，也不授权复制。
