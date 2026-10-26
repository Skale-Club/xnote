'use strict';

// Xnote window: meeting list, the meeting page (record, your notes, live
// transcript, enhanced notes), first-run engine setup and settings.
// The meeting page is split into parts that update on their own, so live
// events (levels, transcript lines, streamed notes) never rebuild the notes
// box you are typing in.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const svg = (body) => `<svg class="i" viewBox="0 0 24 24">${body}</svg>`;
const ICONS = {
  plus: '<path d="M12 5v14M5 12h14"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  mic: '<rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 10a7 7 0 0 0 14 0M12 17v5M8 22h8"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor"/>',
  sparkle: '<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/><path d="M19 15l.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7z"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/>',
  download: '<path d="M12 3v12M7 10l5 5 5-5M5 21h14"/>',
  trash: '<path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="m19 6-1 14H6L5 6"/>',
};
const SIZES_MB = { cuda: 653, cpu: 9, best: 547, light: 181 };

const $ = (id) => document.getElementById(id);
const viewEl = $('view');

let settings = { language: 'en', transcriptionLanguage: 'auto', pack: '', model: '' };
let engine = null;          // { status: { gpu, packs, models }, pack, model }
let meetings = [];
let meeting = null;         // the open meeting
let view = 'welcome';       // welcome | meeting | setup | settings
let recording = null;       // { id, phase: starting | recording | finishing, since, base }
let engineLoading = false;
let notesTab = 'mine';
let enhancing = null;       // { id, text } while notes stream in
let hasKey = false;
let search = '';
let install = null;         // { engine: {done,total}, model: {done,total}, error }
let setupChoice = null;     // { pack, model }
let pendingUpdate = null;
let consentDismissed = false;
let saveTimer = null;

// ---------- helpers ----------

const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

function toast(text) {
  const el = $('toast');
  el.textContent = text;
  el.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove('show'), 3200);
}

function confirmBox(title, text, okLabel) {
  return new Promise((resolve) => {
    $('modal-title').textContent = title;
    $('modal-text').textContent = text;
    $('modal-ok').textContent = okLabel;
    $('modal-ok').className = 'btn rec';
    $('modal-cancel').textContent = t('cancel');
    const done = (answer) => { $('modal').hidden = true; document.removeEventListener('keydown', onKey); resolve(answer); };
    const onKey = (e) => { if (e.key === 'Escape') done(false); };
    $('modal-ok').onclick = () => done(true);
    $('modal-cancel').onclick = () => done(false);
    document.addEventListener('keydown', onKey);
    $('modal').hidden = false;
  });
}

function clock(ms) {
  const s = Math.floor(ms / 1000);
  const two = (n) => String(n).padStart(2, '0');
  return s >= 3600 ? `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}` : `${two(Math.floor(s / 60))}:${two(s % 60)}`;
}

function duration(ms) {
  if (ms < 60000) return `< 1 ${t('min')}`;
  const m = Math.round(ms / 60000);
  return m >= 60 ? `${Math.floor(m / 60)} ${t('h')} ${String(m % 60).padStart(2, '0')} ${t('min')}` : `${m} ${t('min')}`;
}

function dayLabel(ts) {
  const d = new Date(ts), now = new Date();
  const start = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = (start(now) - start(d)) / 86400000;
  if (diff === 0) return t('today');
  if (diff === 1) return t('yesterday');
  return d.toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' });
}

const timeOf = (ts) => new Date(ts).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });
const isRecordingHere = () => recording && meeting && recording.id === meeting.id;

function engineReady() {
  if (!engine) return false;
  const { status, mode, pack, model, provider, cloudKeys } = engine;
  if (mode === 'cloud') return cloudKeys.includes(provider);
  return mode === 'local' && status.packs.includes(pack) && status.models.includes(model);
}

const PROVIDER_NAMES = { groq: 'Groq', openai: 'OpenAI' };

async function refreshList() {
  meetings = await invoke('list_meetings');
  renderSidebar();
}

