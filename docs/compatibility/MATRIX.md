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
- Transport：native、fallback、invalid/replay。
- Provider：PTY/Browser crash/reconnect。

正式支持声明只能来自可复查 matrix evidence。
