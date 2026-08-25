# Milestones

## M0 Architecture Freeze

交付：Charter、10 ADR、Schema、威胁模型、代码地图、tracking、module docs。退出：Architecture/Security/Interop review 接受，`implementation_authorized` 才可改为 true。

## M1 Shell MVP（周 1–4）

Environment、Discovery、Setup、Managed/Attached、原版 DSH Surface。退出：支持显式/PATH/source 三类来源；Attach 零 lifecycle mutation。

## M2 Reliable Runtime（周 5–8）

Supervisor、Job Object/process group、health、restart、native IPC/fallback、chaos。退出：AC-OWN/RUN/REC/IPC 通过。

## M3 Workbench（周 9–13）

Notification、Usage、Persistent Terminal、Diagnostics。退出：DSH restart 不影响 PTY；Usage 来源和估算清晰。

## M4 Shared Browser（周 14–17）

Provider contract、至少两个 candidate PoC、human takeover、profile isolation。退出：安全与 Browser acceptance 通过。

## M5 Interop（周 18–19）

Legacy + optional dsh-std adapters。退出：absent/known/unknown matrix 与 degraded behavior 通过。

## M6 Daemon（周 20–24）

独立 Supervisor、resource migration、Scheduler wake。退出：Shell restart 不影响 DSH/PTY，split-brain tests 通过。

## M7 Stable Candidate（周 25–27）

三平台 hardening、signing/notarization、SBOM、provenance、compatibility matrix。退出：release checklist 全部有 evidence。

## 规划量级

P0 约 2.6–4.1 人月，P1 追加 3.5–5.9，人月，P2 追加 2.3–4.0 人月；仅用于依赖和资源规划，不是交付承诺。
