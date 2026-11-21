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
let view = 'welcome';       // welcome | meeting | setup | settings (setup and settings: providers-ui.js)
let recording = null;       // { id, phase: starting | recording | finishing, since, base, lines }
let engineLoading = false;
let startupRequestId = null;
let notesTab = 'mine';
let enhancing = null;       // { id, text } while notes stream in
let search = '';
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
  const { status, mode, pack, model, provider } = engine;
  if (mode === 'cloud') return !!engine.providers.find((p) => p.id === provider && p.ready && p.canTranscribe);
  return mode === 'local' && status.packs.includes(pack) && status.models.includes(model);
}

async function refreshList() {
  meetings = await invoke('list_meetings');
  renderSidebar();
}

function explain(error) {
  const e = String(error);
  if (e === 'refusal') return t('refusal');
  if (e === 'nothing to enhance') return t('nothingToEnhance');
  if (e === 'no API key') return t('needKey');
  if (e === 'no model') return t('noModel');
  if (e === 'not running') return t('notRunning', { name: providerName(engine?.summaryProvider) });
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
    if (day !== lastDay) { html += `<div class="day silk">${esc(day)}</div>`; lastDay = day; }
    const dot = recording && recording.id === m.id ? '<span class="rec-dot"></span>' : '';
    const meta = [timeOf(m.created), m.durationMs ? duration(m.durationMs) : ''].filter(Boolean).join(' · ');
    const title = m.broken ? t('unreadable') : m.title || t('untitled');
    html += `<button class="item${meeting && meeting.id === m.id && view === 'meeting' ? ' on' : ''}" data-id="${esc(m.id)}">` +
      `<span class="t${m.title && !m.broken ? '' : ' untitled'}">${dot}${esc(title)}</span><span class="m readout">${esc(meta)}</span></button>`;
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
      `<span class="timer readout" id="timer"></span><button class="btn rec" data-act="stop">${svg(ICONS.stop)}${esc(t('stop'))}</button>`;
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
    (engine?.summaryReady
      ? `<button class="btn primary" data-act="enhance"${canEnhance ? '' : ' disabled'}>${svg(ICONS.sparkle)}${esc(enhancing ? t('enhancing') : t('enhance'))}</button>`
      : `<button class="btn ghost" data-act="setup-summary" title="${esc(t('sumCloudDesc'))}">${svg(ICONS.sparkle)}${esc(t('setUpSummary'))}</button>`) +
    `<button class="btn ghost" data-act="copy" title="${esc(t('copyMd'))}">${svg(ICONS.copy)}</button>` +
    `<button class="btn ghost" data-act="export" title="${esc(t('export'))}">${svg(ICONS.download)}</button>` +
    `<button class="btn ghost" data-act="delete" title="${esc(t('delete'))}"${here ? ' disabled' : ''}>${svg(ICONS.trash)}</button></div>`;
  $('title').addEventListener('change', (e) => saveTitle(e.target.value));
  tick();
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
  return `<div class="line"><span class="time readout">${clock(l.ms)}</span><span class="who ${l.who === 'me' ? 'me' : 'them'}">${esc(t(l.who))}</span><span>${esc(l.text)}</span></div>`;
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
  viewEl.innerHTML = '<div class="head" id="head"></div>' +
    '<div class="panes"><section class="pane" id="notes-pane"></section>' +
    `<section class="pane"><div class="pane-head"><h3 class="silk">${esc(t('transcript'))}</h3></div><div class="transcript" id="transcript"></div></section></div>`;
  renderHead();
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
  if (!engineReady()) openSetup(engine?.mode ? 'change' : 'first');
}

async function open(id) {
  try {
    meeting = await invoke('get_meeting', { id });
  } catch (e) {
    toast(t('unreadableNote', { error: String(e) }));
    return;
  }
  view = 'meeting';
  notesTab = meeting.enhanced ? 'enhanced' : 'mine';
  render();
}

async function startRecording() {
  if (recording) return;
  if (!engineReady()) { openSetup(engine?.mode ? 'change' : 'first'); return; }
  const target = meeting;
  const requestId = window.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  startupRequestId = requestId;
  recording = { id: target.id, phase: 'starting', since: Date.now(), base: target.durationMs, lines: target.transcript.length };
  window.XnotePreparation.begin({ requestId, id:target.id, title:target.title || t('untitled'), mode:engine.mode,
    onCancel: (id) => invoke('cancel_recording_start', { requestId:id }), onRetry: async () => { if (meeting?.id !== target.id) await open(target.id); if (meeting?.id === target.id) return startRecording(); } });
  renderHead(); renderSidebar();
  try {
    const warnings = await invoke('start_recording', { id: target.id, requestId });
    // The command result is also authoritative if an event was missed.
    if (startupRequestId !== requestId) return;
    if (recording && recording.phase === 'starting') { recording.phase = 'recording'; recording.since = Date.now(); }
    engineLoading = false;
    window.XnotePreparation.done(requestId);
    renderHead(); renderSidebar();
    if (warnings.length) toast(t('audioWarning', { w: warnings.join('; ') }));
  } catch (e) {
    if (startupRequestId !== requestId) return;
    recording = null; engineLoading = false;
    if (String(e) === 'startup_cancelled') window.XnotePreparation.close();
    else if (String(e) === 'engine') { window.XnotePreparation.close(); openSetup('change'); return; }
    else window.XnotePreparation.fail(requestId, String(e));
    renderHead(); renderSidebar();
  }
}

