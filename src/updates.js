'use strict';

// Updates panel. It is the same file in every Skale app: change it in one and
// copy it to the others. Rust looks for a newer release on its own
// (updater.rs), downloads it in the background and installs it when the app
// closes; this shows where that stands, in the version button at the foot of
// the sidebar, the panel it opens and the bar on top once an update is ready.
//
// Needs i18n.js (language, locale) and an #update-bar. An app can define
// updateBlocker() returning a sentence when restarting now would interrupt
// work (a recording, a conversion), so the panel asks before restarting.

const UPDATE_TEXT = {
  en: {
    updates: 'Updates', close: 'Close', checkNow: 'Check now', restartNow: 'Restart now', restartAnyway: 'Restart anyway',
    idle: 'Not checked yet.', checking: 'Checking for updates…', upToDate: 'You have the latest version.',
    downloading: 'Downloading {app} {v}…', ready: '{app} {v} is ready. It installs when you close the app.',
    failed: 'Could not check for updates: {error}', lastCheck: 'Last checked {when}.',
    howItWorks: '{app} checks GitHub when it opens and every 6 hours, downloads new versions in the background and verifies their signature. Your Data folder is kept.',
    whatsNew: "What's new in {v}", bar: '{app} {v} is ready and installs when you close the app.', restartFailed: 'Could not restart: {error}',
  },
  pt: {
    updates: 'Atualizações', close: 'Fechar', checkNow: 'Procurar agora', restartNow: 'Reiniciar agora', restartAnyway: 'Reiniciar mesmo assim',
    idle: 'Ainda não procurou.', checking: 'Procurando atualizações…', upToDate: 'Você está na versão mais recente.',
    downloading: 'Baixando o {app} {v}…', ready: 'O {app} {v} está pronto. Ele é instalado quando você fecha o app.',
    failed: 'Não foi possível procurar atualizações: {error}', lastCheck: 'Última verificação: {when}.',
    howItWorks: 'O {app} procura no GitHub ao abrir e a cada 6 horas, baixa as versões novas em segundo plano e confere a assinatura delas. Sua pasta Data é mantida.',
    whatsNew: 'Novidades da {v}', bar: 'O {app} {v} está pronto e é instalado quando você fecha o app.', restartFailed: 'Não foi possível reiniciar: {error}',
  },
  es: {
    updates: 'Actualizaciones', close: 'Cerrar', checkNow: 'Buscar ahora', restartNow: 'Reiniciar ahora', restartAnyway: 'Reiniciar de todos modos',
    idle: 'Aún no se ha buscado.', checking: 'Buscando actualizaciones…', upToDate: 'Tienes la versión más reciente.',
    downloading: 'Descargando {app} {v}…', ready: '{app} {v} está listo. Se instala cuando cierras la app.',
    failed: 'No se pudo buscar actualizaciones: {error}', lastCheck: 'Última comprobación: {when}.',
    howItWorks: '{app} busca en GitHub al abrirse y cada 6 horas, descarga las versiones nuevas en segundo plano y comprueba su firma. Tu carpeta Data se conserva.',
    whatsNew: 'Novedades de {v}', bar: '{app} {v} está listo y se instala cuando cierras la app.', restartFailed: 'No se pudo reiniciar: {error}',
  },
};

