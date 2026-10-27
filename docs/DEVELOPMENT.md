# Development and releases — Xnote

This guide is for maintainers building, checking and publishing Xnote for Windows. For everyday use,
return to the [README](../README.md).

## Build locally

Use Windows with Microsoft Edge WebView2 Runtime, Node.js/npm, Rust's MSVC toolchain and Microsoft
C++ Build Tools with the Windows SDK. Install the **Desktop development with C++** workload. The
repository's release workflow currently selects Node.js 20 and stable Rust. See the [Tauri Windows
prerequisites](https://v2.tauri.app/start/prerequisites/#windows) for setup details.

From a directory where you keep source repositories:

~~~powershell
git clone https://github.com/Skale-Club/xnote.git
cd xnote
npm ci
npm run dev
~~~

Private repositories require an authenticated GitHub account with access. The frontend is plain
HTML, CSS and JavaScript served by Tauri 2; opening the HTML directly in a browser does not provide
the native commands.

To check Rust code, run the included tests and build the executable:

~~~powershell
npm run test:ui
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --release --locked
npm run build
~~~

The build produces `src-tauri/target/release/xnote.exe`. It does not create an installer or a
PortableApps package. Copy the executable to a writable folder for manual testing. Its `Data` folder
belongs beside that executable, including when running a development build.

## Architecture

Rust captures microphone and default system-output audio, splits speech into segments and
transcribes with a local whisper.cpp server or a cloud provider. Meetings are JSON documents. The
OpenAI Responses API generates enhanced notes; JavaScript renders streamed text, editable notes and
the transcript. API keys use Windows Credential Manager.

All four apps use the same palette: navy surfaces, primary blue `#5173D6` and soft blue `#8FA9EE`.
Keep English, Portuguese and Spanish UI strings consistent. Settings and application data use the
portable data directory; do not commit runtime data, API credentials or private signing keys.

## Validate a change

Run the checks above when changing application code. Unit tests do not validate real Windows
devices, capture software or external APIs. For documentation-only changes, compare claims with the
implementation and check links and commands.

- Test first-run setup, local transcription and each cloud provider separately.
- Confirm microphone and system-output labels, audio meters and stop/resume behavior.
- Check delayed segments finish before generating notes; verify the automatic summary needs new
  transcript text and an OpenAI key.
- Check save, reopen, search, Markdown export and moving deleted meetings to the trash folder.
- Test credential removal, missing devices, unavailable models and provider errors without exposing
  credentials.
- Use synthetic meeting content for API tests; live API calls require access and may incur charges.

Keep documentation aligned with observed behavior. Update the README's English instructions and
Portuguese/Spanish summaries together. A feature listed in the README should have an implemented
workflow; a release link should not imply that a binary exists before publication succeeds.

## Publish a release

The **Release** GitHub Actions workflow runs when a `v*` tag is pushed. A normal branch push does
not build or publish an executable. Publication needs repository write access, GitHub Actions
enabled and an Actions secret named `TAURI_SIGNING_PRIVATE_KEY` available to this repository. An
organization secret must grant this repository access.

The secret must contain the private signing key matching the app's embedded public update key. Keep
the key outside Git and back it up securely. The current workflow supplies an empty signing-key
password; using a password-protected key requires updating the workflow's secret configuration
first.

Before tagging a release, confirm that the repository has a signing key available through a
repository or permitted organization secret. A normal source push does not publish a binary.

For a reviewed release:

1. Start from a clean checkout of `main`, fetch the remote and confirm `origin`    points to
   `https://github.com/Skale-Club/xnote.git`.
2. Choose an unused semantic version. Set the same version in the npm package,    Rust package and
   Tauri configuration. Refresh the npm lockfile with    `npm install --package-lock-only` and the
   Rust lockfile with    `cargo check --manifest-path src-tauri/Cargo.toml`. Review those changes.
3. Run the tests and build above, then perform the relevant manual checks.
4. Commit the version changes and lockfiles. Create the corresponding `vMAJOR.MINOR.PATCH`    tag on
   that commit, then push `main` and that exact tag to `origin`.
5. Watch the [Actions run](https://github.com/Skale-Club/xnote/actions).    The tag must match the
   Rust package version. The workflow builds the app,    signs it and publishes `Xnote.exe` plus
   `Xnote.exe.sig` in    [Releases](https://github.com/Skale-Club/xnote/releases).
6. Download and smoke-test the release. Confirm both assets are present and    readable by the
   intended users before announcing a download or update.    Replace the dated pre-release notice in
   the README after the first release.

The existing `scripts/release.ps1` helper changes the three version manifests, commits, tags and
pushes immediately. It does not refresh lockfiles, run tests or check that signing secrets exist.
Use the reviewed process above when preparing lockfiles or validating a first release.

The updater queries GitHub without authentication, finds a newer version, verifies its signature and
replaces the executable after the user selects the update. It retains `Data`. Release assets must be
publicly readable for this updater to work; GitHub login in a browser does not authenticate the app.

Xnote is currently private. Its updater requires a public distribution endpoint or an authenticated
update mechanism before it can deliver updates.

The update signature is separate from Windows Authenticode signing; this workflow does not configure
an Authenticode certificate.

## Portable distribution

The app creates `Data` next to its executable. Optional PortableApps-style metadata can be supplied
under `App/AppInfo`; the app only refreshes the version when that metadata already exists. Neither
the build nor the release workflow creates a complete PortableApps package.

Close the app before moving or backing up its folder. Xnote credentials remain in Windows Credential
Manager and must be entered again on another PC.

## Verify the Windows executable icon

After a normal `npm run build`, the postbuild step checks the icon resources
embedded in the resulting executable against `src-tauri/icons/icon.ico`. You can
repeat the data-only check with `npm run verify:icon`. This uses Node.js, does not
launch the app and fails if any icon frame is stale or missing. Cargo tracks icon
inputs through `build.rs`, so updating artwork rebuilds the Windows resource library.
See [branding](BRANDING.md) for the icon source and regeneration instructions.

## Recording preparation verification

The Node VM tests cover stage values, local/cloud modes, cancellation, retry,
stale events, stale timers, HTML escaping and the actual application command-result
fallback. Rust tests cover bounded cancellation history, cancel-before-start,
cancel-vs-commit, request isolation and HTTP health responses using a disposable
loopback test server. These tests do not open microphones, launch Xnote or load
the real Whisper model. Native-device and visual acceptance remains manual.
