'use strict';

// Guided setup (1. transcription, 2. optional AI summary, 3. done), the AI
// summary choice and the Providers card in Settings. Shares the window's
// globals from app.js (engine, view, meeting, render, toast, esc, t…).
// Every provider shows a short step-by-step guide (guides.js).

let setupStep = 1;          // 1 transcription · 2 summary · 3 done
let setupFlow = 'first';    // first: the whole guided setup · change: only transcription, from Settings
let setupChoice = null;     // { mode, pack, model, provider, transcribeModel }
let sumChoice = null;       // { kind: off | local | cloud, provider, model, auto }
let install = null;         // { engine: {done,total}, model: {done,total}, error }
let keyOpen = null;         // provider whose key field is open in the Providers card
const modelLists = {};      // provider id → { state: loading | ok | error, list, error }
let assistant = null;       // { exe, canWrite } for the AI assistants card
let assistantClient = 'claude-desktop';
let settingsSection = 'general'; // general | transcription | summary | providers | assistants | privacy

const providerById = (id) => engine?.providers?.find((p) => p.id === id);
const providerName = (id) => providerById(id)?.name || id;
const tagOf = (text, cls = '') => `<span class="tag${cls ? ` ${cls}` : ''}">${esc(text)}</span>`;
const providerTags = (p) => (FREE_PLAN.includes(p.id) ? tagOf(t('freeTag'), 'free') : '') + (p.local ? tagOf(t('localTag'), 'free') : '');

function providerError(error, p) {
  const e = String(error);
  if (e === 'not running') return t('notRunning', { name: p?.name || '' });
  if (e === 'https required') return t('httpsRequired');
  if (e === 'invalid address') return t('invalidAddress');
  if (e === 'no model') return t('noModel');
  return t('keyInvalid', { error: e });
}

// ---------- shared pieces ----------

/** Still to be set up: no key yet, or (local apps) no model found yet. */
function needsGuide(p) {
  if (p.needsKey) return !p.hasKey;
  if (p.local) return !(modelLists[p.id]?.state === 'ok' && modelLists[p.id].list.length);
  return false;
}

/** The guide of a provider, only while it still needs setting up. */
const guideFor = (p) => (needsGuide(p) ? guideHtml(p.custom ? 'custom' : p.id) : '');

/** The numbered guide for a provider ("local" for the whisper download). */
function guideHtml(id) {
  const steps = guideOf(id).map((step) => {
    if (typeof step === 'string') return `<li>${esc(step)}</li>`;
    return `<li>${esc(step.text)}<div class="code-row"><code>${esc(step.code)}</code>` +
      `<button class="btn ghost small" data-act="copy-code" data-code="${esc(step.code)}" aria-label="${esc(t('copyMd'))}">${svg(ICONS.copy)}</button></div></li>`;
  }).join('');
  return `<div class="guide"><h4 class="silk">${esc(t('howTo'))}</h4><ol>${steps}</ol></div>`;
}

/** Key field, site button and status for a provider that needs a key. */
function keyHtml(p) {
  const site = p.keyPage ? `<button class="btn" data-act="key-page" data-provider="${esc(p.id)}">${esc(t('openSite', { name: p.name }))}</button>` : '';
  if (!p.needsKey) return needsGuide(p) && site ? `<div class="row">${site}</div>` : '';
  if (p.hasKey) {
    return `<div class="row"><span class="ok-text">✓ ${esc(t('keyOk', { name: p.name }))}</span>` +
      `<button class="btn ghost small danger-text" data-act="clear-provider-key" data-provider="${esc(p.id)}">${esc(t('remove'))}</button></div>`;
  }
  return `<div class="row">${site}` +
    `<input id="key-${esc(p.id)}" class="field" type="password" placeholder="${esc(t('keyFor', { name: p.name }))}" autocomplete="off" aria-label="${esc(t('keyFor', { name: p.name }))}">` +
    `<button class="btn primary" data-act="check-key" data-provider="${esc(p.id)}">${esc(t('verify'))}</button></div>`;
}