const updates = (() => {
  const { invoke } = window.__TAURI__.core;
  let status = null;   // updater::Status from Rust
  let appName = '';
  let open = false;
  let confirming = false; // the restart button asked once already

  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const ut = (key, vars = {}) => (UPDATE_TEXT[language]?.[key] ?? UPDATE_TEXT.en[key])
    .replace(/\{(\w+)\}/g, (_, name) => vars[name] ?? (name === 'app' ? appName : ''));
  const busy = () => status && (status.state === 'checking' || status.state === 'downloading');
  // GitHub's generated release notes, as plain lines: no headings, links or authors.
  const plainNotes = (md) => md.split('\n')
    .filter((l) => !/^\s*\*\*Full Changelog\*\*/.test(l))
    .map((l) => l.replace(/^\s*#+\s*/, '').replace(/^\s*[*-]\s+/, '• ').replace(/ by @\S+ in https?:\/\/\S+$/, '').replace(/\*\*/g, ''))
    .join('\n').replace(/\n{3,}/g, '\n\n').trim();
  const blocker = () => (typeof updateBlocker === 'function' ? updateBlocker() : '') || '';

  function line() {
    if (!status) return '';
    const v = status.version;
    switch (status.state) {
      case 'checking': return ut('checking');
      case 'upToDate': return ut('upToDate');
      case 'downloading': return ut('downloading', { v });
      case 'ready': return ut('ready', { v });
      case 'failed': return ut('failed', { error: status.error });
      default: return ut('idle');
    }
  }

  function renderButton() {
    const el = document.querySelector('.ver'); // <button class="ver ver-btn"> in the sidebar foot
    if (!el || !status) return;
    if (!el.dataset.updates) {
      el.dataset.updates = 'on';
      el.addEventListener('click', show);
    }
    el.classList.toggle('ready', status.state === 'ready');
    el.title = `${ut('updates')} · ${line()}`;
    el.innerHTML = `<svg class="i" viewBox="0 0 24 24"><path d="M21 12a9 9 0 0 1-15.5 6.2L3 16"/><path d="M3 21v-5h5"/><path d="M3 12a9 9 0 0 1 15.5-6.2L21 8"/><path d="M21 3v5h-5"/></svg>v${esc(status.current)}${status.state === 'ready' ? '<i class="dot"></i>' : ''}`;
  }

  function renderBar() {
    const bar = document.getElementById('update-bar');
    if (!bar) return;
    bar.hidden = status?.state !== 'ready';
    if (bar.hidden) return;
    bar.innerHTML = `<span>${esc(ut('bar', { v: status.version }))}</span>` +
      `<button class="btn small" data-update="panel">${esc(ut('updates'))}</button>` +
      `<button class="btn primary small" data-update="restart">${esc(ut('restartNow'))}</button>`;
  }

  function renderPanel() {
    let el = document.getElementById('updates');
    if (!open) { if (el) el.hidden = true; return; }
    if (!el) {
      el = document.createElement('div');
      el.id = 'updates';
      el.addEventListener('click', (e) => { if (e.target === el) hide(); });
      document.body.append(el);
    }
    el.hidden = false;
    const ready = status.state === 'ready';
    const when = status.checkedAt
      ? new Date(status.checkedAt * 1000).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' })
      : '';
    const warning = ready && confirming ? blocker() : '';
    const notes = ready ? plainNotes(status.notes) : '';
    el.innerHTML = `<div class="dialog updates-dialog" role="dialog" aria-label="${esc(ut('updates'))}">
      <h2>${esc(ut('updates'))}</h2>
      <div class="updates-state ${esc(status.state)}">
        <b>${esc(appName)} ${esc(status.current)}</b>
        <span>${esc(line())}</span>
        ${status.state === 'downloading' || status.state === 'checking' ? '<div class="pbar busy"><i></i></div>' : ''}
      </div>
      ${when ? `<p class="note">${esc(ut('lastCheck', { when }))}</p>` : ''}
      ${notes ? `<h3 class="updates-notes-title">${esc(ut('whatsNew', { v: status.version }))}</h3><div class="updates-notes">${esc(notes)}</div>` : ''}
      <p class="note">${esc(ut('howItWorks'))}</p>
      ${warning ? `<p class="updates-warning">${esc(warning)}</p>` : ''}
      <div class="dialog-actions">
        <button class="btn" data-update="close">${esc(ut('close'))}</button>
        <button class="btn" data-update="check"${busy() ? ' disabled' : ''}>${esc(ut('checkNow'))}</button>
        ${ready ? `<button class="btn primary" data-update="restart">${esc(ut(warning ? 'restartAnyway' : 'restartNow'))}</button>` : ''}
      </div>
    </div>`;
  }

  function render() {
    renderButton();
    renderBar();
    renderPanel();
  }

  function show() { open = true; confirming = false; render(); }
  function hide() { open = false; confirming = false; render(); }

  async function restart() {
    if (!confirming && blocker()) {
      confirming = true;
      open = true;
      render();
      return;
    }
    try {
      await invoke('restart_to_update'); // the app closes and the new version opens
    } catch (e) {
      confirming = false;
      render();
      if (typeof toast === 'function') toast(ut('restartFailed', { error: e }));
    }
  }

  document.addEventListener('click', (e) => {
    const el = e.target.closest('[data-update]');
    if (!el || el.disabled) return;
    const what = el.dataset.update;
    if (what === 'panel') show();
    else if (what === 'close') hide();
    else if (what === 'check') invoke('check_update');
    else if (what === 'restart') restart();
  });
  document.addEventListener('keydown', (e) => { if (open && e.key === 'Escape') hide(); });

  window.__TAURI__.event.listen('update-status', ({ payload }) => {
    status = payload;
    if (status.state !== 'ready') confirming = false;
    render();
  });

  (async () => {
    appName = await window.__TAURI__.app.getName().then((n) => n.charAt(0).toUpperCase() + n.slice(1)).catch(() => '');
    status = await invoke('update_status');
    render();
  })();

  // Call after a language change.
  return { render, show };
})();
