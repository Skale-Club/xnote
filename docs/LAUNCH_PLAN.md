# Plano de lançamento — Xnote

Objetivo: lançar o Xnote como app **gratuito**, leve e robusto, em que a pessoa usa **as próprias
chaves de API** ou **o próprio computador**, sem ficar presa a um provedor.

Tamanho de cada tarefa: **P** (até meio dia) · **M** (1–2 dias) · **G** (3+ dias).

---

## Visão geral das fases

| Fase | Tema | Resultado | Tamanho |
|---|---|---|---|
| 0 | Fundações | CI em todo push, CSP, licença, `main.rs` dividido | M |
| 1 | Camada de provedores (Rust) | Qualquer provedor grande para transcrição e resumo | G |
| 2 | Tela de Provedores e onboarding | Um só lugar para chaves, primeiro uso em 3 passos | G |
| 3 | Robustez | Fila de áudio, logs, diagnóstico, pasta `Data` verificada | M |
| 4 | Acabamento de UX | Selo de privacidade, atalhos, acessibilidade, busca no conteúdo, modelos de resumo | M |
| 5 | Lançamento | Exe assinado (SmartScreen), README novo, v1.0 | M |

Ordem: 0 → 1 → 2 são sequenciais (cada uma depende da anterior). 3 e 4 podem andar em paralelo
depois da 2. A 5 fecha tudo.

---

## Fase 0 — Fundações

Rede de segurança antes das mudanças grandes.

- [ ] **CI em push e PR** (`.github/workflows/ci.yml`, `windows-latest`): `cargo fmt --check`,
      `cargo clippy -- -D warnings`, `cargo test --locked`, `node --test scripts/*.test.cjs`. **P**
- [ ] **CSP restrita** em `tauri.conf.json` (hoje `"csp": null`):
      `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'`.
      As chamadas de rede saem do Rust, então o webview não precisa de `connect-src` externo. **P**
- [ ] **Licença e arquivos de comunidade**: `LICENSE` (recomendado MIT), `SECURITY.md`
      (como reportar falhas), `CONTRIBUTING.md` curto, modelos de issue (bug / ideia) em
      `.github/ISSUE_TEMPLATE/`. **P**
- [ ] **Dividir `main.rs`** (724 linhas, 27 comandos) em `src-tauri/src/commands/`:
      `settings.rs`, `meetings.rs`, `recording.rs`, `providers.rs`, `updates.rs`. Sem mudar
      comportamento. **M**
- [ ] **Tipos em vez de strings**: `enum TranscriptionMode { Local, Cloud }` e afins, com
      `serde(rename_all = "lowercase")`, para o JSON salvo continuar igual. **P**

Pronto quando: a CI passa num PR e o app abre igual ao de hoje.

---

## Fase 1 — Camada de provedores (Rust)

O coração do plano. Hoje o resumo só funciona com OpenAI e o modelo está fixo em `ai.rs`
(`MODEL = "gpt-6.1-sol"`); a transcrição em nuvem só aceita Groq e OpenAI (`cloud.rs`).

### 1.1 Modelo de dados

Novo módulo `src-tauri/src/providers.rs`:

```rust
pub enum Api { OpenAiCompatible, Anthropic }

pub struct Provider {
    pub id: String,            // "groq", "openrouter", "ollama", "custom-1"...
    pub name: String,          // nome exibido
    pub api: Api,
    pub base_url: String,
    pub needs_key: bool,       // false para Ollama / LM Studio
    pub can_transcribe: bool,  // tem /audio/transcriptions
    pub can_summarize: bool,   // tem chat
    pub key_page: Option<String>,
    pub default_chat_model: Option<String>,
    pub default_stt_model: Option<String>,
}
```

### 1.2 Provedores de fábrica

| Provedor | Transcrição | Resumo | Chave | Observação |
|---|---|---|---|---|
| **Groq** | ✅ `whisper-large-v3-turbo` | ✅ | sim | Padrão recomendado: rápido e tem plano grátis |
| **OpenAI** | ✅ | ✅ | sim | Mantém a chave já salva |
| **OpenRouter** | — | ✅ | sim | Dá acesso a centenas de modelos com uma chave só |
| **Google Gemini** | — | ✅ | sim | Endpoint compatível com OpenAI |
| **Anthropic (Claude)** | — | ✅ | sim | Adaptador próprio (Messages API) |
| **Mistral** | — | ✅ | sim | Compatível com OpenAI |
| **DeepSeek** | — | ✅ | sim | Compatível com OpenAI |
| **Ollama** | — | ✅ | não | Roda no PC, `http://localhost:11434/v1` |
| **LM Studio** | — | ✅ | não | Roda no PC, `http://localhost:1234/v1` |
| **Personalizado** | opcional | ✅ | opcional | Qualquer URL compatível com OpenAI |

A lista de capacidades fica no código e é fácil de ajustar; a transcrição local (whisper.cpp)
continua como está.

### 1.3 Adaptadores