/** A model field with the provider's list as suggestions. */
function modelHtml(p, value, inputId, label) {
  const found = modelLists[p.id];
  const options = (found?.list || []).map((m) => `<option value="${esc(m)}"></option>`).join('');
  let status = t('modelHint');
  if (found?.state === 'loading') status = t('findingModels');
  else if (found?.state === 'ok') status = found.list.length ? t('modelsFound', { n: found.list.length }) : t('noModels');
  else if (found?.state === 'error') status = providerError(found.error, p);
  return `<div class="row"><span class="row-label">${esc(label)}</span>` +
    `<input id="${inputId}" class="field" list="${inputId}-list" value="${esc(value)}" autocomplete="off" aria-label="${esc(label)}">` +
    `<datalist id="${inputId}-list">${options}</datalist>` +
    `<button class="btn" data-act="find-models" data-provider="${esc(p.id)}"${p.ready ? '' : ' disabled'}>${esc(t('findModels'))}</button></div>` +
    `<p class="note${found?.state === 'error' ? ' warn' : ''}">${esc(status)}</p>`;
}

async function findModels(id, { pickFirst = false } = {}) {
  const p = providerById(id);
  if (!p) return;
  modelLists[id] = { state: 'loading', list: modelLists[id]?.list || [] };
  rerender();
  try {
    const list = await invoke('list_models', { provider: id });
    modelLists[id] = { state: 'ok', list };
    if (pickFirst && sumChoice && sumChoice.provider === id && !sumChoice.model && list.length) sumChoice.model = list[0];
  } catch (error) {
    modelLists[id] = { state: 'error', list: [], error: String(error) };
  }
  rerender();
}

/** Redraws whichever page holds the provider pieces. */
function rerender() {
  if (view === 'setup') renderSetup();
  else if (view === 'settings') renderSettings();
}

async function checkKey(id, button) {
  const input = $(`key-${id}`);
  const key = input?.value.trim();
  if (!key) { input?.focus(); return false; }
  if (button) { button.disabled = true; button.textContent = t('checking'); }
  try {
    engine = await invoke('set_provider_key', { provider: id, key });
    toast(t('keyOk', { name: providerName(id) }));
    if (keyOpen === id) keyOpen = null;
    if (sumChoice?.provider === id) findModels(id);
    else rerender();
    return true;
  } catch (e) {
    toast(providerError(e, providerById(id)));
    rerender();
    return false;
  }
}

/** Who gets audio or text, for the privacy lines. */
function recipients() {
  const names = [];
  if (engine?.mode === 'cloud' && !providerById(engine.provider)?.local) names.push(providerName(engine.provider));
  const s = providerById(engine?.summaryProvider);
  if (s && !s.local && !names.includes(s.name)) names.push(s.name);
  return names;
}

// ---------- step 1: transcription ----------

function initSetupChoice() {
  const status = engine.status;
  const cloudProvider = providerById(engine.provider)?.canTranscribe ? engine.provider : 'groq';
  setupChoice = {
    // Without an NVIDIA card local transcription is slow, so the cloud is
    // suggested first there.
    mode: engine.mode || (status.gpu ? 'local' : 'cloud'),
    pack: engine.pack || (status.gpu ? 'cuda' : 'cpu'),
    model: engine.model || 'best',
    provider: cloudProvider,
    transcribeModel: engine.mode === 'cloud' ? engine.transcriptionModel : '',
  };
}

function choiceHtml(group, id, name, desc, opts = {}) {
  const status = engine.status;
  const installed = group === 'pack' ? status.packs.includes(id) : group === 'model' ? status.models.includes(id) : false;
  const tag = installed ? tagOf(t('installed')) : opts.recommended ? tagOf(t('recommended')) : opts.tag || '';
  const size = SIZES_MB[id] && !installed ? `${SIZES_MB[id]} MB` : '';
  const current = group === 'sumkind' ? sumChoice.kind : setupChoice[group];
  return `<label class="choice"><input type="radio" name="${group}" value="${id}"${current === id ? ' checked' : ''}${opts.disabled ? ' disabled' : ''}>` +
    `<div><b>${esc(name)}${tag}</b><span>${esc(opts.disabled ? opts.disabledText : desc)}</span></div>` +
    `<span class="size readout">${size}</span></label>`;
}

