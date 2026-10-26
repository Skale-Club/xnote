# Xnote

AI meeting notes for Windows, in the spirit of Granola. Xnote records your
microphone and what your computer plays, so it works with any call app (Zoom,
Meet, Teams, WhatsApp…) and no bot joins the meeting. It transcribes with
Whisper, **on your computer** or, if you prefer, in the cloud (Groq or OpenAI),
and after the meeting Claude turns your rough notes plus the transcript into
clear notes. 🇧🇷 [Português](#português) · 🇪🇸 [Español](#español)

Made by **[Skale Club](https://skale.club)**.

## Download

Get the latest `Xnote.exe` from **[Releases](https://github.com/skaleclub/xnote/releases/latest)**. It keeps itself up to date: when a new version is out, the app offers it at the top of the window and swaps itself in place, keeping your meetings and settings. Every update is signed and checked before it is used.

## Run it

Double-click `Xnote.exe`; no installer. It needs WebView2, which comes with
Windows 11. Everything lives next to the exe:

| Next to `Xnote.exe` | What it is |
|---|---|
| `xnote.json` | Settings (languages, chosen engine) |
| `engine\` | whisper.cpp and the Whisper model, downloaded on first run |
| `meetings\` | One JSON file per meeting: your notes, transcript, enhanced notes |
| `meetings\.trash\` | Deleted meetings, recoverable by hand |
| `exports\` | Markdown exports |

API keys (Claude, and Groq or OpenAI for cloud transcription) are the only
things not in the folder: they are stored in the Windows Credential Manager.

## First run

When Xnote opens for the first time it asks where to transcribe:

- **On this computer**: private and free, after a one-time download (below).
  Recommended when the computer has an NVIDIA graphics card.
- **In the cloud**: no download and fast on any computer, with an API key from a
  provider you pick in a selector. Recommended without an NVIDIA card.

| Provider | Model | Cost |
|---|---|---|
| Groq (recommended) | Whisper large-v3 turbo | A few cents per hour of meeting; free plan available. Key at [console.groq.com/keys](https://console.groq.com/keys) |
| OpenAI | gpt-transcribe | About US$0.27 per hour of meeting. Key at [platform.openai.com/api-keys](https://platform.openai.com/api-keys) |

The key is checked with a free call before it is saved. In the cloud, the audio
of each phrase is sent to the provider to be transcribed; Xnote never saves
audio. You can switch in **Settings → Transcription → Change**.

### Transcribing on this computer

Xnote downloads the transcription engine once, from the official
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) release and the
[Whisper models](https://huggingface.co/ggerganov/whisper.cpp), and checks each
file against a pinned SHA-256 hash:

| Choice | Download | When |
|---|---|---|
| NVIDIA graphics card (CUDA 12.4) | 653 MB | Recommended when an NVIDIA card is present: transcribes as fast as people talk |
| Processor | 9 MB | Any computer; slower on long meetings |
| Best quality model (large-v3 turbo) | 547 MB | Recommended |
| Light model (small) | 181 MB | Slower computers |

If the NVIDIA engine can't use the card (old driver, for example), Xnote says
so and runs on the processor instead of silently going slow.

## A meeting

1. **New meeting**, then **Start recording**. Tell people they're being
   transcribed: **Copy notice** puts a ready message on the clipboard.
2. Write notes your way while you talk; short and messy is fine. The transcript
   appears on the right, labelled **You** (microphone) and **Them** (computer
   audio).
3. **Stop**, then **Enhance notes**: Claude keeps your points, fills in details,
   decisions and action items from the transcript, and suggests a title.
4. **Copy** or **Export .md** to share.

You can stop and resume recording in the same meeting.

## Enhanced notes (Claude)

Add your Anthropic API key in **Settings** (create one at
[console.anthropic.com](https://console.anthropic.com)). Xnote checks it with a
free call before saving it.

- Model: Claude Opus 5.5, streamed, with server-side fallbacks enabled
  (`fallbacks: "default"`), so a request a safety classifier declines is retried
  on another model instead of failing.
- Only **text** is sent to Anthropic: your notes and the transcript. Audio is
  never sent to Anthropic, and is never saved.
- Cost is a few cents per meeting (a one-hour meeting is roughly 15k input
  tokens).

## Settings

- **App language**: English, Português, Español.
- **Meeting language**: Automatic, or Portuguese / English / Spanish. Choosing it
  makes transcription more accurate and sets the language of the enhanced notes.

## Limits

- Speakers are told apart as "you" and "them" only; several remote people are
  all "them".
- Recording people without telling them is illegal in some places. Use the
  notice.

## Build from source

This is a Tauri v2 app (Rust + plain HTML/JS, no bundler). Needs Rust, Node.js
and the Visual C++ Build Tools.

```bash
npm install
npm run build    # src-tauri/target/release/xnote.exe
```

```bash
cd src-tauri
cargo test --release    # audio segmentation, WAV, Claude stream parsing, updater…
```

- `src-tauri/src/audio.rs`: microphone + WASAPI loopback capture, 16 kHz resampling, speech segmentation.
- `src-tauri/src/engine.rs`: engine download/verification, `whisper-server` process, transcription.
- `src-tauri/src/cloud.rs`: cloud transcription (Groq, OpenAI), keys in Credential Manager.
- `src-tauri/src/ai.rs`: Claude Messages API (streaming), API key in Credential Manager.
- `src-tauri/src/meetings.rs`: meeting files.
- `src-tauri/src/updater.rs`: signed self-update from GitHub Releases.
- `src/app.js`, `src/style.css`, `src/markdown.js`: the window; `src/i18n.js`: English, Portuguese, Spanish.
- `make-icon.ps1`: draws the app icon.

## Releasing a new version

```powershell
./scripts/release.ps1 0.2.0
```

It sets the version in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and `package.json`, commits, tags `v0.2.0` and pushes. GitHub Actions then builds the exe, signs it with the update key (the `TAURI_SIGNING_PRIVATE_KEY` secret) and publishes the release; installed copies offer it on their next start.

---

## Português

O Xnote faz notas de reunião com IA, no estilo do Granola. Ele grava o seu
microfone e o som do computador, então funciona com qualquer app de chamada e
nenhum robô entra na reunião. A transcrição usa o Whisper, **no seu computador**
ou na nuvem (Groq ou OpenAI); depois, o Claude junta as suas anotações com a
transcrição e escreve notas organizadas.

- **Primeira vez:** escolha onde transcrever. **Neste computador** (privado e
  grátis): escolha o motor (placa NVIDIA ou processador) e o modelo; o download
  é verificado por SHA-256. **Na nuvem** (sem download): escolha o provedor no
  seletor (Groq, recomendado, ou OpenAI) e cole a chave da API; o áudio de cada
  frase vai para o provedor.
- **Reunião:** Nova reunião → Gravar → anote do seu jeito → Parar → **Melhorar
  notas**. Use **Copiar aviso** para avisar os participantes.
- **Claude:** coloque sua chave da API em Configurações. Ela fica no Gerenciador
  de Credenciais do Windows. Só texto vai para a Anthropic, nunca o áudio, que
  também nunca é salvo. Custa centavos por reunião.
- Tudo fica ao lado do exe: `meetings\` (reuniões), `engine\` (motor), `exports\`.

## Español

Xnote toma notas de reuniones con IA, al estilo de Granola. Graba tu micrófono
y el sonido del equipo, así que funciona con cualquier app de llamadas y ningún
bot entra en la reunión. La transcripción usa Whisper, **en tu equipo** o en la
nube (Groq u OpenAI); después, Claude combina tus apuntes con la transcripción y
escribe notas claras.

- **Primera vez:** elige dónde transcribir. **En este equipo** (privado y
  gratis): elige el motor (tarjeta NVIDIA o procesador) y el modelo; la descarga
  se verifica con SHA-256. **En la nube** (sin descarga): elige el proveedor en
  el selector (Groq, recomendado, u OpenAI) y pega la clave de API; el audio de
  cada frase se envía al proveedor.
- **Reunión:** Nueva reunión → Grabar → apunta a tu manera → Detener → **Mejorar
  notas**. Usa **Copiar aviso** para avisar a los participantes.
- **Claude:** añade tu clave de API en Configuración; se guarda en el
  Administrador de credenciales de Windows. Solo se envía texto a Anthropic,
  nunca el audio, que tampoco se guarda. Cuesta céntimos por reunión.
- Todo queda junto al exe: `meetings\` (reuniones), `engine\` (motor), `exports\`.

---

© 2026 [Skale Club](https://skale.club)
