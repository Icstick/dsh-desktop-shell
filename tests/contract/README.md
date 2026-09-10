# Contract Tests

**Module ID:** `MOD-TEST-CONTRACT`
**Target milestone:** M2
**Canonical status:** [MOD-TEST-CONTRACT](../../tracking/modules/MOD-TEST-CONTRACT.yaml)

## Purpose

验证 Schema、capability、carrier 和 adapters 的契约。

## Tests

本目录是一个独立的 workspace 成员 crate（`dsh-desktop-shell-contract-tests`），
由 `cargo test --workspace` 与根 `pnpm test`（`test:contract`）执行。
它不含生产代码，只把**已发布的 schema** 与**实现它们的 crate** 对齐：

| 测试 | 断言 |
|---|---|
| `terminal_geometry_bounds_match_the_schemas_and_the_provider` | `specs/terminal/*.schema.json` 里的 cols/rows/data/cwd/shell 边界与 `dsh-terminal-provider` 的常量逐一对齐 |
| `terminal_create_wire_type_agrees_with_every_fixture` | 每个 `.valid.` fixture 都能被 `dsh_daemon::terminal::TerminalCreateRequest` 反序列化且 `is_valid()`；模棱两可的 mode/跨模式组合被拒绝 |
| `terminal_wire_types_reject_unknown_and_out_of_shape_fields` | 未知字段与越界整数在 wire 类型层被拒（`deny_unknown_fields` / `u16`） |
| `terminal_schemas_exist_for_the_exercised_wire_types` | 被引用的 schema 必须存在且 `$id` 自洽 |

分工：`scripts/validate-specs.mjs` 负责 **schema ↔ fixture**；本 crate 负责
**fixture/wire 类型 ↔ 实现**。两者都在 CI 里跑。

尚未覆盖（诚实登记）：Shell 侧 `dsh-desktop-shell` 是 tauri binary crate，其
`commands`/`terminal` 模块的校验函数对外不可见，本 crate 无法从外部驱动；
那部分靠 crate 内的 `#[cfg(test)]` 单测（见 `WI-M10-AUDIT-FIXES`）。

## Owns

- schema validation
- fixture matrix
- cross-version behavior

## Does not own

- GUI appearance

## Inputs

- specs/fixtures

## Outputs

- contract evidence

## Dependencies

- protocol-fixtures

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