function transcriptionStep() {
  const status = engine.status;
  const modeCard = `<div class="card"><h3>${esc(t('modeCard'))}</h3>` +
    choiceHtml('mode', 'local', t('localName'), t('localDesc'), { recommended: status.gpu }) +
    choiceHtml('mode', 'cloud', t('cloudName'), t('cloudDesc'), { recommended: !status.gpu }) + '</div>';
  return modeCard + (setupChoice.mode === 'cloud' ? cloudSetup() : localSetup());
}

function localSetup() {
  const status = engine.status;
  const need = (status.packs.includes(setupChoice.pack) ? 0 : SIZES_MB[setupChoice.pack]) +
    (status.models.includes(setupChoice.model) ? 0 : SIZES_MB[setupChoice.model]);
  const label = need ? t('download', { size: `${need} MB` }) : t('useThis');
  const busy = install && !install.error;
  return `<div class="card">${guideHtml('local')}<h3>${esc(t('engineCard'))}</h3>` +
    choiceHtml('pack', 'cuda', t('gpuName'), t('gpuDesc'), { recommended: status.gpu, disabled: !status.gpu, disabledText: t('noGpu') }) +
    choiceHtml('pack', 'cpu', t('cpuName'), t('cpuDesc'), { recommended: !status.gpu }) +
    `<h3>${esc(t('modelCard'))}</h3>` +
    choiceHtml('model', 'best', t('bestName'), t('bestDesc'), { recommended: true }) +
    choiceHtml('model', 'light', t('lightName'), t('lightDesc')) +
    `<p class="note">${esc(t('localDownloadNote'))}</p></div>` +
    `<div class="setup-actions"><button class="btn primary" data-act="install"${busy ? ' disabled' : ''}>${esc(busy ? t('downloading') : label)}</button></div>` +
    '<div class="progress" id="setup-progress"></div>';
}

function cloudSetup() {
  const transcribers = engine.providers.filter((p) => p.canTranscribe);
  const p = providerById(setupChoice.provider) || transcribers[0];
  const opt = (x) => `<option value="${esc(x.id)}"${x.id === p.id ? ' selected' : ''}>${esc(x.name)}${x.id === 'groq' ? ` (${esc(t('recommended').toLowerCase())})` : ''}</option>`;
  const desc = t(`${p.id}Desc`);
  const model = setupChoice.transcribeModel || p.transcribeModel;
  return `<div class="card"><h3>${esc(t('providerCard'))}</h3>` +
    `<div class="row"><select id="provider" class="field" aria-label="${esc(t('providerCard'))}">${transcribers.map(opt).join('')}</select>${providerTags(p)}</div>` +
    (desc !== `${p.id}Desc` ? `<p class="note">${esc(desc)}</p>` : '') +
    guideFor(p) + keyHtml(p) +
    (p.ready ? modelHtml(p, model, 'transcribe-model', t('transcribeModel')) : '') +
    `<p class="note">${esc(t('cloudKeyNote', { name: p.name }))}</p></div>` +
    `<div class="setup-actions"><button class="btn primary" data-act="use-cloud"${p.ready ? '' : ' disabled'}>${esc(t('useProvider', { name: p.name }))}</button></div>`;
}

function renderInstallProgress() {
  const el = $('setup-progress');
  if (!el || !install) return;
  const bar = (label, p) => {
    if (!p || !p.total) return '';
    const pct = Math.min(100, Math.round((p.done / p.total) * 100));
    return `<div class="lbl"><span>${esc(label)}</span><span class="readout">${Math.round(p.done / 1048576)} / ${Math.round(p.total / 1048576)} MB · ${pct}%</span></div>` +
      `<div class="pbar"><i style="width:${pct}%"></i></div>`;
  };
  el.innerHTML = bar(t('progEngine'), install.engine) + bar(t('progModel'), install.model) +
    (install.error ? `<p class="note warn">${esc(t('failed', { error: install.error }))}</p>` : '');
}

