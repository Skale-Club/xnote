# Limits and troubleshooting — Xnote

- **Missing audio:** check Windows' default input/output devices, microphone access and the in-app
  audio meters. Stop and restart recording after changing devices. Xnote does not select
  individual call-app audio streams.
- **Unrelated sound in the transcript:** system-output capture includes other apps playing through
  the default output device.
- **Speaker names:** only the two sources are distinguished; several remote participants share the
  **Them** label.
- **Delayed text:** transcription is segment-based and may lag on slower hardware or networks. It
  is not word-by-word live captioning.
- **Cloud errors:** check the key, model access, connectivity and the provider's quota. Audio that
  could not be sent waits in `Data/pending` and is sent again automatically (Settings → Help →
  Send now). Settings → Providers and keys → Test checks a provider.
- **Summary errors:** Settings → AI summary. With Ollama or LM Studio, the app must be open and a
  model downloaded.
- **Cannot save:** keep `Xnote.exe` in a folder of your own (for example `C:\Apps\Xnote`), not
  under Program Files.
- **Reporting a problem:** Settings → Help → Copy diagnostics (no keys, no meeting text), then
  Report a problem.
- **Updates:** Xnote updates itself from signed GitHub Releases and installs a new version when you
  close it. Restarting from the updates panel stops a recording in progress.

## Recording preparation

Starting a recording shows an inline progress bar: check setup, load the local model,
connect to the engine, then prepare audio. Cloud mode skips the local model/engine
steps. The bar counts completed stages; it does not invent a model-download percentage.
Elapsed time and the message **Recording has not started yet** stay visible while
preparation runs, and notes remain editable.

You can cancel preparation, retry after a failure, or inspect the bounded technical
details. Cancellation cannot silently start a recording afterwards. The local engine
is accepted only when its HTTP `/health` endpoint reports ready; a listening port alone
does not count. Startup still allows up to three minutes, so slow storage or blocked
local connections can still cause a wait. The engine/model remain in the portable
`Data/engine` folder; this change does not introduce an SSD model cache.

Back to the [README](../README.md).