function explain(error) {
  const e = String(error);
  if (e === 'refusal') return t('refusal');
  if (e === 'nothing to enhance') return t('nothingToEnhance');
  if (e === 'no API key') return t('needKey');
  return t('failed', { error: e });
}

// ---------- sidebar ----------

function renderSidebar() {
  for (const el of document.querySelectorAll('[data-t]')) el.textContent = t(el.dataset.t);
  $('new-meeting').innerHTML = `${svg(ICONS.plus)}<span>${esc(t('newMeeting'))}</span>`;
  $('search').placeholder = t('search');
  $('open-settings').innerHTML = `${svg(ICONS.gear)}<span>${esc(t('settings'))}</span>`;
  const q = search.trim().toLowerCase();
  const list = meetings.filter((m) => !q || (m.title || t('untitled')).toLowerCase().includes(q));
  if (!list.length) {
    $('list').innerHTML = `<div class="empty-list">${esc(t('noMeetings'))}</div>`;
    return;
  }
  let html = '', lastDay = '';
  for (const m of list) {
    const day = dayLabel(m.created);
    if (day !== lastDay) { html += `<div class="day">${esc(day)}</div>`; lastDay = day; }
    const dot = recording && recording.id === m.id ? '<span class="rec-dot"></span>' : '';
    const meta = [timeOf(m.created), m.durationMs ? duration(m.durationMs) : ''].filter(Boolean).join(' · ');
    html += `<button class="item${meeting && meeting.id === m.id && view === 'meeting' ? ' on' : ''}" data-id="${m.id}">` +
      `<span class="t${m.title ? '' : ' untitled'}">${dot}${esc(m.title || t('untitled'))}</span><span class="m">${esc(meta)}</span></button>`;
  }
  $('list').innerHTML = html;
}

// ---------- meeting page ----------

function renderHead() {
  const el = $('head');
  if (!el || !meeting) return;
  const here = isRecordingHere();
  const busyElsewhere = recording && !here;
  let rec;
  if (here && recording.phase === 'recording') {
    rec = `<div class="meters"><div class="meter me">${esc(t('me'))}<span class="bar"><i id="lv-me"></i></span></div>` +
      `<div class="meter them">${esc(t('them'))}<span class="bar"><i id="lv-them"></i></span></div></div>` +
      `<span class="timer" id="timer"></span><button class="btn rec" data-act="stop">${svg(ICONS.stop)}${esc(t('stop'))}</button>`;
  } else if (here) {
    const label = engineLoading ? t('loadingEngine') : recording.phase === 'finishing' ? t('finishing') : '…';
    rec = `<button class="btn" disabled>${esc(label)}</button>`;
  } else {
    const label = meeting.transcript.length ? t('resume') : t('record');
    rec = `<button class="btn ${meeting.transcript.length ? 'rec-outline' : 'rec'}" data-act="record"${busyElsewhere ? ' disabled' : ''}>${svg(ICONS.mic)}${esc(label)}</button>`;
  }
  const canEnhance = !here && !enhancing && (meeting.transcript.length || meeting.notes.trim());
  const meta = [dayLabel(meeting.created), timeOf(meeting.created), meeting.durationMs ? duration(meeting.durationMs) : ''].filter(Boolean).join(' · ');
  el.innerHTML =
    `<div class="titles"><input id="title" class="title-input" value="${esc(meeting.title)}" placeholder="${esc(t('untitled'))}">` +
    `<div class="meta">${esc(meta)}</div></div><div class="actions">${rec}` +
    `<button class="btn primary" data-act="enhance"${canEnhance ? '' : ' disabled'}>${svg(ICONS.sparkle)}${esc(enhancing ? t('enhancing') : t('enhance'))}</button>` +
    `<button class="btn ghost" data-act="copy" title="${esc(t('copyMd'))}">${svg(ICONS.copy)}</button>` +
    `<button class="btn ghost" data-act="export" title="${esc(t('export'))}">${svg(ICONS.download)}</button>` +
    `<button class="btn ghost" data-act="delete" title="${esc(t('delete'))}"${here ? ' disabled' : ''}>${svg(ICONS.trash)}</button></div>`;
  $('title').addEventListener('change', (e) => saveTitle(e.target.value));
  tick();
}

