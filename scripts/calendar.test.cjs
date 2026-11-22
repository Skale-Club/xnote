'use strict';
// Mini calendar in the sidebar, run against the real window scripts with a
// fake DOM and fake Tauri commands.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const src = process.env.XNOTE_SOURCE_DIR || path.join(__dirname, '..', 'src');
const flush = () => new Promise(setImmediate);

const at = (y, m, d, h = 15) => new Date(y, m - 1, d, h).getTime();
const MEETINGS = [
  { id: 'c', title: 'Planning', created: at(2026, 11, 21, 18), durationMs: 600000 },
  { id: 'b', title: 'Stand-up', created: at(2026, 11, 21, 9), durationMs: 300000 },
  { id: 'a', title: 'Kickoff', created: at(2026, 10, 26), durationMs: 60000 },
];

async function harness(language = 'en', stored = {}) {
  const nodes = new Map();
  const node = (id) => {
    if (!nodes.has(id)) nodes.set(id, { id, innerHTML: '', textContent: '', hidden: true, disabled: false, value: '', dataset: {}, style: {}, classList: { add() {}, remove() {}, toggle(name, on) { this[name] = on; } }, setAttribute() {}, addEventListener() {}, focus() {}, insertAdjacentHTML() {} });
    return nodes.get(id);
  };
  const engine = { status: { gpu: true, packs: ['cuda'], models: ['best'] }, mode: 'local', pack: 'cuda', model: 'best', provider: 'groq', transcriptionModel: '', cloudKeys: [], providers: [], summaryProvider: '', summaryModel: '', autoSummary: true, summaryReady: false };
  const answers = {
    get_settings: { language, transcriptionLanguage: 'auto' },
    engine_status: engine,
    list_meetings: MEETINGS,
    update_status: { current: '1.0.0', state: 'idle', version: null, notes: '', error: null, checkedAt: null },
    help_info: { version: '1.0.0', dataFolder: 'C:\\Xnote\\Data', writable: true, pending: 0 },
  };
  const storage = { ...stored }, calls = [];
  const full = (id) => ({ ...MEETINGS.find((m) => m.id === id), notes: '', transcript: [], enhanced: '', sentTo: [] });
  const context = vm.createContext({
    console, Date, setInterval: () => 0, clearInterval() {}, setTimeout: () => 0, clearTimeout() {},
    navigator: { clipboard: { writeText: async () => {} } },
    document: { documentElement: {}, getElementById: node, querySelector: () => node('ver'), querySelectorAll: () => [], addEventListener() {}, removeEventListener() {} },
    window: {
      localStorage: { getItem: (k) => storage[k] ?? null, setItem: (k, v) => { storage[k] = String(v); } },
      __TAURI__: { core: { invoke: async (command, args) => { calls.push({ command, args }); if (command === 'get_meeting') return full(args.id); if (command === 'update_meeting') return { ...full(args.id), created: args.created }; return answers[command]; } }, event: { listen: async () => () => {} }, app: { getVersion: async () => '1.0.0', getName: async () => 'Xnote' } },
    },
  });
  for (const file of ['i18n.js', 'guides.js', 'updates.js', 'markdown.js', 'preparation.js', 'providers-ui.js', 'calendar.js', 'app.js']) {
    vm.runInContext(fs.readFileSync(path.join(src, file), 'utf8'), context);
  }
  await flush();
  const run = (code) => vm.runInContext(code, context);
  run('calMonth = { year: 2026, month: 10 }; renderSidebar()');
  const target = (selector, dataset) => `{ target: { closest: (s) => s === ${JSON.stringify(selector)} ? { dataset: ${JSON.stringify(dataset)}, disabled: false } : null } }`;
  return {
    run, storage, calls, node,
    head: () => node('head').innerHTML,
    calendar: () => node('calendar').innerHTML,
    list: () => node('list').innerHTML,
    clickDay: (day) => run(`handleCalendarClick(${target('[data-day]', { day })})`),
    clickAct: (cal) => run(`handleCalendarClick(${target('[data-cal]', { cal })})`),
  };
}

test('the month grid has 6 weeks starting on the locale week start', async () => {
  const h = await harness();
  // November 2026 starts on a Sunday.
  const sunday = h.run('monthGrid(2026, 10, 0)');
  assert.equal(sunday.length, 42);
  assert.deepEqual({ ...sunday[0] }, { key: '2026-11-01', day: 1, inMonth: true });
  const monday = h.run('monthGrid(2026, 10, 1)');
  assert.deepEqual({ ...monday[0] }, { key: '2026-10-26', day: 26, inMonth: false });
  assert.equal(monday[6].key, '2026-11-01');
  assert.equal(h.run("dayKey(new Date(2026, 0, 5, 23, 59).getTime())"), '2026-01-05');
});

