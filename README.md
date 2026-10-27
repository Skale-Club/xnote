# Xnote

Meeting transcription and AI-assisted notes for Windows, by **[Skale Club](https://skale.club)**.
Xnote captures your microphone and the computer's default audio output, so it can work alongside
Zoom, Meet, Teams, WhatsApp and other call apps without a meeting bot. Transcription appears as
speech segments are processed; after you stop, your notes and transcript can become a summary with
decisions and tasks.

[Português](#português) · [Español](#español) · [Development and releases](docs/DEVELOPMENT.md)

## Availability

**Status checked October 26, 2026:** version 0.1.0 is in source control; no GitHub Release or
downloadable executable has been published. This repository is currently **private** and requires
access to [Skale-Club/xnote](https://github.com/Skale-Club/xnote). Use the [build
instructions](docs/DEVELOPMENT.md) until a release is available. Future binaries belong in
[Releases](https://github.com/Skale-Club/xnote/releases).

The built-in updater uses unauthenticated GitHub requests. It cannot retrieve private releases, even
if you are signed into GitHub in your browser.

## Run it

Use a Windows PC with Microsoft Edge WebView2 Runtime and a writable folder for the executable. No
app installer is required. Open `Xnote.exe`, choose a transcription mode and set the app and meeting
languages in Settings. The interface supports English, Portuguese and Spanish.

### Choose transcription

| Mode | Requirements | What leaves the computer |
|---|---|---|
| Local | One-time engine and model download; CPU or compatible NVIDIA GPU | Audio stays on the computer during transcription |
| Groq | Internet and a Groq API key with access to the configured model | Speech-segment audio goes to Groq |
| OpenAI | Internet and an OpenAI API key with access to the configured model | Speech-segment audio goes to OpenAI |

Local mode uses whisper.cpp with a CPU build or a CUDA 12.4 NVIDIA build. Choose the larger
large-v3-turbo model or the smaller small model in setup. The app verifies downloaded files with
pinned SHA-256 hashes. Processing speed depends on the hardware and model; CPU transcription can lag
behind a call. Once the files are installed, local transcription does not require a cloud key.
Enhanced notes still use the OpenAI API, even when transcription is local.

For cloud mode, choose the provider and enter its key in setup. The app checks the key by requesting
the provider's model list before saving it. This checks authentication, not successful
transcription, billing or access to a particular model. You can change transcription mode in
Settings.

### Take meeting notes

1. Choose **New meeting**. Let participants know that you are transcribing the call.
2. Make sure the call uses the same microphone and output device selected as    Windows defaults.
   Xnote captures those devices when recording starts.
3. Choose **Start recording** and write your own notes. The transcript labels    microphone audio
   **You** and computer-output audio **Them**.
4. Choose **Stop** and wait for pending segments to finish. If new transcript    text was added and
   an OpenAI key is configured, enhanced notes start    automatically. Without that key, your notes
   and transcript remain available.
5. Use **Enhance notes** or **Regenerate** to generate them manually, then    **Copy** or **Export
   .md** to share the result.

You can stop and resume within the same meeting. Enhanced notes can also be generated from written
notes without recording. Review generated decisions, names and tasks before sharing.

## Enhanced notes and API usage

Add an OpenAI API key in Settings. The same key is used if you choose OpenAI for cloud
transcription. A Groq key only enables Groq transcription; it does not enable enhanced notes.

The app calls the OpenAI Responses API directly; it does not sign into the ChatGPT website. API
usage has [separate billing from a ChatGPT
subscription](https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform).
Costs, quotas and model access depend on the provider account and current rates; there is no fixed
per-meeting price promised by this app.

The current source configures `whisper-large-v3-turbo` for Groq transcription, `gpt-transcribe` for
OpenAI transcription and `gpt-6.1-sol` for enhanced notes. These identifiers describe the code
configuration, not a guarantee of account access or provider availability. A model error requires
checking the provider response and configuration.

## Data and privacy

| Data | Storage or destination |
|---|---|
| Settings | `Data/xnote.json` beside the executable |
| Local engine and models | `Data/engine` |
| Notes, transcript and enhanced notes | JSON files in `Data/meetings` |
| Deleted meetings | Moved to `Data/meetings/.trash` |
| Markdown exports | `Data/exports` |
| API keys | Windows Credential Manager, outside the portable folder |

Xnote does not save an audio recording for later playback. It processes audio segments in memory
and, in cloud mode, sends them to the selected provider. Enhanced notes send your written notes and
transcript to OpenAI. After a recording stops, this text transfer is automatic when the conditions
above apply.

Enhanced-note requests set `store: false`. This is not a promise of zero retention across all
provider systems; see [OpenAI's data
controls](https://developers.openai.com/api/docs/guides/your-data). Cloud transcription also follows
the selected provider's data policies. Local meeting files and exports are ordinary files, without
app-level encryption.

Close Xnote before copying its folder for backup. API keys do not move with the folder and must be
configured on another PC. Deleted meetings remain in the trash directory; deletion in the UI is not
secure erasure.

## Limits and troubleshooting

- **Missing audio:** check Windows' default input/output devices, microphone   access and the in-app
  audio meters. Stop and restart recording after changing   devices. Xnote does not select
  individual call-app audio streams.
- **Unrelated sound in the transcript:** system-output capture includes other   apps playing through
  the default output device.
- **Speaker names:** only the two sources are distinguished; several remote   participants share the
  **Them** label.
- **Delayed text:** transcription is segment-based and may lag on slower   hardware or networks. It
  is not word-by-word live captioning.
- **Cloud errors:** check credentials, model access, connectivity and provider   quota. No
  transcript audio archive is saved for later recovery.
- **No automatic update:** no release is published yet, and private release   access is unsupported
  by the current updater.

## Development

Tauri 2, Rust and plain HTML/CSS/JavaScript. See the [development and release
guide](docs/DEVELOPMENT.md) for prerequisites, clone, build, test and signed-release instructions.

## Português

O Xnote transcreve reuniões e organiza anotações com IA. Captura o microfone e a saída de áudio
padrão do Windows; funciona junto de apps de chamada sem inserir um robô na reunião.

- **Disponibilidade:** em 26/10/2026, o repositório é privado e não há release   nem executável
  publicado. Pessoas com acesso podem [compilar o app](docs/DEVELOPMENT.md).
- **Transcrição:** escolha processamento local com whisper.cpp ou nuvem com   Groq/OpenAI. No modo
  local, o áudio permanece no computador; na nuvem, os   trechos de áudio são enviados ao provedor.
- **Uso:** Nova reunião → Gravar → anotar → Parar. Configure a chamada com os   dispositivos padrão
  do Windows e avise os participantes de que a reunião está sendo transcrita.
- **Resumo:** ao parar, havendo texto novo e uma chave OpenAI, as notas   melhoradas são geradas
  automaticamente. Isso envia anotações e transcrição   à OpenAI mesmo com transcrição local. Uma
  chave Groq não habilita o resumo.
- **Custos:** a API tem cobrança separada do ChatGPT; não há preço fixo por   reunião. A validação
  da chave não garante acesso ao modelo ou saldo.
- **Dados:** arquivos ficam em `Data` ao lado do executável; chaves ficam no   Gerenciador de
  Credenciais. O app não salva áudio para reprodução. Os   arquivos de reunião não têm criptografia
  própria, e `store: false` não   significa retenção zero em todos os sistemas do provedor.
- **Limites:** distingue “Você” de “Eles”, sem separar cada participante   remoto. A transcrição
  pode atrasar. O atualizador não acessa releases privadas.

## Español

Xnote transcribe reuniones y organiza apuntes con IA. Captura el micrófono y la salida de audio
predeterminada de Windows, sin añadir un bot a la llamada.

- **Disponibilidad:** al 26/10/2026, el repositorio es privado y no hay release   ni ejecutable
  publicado. Quienes tengan acceso pueden [compilar la app](docs/DEVELOPMENT.md).
- **Transcripción:** local con whisper.cpp o en la nube con Groq/OpenAI. El modo   local procesa el
  audio en el equipo; la nube envía segmentos al proveedor.
- **Uso:** Nueva reunión → Grabar → tomar apuntes → Detener. Use los dispositivos   predeterminados
  de Windows y avise a los participantes de que la reunión se está transcribiendo.
- **Resumen:** al detener, si hay texto nuevo y una clave OpenAI, se generan las   notas
  automáticamente. Se envían apuntes y transcripción a OpenAI incluso con   transcripción local. Una
  clave Groq no habilita el resumen.
- **Costes:** la API se factura por separado de ChatGPT; no se promete un precio   fijo por reunión.
  Validar la clave no garantiza acceso al modelo ni saldo.
- **Datos:** archivos en `Data` junto al ejecutable; claves en el Administrador   de credenciales.
  No se guarda audio para reproducción. Los archivos no tienen   cifrado propio y `store: false` no
  equivale a retención cero en todos los   sistemas del proveedor.
- **Límites:** distingue “Tú” y “Ellos”, sin identificar a cada participante   remoto. Puede haber
  retrasos. El actualizador no accede a releases privadas.

## Skale Club apps

These Windows apps belong to the [Skale-Club organization](https://github.com/Skale-Club). They
share a dark navy interface, blue accents and English, Portuguese and Spanish UI text. Each app has
its own executable and data folder.

| App | Purpose |
|---|---|
| [Xink](https://github.com/Skale-Club/xink) | Draw over the screen during presentations or recordings |
| [Xpecs](https://github.com/Skale-Club/xpecs) | Inspect hardware, live sensors and benchmark results |
| [Xcare](https://github.com/Skale-Club/xcare) | Review PC maintenance, clean selected files and manage startup programs |
| [Xnote](https://github.com/Skale-Club/xnote) | Transcribe meeting audio and turn notes into summaries and tasks |

Xnote currently requires repository access. Report problems through the relevant repository's Issues
tab, including the app version, Windows version and steps to reproduce. Remove personal information
from reports and transcripts before sharing.

---

© 2026 [Skale Club](https://skale.club)
