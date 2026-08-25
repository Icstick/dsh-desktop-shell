# Compatibility Ladder

| Level | 条件 | Desktop 能力 |
|---|---|---|
| L0 Baseline | DSH process + HTTP Web UI | Surface、health、Managed lifecycle |
| L1 Enhanced | Legacy companion adapter | Usage、Notification、restart hints |
| L2 Standard-aware | optional dsh-std adapter | negotiation/facets/conformance |
| L3 Future native interop | stable community protocols/wire | thinner adapters |

Compatibility 是 additive。Adapter 失效不得破坏低层级能力。

## 变化吸收点

- DSH launch/internal API -> `adapter-dsh` / discovery。
- dsh-std alpha change -> `adapter-dsh-std`。
- transport standard change -> `local-transport` adapter。
- Web router change -> Surface URL/route restore，不使用 DOM。