test('days with meetings get dots and a label; empty days are disabled', async () => {
  const h = await harness();
  assert.match(h.calendar(), /November 2026/);
  assert.match(h.calendar(), /data-day="2026-11-21"[^>]*aria-label="Saturday, November 21 · 2 meetings"[^>]*>21<i class="dots"><b><\/b><b><\/b><\/i>/);
  assert.match(h.calendar(), /data-day="2026-11-20"[^>]* disabled>20</);
  // Every meeting is listed while no day is chosen.
  for (const title of ['Planning', 'Stand-up', 'Kickoff']) assert.match(h.list(), new RegExp(title));
});

test('clicking a day shows only its meetings; the chip or a second click shows all again', async () => {
  const h = await harness();
  h.clickDay('2026-11-21');
  assert.match(h.list(), /class="cal-chip"/);
  assert.match(h.list(), /Planning/);
  assert.match(h.list(), /Stand-up/);
  assert.doesNotMatch(h.list(), /Kickoff/);
  assert.match(h.calendar(), /class="cal-day[^"]* on has" data-day="2026-11-21"[^>]*aria-pressed="true"/);
  h.clickAct('clear');
  assert.match(h.list(), /Kickoff/);
  assert.doesNotMatch(h.list(), /cal-chip/);
  h.clickDay('2026-11-21');
  h.clickDay('2026-11-21');
  assert.match(h.list(), /Kickoff/);
});

test('a grey day from another month brings that month; the arrows move a month', async () => {
  const h = await harness();
  h.clickDay('2026-10-26');
  assert.match(h.calendar(), /October 2026/);
  assert.match(h.list(), /Kickoff/);
  assert.doesNotMatch(h.list(), /Planning/);
  h.clickAct('next');
  assert.match(h.calendar(), /November 2026/);
  h.clickAct('prev');
  h.clickAct('prev');
  assert.match(h.calendar(), /September 2026/);
});

test('the day filter combines with the search, with its own empty message', async () => {
  const h = await harness();
  h.clickDay('2026-11-21');
  h.run("search = 'kickoff'; renderSidebar()");
  assert.match(h.list(), /No meetings on this day\./);
  assert.match(h.list(), /cal-chip/);
});

test('the header folds the calendar and remembers it', async () => {
  const h = await harness();
  h.clickAct('fold');
  assert.equal(h.storage['xnote.calendarFolded'], '1');
  assert.doesNotMatch(h.calendar(), /cal-grid/);
  assert.match(h.calendar(), /aria-expanded="false"/);
  const again = await harness('en', { 'xnote.calendarFolded': '1' });
  assert.doesNotMatch(again.calendar(), /cal-grid/);
  again.clickAct('fold');
  assert.match(again.calendar(), /cal-grid/);
  assert.equal(again.storage['xnote.calendarFolded'], '0');
});

test('a new meeting clears the chosen day so it shows in the list', async () => {
  const h = await harness();
  h.clickDay('2026-10-26');
  h.run('resetCalendar(); renderSidebar()');
  assert.doesNotMatch(h.list(), /cal-chip/);
  assert.equal(h.run('calDay'), '');
});

test('Portuguese starts the week on Sunday and Spanish on Monday, in their own words', async () => {
  const pt = await harness('pt');
  assert.match(pt.calendar(), /Novembro de 2026/);
  assert.match(pt.calendar(), /^.*?<div class="cal-grid"><span class="cal-wd">D<\/span>/s);
  assert.match(pt.calendar(), /2 reuniões/);
  const es = await harness('es');
  assert.match(es.calendar(), /<div class="cal-grid"><span class="cal-wd">L<\/span>/);
  assert.match(es.calendar(), /2 reuniones/);
});

test('the date under the title opens a picker and moves the meeting', async () => {
  const h = await harness('pt');
  h.run("open('a')");
  await flush();
  assert.match(h.head(), /<button class="meta-date" data-act="edit-date" title="Mudar data e hora">26 de out\. · 15:00<\/button>/);
  h.run('editingDate = true; renderHead()');
  assert.match(h.head(), /type="datetime-local" id="date-input" class="date-input" value="2026-10-26T15:00"/);
  h.node('date-input').value = '2026-11-03T10:30';
  h.run('calDay = "2026-10-26"');
  await h.run('saveDate()');
  const call = h.calls.find((c) => c.command === 'update_meeting');
  assert.equal(call.args.created, new Date(2026, 10, 3, 10, 30).getTime());
  assert.equal(h.run('editingDate'), false);
  assert.match(h.head(), /3 de nov\. · 10:30/);
  // A day chosen in the calendar follows the meeting to its new day.
  assert.equal(h.run('calDay'), '2026-11-03');
});

test('an empty date is refused without calling the app, and recording locks the date', async () => {
  const h = await harness();
  h.run("open('a')");
  await flush();
  h.run('editingDate = true; renderHead()');
  h.node('date-input').value = '';
  await h.run('saveDate()');
  assert.equal(h.calls.some((c) => c.command === 'update_meeting'), false);
  h.run("editingDate = false; recording = { id: 'a', phase: 'recording', since: Date.now(), base: 0, lines: 0 }; renderHead()");
  assert.match(h.head(), /data-act="edit-date" title="The date can’t change while this meeting records\." disabled/);
  h.run('editingDate = true; renderHead()');
  assert.doesNotMatch(h.head(), /date-input/);
});
