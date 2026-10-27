# Xnote

Meeting transcription and AI-assisted notes for Windows, by **[Skale Club](https://skale.club)**.
Xnote captures your microphone and the computer's default audio output, so it can work alongside
Zoom, Meet, Teams, WhatsApp and other call apps without a meeting bot. Transcription appears as
speech segments are processed; after you stop, your notes and transcript can become a summary with
decisions and tasks.

[Português](#português) · [Español](#español) · [Development and releases](docs/DEVELOPMENT.md)

## Availability

**Status checked October 27, 2026:** source version 0.1.1. No GitHub Release or
publicly downloadable executable has been published. Build from source using the
[development guide](docs/DEVELOPMENT.md). Future release binaries belong in
[Releases](https://github.com/Skale-Club/xnote/releases).

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

## Use with Claude Code and Codex

`Xnote.exe --mcp` runs Xnote as a local [MCP](https://modelcontextprotocol.io) server, with no
window, so AI assistants such as Claude Code and Codex can use its engine on this PC. Xnote itself
sends nothing over the network; what the assistant reads goes to its AI provider, as with anything
else you share with it. It only reads, and never returns API keys; connecting it lets the assistant
read your meeting notes and transcripts.

Claude Code (available in every project):

```bash
claude mcp add --scope user xnote -- "C:\Apps\Xnote\Xnote.exe" --mcp
```

Codex, in `%USERPROFILE%\.codex\config.toml`:

```toml
[mcp_servers.xnote]
command = 'C:\Apps\Xnote\Xnote.exe'
args = ["--mcp"]
```

Use the path where your `Xnote.exe` is. Tools:

- `app_info`: version, where the exe and `Data` folder are, and the saved settings.
- `list_meetings`: saved meetings, newest first.
- `get_meeting`: `id`: one meeting with notes, enhanced notes and transcript.
- `search_meetings`: `query`, `limit`: finds text in every meeting and returns the matching
  passages.
- `transcription_status`: local or cloud transcription, installed engine and models, providers with
  a key.

## Development

Tauri 2, Rust and plain HTML/CSS/JavaScript. See the [development and release
guide](docs/DEVELOPMENT.md) for prerequisites, clone, build, test and signed-release instructions.

## Português

A preparação da gravação mostra uma barra por etapas, tempo decorrido, cancelamento e nova tentativa. O aviso informa quando a gravação ainda não começou.

O Xnote transcreve reuniões e organiza anotações com IA. Captura o microfone e a saída de áudio
padrão do Windows; funciona junto de apps de chamada sem inserir um robô na reunião.

- **Disponibilidade:** em 27/10/2026, o repositório é privado e não há release   nem executável
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
- **Claude Code e Codex**: `Xnote.exe --mcp` roda o Xnote como servidor MCP local, sem janela, para
  assistentes de IA usarem as reuniões salvas (só leitura; o que o assistente lê vai para o provedor
  de IA dele). Veja [Use with Claude Code and Codex](#use-with-claude-code-and-codex).

## Español

La preparación muestra una barra por etapas, tiempo transcurrido, cancelación y reintento. Un aviso indica que la grabación aún no ha comenzado.

Xnote transcribe reuniones y organiza apuntes con IA. Captura el micrófono y la salida de audio
predeterminada de Windows, sin añadir un bot a la llamada.

- **Disponibilidad:** al 27/10/2026, el repositorio es privado y no hay release   ni ejecutable
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
- **Claude Code y Codex**: `Xnote.exe --mcp` ejecuta Xnote como servidor MCP local, sin ventana,
  para que asistentes de IA usen las reuniones guardadas (solo lectura; lo que lee el asistente va a
  su proveedor de IA). Consulta [Use with Claude Code and Codex](#use-with-claude-code-and-codex).

## Skale Club apps

These Windows apps belong to the [Skale-Club organization](https://github.com/Skale-Club).
They share navy interfaces, blue accents, transparent product symbols, and English,
Portuguese and Spanish UI text. Each app has its own executable and portable Data folder.

| App | Purpose |
| --- | --- |
| [Xpace](https://github.com/Skale-Club/xpace) | Care for the PC (disk space, cleaning, drivers, startup, apps) and inspect hardware, live sensors and benchmarks |
| [Xink](https://github.com/Skale-Club/xink) | Draw over the screen during presentations or recordings |
| [Xnote](https://github.com/Skale-Club/xnote) | Transcribe meetings and turn notes into summaries and tasks |
| [Xvert](https://github.com/Skale-Club/xvert) | Convert videos, music and pictures between formats |

Repository access may be required depending on project visibility. Report problems through the
relevant repository, including app version, Windows version and reproduction steps. Remove
personal information from reports and transcripts before sharing.


---

© 2026 [Skale Club](https://skale.club)
