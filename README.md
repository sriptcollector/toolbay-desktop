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

Both are wired into the release workflow and switch on when the secrets listed at the top of
`.github/workflows/release.yml` are set.

- **Windows: Azure Artifact Signing** (formerly Trusted Signing), Basic plan, about $9.99/month.
  Needs an Azure subscription and identity validation of the publisher (organization, or an individual
  in the US/Canada). No hardware token. Alternative: an OV or EV code-signing certificate from a CA
  (DigiCert, Sectigo, SSL.com), roughly $200–600/year, key held on a hardware token or cloud HSM.
  SmartScreen reputation still builds with downloads either way.
- **macOS: Apple Developer Program**, $99/year. From it, create a **Developer ID Application**
  certificate (signing) and an app-specific password or App Store Connect API key (notarization).
  A Developer ID Installer certificate is not needed (we ship .dmg, not .pkg).

MIT licensed.
