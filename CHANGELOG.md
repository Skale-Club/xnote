# Changelog

## 1.0.0 — 2026-11-21, first public release

- **Calendar in the sidebar.** Days with meetings get dots; one click shows only that day's
  meetings, the arrows change month and the calendar folds to one line (remembered).
- **Change a meeting's date and time** by clicking it under the title, for meetings recorded
  earlier or planned ahead. The list and the calendar follow.
- Settings are easier to use: transcription is chosen right inside Settings → Transcription (no
  separate page), Settings has a Close button (and Esc), and provider rows line up at any width.
- The meeting header no longer squeezes the title in a narrow window: the buttons move to a second
  line. The start page explains that speech is transcribed on this computer.
- **Any large AI provider, with your own key or your own computer.** Transcription: on this
  computer (whisper.cpp), Groq, OpenAI or any OpenAI-compatible address. Summaries: Groq, OpenAI,
  OpenRouter, Anthropic (Claude), Google Gemini, Mistral, DeepSeek, Ollama, LM Studio or any
  compatible address. Models are listed from the provider and can be typed freely.
- **AI summaries are optional.** Xnote records and transcribes with no API at all; a summary
  provider can be added at any time. Settings from earlier versions with an OpenAI key keep
  summarizing with OpenAI.
- **AI assistants write summaries over MCP.** `save_summary` lets Claude, Codex, Cursor and other
  MCP clients save the notes they write into a meeting (can be turned off; nothing is ever
  deleted), and the `summarize_meeting` prompt gives them Xnote's summary instructions. Settings →
  AI assistants shows the configuration for each client with the exe path. The window shows a
  summary saved by an assistant when it comes back to the front.
- **Nothing lost when the connection drops.** Speech segments that could not reach the cloud wait
  in `Data/pending` and are sent again automatically (every minute and after a recording); a bar
  says how many are waiting.
- Settings → Help: version, the Data folder, waiting audio with Send now, Copy diagnostics (no keys,
  no meeting text) and Report a problem. A diagnostic log is kept in `Data/logs`.
- Warnings when Xnote cannot save in its folder (for example under Program Files) and when neither
  the microphone nor the computer audio has made a sound for a minute while recording.
- Each meeting shows where its data went: “Only on this PC” or “Sent to Groq, OpenAI…”.
- Summary styles: general, sales or client call, one-on-one, daily stand-up, interview.
- The sidebar search also finds text in notes, summaries and transcripts.
- Keyboard shortcuts: Ctrl+N new meeting, Ctrl+R record / stop, Ctrl+F search, Ctrl+E export,
  Ctrl+, Settings. Icon buttons have accessible names.
- **Guided first run** in three steps, with a short step-by-step guide for each provider.
- Settings: AI summary card and a Providers and keys card (add, test and remove keys; add
  compatible providers). The privacy note lists who receives audio or text.
- Free and open source under the MIT License; CI checks every push; strict content security policy.

## 0.3.1 — 2026-11-20

- Simplify the sidebar footer: the Made by Skale Club link, the language picker and the version
  button lose their boxes and tinted background; one hairline separates each group.

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
