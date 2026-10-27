'use strict';

// A persistent inline progress bar: meeting notes remain editable while startup
// runs. No guessed model-load percentage; values count completed startup stages.
window.XnotePreparation = (() => {
  const el = () => document.getElementById('preparation');
  const escape = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
  let state = null, ticker = null, hideTimer = null, disabledBefore = [];
  const matches = (requestId) => state?.requestId === requestId;
  const stages = () => state.mode === 'cloud' ? ['checking', 'audio'] : ['checking', 'loading', 'connecting', 'audio'];

  function lockActions(lock) {
    if (lock) {
      disabledBefore = ['new-meeting','open-settings'].map((id) => {
        const node = document.getElementById(id); const disabled = !!node?.disabled;
        if (node) node.disabled = true;
        return [id, disabled];
      });
    } else {
      for (const [id, disabled] of disabledBefore) { const node = document.getElementById(id); if (node) node.disabled = disabled; }
      disabledBefore = [];
    }
  }
  function tick() {
    if (!state) return;
    const seconds = Math.max(0, Math.floor((Date.now() - state.since) / 1000));
    const node = document.getElementById('prep-time');
    if (node) node.textContent = t('prepElapsed', { time: `${String(Math.floor(seconds / 60)).padStart(2,'0')}:${String(seconds % 60).padStart(2,'0')}` });
    const hint = document.getElementById('prep-wait');
    if (hint) hint.hidden = seconds < 30 || state.done || !!state.error;
  }
  function render() {
    if (!state || !el()) return;
    const list = stages(), index = state.done ? list.length : Math.max(0, list.indexOf(state.stage));
    const label = state.done ? t('prepStarted') : state.cancelling ? t('prepCancelling') : t(`prep_${state.stage}`);
    const title = state.error ? t('prepFailed') : state.done ? t('prepStarted') : t('prepTitle');
    const step = state.done ? t('prepComplete') : t('prepStep', { current: Math.min(index + 1, list.length), total: list.length });
    const buttons = state.error
      ? `<button class="btn primary small" data-prep="retry">${escape(t('prepRetry'))}</button><button class="btn small" data-prep="close">${escape(t('prepBack'))}</button>`
      : state.done ? '' : `<button class="btn small" data-prep="cancel"${state.cancelling ? ' disabled' : ''}>${escape(t(state.cancelling ? 'prepCancelling' : 'cancel'))}</button>`;
    let error = '';
    if (state.error) {
      const [code,...rest] = state.error.split('|');
      const key = ({engine_start_timeout:'prepTimeout',engine_start_failed:'prepEngineFailed',startup_busy:'prepBusy'})[code] || 'prepGenericError';
      const detail = (rest.length ? rest.join('|') : state.error).slice(0,2400);
      error = `<div class="prep-error" role="alert"><p>${escape(t(key))}</p><details><summary>${escape(t('prepDetails'))}</summary><pre>${escape(detail)}</pre></details></div>`;
    }
    el().hidden = false;
    el().setAttribute('aria-label', t('prepTitle'));
    el().className = `preparation${state.error ? ' has-error' : state.done ? ' complete' : ''}`;
    el().setAttribute('aria-busy', String(!state.done && !state.error));
    el().innerHTML = `<div class="prep-head"><img src="logo.png" alt=""><div class="prep-copy"><b>${escape(title)}</b><span>${escape(state.title)}</span></div>` +
      `<span class="prep-time" id="prep-time"></span><div class="prep-actions">${buttons}</div></div>` +
      `<div class="prep-caption"><span role="status" aria-live="polite">${escape(label)}</span><span>${escape(step)}</span></div>` +
      `<div class="prep-track" role="progressbar" aria-valuemin="0" aria-valuemax="${list.length}" aria-valuenow="${index}" aria-valuetext="${escape(label + '. ' + step)}"><i style="width:${index / list.length * 100}%"></i></div>` +
      `<ol class="prep-steps">${list.map((s,i) => `<li class="${i < index ? 'done' : i === index && !state.done ? 'current' : ''}"${i === index && !state.done ? ' aria-current="step"' : ''}>${escape(t(`prep_${s}`))}</li>`).join('')}</ol>` +
      (state.done ? '' : `<p class="prep-not-recording">${escape(t('prepNotRecording'))}</p>`) +
      `<p class="prep-wait" id="prep-wait" hidden>${escape(t('prepWait'))}</p>` +
      (state.cancelError ? `<p class="prep-error" role="alert">${escape(t('prepCancelError'))}</p>` : '') + error;
    el().onclick = (event) => {
      const button = event.target.closest('[data-prep]');
      if (!button || button.disabled) return;
      if (button.dataset.prep === 'cancel') cancel();
      else if (button.dataset.prep === 'retry') { const retry = state.onRetry; close(); retry(); }
      else if (button.dataset.prep === 'close') close();
    };
    tick();
  }
  function close() {
    clearInterval(ticker); clearTimeout(hideTimer); ticker = null; hideTimer = null;
    lockActions(false); state = null;
    if (el()) { el().hidden = true; el().innerHTML = ''; }
  }
  function begin(options) {
    close();
    state = { ...options, since:Date.now(), stage:'checking', cancelling:false, error:null, done:false };
    lockActions(true); render(); ticker = setInterval(tick,1000);
  }
  function update(payload) {
    if (!matches(payload.requestId) || state.done || state.error || state.cancelling || !stages().includes(payload.stage)) return;
    // Events from a slow/stale stage cannot move the bar backwards.
    if (stages().indexOf(payload.stage) < stages().indexOf(state.stage)) return;
    state.stage = payload.stage; render();
  }
  function done(requestId) {
    if (!matches(requestId) || state.done) return;
    state.done = true; state.cancelling = false; clearInterval(ticker); lockActions(false); render();
    hideTimer = setTimeout(() => { if (matches(requestId)) close(); },700);
  }
  function fail(requestId, error) {
    if (!matches(requestId)) return;
    state.error = String(error); state.cancelling = false; clearInterval(ticker); lockActions(false); render();
  }
  async function cancel() {
    if (!state || state.done || state.error || state.cancelling) return;
    const requestId = state.requestId;
    state.cancelling = true; state.cancelError = false; render();
    try { await state.onCancel(requestId); }
    catch (_) { if (matches(requestId) && !state.done) { state.cancelling = false; state.cancelError = true; render(); } }
    // Keep the bar until the backend acknowledges cancellation or commits the
    // session; a button click alone must never pretend capture was stopped.
  }
  return {begin,update,done,fail,close,matches};
})();