async function runInstall() {
  install = { engine: null, model: null, error: null };
  renderSetup();
  try {
    engine = await invoke('install_engine', { pack: setupChoice.pack, model: setupChoice.model });
    settings.pack = engine.pack;
    settings.model = engine.model;
    install = null;
    transcriptionDone();
  } catch (e) {
    install.error = String(e);
    renderSetup();
  }
}

async function useCloud(button) {
  const id = setupChoice.provider;
  const model = $('transcribe-model')?.value.trim() || '';
  button.disabled = true;
  button.textContent = t('checking');
  try {
    engine = await invoke('use_cloud', { provider: id, model: model || null });
    transcriptionDone();
  } catch (e) {
    toast(providerError(e, providerById(id)));
    renderSetup();
  }
}

/** Transcription works: go on to the optional summary, or back to Settings. */
function transcriptionDone() {
  if (setupFlow === 'first') {
    setupStep = 2;
    sumChoice = null;
    renderSetup();
  } else {
    toast(t('setupDone'));
    view = 'settings';
    render();
  }
}

// ---------- step 2: optional AI summary ----------

function initSumChoice() {
  const p = providerById(engine.summaryProvider);
  sumChoice = {
    kind: !p ? 'off' : p.local ? 'local' : 'cloud',
    provider: p ? p.id : '',
    model: p ? engine.summaryModel : '',
    auto: engine.autoSummary,
  };
}

/** The provider list for a summary kind. */
const summaryProviders = (kind) => engine.providers.filter((p) => p.canSummarize && (kind === 'local' ? p.local : !p.local));

function selectSumProvider(id) {
  const p = providerById(id);
  sumChoice.provider = id;
  sumChoice.model = p?.summaryModel || '';
  // Look for the models right away (for local apps, the installed ones),
  // after the page that asked is drawn.
  if (p?.ready && (p.local || !modelLists[id])) setTimeout(() => findModels(id, { pickFirst: true }), 0);
}

function summaryPicker() {
  if (!sumChoice) initSumChoice();
  let body = '';
  if (sumChoice.kind !== 'off') {
    const list = summaryProviders(sumChoice.kind);
    if (!list.some((p) => p.id === sumChoice.provider)) selectSumProvider(list[0].id);
    const p = providerById(sumChoice.provider);
    const opt = (x) => `<option value="${esc(x.id)}"${x.id === p.id ? ' selected' : ''}>${esc(x.name)}${FREE_PLAN.includes(x.id) ? ` · ${esc(t('freeTag'))}` : ''}</option>`;
    body = `<div class="sum-detail"><div class="row"><span class="row-label">${esc(t('sumProvider'))}</span>` +
      `<select id="sum-provider" class="field" aria-label="${esc(t('sumProvider'))}">${list.map(opt).join('')}</select>${providerTags(p)}</div>` +
      guideFor(p) + keyHtml(p) +
      modelHtml(p, sumChoice.model, 'sum-model', t('sumModel')) +
      `<label class="check"><input type="checkbox" id="sum-auto"${sumChoice.auto ? ' checked' : ''}> ${esc(t('sumAuto'))}</label></div>`;
  }
  return choiceHtml('sumkind', 'off', t('sumOffName'), t('sumOffDesc'), { tag: tagOf(t('freeOnly'), 'free') }) +
    choiceHtml('sumkind', 'local', t('sumLocalName'), t('sumLocalDesc')) +
    choiceHtml('sumkind', 'cloud', t('sumCloudName'), t('sumCloudDesc')) + body;
}