function renderBanner() {
  const el = $('banner-slot');
  if (!el) return;
  const show = isRecordingHere() && recording.phase === 'recording' && !consentDismissed;
  el.innerHTML = show
    ? `<div class="banner"><span>${esc(t('consent'))}</span><span class="row">` +
      `<button class="btn small" data-act="notice">${esc(t('copyNotice'))}</button>` +
      `<button class="btn ghost small" data-act="dismiss">✕</button></span></div>`
    : '';
}

function enhancedHtml() {
  if (enhancing && enhancing.id === meeting.id) return renderMarkdown(enhancing.text) + '<span class="cursor"></span>';
  return renderMarkdown(meeting.enhanced);
}

function renderNotesPane() {
  const el = $('notes-pane');
  if (!el) return;
  const hasEnhanced = meeting.enhanced || (enhancing && enhancing.id === meeting.id);
  if (!hasEnhanced) notesTab = 'mine';
  const tabs = `<div class="tabs"><button class="tab${notesTab === 'mine' ? ' on' : ''}" data-tab="mine">${esc(t('myNotes'))}</button>` +
    (hasEnhanced ? `<button class="tab${notesTab === 'enhanced' ? ' on' : ''}" data-tab="enhanced">${svg(ICONS.sparkle)} ${esc(t('enhanced'))}</button>` : '') +
    '</div>';
  const regen = notesTab === 'enhanced' && meeting.enhanced && !enhancing
    ? `<button class="btn ghost small" data-act="enhance">${esc(t('regenerate'))}</button>` : '';
  const body = notesTab === 'enhanced'
    ? `<div class="enhanced" id="enhanced">${enhancedHtml()}</div>`
    : `<textarea id="notes" class="notes" placeholder="${esc(t('notesPlaceholder'))}">${esc(meeting.notes)}</textarea>`;
  el.innerHTML = `<div class="pane-head">${tabs}${regen}</div>${body}`;
  const notes = $('notes');
  if (notes) notes.addEventListener('input', () => scheduleNotesSave(notes.value));
}

function lineHtml(l) {
  return `<div class="line"><span class="time">${clock(l.ms)}</span><span class="who ${l.who}">${esc(t(l.who))}</span><span>${esc(l.text)}</span></div>`;
}

function renderTranscript() {
  const el = $('transcript');
  if (!el) return;
  if (!meeting.transcript.length) {
    el.innerHTML = `<div class="placeholder">${esc(isRecordingHere() && recording.phase === 'finishing' ? t('finishing') : t('transcriptEmpty'))}</div>`;
    return;
  }
  el.innerHTML = meeting.transcript.map(lineHtml).join('');
  el.scrollTop = el.scrollHeight;
}

function renderMeeting() {
  viewEl.innerHTML = '<div class="head" id="head"></div><div id="banner-slot"></div>' +
    '<div class="panes"><section class="pane" id="notes-pane"></section>' +
    `<section class="pane"><div class="pane-head"><h3>${esc(t('transcript'))}</h3></div><div class="transcript" id="transcript"></div></section></div>`;
  renderHead();
  renderBanner();
  renderNotesPane();
  renderTranscript();
}

/** Timer and meters while recording. */
function tick() {
  if (!isRecordingHere() || recording.phase !== 'recording') return;
  const timer = $('timer');
  if (timer) timer.textContent = clock(recording.base + Date.now() - recording.since);
}

function scheduleNotesSave(value) {
  meeting.notes = value;
  clearTimeout(saveTimer);
  const id = meeting.id;
  saveTimer = setTimeout(() => invoke('update_meeting', { id, notes: value }).catch((e) => toast(explain(e))), 600);
}

async function saveTitle(value) {
  meeting.title = value.trim();
  await invoke('update_meeting', { id: meeting.id, title: meeting.title });
  refreshList();
}

