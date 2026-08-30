# Adapter: dsh-std (optional, L2)

**Module ID:** `MOD-ADAPTER-DSH-STD`
**Target milestone:** M5 (slice M5-D)
**Canonical status:** [MOD-ADAPTER-DSH-STD](../../tracking/modules/MOD-ADAPTER-DSH-STD.yaml)
**Architecture:** [ADR-0018](../../docs/decisions/ADR-0018-adapter-architecture.md)

## Purpose

Optional dsh-std adapter. It is the change absorption point for dsh-std
alpha churn (compatibility ladder L2): dsh-std alpha types stop at this
adapter boundary and never cross into Desktop internals (ADR-0018 decision
3). The adapter models negotiation, facets and conformance locally; it
never depends on an npm package at runtime.

## L2 semantics boundary (what this adapter does and does not claim)

- **Only known dsh-std versions are represented.** A conformance
  declaration must bind the exact package version + Git commit + SRI
  artifact integrity (ADR-0018 decision 2). Floating registry tags
  (`latest` / `rc`) are never accepted; the connection `rc` tag moved on
  2026-08-29, which is exactly why the pin is explicit.
- **The unstable dsh-std wire is not adopted.** L2 means "a known dsh-std
  version passes this adapter's conformance check", not "we speak the
  dsh-std wire". The wire/shape authority stays
  `packages/capability-contracts` (M5-B); the negotiation state machine in
  this crate is a Rust port of its `negotiate.ts` semantics because the
  Rust side cannot import TypeScript.
- **Legacy / L0 fallback is never skipped.** Compatibility is additive:
  any L2 adapter failure records a degradation entry and falls back to the
  L0 baseline (DSH process + HTTP Web UI: Surface/health/Managed
  lifecycle). `degrade_to_l0` is total - it never panics and never blocks
  L0 behavior (ADR-0018 decision 4).
- **Every activation negotiates independently.** An Agreement is never
  cached as a fact for a later generation; each activation gets a fresh
  `NegotiationSession` (ADR-0018 decision 1).
- **Alpha types do not cross.** Nothing in this crate re-exports dsh-std
  types; the wire-facing types here are the envelope shapes defined by
  `specs/protocol/*.schema.json`.

## Conformance tri-state

| State | Declaration | Behavior |
|---|---|---|
| `Absent` | none / empty | dsh-std domain protocol not implemented; L0/L1 unchanged |
| `Known` | coordinates match pinned baseline (`@dsh-std/core@0.1.1-rc.1` + commit `3df0543` + SRI from SOURCE_REGISTER) and pass local fixture validation | L2 capability |
| `Unknown` | coordinate drift or fixture/format failure | fail-closed + recorded; no L2 promise |

Known coordinates live in `conformance.rs` (`KNOWN_PACKAGE` /
`KNOWN_VERSION` / `KNOWN_COMMIT` / `KNOWN_INTEGRITY`); their truth source
is [EXTERNAL_BASELINE](../../docs/research/EXTERNAL_BASELINE.md) and
[SOURCE_REGISTER](../../docs/compliance/SOURCE_REGISTER.yaml)
(`SRC-DSH-STD`, distribution `rc`). Refresh procedure: update the four
constants, the fixture files in `fixtures/`, and the external baseline
documents together - a conformance claim without a fixture is not a
claim.

## Components

- `conformance` - record/declaration model, format validation, tri-state
  evaluation (`conforms`), append-only evaluation log.
- `negotiate` - per-activation negotiation state machine
  (proposed -> agreed -> active; reject/degrade paths;
  granted subset of supports; no Agreement caching).
- `facets` - minimal dsh-std facet model. **Inference note:** the exact
  dsh-std facet semantics could not be verified in the M5-D session (the
  pinned README and registry were unreachable from the build sandbox);
  the minimal set (`negotiation` / `conformance` / `invocation`) is
  inferred from LADDER and DSH_STD_POLICY and is documented as such in
  `facets.rs`. Re-verify against the pinned README before real peers.
- `degrade` - L0 fallback path with an append-only degradation log.
- `time` - std-only UTC RFC 3339 timestamps (matches the TS side's
  `toISOString()` shape).

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`

## Tests

- Conformance: format validation, tri-state matrix, fixture-driven
  parsing (fixtures under `fixtures/`), audit log behavior.
- Negotiate: full state machine paths, idempotency, degrade/partial
  grants, no-Agreement-caching across activations.
- Degrade: L0 fallback is recorded, never claims L2, never panics.
- Integration (`tests/l2_matrix.rs`): absent/known/unknown x degrade
  matrix from fixtures.

## Gate

`cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace`