/** Saves the summary choice; a key typed but not checked is checked first. */
async function saveSummary() {
  if (sumChoice.kind === 'off') {
    engine = await invoke('set_summary', { provider: '', model: '', auto: sumChoice.auto });
    return true;
  }
  let p = providerById(sumChoice.provider);
  if (p.needsKey && !p.hasKey) {
    if (!(await checkKey(p.id))) return false;
    p = providerById(p.id);
  }
  const model = (sumChoice.model || '').trim(); // kept in step by handleProviderInput
  if (!model) { toast(t('noModel')); $('sum-model')?.focus(); return false; }
  try {
    engine = await invoke('set_summary', { provider: p.id, model, auto: sumChoice.auto });
    return true;
  } catch (e) {
    toast(providerError(e, p));
    return false;
  }
}

// ---------- step 3: done ----------

function doneStep() {
  const sum = providerById(engine.summaryProvider);
  const items = [
    engine.mode === 'cloud' ? t('doneCloud', { name: providerName(engine.provider) }) : t('doneLocal'),
    sum ? t('doneSum', { name: sum.name }) : t('doneSumOff'),
  ];
  const names = recipients();
  return `<div class="card done"><ul>${items.map((i) => `<li><span class="ok-text">✓</span> ${esc(i)}</li>`).join('')}</ul>` +
    `<p class="note">${esc(names.length ? t('privacySent', { names: names.join(', ') }) : t('privacyAllLocal'))}</p></div>` +
    `<div class="setup-actions"><button class="btn primary" data-act="finish-setup">${esc(t('startFirst'))}</button></div>`;
}

// ---------- the setup page ----------

function renderSetup() {
  if (!setupChoice) initSetupChoice();
  const first = setupFlow === 'first';
  const steps = first ? `<p class="steps readout">${esc(t('stepOf', { n: setupStep, total: 3 }))}</p>` : '';
  let title, text, body;
  if (setupStep === 2) {
    [title, text] = [t('step2Title'), t('step2Text')];
    body = `<div class="card">${summaryPicker()}</div>` +
      `<div class="setup-actions"><button class="btn ghost" data-act="setup-back">${esc(t('back'))}</button>` +
      `<button class="btn primary" data-act="summary-next">${esc(t('next'))}</button></div>`;
  } else if (setupStep === 3) {
    [title, text, body] = [t('step3Title'), t('step3Text'), doneStep()];
  } else {
    [title, text] = first ? [t('step1Title'), t('step1Text')] : [t('setupTitle'), t('setupText')];
    body = transcriptionStep();
  }
  viewEl.innerHTML = `<div class="page setup">${steps}<h1>${esc(title)}</h1><p>${esc(text)}</p>${body}</div>`;
  renderInstallProgress();
}

/** Opens the guided setup: everything on first run, only transcription later. */
function openSetup(flow) {
  setupFlow = flow;
  setupStep = 1;
  setupChoice = null;
  sumChoice = null;
  view = 'setup';
  render();
}

// ---------- Settings ----------

