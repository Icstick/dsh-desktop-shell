# Development: adapter-dsh-std

## Build & test

```text
cargo fmt -p dsh-adapter-dsh-std --check
cargo clippy -p dsh-adapter-dsh-std --all-targets
cargo test -p dsh-adapter-dsh-std
```

## Conformance baseline refresh (when dsh-std moves)

1. Re-verify the pinned coordinates (EXTERNAL_BASELINE + SOURCE_REGISTER
   `SRC-DSH-STD`): package, exact version, commit, SRI integrity.
2. Update `KNOWN_PACKAGE` / `KNOWN_VERSION` / `KNOWN_COMMIT` /
   `KNOWN_INTEGRITY` in `src/conformance.rs`.
3. Update the fixtures in `fixtures/` (known + drift cases).
4. Run the conformance matrix tests; the tri-state must stay
   absent/known/unknown with fail-closed Unknown.

Floating tags (`latest` / `rc`) are never valid declarations.