- [ ] **`OpenAiCompatible`**: `POST {base}/chat/completions` com `stream: true` (SSE,
      `choices[0].delta.content`). Serve para quase todos. Reaproveita o leitor de stream e as
      regras de erro/recusa de `ai.rs`. **M**
- [ ] **`Anthropic`**: `POST /v1/messages` com `stream: true` (eventos `content_block_delta`),
      cabeçalhos `x-api-key` e `anthropic-version`. **P**
- [ ] **Transcrição** (`cloud.rs`) passa a aceitar qualquer provedor com `can_transcribe`,
      inclusive o personalizado. A lógica de `fields()` por provedor continua. **P**
- [ ] **Listar modelos**: `GET {base}/models` (também valida a chave, de graça). A UI mostra uma
      lista e aceita texto livre, para nenhum app quebrar quando o provedor renomear um modelo. **P**
- [ ] **Tempo limite e novas tentativas** iguais para todos (429/5xx com `retry-after`, já
      existe em `cloud.rs`, sobe para um helper comum). **P**

### 1.4 Configurações e migração

- [ ] `Settings` ganha:
      `summary: { provider, model }`, `transcriptionProvider`/`transcriptionModel`,
      `customProviders: [{ id, name, baseUrl, canTranscribe }]`. **P**
- [ ] **Migração sem dor**: quem já tem chave OpenAI continua com resumo na OpenAI; quem usa
      Groq na transcrição continua igual. Os nomes no Gerenciador de Credenciais
      (`Xnote / groq-api-key`, `openai-api-key`) **não mudam**. **P**
- [ ] Chaves de novos provedores seguem o mesmo padrão `Xnote / <id>-api-key`. URL base de
      provedor personalizado fica no JSON (não é segredo). **P**

### 1.5 Comandos Tauri e MCP

- [ ] Substituir `has_api_key`/`set_api_key`/`clear_api_key`/`set_cloud_key`/`use_cloud` por:
      `list_providers`, `set_provider_key`, `clear_provider_key`, `list_models`,
      `set_summary_provider`, `set_transcription_provider`, `add_custom_provider`,
      `remove_custom_provider`, `test_provider`. **M**
- [ ] `ai::enhance` recebe provedor + modelo das configurações. O texto do prompt (`SYSTEM`)
      não muda. **P**
- [ ] MCP `transcription_status` passa a informar também o provedor do resumo (sem chaves). **P**

### 1.6 Testes

- [ ] Leitura de SSE do Chat Completions e da Anthropic (mesmo estilo dos testes atuais de `ai.rs`).
- [ ] Migração das configurações antigas.
- [ ] Montagem de URL/cabeçalhos por provedor; provedor sem chave (Ollama) não envia `Authorization`.

Pronto quando: dá para gravar com transcrição local e resumir com Ollama **sem internet**, e
também resumir com Groq, OpenRouter ou Claude trocando só a configuração.

---

## Fase 2 — Tela de Provedores e onboarding

Hoje a chave OpenAI aparece em dois lugares (transcrição em nuvem e card "ChatGPT" nas
Configurações). Tudo isso vira uma tela só.

### 2.1 Tela "Provedores" (nova, no menu lateral ou em Configurações)

```
Provedores
┌───────────────────────────────────────────────────────────────┐
│ Groq            🎙 Transcrição  ✨ Resumo     ✓ Conectado  [⋯] │
│ OpenAI          🎙 Transcrição  ✨ Resumo     Adicionar chave  │
│ OpenRouter                      ✨ Resumo     Adicionar chave  │
│ Ollama (no PC)                  ✨ Resumo     ✓ Encontrado     │
│ + Provedor personalizado                                      │
└───────────────────────────────────────────────────────────────┘

Usar para
  Transcrição   [ No computador (whisper) ▾ ]  modelo [ best ▾ ]
  Resumo        [ Groq ▾ ]                     modelo [ llama-… ▾ ]
```

- [ ] Uma linha por provedor: capacidades, status, "obter chave", "testar", "remover". **M**
- [ ] Bloco **"Usar para"** com dois seletores (transcrição e resumo) e o modelo de cada um. **M**
- [ ] Ollama/LM Studio detectados automaticamente (ping em `localhost`), sem chave. **P**
- [ ] Formulário de provedor personalizado: nome, URL base, chave opcional, "faz transcrição?". **P**

### 2.2 Onboarding de primeiro uso (3 passos)

1. **Como transcrever?** No computador (recomendado com placa NVIDIA) ou na nuvem
   (recomendado sem placa) — reaproveita a tela de setup atual.
2. **Quem escreve o resumo?** Lista de provedores com uma opção "agora não". Mostra o custo
   aproximado em palavras simples ("grátis no seu PC", "plano grátis", "pago por uso").
3. **Teste rápido:** 10 segundos de gravação mostrando os medidores de áudio e a primeira frase
   transcrita.

### 2.3 Textos