async function stopRecording() {
  const linesBefore = recording?.lines ?? 0;
  let summarize = false;
  try {
    const m = await invoke('stop_recording');
    if (meeting && meeting.id === m.id) {
      meeting.durationMs = m.durationMs;
      meeting.transcript = m.transcript;
      // The summary is optional: written as soon as the meeting ends only when
      // a summary provider is set up and something new was said.
      summarize = engine?.summaryReady && engine.autoSummary && !enhancing && m.transcript.length > linesBefore;
    }
  } catch (e) {
    toast(explain(e));
  }
  await refreshList();
  if (view === 'meeting') renderMeeting();
  if (summarize) enhance();
}

async function enhance() {
  if (!engine?.summaryReady) { toast(t('needSummary')); handleProviderAct('setup-summary', { dataset: {} }); return; }
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

// ---------- page & updates ----------

function renderWelcome() {
  viewEl.innerHTML = `<div class="welcome"><div><img src="logo.png" alt=""><h1>${esc(t('welcomeTitle'))}</h1>` +
    `<p>${esc(t('welcomeText'))}${engine?.mode === 'local' ? ` ${esc(t('welcomeLocal'))}` : ''}</p><button class="btn primary" data-act="new">${svg(ICONS.plus)}${esc(t('newMeeting'))}</button></div></div>`;
}

// Read by updates.js before restarting into a new version.
function updateBlocker() {
  return recording ? t('updateStopsRecording') : '';
}

function render() {
  renderSidebar();
  updates.render();
  if (view === 'setup' && engine) renderSetup();
  else if (view === 'settings') renderSettings();
  else if (view === 'meeting' && meeting) renderMeeting();
  else renderWelcome();
}

// ---------- events ----------

$('new-meeting').addEventListener('click', newMeeting);
$('open-settings').addEventListener('click', () => openSettings());
$('skale').addEventListener('click', () => invoke('open_site'));
$('search').addEventListener('input', (e) => { search = e.target.value; renderSidebar(); });
$('list').addEventListener('click', (e) => {
  const item = e.target.closest('[data-id]');
  if (item) open(item.dataset.id);
});

viewEl.addEventListener('change', handleProviderChange);
viewEl.addEventListener('input', handleProviderInput);

viewEl.addEventListener('click', (e) => {
  const section = e.target.closest('[data-section]');
  if (section) { settingsSection = section.dataset.section; renderSettings(); return; }
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
  else handleProviderAct(act, el);
});

listen('recording-state', ({ payload }) => {
  if (payload.phase === 'recording' && recording?.id === payload.id && startupRequestId) window.XnotePreparation.done(startupRequestId);
  if (payload.phase === 'idle') recording = null;
  else if (recording && recording.id === payload.id) {
    if (payload.phase === 'recording' && recording.phase !== 'recording') recording.since = Date.now();
    recording.phase = payload.phase;
  } else {
    recording = { id: payload.id, phase: payload.phase, since: Date.now(), base: 0 };
  }
  if (view === 'meeting') renderHead();
  renderSidebar();
});

listen('recording-preparation', ({ payload }) => window.XnotePreparation.update(payload));

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

// An AI assistant (MCP) may have saved a summary while the window was in the
// background: show it when the window comes back.
window.addEventListener?.('focus', async () => {
  if (recording || enhancing) return;
  await refreshList().catch(() => {});
  if (view === 'meeting' && meeting) {
    try {
      const fresh = await invoke('get_meeting', { id: meeting.id });
      if (fresh.enhanced !== meeting.enhanced || fresh.title !== meeting.title) {
        meeting.enhanced = fresh.enhanced;
        meeting.title = fresh.title;
        renderMeeting();
      }
    } catch (_) { /* deleted or unreadable: the list shows it */ }
  }
});

(async () => {
  try {
    settings = await invoke('get_settings');
  } catch (_) { /* defaults */ }
  setLanguage(settings.language);
  [engine, meetings] = await Promise.all([invoke('engine_status'), invoke('list_meetings')]);
  if (engineReady()) { view = 'welcome'; render(); } else openSetup(engine.mode ? 'change' : 'first');
})();
