# Windows code signing — Xnote

Xnote's executable has two signatures with different jobs:

| Signature | What it does | Status |
|---|---|---|
| **Updater signature** (minisign, `Xnote.exe.sig`) | The app checks it before installing an update | Always on (`TAURI_SIGNING_PRIVATE_KEY`) |
| **Authenticode** (Windows code signing) | Windows and SmartScreen show the publisher instead of "Windows protected your PC" | Optional, set up once below |

Without Authenticode the app works the same, but first-time users see a SmartScreen warning and must
click **More info → Run anyway**. For a free public launch, set one of these up.

## Option A — SignPath Foundation (free for open source)

SignPath signs open-source projects for free. Requirements (check their current terms): a public
repository, an OSI license (Xnote uses MIT) and releases built by CI from that repository.

1. Make the repository public.
2. Apply at <https://signpath.org> with the repository URL.
3. Once approved, follow their GitHub Actions guide: the release job uploads `dist/Xnote.exe` to
   SignPath and downloads the signed file. Put that step where the **Code-sign the exe** step is in
   `.github/workflows/release.yml` (before the updater signature, which must cover the final bytes).

## Option B — Azure Trusted Signing (paid, low monthly cost)

Already wired into `release.yml`: the step runs only when the secrets below exist.

1. In Azure, create a Trusted Signing account and a public-trust certificate profile, and complete
   identity validation for Skale Club.
2. Create an app registration (service principal) with the *Trusted Signing Certificate Profile
   Signer* role on the account.
3. Add these repository secrets: `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`,
   `AZURE_SIGNING_ENDPOINT` (for example `https://eus.codesigning.azure.net/`),
   `AZURE_SIGNING_ACCOUNT`, `AZURE_CERTIFICATE_PROFILE`.
4. Tag a release. The log shows the **Code-sign the exe** step running; the downloaded exe shows
   Skale Club under Properties → Digital Signatures.

Check the action's current name and version before the first signed release; Microsoft has been
renaming this service.
