# Runtime Topology

## P0 Integrated Supervisor

```text
dsh-desktop-shell
  ├─ Shell WebView
  ├─ unprivileged DSH WebView
  └─ Tauri Rust backend
       └─ Supervisor
            └─ user-owned DSH
```

P0 的 Supervisor 作为独立 crate boundary 设计，但与 Tauri process 同生命周期。即使物理同进程，UI command、Supervisor API、process manager 与 transport 仍保持逻辑隔离。

## P1 Providers

```text
Supervisor
  ├─ user-owned DSH
  ├─ PTY sessions
  └─ Browser provider process
```

DSH restart 不终止 PTY/Browser。DSH Adapter 重新连接后重新协商 capability lease。

## P2 Daemon

```text
Shell UI process
      │ authenticated local IPC
Supervisor daemon
  ├─ DSH
  ├─ PTY
  └─ Browser provider
```

Shell update/restart 不影响 daemon-owned resources。Daemon 安装、升级、多实例、split-brain 和 ownership migration 必须在 M6 前单独 ADR。
