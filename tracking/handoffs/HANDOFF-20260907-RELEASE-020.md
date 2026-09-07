# v0.2.0 release day handoff (2026-09-07)

- Work items: WI-M8-DEEPSEEK-UI + WI-M9-BROWSER-TABS (both done, shipped in v0.2.0).
- Branch: main @ 15718c8; release synced (cce7e11); tag v0.2.0 @ 1a63c5d (moved after gate fixes); CI green (fmt/clippy/specs/tauri/live-qa).
- Release: github.com/Icstick/dsh-desktop-shell/releases/tag/v0.2.0 (Latest; windows exe+msi locally signed, dmg, deb, checksums, SLSA attestation, 12 SBOM files, Chinese notes).

## Delivered today

- Browser multi-session tabs: title events (title_changed), open-URL-never-displaces, orphan cleanup on list, daemon owner release on disconnect, browser.close returns the closed report.
- Theme merged to main with marine overlay for the tab strip; assets webp (19.4MB→1.3MB) with PNG provenance retained; THIRD_PARTY_NOTICES.md added.
- Release gates fixed: rustfmt pass, clippy clean, specs conditional schema (title_changed requires title), CI stages the daemon sidecar in both jobs (build.rs resolves externalBin at compile time).
- ACL inventory aligned (remove_environment was never registered → delete-environment was ACL-rejected; check:acl 40 commands green).

## Open backlog (0.2.1+)

- M6-C TODOs (daemon lease revocation on disconnect, envelope fixed port); M6-C4 browser navigation state reporting; live-daemon-qa into CI (partially: sidecar staging pattern now established).
- WI-M9-TIMER-UI (planned): design pass with user before ADR.
- WI-M9-STARTUP-ROLLBACK (planned): ADR first; two rollback classes (repo last-good commit with dirty-tree policy; profile/plugin snapshot). Trigger = startup crash / readiness timeout under explicit policy.
- Artwork: upstream (ZipZipPipe) authorization still outstanding for future commercial use; CC BY-NC-SA non-commercial distribution is compliant as shipped.

## Notes for the next session

- daemon binary is NOT rebuilt by tauri dev/watch; cargo build -p dsh-daemon can silently fail to relink while the running daemon holds the exe (verify exe mtime after rebuilds; stop daemon first).
- CI green on main is the gate before any further release; release.yml stages daemon sidecars itself.
- Local preview fixture: vite on 5186 (mockIPC) for the theme; dev GUI runs the real daemon on the isolated DSH_HOME.
