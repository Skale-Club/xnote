# Release checklist — Xnote 1.0

The code is ready on the branch; these steps need a person and a Windows PC.

## 1. Before merging

- [ ] Make the repository public (GitHub → Settings → General → Change visibility).
- [ ] Enable private vulnerability reporting (Settings → Code security), used by `SECURITY.md`.
- [ ] Review and merge the pull request into `main`. CI must be green.

## 2. Test on a clean Windows PC

Build with `npm ci && npm run build`, copy `src-tauri/target/release/xnote.exe` to a folder of your
own and test with a fresh `Data` folder (rename the old one to keep your meetings).

**First run**
- [ ] The window loads with icons, styles and the logo (the new content security policy blocks
      anything that is not part of the app; the dev tools console shows no CSP errors).
- [ ] The guided setup opens at step 1 of 3.
- [ ] Local: download the CPU (and, with an NVIDIA card, the GPU) engine and a model; record 30 s.
- [ ] Cloud: paste a Groq key, *Check key*, *Find models*, *Use Groq*; record 30 s.
- [ ] Step 2 with **No, just the transcript**: record, stop; no summary is attempted, and the
      meeting shows **Only on this PC** (local) or **Sent to Groq** (cloud).

**Summaries**
- [ ] Groq or OpenRouter with a key: automatic summary after Stop; the badge lists the provider.
- [ ] Ollama open with `llama3.2` pulled: *Find models* lists it; summary works offline.
- [ ] Anthropic or Gemini key, if available: one summary each.
- [ ] Each summary style changes the sections (try Sales and Stand-up).

**Robustness**
- [ ] Cloud mode: start recording, turn Wi-Fi off for a minute while talking, turn it on. The bar
      shows waiting audio, then the lines appear and the bar goes away.
- [ ] Run the exe from `C:\Program Files\Xnote` as a normal user: the "cannot save" warning shows.
- [ ] Settings → Help → Copy diagnostics: contains no key and no meeting text.

**AI assistants (MCP)**
- [ ] Settings → AI assistants → Claude Code snippet; run it; ask "Summarize my last meeting in
      Xnote". The summary appears in Xnote when the window comes back to the front.
- [ ] Turn off "Assistants may save summaries": the assistant gets a clear refusal and shows the
      summary instead.

**Everyday use**
- [ ] Ctrl+N, Ctrl+R (start and stop), Ctrl+F, Ctrl+E, Ctrl+, work.
- [ ] Search finds a word that is only in a transcript.
- [ ] Settings from 0.3.x with an OpenAI key keep summarizing with OpenAI after the update.
- [ ] Update from the previous release through the updates panel.

## 3. Publish

- [ ] Optional but recommended: set up code signing ([SIGNING.md](SIGNING.md)).
- [ ] On `main`, run `./scripts/release.ps1 1.0.0` (sets the version, tags and pushes).
- [ ] When the Release workflow finishes, put the `1.0.0` section of `CHANGELOG.md` in the release
      notes and download the exe once to check it runs.
