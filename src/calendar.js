'use strict';

// Mini month calendar in the sidebar, between the search and the meeting list.
// Days with meetings get dots; clicking one shows only that day's meetings
// (clicking it again, or the chip above the list, shows them all again). The
// month header folds the calendar to one line, remembered between sessions.
// app.js calls renderCalendar() from renderSidebar() and filters the list with
// calendarFilter().

const CAL_FOLD_KEY = 'xnote.calendarFolded';

let calFolded = (() => { try { return window.localStorage.getItem(CAL_FOLD_KEY) === '1'; } catch (_) { return false; } })();
let calMonth = null;        // { year, month } on screen; null = the current month
let calDay = '';            // 'YYYY-MM-DD' chosen, '' = every day

const CAL_ICONS = {
  prev: '<path d="m15 6-6 6 6 6"/>',
  next: '<path d="m9 6 6 6-6 6"/>',
  fold: '<path d="m6 9 6 6 6-6"/>',
  close: '<path d="M6 6l12 12M18 6 6 18"/>',
};

/** Local calendar day of a timestamp, as 'YYYY-MM-DD'. */
function dayKey(ts) {
  const d = new Date(ts);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/**
 * The 6 weeks shown for a month, starting on `weekStart` (0 = Sunday,
 * 1 = Monday): [{ key, day, inMonth }].
 */
function monthGrid(year, month, weekStart) {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() - weekStart + 7) % 7;
  const cells = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(year, month, 1 - lead + i);
    cells.push({ key: dayKey(d.getTime()), day: d.getDate(), inMonth: d.getMonth() === month });
  }
  return cells;
}

/** Meetings per day: Map of 'YYYY-MM-DD' to a count. */
function meetingsPerDay(list) {
  const counts = new Map();
  for (const m of list) {
    const key = dayKey(m.created);
    counts.set(key, (counts.get(key) || 0) + 1);
  }
  return counts;
}

/** True when the meeting passes the chosen day (or no day is chosen). */
function calendarFilter(m) {
  return !calDay || dayKey(m.created) === calDay;
}

const capitalize = (s) => s.charAt(0).toUpperCase() + s.slice(1);
const keyDate = (key) => { const [y, m, d] = key.split('-').map(Number); return new Date(y, m - 1, d); };

function shownMonth() {
  if (calMonth) return calMonth;
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() };
}

function renderCalendar() {
  const el = document.getElementById('calendar');
  if (!el) return;
  const { year, month } = shownMonth();
  const loc = locale();
  const title = capitalize(new Date(year, month, 1).toLocaleDateString(loc, { month: 'long', year: 'numeric' }));
  const head = `<div class="cal-head">` +
    `<button class="cal-title" data-cal="fold" aria-expanded="${!calFolded}" title="${esc(t(calFolded ? 'calShow' : 'calHide'))}">` +
    `<span>${esc(title)}</span>${svg(CAL_ICONS.fold)}</button>` +
    (calFolded ? '' :
      `<button class="cal-nav" data-cal="prev" title="${esc(t('calPrev'))}" aria-label="${esc(t('calPrev'))}">${svg(CAL_ICONS.prev)}</button>` +
      `<button class="cal-nav" data-cal="next" title="${esc(t('calNext'))}" aria-label="${esc(t('calNext'))}">${svg(CAL_ICONS.next)}</button>`) +
    '</div>';
  el.classList.toggle('folded', calFolded);
  if (calFolded) { el.innerHTML = head; return; }

  const weekStart = I18N[language].weekStart || 0;
  const names = [];
  for (let i = 0; i < 7; i++) {
    // 2026-11-01 was a Sunday.
    names.push(new Date(2026, 10, 1 + ((weekStart + i) % 7)).toLocaleDateString(loc, { weekday: 'narrow' }));
  }
  const counts = meetingsPerDay(meetings);
  const today = dayKey(Date.now());
  let grid = names.map((n) => `<span class="cal-wd">${esc(n)}</span>`).join('');
  for (const c of monthGrid(year, month, weekStart)) {
    const n = counts.get(c.key) || 0;
    const cls = ['cal-day', c.inMonth ? '' : 'out', c.key === today ? 'today' : '', c.key === calDay ? 'on' : '', n ? 'has' : ''].filter(Boolean).join(' ');
    const label = keyDate(c.key).toLocaleDateString(loc, { weekday: 'long', day: 'numeric', month: 'long' }) +
      (n ? ` · ${t(n === 1 ? 'calOne' : 'calMany', { n })}` : '');
    const dots = n ? `<i class="dots">${'<b></b>'.repeat(Math.min(n, 3))}</i>` : '';
    grid += `<button class="${cls}" data-day="${c.key}" aria-label="${esc(label)}" title="${esc(label)}"` +
      `${c.key === calDay ? ' aria-pressed="true"' : ''}${n ? '' : ' disabled'}>${c.day}${dots}</button>`;
  }
  el.innerHTML = `${head}<div class="cal-grid">${grid}</div>`;
}

/** The chip above the list while a day is chosen. */
function calendarChipHtml() {
  if (!calDay) return '';
  const label = keyDate(calDay).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' });
  return `<button class="cal-chip" data-cal="clear" title="${esc(t('calAll'))}">` +
    `<span>${esc(label)}</span>${svg(CAL_ICONS.close)}<em>${esc(t('calAll'))}</em></button>`;
}

function setCalendarDay(key) {
  calDay = key;
  // A day from the next or previous month (the grey cells) brings that month.
  if (key) { const d = keyDate(key); calMonth = { year: d.getFullYear(), month: d.getMonth() }; }
  renderSidebar();
}

/** Back to every day and the current month (a new meeting is always today). */
function resetCalendar() {
  calDay = '';
  calMonth = null;
}

/** Clicks inside the calendar and on the chip above the list. */
function handleCalendarClick(e) {
  const day = e.target.closest('[data-day]');
  if (day && !day.disabled) {
    setCalendarDay(day.dataset.day === calDay ? '' : day.dataset.day);
    return true;
  }
  const act = e.target.closest('[data-cal]')?.dataset.cal;
  if (!act) return false;
  if (act === 'fold') {
    calFolded = !calFolded;
    try { window.localStorage.setItem(CAL_FOLD_KEY, calFolded ? '1' : '0'); } catch (_) { /* not remembered */ }
  } else if (act === 'prev' || act === 'next') {
    const { year, month } = shownMonth();
    const d = new Date(year, month + (act === 'next' ? 1 : -1), 1);
    calMonth = { year: d.getFullYear(), month: d.getMonth() };
  } else if (act === 'clear') {
    calDay = '';
  }
  renderSidebar();
  return true;
}
