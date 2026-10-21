# Code signing for the Toolbay hub

Same setup and same secret names as Toolbay Flow; the full step-by-step (what to buy, how to make
the certificate, which secrets to add) is in
[toolbay-flow/docs/SIGNING.md](https://github.com/sriptcollector/toolbay-flow/blob/main/docs/SIGNING.md).
Add the same secrets to this repo (GitHub → sriptcollector/toolbay-desktop → Settings → Secrets and
variables → Actions) and tag a release; `.github/workflows/release.yml` turns signing on by itself.

- **macOS today:** every build is ad-hoc signed with the hardened runtime and
  `src-tauri/Entitlements.plist` (Apple Events only, for Hide/Show of running tools). A downloaded copy
  shows "Open Anyway" in System Settings → Privacy & Security instead of "is damaged". CI fails the
  release unless `codesign --verify --deep --strict` passes on the app and on the app inside the DMG.
- **macOS with Apple secrets** (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`,
  `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`): Developer ID signing, notarization, stapling, `spctl` check.
- **Windows** (`AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`, `TRUSTED_SIGNING_ACCOUNT`,
  `TRUSTED_SIGNING_PROFILE`): Azure Artifact Signing.
- Optional: `TAURI_SIGNING_PRIVATE_KEY` (the updater key, `~/.tauri/toolbay-updater.key`) lets the
  hub update itself. Without it releases still build and sign, just without the auto-update files.
