# DeepSeek frontend verification

Branch: feat/deepseek-character-ui. Work item: WI-M8-DEEPSEEK-UI. Verification run: 2026-09-06 22:54–22:56 Asia/Shanghai.

## Automated evidence

- [Typecheck](typecheck.txt): passed.
- [Frontend tests](tests.txt): 7 files, 105 tests passed. Existing jsdom canvas warning remains non-fatal.
- [Production build](build.txt): passed, including all three local art assets. Existing large JS chunk warning remains; original wallpaper is retained losslessly.
- [ACL](acl.txt): failed on existing pick_directory inventory mismatch. apps/desktop/scripts/validate-acl.mjs, src-tauri/build.rs and src-tauri/src/lib.rs have no diff against HEAD. This change introduces no native commands or permissions.
- git diff --check: passed. pnpm's auto-install preflight tried to rebuild the existing dependency layout and refused without a TTY; checks above used the already-installed Node CLI entry points directly. Dependencies and lockfiles were not changed.

## Browser evidence

All screenshots use the explicitly labeled synthetic-data preview, not a live native backend.

- [Welcome](welcome-1320.png), [settings](settings-1320.png), [edit form](edit-1320.png), [usage](usage-1320.png), [notifications](notifications-1320.png), [runtime](runtime-1320.png), [browser](browser-1320.png), [terminal](terminal-1320.png): 1320 × 820 desktop viewport.
- [Browser minimum-window check](browser-980.png), [wizard step](wizard-980.png): 980 × 640, matching the native configured minimum size.
- [English wizard](wizard-en-780.png), [English usage](usage-en-780.png): 780 × 800 browser viewport.
- At 1320 × 820: DOM width 1320; loaded image elements all complete with positive naturalWidth and decorative empty alt.
- At 780 × 800: DOM width 780, rail clientWidth/scrollWidth both 58 after final correction. Enter on the Usage navigation button switches the page; aria-current and focused element both identify Usage.
- Wizard Next: computed background rgb(49,95,144), foreground white; enabled button successfully advances Mode → Source.
- Terminal browser frame: document height 820, workspace height 820, terminal panel height 733. No native terminal session was started.

## Independent review

Read-only code-reviewer review found one actionable defect: old qualified wizard Next hover selector overrode the new blue background, leaving white text on pale blue. Fixed the normal and hover selectors with the same .setup-wizard__nav qualification. Follow-up review: approve, no unresolved findings. Native flex bounds, terminal mounted/hidden strategy, state colors and preview isolation reviewed.

## Remaining verification boundary

Live Tauri/WebView2/PTY integration was not exercised. Existing frontend regression tests cover the lifecycle and viewport coordination; browser evidence confirms presentation geometry only. No build artifact was published and no release configuration was changed. Artwork licensing remains separately documented in the assets attribution file.

## Distinct banner follow-up (2026-09-07)

User requested a different, purpose-specific background for every configuration page banner. New workshop, coastal exploration and correspondence images replace the three reused mappings. Usage retains the ledger and Runtime retains Harness observatory. The five assets have distinct SHA-256 hashes, each served HTTP 200 with image/png; see [asset evidence](banners-20260907.json). Typecheck and production build passed after the mapping change. No CSS/layout/behavior changed in this follow-up. The earlier page screenshots describe layout, but Settings, Browser and Notifications now display the new artwork. Generated images were visually inspected on generation.
