# Changelog

## 0.3.0 — 2026-11-20

- Move to the Skale Club family design (skale.css v2) with Xnote's voice-violet signal: the selected
  meeting, the meeting header, the welcome screen, your own transcript lines and the recording
  preparation follow the signal; text on signal fills uses dark ink. Day groups and the transcript
  title are silkscreen labels; timers, timestamps, sizes and download progress use mono readouts.
- Replace the note-and-waveform symbol with the family mark (violet tile, quote marks) in the
  interface and the Windows executable. `make-icon.ps1` renders it with the shared
  `render-mark.ps1`, without npm.

## 0.2.0 — 2026-11-04

- Internal signed downloads and updates through a stable per-app manifest endpoint.
- Missing feeds report an error; staged packages are verified again before replacement.
- Release automation publishes the executable, signature and latest.json together.

- `Xnote.exe --mcp` runs a local MCP server (no window) for Claude Code, Codex and other MCP
  clients: list, read and search saved meetings, and the transcription setup; read-only.

## 0.1.1 — 2026-10-27

- Transparent product symbols without navy background tiles or decorative outer rims.
- Build inputs track the ICO/PNG artwork, and every npm build checks embedded Windows icons.
- English documentation now lists the complete five-app Skale Club family.
- Recording preparation has an inline progress bar, elapsed time, cancellation and retry; local-engine readiness uses its HTTP health endpoint.
