# Data Flows

## Start Managed DSH

1. Shell 选择 `DshEnvironment`。
2. Supervisor canonicalize paths，验证 executable/repository、`DSH_HOME`、Profile 和 endpoint。
3. 生成 launch identity、instance ID、generation 和 ephemeral transport credential。
4. Process Manager 建立 Windows Job Object 或 Unix process group。
5. 启动 DSH，执行 readiness probe。
6. 可用时 Adapter negotiation；无 Adapter 时进入 baseline。
7. Shell 导航到 loopback DSH Web endpoint。

## Capability Invocation

```text
Agent
 -> DSH Tool/Permission
 -> DSH Adapter
 -> authenticated local transport
 -> Capability Broker
 -> grant/scope/lease validation
 -> provider
 -> Result/Event
```

DSH WebView 不参与此 privileged 流程。

## Restart

先返回 Accepted，再记录 route/session hint，停止旧 generation，确认 process/endpoint 释放，启动新 generation，health 后重新协商并让 Web Surface reconnect。迟到的旧 generation message 返回 `STALE_GENERATION`。

## Usage

DSH collector 从权威语义 seam 采集、聚合和标注来源；Desktop 只消费 normalized telemetry。若 seam 失效，Usage capability unavailable，不解析未知内部日志猜测。

## Diagnostics

结构化事件经过 redaction 后写入 Desktop log；导出时再次最小化，包含版本、状态、错误码和 correlation，不包含 credential、Authorization、完整环境变量或 session body。