- [ ] Todos os textos novos em `i18n.js` nos três idiomas (en, pt, es). Remover as menções fixas a
      "ChatGPT" (`aiCard`, `needKey`, `keyNote`). **P**
- [ ] Mensagens de erro claras por provedor ("Chave inválida", "Sem saldo", "Modelo não
      encontrado — escolha outro", "Ollama não está aberto"). **P**

### 2.4 Organização do frontend

- [ ] Dividir `app.js` (666 linhas) em `views/meeting.js`, `views/setup.js`,
      `views/settings.js`, `views/providers.js` e `state.js`. Sem framework e sem etapa de build. **M**

Pronto quando: uma pessoa nova configura tudo sem abrir as Configurações e sem ler o README.

---

## Fase 3 — Robustez

- [ ] **Fila de áudio na transcrição em nuvem**: se um trecho falhar depois das tentativas,
      ele vai para `Data/pending/<reunião>/` e é reenviado quando a rede voltar. É apagado assim
      que transcrito. Aviso na tela: "3 trechos aguardando conexão". **M**
- [ ] **Resumo que falha não perde nada**: botão "tentar de novo" e "tentar com outro provedor". **P**
- [ ] **Logs**: `Data/logs/xnote.log` com rotação (ex.: 5 arquivos × 1 MB). Nunca registrar
      chaves, texto de notas ou transcrição. **P**
- [ ] **"Copiar diagnóstico"** em Configurações: versão, Windows, modo, provedores (sem chaves),
      últimas linhas do log. Facilita o suporte. **P**
- [ ] **Pasta `Data` sem permissão de escrita** (ex.: exe em `Program Files`): detectar na
      abertura e mostrar o aviso "mova o Xnote para uma pasta sua". Hoje `portable.rs` ignora o erro. **P**
- [ ] **Teste de dispositivos**: aviso se o microfone ou a saída de áudio ficarem em silêncio por
      mais de 30 s durante a gravação. **P**

---

## Fase 4 — Acabamento de UX

- [ ] **Selo de privacidade em cada reunião**: "🔒 Tudo no seu PC" ou "☁️ Groq · OpenRouter". A
      reunião guarda quais provedores foram usados. **P**
- [ ] **Atalhos**: `Ctrl+N` nova reunião, `Ctrl+R` gravar/parar, `Ctrl+F` buscar,
      `Ctrl+E` exportar. **P**
- [ ] **Acessibilidade**: `aria-label` em todos os botões só com ícone, foco visível, navegação
      por teclado na lista de reuniões. **P**
- [ ] **Busca no conteúdo** na barra lateral (notas e transcrição), reaproveitando a busca que o
      MCP já faz em `mcp_tools.rs`. **P**
- [ ] **Modelos de resumo**: Geral, Vendas, 1:1, Daily, Entrevista. Uma lista simples de
      instruções extras somadas ao prompt base, escolhida antes ou depois da reunião. **M**
- [ ] **Exportar**: além do Markdown, "copiar para a área de transferência" já formatado para
      colar em e-mail, Notion ou Docs. **P**

---

## Fase 5 — Lançamento

- [ ] **Assinatura Authenticode** do `Xnote.exe` para não aparecer o aviso do SmartScreen.
      Opção gratuita: **SignPath Foundation** (exige repositório público com licença OSI).
      Alternativa paga e simples: Azure Trusted Signing. Entra no `release.yml` antes da
      assinatura minisign do atualizador. **M**
- [ ] **README novo** (os textos longos de privacidade e limites vão para `docs/PRIVACY.md` e
      `docs/TROUBLESHOOTING.md`):
      1. Uma frase do que é e um GIF de 15 s.
      2. Botão de download.
      3. Começar em 3 passos.
      4. Tabela **"Quanto custa?"**: grátis no PC (whisper + Ollama), plano grátis (Groq), pago
         por uso (OpenAI, OpenRouter, Claude…).
      5. Privacidade em 4 linhas, com link para os detalhes. **M**
- [ ] **Capturas de tela** das telas principais (reunião, provedores, onboarding). **P**
- [ ] **CHANGELOG** e versão **1.0.0** via `scripts/release.ps1`. **P**
- [ ] **Teste final em PC limpo**: Windows 10 e 11, com e sem placa NVIDIA, sem internet
      (local + Ollama), com cada provedor de fábrica. **M**

---

## Decisões em aberto

| Decisão | Recomendação |
|---|---|
| Repositório público? | Sim. Libera a assinatura grátis e passa confiança num app que lida com chaves |
| Licença | MIT |
| Provedor padrão do resumo para quem não tem nada | Groq (plano grátis e a mesma chave da transcrição) |
| Incluir Anthropic com adaptador próprio | Sim, é pequeno e é um provedor grande |
| Transcrição em nuvem além de Groq/OpenAI (Deepgram, AssemblyAI) | Depois do 1.0; o provedor personalizado já cobre servidores compatíveis |
| macOS | Depois do 1.0; captura do áudio do sistema no Mac exige outro caminho (ScreenCaptureKit) |
