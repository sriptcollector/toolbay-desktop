# AGENTS.md: rules for working on the Toolbay hub

Toolbay is one free launcher (Mac + Windows) for free, open-source tools. Users browse, install,
open, update and remove tools from it, like games under Steam or Roblox. These rules come from Orion
and apply to the hub and to every tool it installs.

## 1. Consumer-first
The people using this are not technical. Everything must work for them without help.
- Double-click to install. Per-user installs, no admin prompt wherever possible.
- No terminal, no command lines, no "install Node/Python first". No dependencies the user has to get.
- Plain language everywhere: "Install", "Open", "Couldn't reach the download. Check your internet
  connection and try again." Never show raw error text, codes, paths or stack traces to users.
- Every error says what to do next.
- Updates install by themselves, quietly (tools when they're closed; the hub on launch).
- Signed builds so SmartScreen and Gatekeeper don't warn people (see "Signing" in README.md).
- Test as a brand-new user: no app data, no dev tools on PATH.

## 2. Simplicity first
- One big button per tool: Install → (one progress bar) → Open. Nothing else is required.
- Few screens: Home, Tools, Library, Community, Settings. Don't add sections without a reason.
- Fewer settings beat more settings. Sensible defaults, nothing to configure to get started.

## 3. One click installs everything a tool needs
- A tool ships a `toolbay.json` (repo root and attached to every GitHub Release). The hub reads it
  from the latest release and installs the program plus its `files` (AI models etc.) in one go,
  behind one progress bar. No separate "download model" step anywhere. Format: `src-tauri/src/manifest.rs`.
- After installing, offer "Add a desktop shortcut" (also available on the tool's page later).
  Shortcuts open the tool through the hub (`toolbay://launch/<slug>`) so it starts signed in.
- An installed tool opens as its own app/window with its own full UI, launched by the hub with
  `--from-toolbay`, `TOOLBAY_HUB=1` and `TOOLBAY_TOKEN` (the user's toolbay.ai token, when signed in).
- Tools install to `%LOCALAPPDATA%\Programs\Toolbay Tools\<id>` (Windows) or `~/Applications/Toolbay`
  (Mac); their extra files go to the tool's own app data dir. Uninstall removes all of it.

## 4. Branding and credit
- Every tool has its own original logo. It may echo the style of the product it's inspired by
  (same visual family) but must be an original drawing that can't be mistaken for theirs.
  Never use the original product's logo or any of its files.
- Cards and tool pages show plain text "Inspired by <original product>" (e.g. Toolbay Flow is
  "Inspired by Wispr Flow"). Tool pages also say: "Independent open-source project, not affiliated
  with or endorsed by <company>." Data: the API's optional `inspiredBy {name, company}`.
- Every tool is open source: show the repo link and license, plus the privacy check status.

## 5. Look
- toolbay.ai's dark theme, shared with every tool: tokens in `src/styles.css` (`:root`), Space Grotesk
  for UI and Space Mono for small meta text, #0a0a0a canvas, #8b6dff as the one accent, 10/14 px radii.
- It's a launcher, not a website: big tool tiles, a big Open/Install button, clear installed/running state.

## 6. Scope
- Tools + Community. There is no skills marketplace in the hub.
- The catalog comes from `https://toolbay.ai/api/v1/*` (owned by the site repo, agentmarketplace).
  Don't hard-filter or hard-code tools in the app; fixtures in `src/fixtures` are only the offline fallback.

## Working on the code
- `npm run tauri dev` to run; `npx tsc --noEmit` and `cargo test` (in `src-tauri`) before committing.
- Rust holds anything sensitive (token in the OS keychain, all network calls); the webview never sees the token.
