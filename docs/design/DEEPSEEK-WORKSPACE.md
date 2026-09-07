# DeepSeek character workspace

User request: redesign the existing desktop frontend on a fresh branch, referencing the DeepSeek character gallery and matching backgrounds to page purpose. Branch: feat/deepseek-character-ui. Work item: WI-M8-DEEPSEEK-UI.

## Visual system

Navy ink #233a63, porcelain #f5f8fc, white #ffffff, mist #e8f0f9, lake blue #467caf, restrained gold #b89759. Blue-white contrast comes from the reference character's navy dress, white apron and blue hair; the sidebar is a quiet dark anchor. Bahnschrift handles Latin text; Microsoft YaHei UI/PingFang handles Chinese, using locally installed fonts. Controls and body copy are left aligned. One illustration occupies each page header; content stays on solid panels.

## Page roles

| Page | Artwork and role |
|---|---|
| DSH waiting / unconfigured | Water wallpaper, large welcome composition with text and settings action on left |
| Active DSH | Compact header; no art over the native WebView |
| Browser | DeepSeek coastal balcony, nautical map and telescope for exploration |
| Terminal | Compact header and shared character avatar; opaque terminal output |
| Runtime | Harness observatory and diagnostic tablet; canonical states below |
| Settings | DeepSeek workshop, organized tools and brass fittings; environment list and wizard below |
| Usage | DeepSeek ledger scene; tabular counters and records below |
| Notifications | DeepSeek correspondence alcove, envelope and service bell; actual notices below |

## Implementation boundaries

The changes are presentation-only. No backend, API, upstream WebView DOM, lifecycle state machine or protocol changes. Terminal remains mounted after its first visit. Fixed workspace height, flex/min-height sizing chain and native bounds reporting remain intact. Artwork is decorative (empty alt, aria-hidden, pointer-events none), bundled locally, and absent from native content slots. Navigation retains accessible labels when collapsed and adds aria-current. Existing reduced-motion handling remains.

The desktop minWidth is 980. The navigation collapses at 1050 and the theme also accommodates smaller browser previews. Long utility pages scroll in the workspace; immersive pages retain bounded geometry.

## Local visual preview

Run the installed Vite CLI from apps/desktop: node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 5186 . Open http://127.0.0.1:5186/features/shell-ui/preview/index.html . This separate development-only entry uses Tauri's official mockIPC with synthetic data and visibly labels itself as a preview. It never connects to the desktop backend and is not imported into the production entry. Unsupported native actions display a preview-only message.

## Artwork

See apps/desktop/features/shell-ui/src/assets/ATTRIBUTION.md for source URLs and separate CC BY-NC-SA 4.0 terms. This local design branch does not change Apache-2.0 code licensing or authorize a release containing restricted artwork.

## Delivery records

- [Verification and screenshots](evidence/deepseek-ui/README.md)
- [Exact imagegen prompts](IMAGE-PROMPTS.md)

## Page-specific banner follow-up (2026-09-07)

All five non-immersive pages now use distinct images: Settings workshop, Browser exploration balcony, Notifications correspondence alcove, Usage ledger/library, Runtime Harness observatory. The layout, native surfaces and actions are unchanged. New images use built-in imagegen with the same supplied DeepSeek character reference. Exact prompts are appended to IMAGE-PROMPTS.md.
