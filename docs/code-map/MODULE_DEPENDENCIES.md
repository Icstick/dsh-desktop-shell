# Module Dependencies

允许依赖方向：

```mermaid
flowchart LR
  UI[Desktop Features] --> CONTRACTS[Capability Contracts]
  UI --> TAURI[Tauri Commands]
  TAURI --> SUP[Supervisor]
  SUP --> PM[Process Manager]
  SUP --> LT[Local Transport]
  SUP --> TP[Terminal Provider]
  SUP --> BP[Browser Provider]
  AD[Legacy DSH Adapter] --> CONTRACTS
  STD[dsh-std Adapter] --> CONTRACTS
  UC[Usage Collector] --> AD
  BA[Browser Agent Adapter] --> AD
  TA[Terminal Agent Adapter] --> AD
```

禁止：

- Rust crate 依赖 React feature。
- Capability contracts 依赖 Cordis、DSH 或 dsh-std types。
- Supervisor 直接 import DSH internals。
- Adapter 调用任意 native command，必须使用 versioned capability。
- UI 直接管理 child process 或 PTY。
- Browser/Terminal provider 决定 Agent 权限。