function providersCard() {
  const rows = engine.providers.map((p) => {
    const caps = (p.canTranscribe ? tagOf(t('canTranscribe')) : '') + (p.canSummarize ? tagOf(t('canSummarize')) : '');
    // Apps on this PC need no key; "Test" tells whether they are open.
    const state = !p.needsKey ? '' : p.hasKey ? `<span class="ok-text">✓ ${esc(t('connected'))}</span>` : `<span class="muted">${esc(t('noKey'))}</span>`;
    const actions = (p.needsKey && !p.hasKey ? `<button class="btn small" data-act="open-key" data-provider="${esc(p.id)}">${esc(t('addKey'))}</button>` : '') +
      (p.ready ? `<button class="btn ghost small" data-act="test-provider" data-provider="${esc(p.id)}">${esc(t('test'))}</button>` : '') +
      (p.hasKey ? `<button class="btn ghost small danger-text" data-act="clear-provider-key" data-provider="${esc(p.id)}">${esc(t('remove'))}</button>` : '') +
      (p.custom ? `<button class="btn ghost small danger-text" data-act="remove-custom" data-provider="${esc(p.id)}" aria-label="${esc(t('delete'))}">${svg(ICONS.trash)}</button>` : '');
    const open = keyOpen === p.id ? `<div class="prov-key">${guideHtml(p.custom ? 'custom' : p.id)}${keyHtml(p)}</div>` : '';
    return `<div class="prov"><div class="prov-row"><b>${esc(p.name)}</b>${providerTags(p)}<span class="caps">${caps}</span>` +
      `<span class="prov-state">${state}</span><span class="prov-actions">${actions}</span></div>${open}</div>`;
  }).join('');
  const custom = `<details class="custom"><summary>${esc(t('addCustom'))}</summary>${guideHtml('custom')}` +
    `<div class="row"><input id="custom-name" class="field" placeholder="${esc(t('customName'))}" aria-label="${esc(t('customName'))}"></div>` +
    `<div class="row"><input id="custom-url" class="field" placeholder="${esc(t('customUrl'))}" aria-label="${esc(t('customUrl'))}"></div>` +
    `<div class="row"><input id="custom-key" class="field" type="password" placeholder="${esc(t('customKey'))}" autocomplete="off" aria-label="${esc(t('customKey'))}"></div>` +
    `<label class="check"><input type="checkbox" id="custom-stt"> ${esc(t('customTranscribes'))}</label>` +
    `<div class="row"><button class="btn primary" data-act="add-custom">${esc(t('add'))}</button></div></details>`;
  return `<div class="card">${rows}${custom}</div><p class="note">${esc(t('providersNote'))}</p>`;
}

const SETTINGS_SECTIONS = ['general', 'transcription', 'summary', 'providers', 'assistants', 'privacy'];

function settingsBody(section) {
  const opt = (value, label, current) => `<option value="${value}"${value === current ? ' selected' : ''}>${esc(label)}</option>`;
  const cloud = engine?.mode === 'cloud';
  if (section === 'general') {
    return `<div class="card"><div class="row"><span class="row-label">${esc(t('appLanguage'))}</span>` +
      `<select id="app-lang" class="field">${opt('en', 'English', language)}${opt('pt', 'Português', language)}${opt('es', 'Español', language)}</select></div>` +
      `<div class="row"><span class="row-label">${esc(t('meetingLanguage'))}</span><select id="meet-lang" class="field">` +
      opt('auto', t('auto'), settings.transcriptionLanguage) + opt('pt', 'Português', settings.transcriptionLanguage) +
      opt('en', 'English', settings.transcriptionLanguage) + opt('es', 'Español', settings.transcriptionLanguage) +
      `</select></div><p class="note">${esc(t('meetingLanguageNote'))}</p></div>`;
  }
  if (section === 'transcription') {
    const current = cloud
      ? [t('cloudName'), providerName(engine.provider), engine.transcriptionModel]
      : engine?.mode === 'local'
        ? [t('localName'), { cuda: t('gpuName'), cpu: t('cpuName') }[engine.pack], { best: t('bestName'), light: t('lightName') }[engine.model]]
        : ['—'];
    return `<div class="card"><div class="row"><span class="row-label">${esc(t('currentSetup'))}</span><b>${esc(current.filter(Boolean).join(' · '))}</b>` +
      `<button class="btn small" data-act="setup">${esc(t('change'))}</button></div></div>`;
  }
  if (section === 'summary') {
    const sum = providerById(engine.summaryProvider);
    const state = !sum ? t('summaryOff') : engine.summaryReady ? t('summaryOn', { name: sum.name, model: engine.summaryModel }) : t('summaryNotReady', { name: sum.name });
    return `<p class="status-line">${esc(state)}</p><div class="card">${summaryPicker()}</div>` +
      `<div class="setup-actions"><button class="btn primary" data-act="save-summary">${esc(t('save'))}</button></div>`;
  }
  if (section === 'providers') return providersCard();
  if (section === 'assistants') return assistantsCard();
  const names = recipients();
  return `<div class="card"><p>${esc(cloud ? t('privacyCloud', { name: providerName(engine.provider) }) : t('privacy'))}</p>` +
    `<p class="note">${esc(names.length ? t('privacySent', { names: names.join(', ') }) : t('privacyAllLocal'))}</p>` +
    `<p class="note">${esc(t('keysNote'))}</p></div>`;
}