// ---------- actions ----------

async function newMeeting() {
  const m = await invoke('create_meeting');
  await refreshList();
  await open(m.id);
  if (!engineReady()) { view = 'setup'; render(); }
}

async function open(id) {
  meeting = await invoke('get_meeting', { id });
  view = 'meeting';
  notesTab = meeting.enhanced ? 'enhanced' : 'mine';
  consentDismissed = false;
  render();
}

async function startRecording() {
  if (!engineReady()) { view = 'setup'; render(); return; }
  recording = { id: meeting.id, phase: 'starting', since: Date.now(), base: meeting.durationMs };
  renderHead();
  renderSidebar();
  try {
    const warnings = await invoke('start_recording', { id: meeting.id });
    if (warnings.length) toast(t('audioWarning', { w: warnings.join('; ') }));
  } catch (e) {
    recording = null;
    if (String(e) === 'engine') { view = 'setup'; render(); return; }
    toast(explain(e));
    renderHead();
    renderSidebar();
  }
}

async function stopRecording() {
  try {
    const m = await invoke('stop_recording');
    if (meeting && meeting.id === m.id) {
      meeting.durationMs = m.durationMs;
      meeting.transcript = m.transcript;
    }
  } catch (e) {
    toast(explain(e));
  }
  await refreshList();
  if (view === 'meeting') renderMeeting();
}

async function enhance() {
  if (!hasKey) { toast(t('needKey')); view = 'settings'; render(); return; }
  const id = meeting.id;
  enhancing = { id, text: '' };
  notesTab = 'enhanced';
  renderHead();
  renderNotesPane();
  try {
    const m = await invoke('enhance', { id });
    if (meeting && meeting.id === id) { meeting.enhanced = m.enhanced; meeting.title = m.title; }
  } catch (e) {
    toast(explain(e));
  }
  enhancing = null;
  if (meeting && meeting.id === id && view === 'meeting') renderMeeting();
  refreshList();
}

