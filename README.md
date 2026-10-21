# Toolbay

One free launcher for free, open-source apps, for Mac and Windows. Install a tool with one click,
open it from Toolbay or a desktop shortcut, and it keeps itself up to date. First tool: Toolbay Flow
(voice dictation, inspired by Wispr Flow).

Built with Tauri 2 (Rust) and React + TypeScript. Rules for contributors: [AGENTS.md](AGENTS.md).

## Run

Needs Node 20+, Rust stable and the Tauri prerequisites (Windows: VS Build Tools C++; macOS: Xcode CLT).

```
npm install
npm run tauri dev
```

Windows installer (`src-tauri/target/release/bundle/nsis`), per-user, no admin prompt:

```
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/toolbay-updater.key)" npx tauri build --bundles nsis
```

Mac builds run on a Mac or in CI: push a `v*` tag and `.github/workflows/release.yml` builds
macOS (Apple Silicon + Intel) and Windows into a draft GitHub Release with `latest.json` for updates.

## How it fits together

- `src/` React UI. `lib/api.ts` reads `https://toolbay.ai/api/v1/*` through Rust, with
  `src/fixtures/*.json` as the offline fallback.
- `src-tauri/src/manifest.rs` the `toolbay.json` format every tool ships (program per platform, extra
  files such as AI models, launch args). Read from the tool's latest GitHub Release.
- `installer.rs` one-click install (one progress bar, sha256-checked), launch as a separate app,
  running state, desktop shortcuts, silent updates every 6 hours (tools that are open update when they close), uninstall.
- `auth.rs` sign-in in the browser: `/desktop/auth?state=…` → `toolbay://auth?code=…` →
  `POST /api/v1/desktop/token`. Token in the OS keychain; tools get it as `TOOLBAY_TOKEN` at launch.
- Deep links: `toolbay://open/tools/<slug>` (website "Open in the app"), `toolbay://launch/<slug>` (shortcuts).
- Hub updates: Tauri updater, signed, quiet install on launch.

Testing a tool before it has a release: put `"source:<slug>": "file:///path/to/toolbay.json"` in
`%APPDATA%\ai.toolbay.desktop\settings.json`; platform entries and files may use `file://` URLs.

## Signing (so Windows and macOS don't warn people)

See [docs/SIGNING.md](docs/SIGNING.md). Mac builds are always ad-hoc signed (Gatekeeper shows
"Open Anyway", not "damaged"); Developer ID + notarization and Windows signing turn on when the
secrets (same names as toolbay-flow) are added to this repo.

MIT licensed.
