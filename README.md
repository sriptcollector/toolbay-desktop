# Toolbay desktop

One free launcher for open-source software tools, for macOS and Windows. Browse, install, update,
launch and uninstall tools; install Claude Code skills; read and post in the community. Toolbay Flow,
a local dictation tool, is built in.

Built with Tauri 2 (Rust) and React + TypeScript, after the architecture in
`toolbay-research/wispr-flow-report.pdf`.

## Run

Needs Node 20+, Rust stable and the Tauri prerequisites (Windows: VS Build Tools C++ + WebView2;
macOS: Xcode command line tools).

```
npm install
npm run tauri dev
```

Windows installers (`src-tauri/target/release/bundle/{nsis,msi}`):

```
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/toolbay-updater.key)" npx tauri build --bundles nsis,msi
```

macOS builds run on a Mac or in CI (`.github/workflows/release.yml`, push a `v*` tag).

## How it fits together

- `src/` React UI. `lib/api.ts` reads the catalog from `https://toolbay.ai/api/v1/*` through Rust and
  falls back to `src/fixtures/*.json` while the API isn't live.
- `src-tauri/src/api.rs` HTTP client; the session token lives in the OS keychain.
- `auth.rs` browser sign-in: opens `/desktop/auth?state=…`, the site redirects to
  `toolbay://auth?code=…&state=…`, the app exchanges the code at `POST /api/v1/desktop/token`.
- `installer.rs` per-tool folders under the app data dir; zips unpacked, dmgs copied, setup exes handed to the OS.
- `flow.rs` Toolbay Flow (see `docs/flow-plan.md`).
- Updates: Tauri updater, signed, from `releases/latest/download/latest.json`.
- `TOOLBAY_API_BASE` env var or Settings → Catalog server points the app at a local site.

MIT licensed.
