'use strict';
// Guided setup, the optional AI summary and the provider guides, run against
// the real window scripts with a fake DOM and fake Tauri commands.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const src = process.env.XNOTE_SOURCE_DIR || path.join(__dirname, '..', 'src');
const flush = () => new Promise(setImmediate);

const PROVIDERS = [
  { id: 'groq', name: 'Groq', local: false, needsKey: true, canTranscribe: true, canSummarize: true, keyPage: 'https://console.groq.com/keys', transcribeModel: 'whisper-large-v3-turbo', summaryModel: 'llama-3.3-70b-versatile', custom: false, hasKey: false, ready: false },
  { id: 'openai', name: 'OpenAI', local: false, needsKey: true, canTranscribe: true, canSummarize: true, keyPage: 'https://platform.openai.com/api-keys', transcribeModel: 'gpt-transcribe', summaryModel: 'gpt-6.1-sol', custom: false, hasKey: false, ready: false },
  { id: 'ollama', name: 'Ollama', local: true, needsKey: false, canTranscribe: false, canSummarize: true, keyPage: 'https://ollama.com/download', transcribeModel: '', summaryModel: '', custom: false, hasKey: false, ready: true },
];

async function harness(engineOverrides = {}) {
  const nodes = new Map(), calls = [], timers = [];
  const node = (id) => {
    if (!nodes.has(id)) nodes.set(id, { id, innerHTML: '', textContent: '', hidden: true, disabled: false, value: '', checked: false, dataset: {}, style: {}, classList: { add() {}, remove() {}, toggle() {} }, setAttribute() {}, addEventListener() {}, focus() {}, scrollIntoView() {}, insertAdjacentHTML() {} });
    return nodes.get(id);
  };
  const engine = { status: { gpu: false, packs: [], models: [] }, mode: '', pack: '', model: '', provider: 'groq', transcriptionModel: '', cloudKeys: [], providers: PROVIDERS.map((p) => ({ ...p })), summaryProvider: '', summaryModel: '', autoSummary: true, summaryReady: false, ...engineOverrides };
  const answers = {
    get_settings: { language: 'en', transcriptionLanguage: 'auto' },
    engine_status: engine,
    list_meetings: [],
    update_status: { current: '1.0.0', state: 'idle', version: null, notes: '', error: null, checkedAt: null },
    list_models: ['llama3.2', 'qwen3'],
    help_info: { version: '1.0.0', dataFolder: 'C:\\Apps\\Xnote\\Data', writable: true, pending: 2 },
    diagnostics: 'Xnote 1.0.0',
    assistant_info: { exe: 'C:\\Apps\\Xnote\\Xnote.exe', canWrite: true },
  };
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === 'set_summary') return { ...engine, summaryProvider: args.provider, summaryModel: args.model, autoSummary: args.auto, summaryReady: !!args.provider };
    if (command === 'use_cloud') return { ...engine, mode: 'cloud', provider: args.provider };
    if (command === 'set_provider_key') return { ...engine, providers: engine.providers.map((p) => (p.id === args.provider ? { ...p, hasKey: true, ready: true } : p)) };
    return answers[command];
  };
  const context = vm.createContext({
    console, Date, setInterval: () => 0, clearInterval() {}, setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout() {},
    navigator: { clipboard: { writeText: async () => {} } },
    document: { documentElement: {}, getElementById: node, querySelector: () => node('ver'), querySelectorAll: () => [], addEventListener() {}, removeEventListener() {} },
    window: { __TAURI__: { core: { invoke }, event: { listen: async () => () => {} }, app: { getVersion: async () => '1.0.0', getName: async () => 'Xnote' } } },
  });
  for (const file of ['i18n.js', 'guides.js', 'updates.js', 'markdown.js', 'preparation.js', 'providers-ui.js', 'app.js']) {
    vm.runInContext(fs.readFileSync(path.join(src, file), 'utf8'), context);
  }
  await flush();
  const run = (code) => vm.runInContext(code, context);
  const act = async (name, dataset = {}) => { run(`handleProviderAct(${JSON.stringify(name)}, { dataset: ${JSON.stringify(dataset)}, textContent: '', disabled: false })`); await flush(); await flush(); };
  const change = (target) => run(`handleProviderChange({ target: ${JSON.stringify(target)} })`);
  return { run, act, change, calls, timers, html: () => node('view').innerHTML, node };
}

test('first run opens the guided setup at step 1, suggesting the cloud without an NVIDIA card', async () => {
  const h = await harness();
  assert.equal(h.run('view'), 'setup');
  assert.match(h.html(), /Step 1 of 3/);
  assert.match(h.html(), /value="cloud" checked/);
  // The Groq guide and its key page are on screen.
  assert.match(h.html(), /How to set it up/);
  assert.match(h.html(), /Create API Key/);
  assert.match(h.html(), /data-act="key-page" data-provider="groq"/);
});

test('local transcription with no AI summary needs no key at all', async () => {
  const h = await harness({ status: { gpu: true, packs: ['cuda'], models: ['best'] }, mode: 'local', pack: 'cuda', model: 'best' });
  h.run("openSetup('first'); setupStep = 2; renderSetup()");
  assert.match(h.html(), /Do you want AI summaries\? \(optional\)/);
  assert.match(h.html(), /value="off" checked/, 'summaries start off');
  await h.act('summary-next');
  const call = h.calls.find((c) => c.command === 'set_summary');
  assert.deepEqual({ ...call.args }, { provider: '', model: '', auto: true });
  assert.match(h.html(), /Step 3 of 3/);
  assert.match(h.html(), /Everything stays on this computer/);
  assert.equal(h.calls.some((c) => c.command === 'set_provider_key'), false);
});

