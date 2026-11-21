# Data and privacy — Xnote

| Data | Storage or destination |
|---|---|
| Settings | `Data/xnote.json` beside the executable |
| Local engine and models | `Data/engine` |
| Notes, transcript and enhanced notes | JSON files in `Data/meetings` |
| Deleted meetings | Moved to `Data/meetings/.trash` |
| Audio waiting for the cloud | `Data/pending`, only when a cloud transcription failed; deleted once sent |
| Diagnostic log (no keys, no meeting text) | `Data/logs` |
| Markdown exports | `Data/exports` |
| API keys | Windows Credential Manager, outside the portable folder |

Xnote does not save an audio recording for later playback. If a cloud transcription fails (for example, no connection), only those speech segments wait in `Data/pending` and are sent again automatically. It processes audio segments in memory
and, in cloud mode, sends them to the selected provider. When an AI summary is turned on, your written notes and the transcript are sent to
the summary provider you chose (nothing leaves the PC with Ollama or LM Studio). With automatic
summaries on, this happens when a recording stops.

Each provider's own data policy applies to what it receives. Requests to OpenAI set `store: false`,
which is not a promise of zero retention across all its systems. Local meeting files and exports are ordinary files, without
app-level encryption.

Close Xnote before copying its folder for backup. API keys do not move with the folder and must be
configured on another PC. Deleted meetings remain in the trash directory; deletion in the UI is not
secure erasure.

Back to the [README](../README.md).
