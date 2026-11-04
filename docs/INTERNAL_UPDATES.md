# Xnote internal updates

The portable executable checks `https://github.com/Skale-Club/xnote/releases/latest/download/latest.json` on startup and every six hours.
The JSON manifest contains `version`, `url`, `signature` and optional `notes`.
The download URL must use HTTPS. The executable is verified against the embedded
Skale update public key before staging and again before replacement. A pending
download survives a restart. A missing feed is an update error, never proof that
the app is current. The update replaces only the executable; `Data` is preserved.

Users can restart from the app when an update is ready, or close it to apply the
download. Xink exposes this through its tray Updates menu. The other apps expose
an Updates panel and a ready notification. Existing copies containing the legacy
GitHub updater can discover the same release through the GitHub releases API and
the `.exe` plus `.exe.sig` assets. Copies predating the updater require a one-time
installation of an updater-enabled build; they cannot acquire code remotely by
themselves. Never replace a user's installed copy during development.

For another update host, place `{"url":"https://host/latest.json"}` in
`Data/update-source.json`, or set `XPACE_UPDATE_URL` at compile time. The embedded
signature key remains mandatory for every host.

Push a matching `v0.2.0` version tag to trigger `.github/workflows/release.yml`.
The workflow tests, builds, signs, verifies with `--verify-update`, generates
`latest.json`, and publishes all three release assets. The Actions signing secret
must match `src-tauri/updater.pub`. Never commit private signing keys.

Before announcing a release, verify the public manifest, binary and signature.
Run `node scripts/ci_monitor.cjs --help` for the repository's CI monitor.