function renderSettings() {
  if (!sumChoice) initSumChoice();
  const section = SETTINGS_SECTIONS.includes(settingsSection) ? settingsSection : 'general';
  const nav = SETTINGS_SECTIONS.map((id) =>
    `<button class="nav-item${id === section ? ' on' : ''}" data-section="${id}"${id === section ? ' aria-current="page"' : ''}>${esc(t(`nav_${id}`))}</button>`).join('');
  viewEl.innerHTML = `<div class="page settings"><h1>${esc(t('settings'))}</h1>` +
    `<div class="settings-grid"><nav class="settings-nav" aria-label="${esc(t('settings'))}">${nav}</nav>` +
    `<section class="settings-body"><h2>${esc(t(`nav_${section}`))}</h2><p class="lead">${esc(t(`desc_${section}`))}</p>${settingsBody(section)}</section></div></div>`;
  $('app-lang')?.addEventListener('change', (e) => {
    setLanguage(e.target.value);
    settings.language = language;
    invoke('set_language', { language });
    render();
  });
  $('meet-lang')?.addEventListener('change', (e) => {
    settings.transcriptionLanguage = e.target.value;
    invoke('set_transcription_language', { language: e.target.value });
  });
}

/** Opens Settings at a section. */
function openSettings(section = 'general') {
  settingsSection = section;
  sumChoice = null;
  view = 'settings';
  render();
}

async function addCustom(button) {
  const name = $('custom-name').value.trim();
  const baseUrl = $('custom-url').value.trim();
  if (!name) { $('custom-name').focus(); return; }
  if (!baseUrl) { $('custom-url').focus(); return; }
  button.disabled = true;
  button.textContent = t('checking');
  try {
    engine = await invoke('add_custom_provider', { name, baseUrl, key: $('custom-key').value.trim() || null, canTranscribe: $('custom-stt').checked });
    toast(t('customAdded', { name }));
  } catch (e) {
    toast(providerError(e));
  }
  renderSettings();
}

// ---------- AI assistants (MCP) ----------

/** Configuration for each MCP client, with this exe's path. */
function assistantSnippet(client, exe) {
  const json = () => JSON.stringify({ mcpServers: { xnote: { command: exe, args: ['--mcp'] } } }, null, 2);
  return {
    'claude-desktop': [t('whereClaudeDesktop'), json()],
    'claude-code': [t('whereClaudeCode'), `claude mcp add --scope user xnote -- "${exe}" --mcp`],
    codex: [t('whereCodex'), `[mcp_servers.xnote]\ncommand = '${exe}'\nargs = ["--mcp"]`],
    cursor: [t('whereCursor'), json()],
    other: [t('whereOther'), `${exe} --mcp`],
  }[client];
}

function assistantsCard() {
  if (!assistant) {
    invoke('assistant_info').then((info) => { assistant = info; if (view === 'settings') renderSettings(); });
    return '';
  }
  const clients = [['claude-desktop', 'Claude Desktop'], ['claude-code', 'Claude Code'], ['codex', 'Codex'], ['cursor', 'Cursor'], ['other', t('otherClient')]];
  const opt = ([id, name]) => `<option value="${id}"${id === assistantClient ? ' selected' : ''}>${esc(name)}</option>`;
  const [where, snippet] = assistantSnippet(assistantClient, assistant.exe);
  const step = (n, title, body) => `<li class="step"><span class="step-n">${n}</span><div><b>${esc(title)}</b>${body}</div></li>`;
  return `<ol class="steps-list card">` +
    step(1, t('asStep1'), `<div class="row"><select id="assistant-client" class="field" aria-label="${esc(t('assistantsPick'))}">${clients.map(opt).join('')}</select></div>`) +
    step(2, t('asStep2'), `<p class="note">${esc(where)}</p><div class="snippet"><pre>${esc(snippet)}</pre>` +
      `<button class="btn small" data-act="copy-code" data-code="${esc(snippet)}">${svg(ICONS.copy)}${esc(t('copy'))}</button></div>`) +
    step(3, t('asStep3'), `<p class="note">${esc(t('assistantsAsk'))}</p>`) +
    `</ol><div class="card"><h3>${esc(t('permissionCard'))}</h3>` +
    `<label class="check"><input type="checkbox" id="assistants-write"${assistant.canWrite ? ' checked' : ''}> ${esc(t('assistantsWrite'))}</label>` +
    `<p class="note">${esc(t('assistantsPrivacy'))}</p></div>` +
    `<p class="note">${esc(t('chatgptNote'))}</p>`;
}