test('choosing AI on this computer shows the Ollama guide and finds the installed models', async () => {
  const h = await harness({ mode: 'local' });
  h.run("openSetup('first'); setupStep = 2; renderSetup()");
  h.change({ name: 'sumkind', value: 'local' });
  assert.match(h.html(), /ollama pull llama3\.2/);
  assert.match(h.html(), /data-act="copy-code"/);
  h.timers.forEach((fn) => fn());
  await flush(); await flush();
  assert.ok(h.calls.some((c) => c.command === 'list_models' && c.args.provider === 'ollama'));
  assert.equal(h.run('sumChoice.model'), 'llama3.2', 'the first installed model is picked');
  assert.match(h.html(), /2 models found/);
  await h.act('summary-next');
  const call = h.calls.find((c) => c.command === 'set_summary');
  assert.equal(call.args.provider, 'ollama');
});

test('a cloud summary provider shows its own guide and key field', async () => {
  const h = await harness({ mode: 'local' });
  h.run("openSetup('first'); setupStep = 2; renderSetup()");
  h.change({ name: 'sumkind', value: 'cloud' });
  assert.match(h.html(), /id="key-groq"/);
  h.change({ id: 'sum-provider', value: 'openai' });
  assert.match(h.html(), /Billing/);
  assert.match(h.html(), /value="gpt-6\.1-sol"/, 'the suggested model is filled in');
});

test('the meeting page offers to set up summaries instead of failing when they are off', async () => {
  const h = await harness({ status: { gpu: true, packs: ['cuda'], models: ['best'] }, mode: 'local', pack: 'cuda', model: 'best' });
  h.run("meeting = { id: 'm', title: '', created: 0, durationMs: 0, transcript: [], notes: '' }; view = 'meeting'; renderMeeting()");
  assert.match(h.node('head').innerHTML, /data-act="setup-summary"/);
  assert.doesNotMatch(h.node('head').innerHTML, /data-act="enhance"/);
});

test('guides and setup texts have the same keys and placeholders in EN/PT/ES', async () => {
  const h = await harness();
  const guides = h.run('GUIDES'), text = h.run('GUIDE_TEXT');
  for (const lang of ['pt', 'es']) {
    assert.deepEqual(Object.keys(guides[lang]).sort(), Object.keys(guides.en).sort());
    for (const id of Object.keys(guides.en)) assert.equal(guides[lang][id].length, guides.en[id].length, `${lang}/${id} steps`);
    assert.deepEqual(Object.keys(text[lang]).sort(), Object.keys(text.en).sort());
    const vars = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const key of Object.keys(text.en)) assert.deepEqual(vars(text[lang][key]), vars(text.en[key]), `${lang}.${key}`);
  }
  // Every built-in provider has a guide.
  for (const p of PROVIDERS) assert.ok(guides.en[p.id], p.id);
});

test('Settings show how to connect AI assistants, with this exe in each snippet', async () => {
  const h = await harness({ status: { gpu: true, packs: ['cuda'], models: ['best'] }, mode: 'local', pack: 'cuda', model: 'best' });
  h.run("openSettings()");
  // One section at a time: General first, with the side menu.
  assert.match(h.html(), /data-section="assistants"/);
  assert.match(h.html(), /id="app-lang"/);
  assert.doesNotMatch(h.html(), /mcpServers/);
  h.run("openSettings('assistants')");
  await flush(); await flush();
  assert.match(h.html(), /<h2>AI assistants<\/h2>/);
  assert.match(h.html(), /Choose the assistant you use/);
  // JSON escapes the backslashes of the Windows path.
  assert.match(h.html(), /&quot;command&quot;: &quot;C:\\\\Apps\\\\Xnote\\\\Xnote\.exe&quot;/);
  h.change({ id: 'assistant-client', value: 'claude-code' });
  assert.match(h.html(), /claude mcp add --scope user xnote -- &quot;C:\\Apps\\Xnote\\Xnote\.exe&quot; --mcp/);
  h.change({ id: 'assistants-write', checked: false });
  assert.deepEqual({ ...h.calls.find((c) => c.command === 'set_assistants_can_write').args }, { allowed: false });
});

test('Help shows the version, the Data folder and audio waiting for the connection', async () => {
  const h = await harness({ status: { gpu: true, packs: ['cuda'], models: ['best'] }, mode: 'local', pack: 'cuda', model: 'best' });
  await flush();
  // At start the waiting audio shows in the notice bar.
  assert.equal(h.node('notice-bar').hidden, false);
  assert.match(h.node('notice-bar').textContent, /2 part\(s\) of the audio/);
  h.run("openSettings('help')");
  await flush(); await flush();
  assert.match(h.html(), /1\.0\.0/);
  assert.match(h.html(), /data-act="open-data"/);
  assert.match(h.html(), /data-act="retry-pending"/);
  assert.match(h.html(), /data-act="copy-diagnostics"/);
  h.run('showPending(0)');
  assert.equal(h.node('notice-bar').hidden, true);
});
