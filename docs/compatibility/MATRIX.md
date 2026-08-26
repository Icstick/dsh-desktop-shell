# Compatibility Matrix Plan

Release matrix 维度：

- OS：Windows 10/11、macOS arm64/目标 Intel、Linux 目标发行版。
- WebView：WebView2、WKWebView、WebKitGTK，Linux X11/Wayland。
- DSH：latest、N-1、upstream main advisory。
- 来源：PATH、global、source checkout、custom executable。
- Ownership：Managed、Attached。
- Profile：clean、mature、plugin-heavy、broken boot。
- Port/process：free、occupied、hijacked、delayed release、stale PID、orphan child。
- Adapter：absent、legacy、known std、unknown std。
- Protocol：Hello/Agreement required/optional、success/error Result、unknown field/version、stale generation、lease revoke/expiry。
- Transport：native、fallback、invalid/replay。
- Provider：PTY/Browser crash/reconnect。

正式支持声明只能来自可复查 matrix evidence。

## 当前冻结 Fixture 坐标

以下坐标只定义 M1/M5 测试输入，不构成支持声明：

- DSH latest：`@deepseek-ai/dsh@0.1.1-rc.2`，必须验证 registry integrity。
- DSH N-1：`@deepseek-ai/dsh@0.1.1-rc.1`，必须验证 registry integrity。
- DSH upstream advisory：`master@b150a551b8d465e31e418e1b2eaf5e79bbb7d28e`；源码行为不能替代已发布 package fixture。
- dsh-std known candidates：分别覆盖 `@dsh-std/core@0.1.0-rc1` (`latest`) 与 `@dsh-std/core@0.1.1-rc.1` (`rc`)；M5 选择前运行 conformance。

SHA-1/SHA-512 artifact 值、发布时间和 immutable evidence 统一引用 [External Baseline](../research/EXTERNAL_BASELINE.md)，不得在本文件重复维护。