// ---------- events ----------

/** Clicks on provider pieces; true when handled. */
function handleProviderAct(act, el) {
  const id = el.dataset.provider;
  if (act === 'install') runInstall();
  else if (act === 'use-cloud') useCloud(el);
  else if (act === 'key-page') invoke('open_key_page', { provider: id });
  else if (act === 'check-key') checkKey(id, el);
  else if (act === 'find-models') findModels(id, { pickFirst: true });
  else if (act === 'copy-code') navigator.clipboard.writeText(el.dataset.code).then(() => toast(t('copied')));
  else if (act === 'clear-provider-key') {
    invoke('clear_provider_key', { provider: id }).then((info) => { engine = info; delete modelLists[id]; rerender(); });
  } else if (act === 'open-key') { keyOpen = keyOpen === id ? null : id; renderSettings(); }
  else if (act === 'test-provider') {
    invoke('list_models', { provider: id }).then(() => toast(`${providerName(id)}: ${t('works')}`), (e) => toast(providerError(e, providerById(id))));
  } else if (act === 'remove-custom') {
    invoke('remove_custom_provider', { id }).then((info) => { engine = info; sumChoice = null; renderSettings(); }, (e) => toast(explain(e)));
  } else if (act === 'add-custom') addCustom(el);
  else if (act === 'setup') openSetup('change');
  else if (act === 'setup-back') { setupStep = 1; renderSetup(); }
  else if (act === 'summary-next') saveSummary().then((ok) => { if (ok) { setupStep = 3; renderSetup(); } });
  else if (act === 'save-summary') saveSummary().then((ok) => { if (ok) { toast(t('save') + ' ✓'); sumChoice = null; renderSettings(); } });
  else if (act === 'finish-setup') {
    setupFlow = 'change';
    setupStep = 1;
    if (meeting) { view = 'meeting'; render(); } else newMeeting();
  } else if (act === 'setup-summary') openSettings('summary');
  else return false;
  return true;
}

/** Changes in provider pieces; true when handled. */
function handleProviderChange(e) {
  const el = e.target;
  if (['mode', 'pack', 'model'].includes(el.name)) { setupChoice[el.name] = el.value; renderSetup(); }
  else if (el.id === 'provider') { setupChoice.provider = el.value; setupChoice.transcribeModel = ''; renderSetup(); }
  else if (el.name === 'sumkind') { sumChoice.kind = el.value; sumChoice.provider = ''; rerender(); }
  else if (el.id === 'sum-provider') { selectSumProvider(el.value); rerender(); }
  else if (el.id === 'sum-auto') sumChoice.auto = el.checked;
  else if (el.id === 'assistant-client') { assistantClient = el.value; renderSettings(); }
  else if (el.id === 'assistants-write') {
    assistant.canWrite = el.checked;
    invoke('set_assistants_can_write', { allowed: el.checked });
  }
  else return false;
  return true;
}

/** Typing in model fields keeps the choice across redraws. */
function handleProviderInput(e) {
  if (e.target.id === 'sum-model' && sumChoice) sumChoice.model = e.target.value;
  else if (e.target.id === 'transcribe-model' && setupChoice) setupChoice.transcribeModel = e.target.value;
}