function markdownOf(m) {
  const title = m.title || t('untitled');
  const out = [`# ${title}`, `${dayLabel(m.created)} · ${timeOf(m.created)}${m.durationMs ? ` · ${duration(m.durationMs)}` : ''}`, ''];
  if (m.enhanced) out.push(m.enhanced.replace(/^# .*\n?/, '').trim(), '');
  if (m.notes.trim()) out.push(`## ${t('myNotes')}`, '', m.notes.trim(), '');
  if (m.transcript.length) {
    out.push(`## ${t('transcript')}`, '');
    for (const l of m.transcript) out.push(`- **[${clock(l.ms)}] ${t(l.who)}:** ${l.text}`);
  }
  out.push('', '---', '_Xnote · Skale Club_');
  return out.join('\n').trim() + '\n';
}

async function removeMeeting() {
  if (!(await confirmBox(t('confirmDelete'), t('confirmDeleteText'), t('delete')))) return;
  try {
    await invoke('delete_meeting', { id: meeting.id });
    meeting = null;
    view = 'welcome';
    await refreshList();
    render();
  } catch (e) {
    toast(explain(e));
  }
}

// ---------- setup ----------

function renderSetup() {
  const status = engine.status;
  if (!setupChoice) {
    setupChoice = {
      // Without an NVIDIA card local transcription is slow, so the cloud is
      // suggested first there.
      mode: engine.mode || (status.gpu ? 'local' : 'cloud'),
      pack: engine.pack || (status.gpu ? 'cuda' : 'cpu'),
      model: engine.model || 'best',
      provider: engine.provider || 'groq',
    };
  }
  const tagOf = (text) => `<span class="tag">${esc(text)}</span>`;
  const choice = (group, id, name, desc, opts = {}) => {
    const installed = group === 'pack' ? status.packs.includes(id) : group === 'model' ? status.models.includes(id) : false;
    const tag = installed ? tagOf(t('installed')) : opts.recommended ? tagOf(t('recommended')) : '';
    const size = group === 'mode' || installed ? '' : `${SIZES_MB[id]} MB`;
    return `<label class="choice"><input type="radio" name="${group}" value="${id}"${setupChoice[group] === id ? ' checked' : ''}${opts.disabled ? ' disabled' : ''}>` +
      `<div><b>${esc(name)}${tag}</b><span>${esc(opts.disabled ? opts.disabledText : desc)}</span></div>` +
      `<span class="size">${size}</span></label>`;
  };
  const modeCard = `<div class="card"><h3>${esc(t('modeCard'))}</h3>` +
    choice('mode', 'local', t('localName'), t('localDesc'), { recommended: status.gpu }) +
    choice('mode', 'cloud', t('cloudName'), t('cloudDesc'), { recommended: !status.gpu }) + '</div>';
  const body = setupChoice.mode === 'cloud' ? cloudSetup() : localSetup(choice);
  viewEl.innerHTML = `<div class="page"><h1>${esc(t('setupTitle'))}</h1><p>${esc(t('setupText'))}</p>${modeCard}${body}</div>`;
  renderInstallProgress();
}

function localSetup(choice) {
  const status = engine.status;
  const need = (status.packs.includes(setupChoice.pack) ? 0 : SIZES_MB[setupChoice.pack]) +
    (status.models.includes(setupChoice.model) ? 0 : SIZES_MB[setupChoice.model]);
  const label = need ? t('download', { size: `${need} MB` }) : t('useThis');
  return `<div class="card"><h3>${esc(t('engineCard'))}</h3>` +
    choice('pack', 'cuda', t('gpuName'), t('gpuDesc'), { recommended: status.gpu, disabled: !status.gpu, disabledText: t('noGpu') }) +
    choice('pack', 'cpu', t('cpuName'), t('cpuDesc'), { recommended: !status.gpu }) + '</div>' +
    `<div class="card"><h3>${esc(t('modelCard'))}</h3>` +
    choice('model', 'best', t('bestName'), t('bestDesc'), { recommended: true }) +
    choice('model', 'light', t('lightName'), t('lightDesc')) + '</div>' +
    `<button class="btn primary" data-act="install"${install && !install.error ? ' disabled' : ''}>${esc(install && !install.error ? t('downloading') : label)}</button>` +
    '<div class="progress" id="setup-progress"></div>';
}

function cloudSetup() {
  const id = setupChoice.provider;
  const name = PROVIDER_NAMES[id];
  const saved = engine.cloudKeys.includes(id);
  const opt = (value) => `<option value="${value}"${value === id ? ' selected' : ''}>${esc(PROVIDER_NAMES[value])}${value === 'groq' ? ` (${esc(t('recommended').toLowerCase())})` : ''}</option>`;
  return `<div class="card"><h3>${esc(t('providerCard'))}</h3>` +
    `<div class="row"><select id="provider" class="field">${opt('groq')}${opt('openai')}</select></div>` +
    `<p class="note">${esc(t(`${id}Desc`))}</p>` +
    (saved ? `<p><span class="ok-text">✓ ${esc(t('keySaved'))}</span></p>` : '') +
    `<div class="row"><input id="cloud-key" class="field" type="password" placeholder="${esc(t('cloudKeyPlaceholder', { name }))}" autocomplete="off">` +
    `<button class="btn" data-act="key-page" data-provider="${id}">${esc(t('getKey'))}</button>` +
    (saved ? `<button class="btn danger-text" data-act="clear-cloud-key">${esc(t('remove'))}</button>` : '') + '</div>' +
    `<p class="note">${esc(t('cloudKeyNote', { name }))}</p></div>` +
    `<button class="btn primary" data-act="use-cloud">${esc(saved ? t('useProvider', { name }) : t('saveAndUse'))}</button>`;
}

async function useCloud(button) {
  const provider = setupChoice.provider;
  const key = $('cloud-key').value.trim();
  if (!key && !engine.cloudKeys.includes(provider)) { $('cloud-key').focus(); return; }
  button.disabled = true;
  button.textContent = t('checking');
  try {
    engine = await invoke(key ? 'set_cloud_key' : 'use_cloud', key ? { provider, key } : { provider });
    hasKey = engine.cloudKeys.includes('openai'); // the OpenAI key also writes the notes
    toast(t('setupDone'));
    view = meeting ? 'meeting' : 'welcome';
    render();
  } catch (e) {
    toast(t('keyInvalid', { error: e }));
    renderSetup();
  }
}

function renderInstallProgress() {
  const el = $('setup-progress');
  if (!el || !install) return;
  const bar = (label, p) => {
    if (!p || !p.total) return '';
    const pct = Math.min(100, Math.round((p.done / p.total) * 100));
    return `<div class="lbl"><span>${esc(label)}</span><span>${Math.round(p.done / 1048576)} / ${Math.round(p.total / 1048576)} MB · ${pct}%</span></div>` +
      `<div class="pbar"><i style="width:${pct}%"></i></div>`;
  };
  el.innerHTML = bar(t('progEngine'), install.engine) + bar(t('progModel'), install.model) +
    (install.error ? `<p class="note" style="color:var(--rec)">${esc(t('failed', { error: install.error }))}</p>` : '');
}

async function runInstall() {
  install = { engine: null, model: null, error: null };
  renderSetup();
  try {
    engine = await invoke('install_engine', { pack: setupChoice.pack, model: setupChoice.model });
    settings.pack = engine.pack;
    settings.model = engine.model;
    install = null;
    toast(t('setupDone'));
    view = meeting ? 'meeting' : 'welcome';
    render();
  } catch (e) {
    install.error = String(e);
    renderSetup();
  }
}

// ---------- settings ----------

function renderSettings() {
  const opt = (value, label, current) => `<option value="${value}"${value === current ? ' selected' : ''}>${esc(label)}</option>`;
  const cloud = engine?.mode === 'cloud';
  const transcription = cloud
    ? [t('cloudName'), PROVIDER_NAMES[engine.provider]]
    : engine?.mode === 'local'
      ? [t('localName'), { cuda: t('gpuName'), cpu: t('cpuName') }[engine.pack], { best: t('bestName'), light: t('lightName') }[engine.model]]
      : ['—'];
  viewEl.innerHTML =
    `<div class="page"><h1>${esc(t('settings'))}</h1><p></p>` +
    `<div class="card"><h3>${esc(t('langCard'))}</h3><div class="row" style="margin-bottom:12px"><span style="min-width:180px">${esc(t('appLanguage'))}</span>` +
    `<select id="app-lang" class="field">${opt('en', 'English', language)}${opt('pt', 'Português', language)}${opt('es', 'Español', language)}</select></div>` +
    `<div class="row"><span style="min-width:180px">${esc(t('meetingLanguage'))}</span><select id="meet-lang" class="field">` +
    opt('auto', t('auto'), settings.transcriptionLanguage) + opt('pt', 'Português', settings.transcriptionLanguage) +
    opt('en', 'English', settings.transcriptionLanguage) + opt('es', 'Español', settings.transcriptionLanguage) +
    `</select></div><p class="note">${esc(t('meetingLanguageNote'))}</p></div>` +
    `<div class="card"><h3>${esc(t('aiCard'))}</h3><p>${hasKey ? `<span class="ok-text">✓ ${esc(t('keySaved'))}</span>` : esc(t('keyMissing'))}</p>` +
    `<div class="row"><input id="api-key" class="field" type="password" placeholder="sk-…" autocomplete="off">` +
    `<button class="btn primary" data-act="save-key">${esc(t('save'))}</button>` +
    (hasKey ? `<button class="btn danger-text" data-act="clear-key">${esc(t('remove'))}</button>` : '') +
    `<button class="btn" data-act="key-page" data-provider="openai">${esc(t('getKey'))}</button>` +
    `</div><p class="note">${esc(t('keyNote'))}</p></div>` +
    `<div class="card"><h3>${esc(t('engineSettingsCard'))}</h3><div class="row"><span>${esc(transcription.filter(Boolean).join(' · '))}</span>` +
    `<button class="btn small" data-act="setup">${esc(t('change'))}</button></div></div>` +
    `<div class="card"><h3>${esc(t('privacyCard'))}</h3><p class="note" style="margin:0">` +
    `${esc(cloud ? t('privacyCloud', { name: PROVIDER_NAMES[engine.provider] }) : t('privacy'))}</p></div></div>`;
  $('app-lang').addEventListener('change', (e) => {
    setLanguage(e.target.value);
    settings.language = language;
    invoke('set_language', { language });
    render();
  });
  $('meet-lang').addEventListener('change', (e) => {
    settings.transcriptionLanguage = e.target.value;
    invoke('set_transcription_language', { language: e.target.value });
  });
}

async function saveKey(button) {
  const key = $('api-key').value.trim();
  if (!key) return;
  button.disabled = true;
  button.textContent = t('checking');
  try {
    await invoke('set_api_key', { key });
    hasKey = true;
    engine = await invoke('engine_status'); // same key as OpenAI transcription
    toast(t('keySaved'));
  } catch (e) {
    toast(t('keyInvalid', { error: e }));
  }
  renderSettings();
}

// ---------- page & updates ----------

function renderWelcome() {
  viewEl.innerHTML = `<div class="welcome"><div><img src="logo.png" alt=""><h1>${esc(t('welcomeTitle'))}</h1>` +
    `<p>${esc(t('welcomeText'))}${engine?.mode === 'local' ? ` ${esc(t('welcomeLocal'))}` : ''}</p><button class="btn primary" data-act="new">${svg(ICONS.plus)}${esc(t('newMeeting'))}</button></div></div>`;
}

function renderUpdateBar() {
  const bar = $('update-bar');
  bar.hidden = !pendingUpdate;
  if (!pendingUpdate) return;
  bar.innerHTML = `<span>${esc(t('updateAvailable', { v: pendingUpdate.version }))}</span>` +
    `<button class="btn small primary" id="update-now">${esc(t('updateNow'))}</button>`;
  $('update-now').onclick = async (e) => {
    e.target.disabled = true;
    e.target.textContent = t('updating');
    try {
      await invoke('install_update'); // the app restarts on success
    } catch (err) {
      toast(t('updateFailed', { error: err }));
      renderUpdateBar();
    }
  };
}

function render() {
  renderSidebar();
  renderUpdateBar();
  if (view === 'setup' && engine) renderSetup();
  else if (view === 'settings') renderSettings();
  else if (view === 'meeting' && meeting) renderMeeting();
  else renderWelcome();
}

// ---------- events ----------

$('new-meeting').addEventListener('click', newMeeting);
$('open-settings').addEventListener('click', () => { view = 'settings'; render(); });
$('skale').addEventListener('click', () => invoke('open_site'));
$('search').addEventListener('input', (e) => { search = e.target.value; renderSidebar(); });
$('list').addEventListener('click', (e) => {
  const item = e.target.closest('[data-id]');
  if (item) open(item.dataset.id);
});

viewEl.addEventListener('change', (e) => {
  if (['mode', 'pack', 'model'].includes(e.target.name) || e.target.id === 'provider') {
    setupChoice[e.target.name || e.target.id] = e.target.value;
    renderSetup();
  }
});

viewEl.addEventListener('click', (e) => {
  const tab = e.target.closest('[data-tab]');
  if (tab) { notesTab = tab.dataset.tab; renderNotesPane(); return; }
  const el = e.target.closest('[data-act]');
  if (!el || el.disabled) return;
  const act = el.dataset.act;
  if (act === 'new') newMeeting();
  else if (act === 'record') startRecording();
  else if (act === 'stop') stopRecording();
  else if (act === 'enhance') enhance();
  else if (act === 'copy') navigator.clipboard.writeText(markdownOf(meeting)).then(() => toast(t('copied')));
  else if (act === 'export') {
    invoke('save_export', { name: meeting.title || t('untitled'), markdown: markdownOf(meeting) })
      .then((path) => toast(t('exported', { path })), (err) => toast(explain(err)));
  } else if (act === 'delete') removeMeeting();
  else if (act === 'notice') navigator.clipboard.writeText(t('noticeText')).then(() => toast(t('copied')));
  else if (act === 'dismiss') { consentDismissed = true; renderBanner(); }
  else if (act === 'install') runInstall();
  else if (act === 'setup') { view = 'setup'; setupChoice = null; render(); }
  else if (act === 'save-key') saveKey(el);
  else if (act === 'clear-key') {
    invoke('clear_api_key').then(() => invoke('engine_status')).then((info) => { engine = info; hasKey = false; renderSettings(); });
  }
  else if (act === 'use-cloud') useCloud(el);
  else if (act === 'key-page') invoke('open_key_page', { provider: el.dataset.provider });
  else if (act === 'clear-cloud-key') {
    invoke('clear_cloud_key', { provider: setupChoice.provider }).then((info) => {
      engine = info;
      hasKey = info.cloudKeys.includes('openai');
      renderSetup();
    });
  }
});

listen('recording-state', ({ payload }) => {
  if (payload.phase === 'idle') recording = null;
  else if (recording && recording.id === payload.id) {
    if (payload.phase === 'recording' && recording.phase !== 'recording') recording.since = Date.now();
    recording.phase = payload.phase;
  } else {
    recording = { id: payload.id, phase: payload.phase, since: Date.now(), base: 0 };
  }
  if (view === 'meeting') { renderHead(); renderBanner(); }
  renderSidebar();
});

listen('engine-loading', ({ payload }) => { engineLoading = payload; renderHead(); });
listen('gpu-fallback', () => toast(t('gpuFallback')));

listen('levels', ({ payload }) => {
  const width = (rms) => `${Math.min(100, Math.sqrt(rms) * 220)}%`;
  const me = $('lv-me'), them = $('lv-them');
  if (me) me.style.width = width(payload.me);
  if (them) them.style.width = width(payload.them);
});

listen('transcript', ({ payload }) => {
  if (!meeting || meeting.id !== payload.id) return;
  const last = meeting.transcript[meeting.transcript.length - 1];
  meeting.transcript.push(payload.line);
  const el = $('transcript');
  if (!el) return;
  if (last && payload.line.ms < last.ms) {
    meeting.transcript.sort((a, b) => a.ms - b.ms);
    renderTranscript();
    return;
  }
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  if (meeting.transcript.length === 1) el.innerHTML = '';
  el.insertAdjacentHTML('beforeend', lineHtml(payload.line));
  if (atBottom) el.scrollTop = el.scrollHeight;
  const enhanceBtn = document.querySelector('[data-act="enhance"]');
  if (enhanceBtn && !isRecordingHere()) enhanceBtn.disabled = false;
});

listen('enhance-delta', ({ payload }) => {
  if (!enhancing || enhancing.id !== payload.id) return;
  enhancing.text += payload.text;
  const el = $('enhanced');
  if (el) el.innerHTML = enhancedHtml();
});

let lastErrorToast = 0;
listen('transcribe-error', ({ payload }) => {
  if (Date.now() - lastErrorToast > 10000) { lastErrorToast = Date.now(); toast(t('transcribeError', { error: payload })); }
});

listen('engine-progress', ({ payload }) => {
  if (!install) return;
  install[payload.item] = { done: payload.done, total: payload.total };
  renderInstallProgress();
});

setInterval(tick, 500);

(async () => {
  try {
    settings = await invoke('get_settings');
  } catch (_) { /* defaults */ }
  setLanguage(settings.language);
  [engine, hasKey, meetings] = await Promise.all([invoke('engine_status'), invoke('has_api_key'), invoke('list_meetings')]);
  view = engineReady() ? 'welcome' : 'setup';
  render();
  window.__TAURI__.app.getVersion().then((v) => { document.querySelector('.ver').textContent = `v${v}`; });
  invoke('check_update').then((u) => { pendingUpdate = u; renderUpdateBar(); });
})();
