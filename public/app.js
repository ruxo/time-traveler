import { formatDuration, getLang, locale, setLang, t } from './i18n.js';

// ---------- constants ----------

const ICONS = {
  sleep: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z',
  nap: 'M4 9h5l-5 6h5M13 4h7l-7 8h7',
  eat: 'M7 3v18M4 3v5a3 3 0 0 0 6 0V3M17 3c-2 0-3 2-3 5s1 4 3 4v9',
  work: 'M4 5h16v11H4zM2 19h20',
  rest: 'M4 9h13v4a6 6 0 0 1-6 6h-1a6 6 0 0 1-6-6zM17 10h1.5a2.5 2.5 0 0 1 0 5H17M8 3v3M12 3v3',
  game: 'M6 8h12a4 4 0 0 1 4 4v1a3 3 0 0 1-5.4 1.8L15 13H9l-1.6 1.8A3 3 0 0 1 2 13v-1a4 4 0 0 1 4-4zM8 10v4M6 12h4M15.5 11.5h.01M18 13.5h.01',
  travel: 'M12 21s-7-6.2-7-11a7 7 0 0 1 14 0c0 4.8-7 11-7 11zM12 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z',
  social: 'M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21v-1a6 6 0 0 1 12 0v1M16 3.5a4 4 0 0 1 0 7.5M22 21v-1a6 6 0 0 0-4-5.7',
  sport: 'M6.5 6.5v11M17.5 6.5v11M3 9.5v5M21 9.5v5M6.5 12h11',
  star: 'M12 3l2.6 5.6 6 .7-4.5 4.1 1.3 6L12 16.8l-5.4 2.6 1.3-6-4.5-4.1 6-.7z',
  book: 'M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3zM5 17a3 3 0 0 1 3-3h11',
  music: 'M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z',
  heart: 'M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z',
  code: 'M8 7l-5 5 5 5M16 7l5 5-5 5M13.5 5l-3 14',
  cart: 'M3 4h2.5l2.2 10.5h10.6L20.5 7H7M9.5 19.5h.01M17 19.5h.01',
  home: 'M4 11l8-7 8 7M6 10v10h12V10M10 20v-6h4v6',
  leaf: 'M5 19c0-8 5-13 14-14 0 9-5 14-13 14zM5 19l7-7',
  chat: 'M4 5h16v11H9l-5 4z',
  camera: 'M4 8h3l2-3h6l2 3h3v11H4zM12 16.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z',
  film: 'M3 6h18v12H3zM8 21h8M10 10.5v3l3-1.5z',
  sun: 'M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0z',
  moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z',
  bell: 'M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.9 1.9 0 0 0 3.4 0',
  plus: 'M12 5v14M5 12h14',
  pencil: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4',
  trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6',
  left: 'M15 18l-6-6 6-6',
  right: 'M9 18l6-6-6-6',
  today: 'M4 6h16v14H4zM4 10h16M9 3v4M15 3v4',
  grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1',
};

const BUILTIN = [
  { key: 'sleep', color: '#5B3FA8', ink: '#FFFFFF' },
  { key: 'nap', color: '#A48BE0', ink: '#1C1238' },
  { key: 'eat', color: '#C8643B', ink: '#FFFFFF' },
  { key: 'work', color: '#C9A227', ink: '#2A1F00' },
  { key: 'rest', color: '#6E8F5E', ink: '#FFFFFF' },
  { key: 'game', color: '#8C3F6E', ink: '#FFFFFF' },
  { key: 'travel', color: '#4A90C2', ink: '#FFFFFF' },
  { key: 'social', color: '#E07A9B', ink: '#3A1020' },
  { key: 'sport', color: '#22A699', ink: '#06302B' },
];
const CUSTOM_COLORS = ['#3F7C85', '#B5835A', '#4F6BA8', '#9A6B2F', '#2F7A5C', '#A34A5A'];
// The icons a custom activity may take: the server's `activities::ICONS`, in the same order.
const CUSTOM_ICONS = ['star', 'book', 'music', 'heart', 'code', 'cart', 'home', 'pencil', 'leaf', 'chat', 'camera', 'film'];
// The radial menu on the + button holds this many icons at a touchable size on a narrow phone.
const RADIAL_MAX = 10;
const RING = 2 * Math.PI * 88;
const DAY_MS = 86400e3;
// A reminder first noticed long after its time (say, the laptop was asleep) is
// recorded as shown rather than going off hours late.
const REMIND_GRACE_S = 10 * 60;

// ---------- small helpers ----------

const store = {
  get(key) {
    try { return localStorage.getItem(key); } catch { return null; }
  },
  set(key, value) {
    try { localStorage.setItem(key, value); } catch { /* storage unavailable */ }
  },
};

const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const svg = (name, extra = '') =>
  `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" ${extra}><path d="${ICONS[name]}"/></svg>`;

const startOfDay = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addDays = (d, n) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
const pad = (n) => String(n).padStart(2, '0');

function hms(totalSeconds) {
  const s = Math.max(0, Math.floor(totalSeconds));
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

const clock = (ms) => new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit', hour12: false }).format(ms);
const longDate = (d) => new Intl.DateTimeFormat(locale(), { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }).format(d);

// The CSP forbids inline style attributes, so templates write data-style and
// this copies it into element.style, which the CSP allows.
function paint(node) {
  if (node.nodeType !== 1) return;
  if (node.hasAttribute('data-style')) node.style.cssText = node.getAttribute('data-style');
  node.querySelectorAll('[data-style]').forEach((el) => { el.style.cssText = el.getAttribute('data-style'); });
}
new MutationObserver((mutations) => {
  for (const m of mutations) m.addedNodes.forEach(paint);
}).observe(document.documentElement, { childList: true, subtree: true });

// ---------- state ----------

const state = {
  config: { providers: [], devLogin: false },
  me: null,
  custom: [],
  current: null,
  skew: 0,
  day: startOfDay(new Date()),
  sessions: [],
  picked: null,
  hours: '',
  minutes: '',
  newName: '',
  newColor: CUSTOM_COLORS[0],
  newIcon: CUSTOM_ICONS[0],
  route: 'today',
  pickerOpen: false, // the start card, opened with + while a timer runs
};

const now = () => Date.now() + state.skew;
const isToday = () => state.day.getTime() === startOfDay(new Date(now())).getTime();
const elapsedSec = (session) => (now() - Date.parse(session.startedAt)) / 1000;

function actInfo(key) {
  const builtin = BUILTIN.find((a) => a.key === key);
  if (builtin) return { ...builtin, name: t('act.' + key), icon: key, deleted: false };
  const custom = state.custom.find((a) => a.key === key);
  if (custom) return { key, name: custom.name, color: custom.color, ink: '#FFFFFF', icon: CUSTOM_ICONS.includes(custom.icon) ? custom.icon : 'star', deleted: custom.deleted };
  return { key, name: '—', color: '#8A7A66', ink: '#FFFFFF', icon: 'star', deleted: true };
}

function pickable() {
  return [
    ...BUILTIN.map((a) => actInfo(a.key)),
    ...state.custom.filter((a) => !a.deleted).map((a) => actInfo(a.key)),
  ];
}

const icoHtml = (info, size = '') =>
  `<span class="ico ${size}" data-style="--c:${esc(info.color)};--ink:${esc(info.ink)}">${svg(info.icon)}</span>`;

// The colour a new activity will get: the one picked on the activities page, or
// the one the server assigns by count when none is sent (the today page).
const newActivityColor = () => (state.route === 'activities' ? state.newColor : CUSTOM_COLORS[state.custom.length % CUSTOM_COLORS.length]);

// The round button beside the name field showing the new activity's icon, and
// the grid it opens. `down` opens the grid below the button (activities page).
function iconPickHtml(down = false) {
  const options = CUSTOM_ICONS
    .map((k) => `<button class="icon-opt" data-action="icon" data-icon="${k}" aria-pressed="${state.newIcon === k}" aria-label="${esc(t('icon.' + k))}" title="${esc(t('icon.' + k))}">${svg(k)}</button>`)
    .join('');
  return `
    <div class="icon-pick">
      <button class="ico icon-pick-btn" id="icon-btn" data-action="icons" aria-haspopup="dialog" aria-expanded="false" aria-controls="icon-pop" aria-label="${esc(t('icons.pick', { name: t('icon.' + state.newIcon) }))}" title="${esc(t('icons.title'))}" data-style="--c:${newActivityColor()};--ink:#FFFFFF">${svg(state.newIcon)}</button>
      <div class="icon-pop${down ? ' icon-pop--down' : ''}" id="icon-pop" role="dialog" aria-labelledby="icon-h" hidden>
        <p class="time-h" id="icon-h">${esc(t('icons.title'))}</p>
        <div class="icon-grid">${options}</div>
      </div>
    </div>`;
}

function closeIconPop(focusButton = true) {
  const pop = document.getElementById('icon-pop');
  if (!pop || pop.hidden) return;
  pop.hidden = true;
  const btn = document.getElementById('icon-btn');
  btn?.setAttribute('aria-expanded', 'false');
  if (focusButton) btn?.focus();
}

function toggleIconPop() {
  const pop = document.getElementById('icon-pop');
  if (!pop) return;
  if (!pop.hidden) return closeIconPop();
  pop.hidden = false;
  document.getElementById('icon-btn').setAttribute('aria-expanded', 'true');
  pop.querySelector('[aria-pressed="true"]')?.focus();
}

// Redraws the icon button after the icon or the colour changed.
function renderIconButton() {
  const btn = document.getElementById('icon-btn');
  if (!btn) return;
  btn.innerHTML = svg(state.newIcon);
  btn.style.setProperty('--c', newActivityColor());
  btn.setAttribute('aria-label', t('icons.pick', { name: t('icon.' + state.newIcon) }));
  document.querySelectorAll('.icon-opt').forEach((o) => o.setAttribute('aria-pressed', String(o.dataset.icon === state.newIcon)));
}

// ---------- server ----------

let csrf = '';

async function api(method, path, body) {
  const headers = {};
  if (method !== 'GET') headers['x-csrf-token'] = csrf;
  if (body !== undefined) headers['content-type'] = 'application/json';
  let res;
  try {
    res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
  } catch {
    toast(t('err.offline'));
    throw new Error('offline');
  }
  const data = await res.json().catch(() => null);
  if (res.status === 401 && path !== '/api/me') {
    state.me = null;
    render();
  }
  if (!res.ok) {
    const err = new Error(data?.error || `http_${res.status}`);
    err.status = res.status;
    throw err;
  }
  return data;
}

async function loadState() {
  const s = await api('GET', '/api/state');
  state.skew = Date.parse(s.serverNow) - Date.now();
  state.custom = s.custom;
  state.current = s.current;
}

async function loadDay() {
  const from = state.day.toISOString();
  const to = addDays(state.day, 1).toISOString();
  const r = await api('GET', `/api/sessions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`);
  state.sessions = r.sessions;
}

async function refresh() {
  await Promise.all([loadState(), loadDay()]);
  renderLive();
}

// ---------- toast & dialogs ----------

let toastTimer;
function toast(message) {
  const el = document.getElementById('toast');
  el.textContent = message;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, 4000);
}

const failed = (err) => {
  if (err?.message !== 'offline' && err?.status !== 401) toast(t('err.generic'));
};

// Shows the shared <dialog>; resolves with the button value it closed on.
function openDialog(html) {
  const dlg = document.getElementById('dlg');
  dlg.innerHTML = html;
  dlg.returnValue = '';
  dlg.showModal();
  return new Promise((resolve) => dlg.addEventListener('close', () => resolve(dlg.returnValue), { once: true }));
}

async function confirmDialog({ title, body, confirm, danger = false }) {
  const value = await openDialog(`
    <form method="dialog">
      <h2 id="dlg-title">${esc(title)}</h2>
      <p>${esc(body)}</p>
      <div class="dialog-actions">
        <button class="btn" value="cancel">${esc(t('cancel'))}</button>
        <button class="btn ${danger ? 'btn--danger' : 'btn--primary'}" value="ok">${esc(confirm)}</button>
      </div>
    </form>`);
  return value === 'ok';
}

// ---------- theme & language ----------

function effectiveTheme() {
  const set = document.documentElement.getAttribute('data-theme');
  if (set) return set;
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function syncThemeColor() {
  document.querySelector('meta[name="theme-color"]').content = effectiveTheme() === 'dark' ? '#16100A' : '#FFF8DC';
}

function toggleTheme() {
  const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', next);
  store.set('tt_theme', next);
  syncThemeColor();
  render();
}

function switchLang(next) {
  setLang(next);
  store.set('tt_lang', next);
  render();
}

// ---------- reminders ----------

let audio;
function unlockAudio() {
  if (audio || !window.AudioContext) return;
  try { audio = new AudioContext(); } catch { /* no audio */ }
}

function chime() {
  if (!audio) return;
  audio.resume?.();
  const start = audio.currentTime;
  [880, 1175].forEach((freq, i) => {
    const osc = audio.createOscillator();
    const gain = audio.createGain();
    osc.frequency.value = freq;
    osc.type = 'sine';
    const t0 = start + i * 0.28;
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(0.25, t0 + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.5);
    osc.connect(gain).connect(audio.destination);
    osc.start(t0);
    osc.stop(t0 + 0.55);
  });
}

// Fires once per session per device, at most REMIND_GRACE_S after the set time.
function checkReminder() {
  const cur = state.current;
  if (!cur?.targetSeconds) return;
  const over = elapsedSec(cur) - cur.targetSeconds;
  if (over < 0) return;
  const key = 'tt_reminded_' + cur.id;
  if (store.get(key)) return;
  store.set(key, '1');
  if (over > REMIND_GRACE_S) return;
  const info = actInfo(cur.activity);
  chime();
  if ('Notification' in window && Notification.permission === 'granted') {
    try {
      const n = new Notification(t('notif.title', { name: info.name }), {
        body: t('notif.body', { target: formatDuration(cur.targetSeconds / 60) }),
        tag: 'tt-' + cur.id,
        icon: '/icon.svg',
      });
      n.onclick = () => { window.focus(); n.close(); };
    } catch { /* some browsers only allow notifications from a service worker */ }
  }
}

// ---------- header ----------

function headerHtml(withNav) {
  const nav = withNav
    ? `<nav class="pill-nav" aria-label="${esc(t('nav.label'))}">
        ${[['today', '#/'], ['activities', '#/activities'], ['account', '#/account']]
          .map(([r, href]) => `<a href="${href}" ${state.route === r ? 'aria-current="page"' : ''}>${esc(t('nav.' + r))}</a>`)
          .join('')}
      </nav>`
    : '';
  const dark = effectiveTheme() === 'dark';
  const me = state.me;
  const initial = me ? esc(Array.from(me.name || me.email || '?')[0].toUpperCase()) : '';
  // Wide screens: the avatar links to the account page. Phones: it opens the
  // menu that holds the tabs and the language switch (see openAcctMenu).
  const avatar = me
    ? `<a class="avatar avatar--link" href="#/account" aria-label="${esc(t('nav.account'))}">${initial}</a>
       <button class="avatar avatar--menu" id="acct-btn" data-action="acct-menu" aria-haspopup="dialog" aria-expanded="false" aria-controls="acct-menu" aria-label="${esc(t('nav.menu'))}">${initial}</button>`
    : '';
  // On phones the header becomes a bar at the bottom. Signed in (.hdr--menu) it
  // shows only the name, the theme button and the avatar; signed out it keeps
  // the language switch. On wider screens .hdr-bar is display: contents.
  return `
    <header class="hdr${withNav ? ' hdr--menu' : ''}">
      ${nav}
      <div class="hdr-bar">
        <a class="brand" href="#/">
          <img src="/icon.svg" width="44" height="44" alt="">
          <span class="brand-name">Time Traveler</span>
        </a>
        <div class="hdr-tools">
          <div class="seg" role="group" aria-label="${esc(t('lang.label'))}">
            <button data-action="lang" data-lang="th" aria-pressed="${getLang() === 'th'}">TH</button>
            <button data-action="lang" data-lang="en" aria-pressed="${getLang() === 'en'}">EN</button>
          </div>
          <button class="icon-btn" data-action="theme" aria-label="${esc(t(dark ? 'theme.toLight' : 'theme.toDark'))}">${svg(dark ? 'sun' : 'moon')}</button>
          ${avatar}
        </div>
      </div>
    </header>`;
}

// ---------- login ----------

function renderLogin() {
  const params = new URLSearchParams(location.search);
  const error = params.get('login_error');
  if (error) history.replaceState(null, '', location.pathname + location.hash);
  const { providers, devLogin } = state.config;
  const buttons = providers
    .map((p, i) => `<a class="btn ${i === 0 ? 'btn--primary' : ''}" href="/auth/login?provider=${p}">${esc(t('login.' + p))}</a>`)
    .join('');
  document.getElementById('app').innerHTML = `
    <div class="wrap shell">
      ${headerHtml(false)}
      <main class="login">
        <section class="card card--hero" aria-labelledby="login-h">
          <h1 id="login-h">${esc(t('login.tagline'))}</h1>
          <p class="muted">${esc(t('login.body'))}</p>
          ${error ? `<p class="reminder" role="alert">${esc(t(error === 'disabled' ? 'login.disabled' : 'login.error'))}</p>` : ''}
          <div class="login-buttons">
            ${buttons}
            ${devLogin ? `<button class="btn ${providers.length ? '' : 'btn--primary'}" data-action="dev-login">${esc(t('login.dev'))}</button>` : ''}
            ${!providers.length && !devLogin ? `<p class="note-box">${esc(t('login.none'))}</p>` : ''}
          </div>
        </section>
      </main>
    </div>`;
}

// ---------- today ----------

function greeting() {
  const h = new Date(now()).getHours();
  return t(h < 12 ? 'greet.morning' : h < 17 ? 'greet.afternoon' : 'greet.evening');
}

// Rendered twice: above everything on wide screens, below the current card on
// phones. CSS shows one of them.
function greetHtml(where) {
  return `
    <section class="greet greet--${where}">
      <div class="greet-hello">
        <p class="muted">${esc(greeting())}</p>
        <h1>${esc(t('today.title'))}</h1>
      </div>
      <p class="meta"><span>${esc(longDate(new Date(now())))}</span><span aria-hidden="true">·</span><span class="now-label"></span></p>
    </section>`;
}

function todayHtml() {
  return `
    ${greetHtml('top')}
    <div id="notif-slot"></div>
    <div class="cols">
      <div class="col-main">
        <section class="card card--hero" id="cur-card" aria-label="${esc(t('cur.heading'))}"></section>
        ${greetHtml('below')}
        <section class="card" id="start-card" aria-labelledby="start-h">
          <div class="card-head">
            <h2 id="start-h">${esc(t('start.heading'))}</h2>
            <button class="icon-bare picker-close" id="picker-close" data-action="close-picker" aria-label="${esc(t('start.close'))}" title="${esc(t('start.closeShort'))}" hidden><svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          </div>
          <div class="chips" id="chips"></div>
          <div class="add-row">
            ${iconPickHtml()}
            <div class="field">
              <label for="new-name">${esc(t('start.newLabel'))}</label>
              <input id="new-name" class="input" type="text" maxlength="40" autocomplete="off" placeholder="${esc(t('start.newPlaceholder'))}" value="${esc(state.newName)}">
            </div>
            <button class="btn btn--soft" data-action="add-custom">${svg('plus')}${esc(t('start.add'))}</button>
          </div>
          <div class="start-go" id="start-go" ${state.picked ? '' : 'hidden'}>
            <div class="go-row">
              <button class="icon-bare" id="time-btn" data-action="time" aria-haspopup="dialog" aria-expanded="false" aria-controls="time-pop">${CLOCK}</button>
              <button class="btn btn--primary" id="start-btn" data-action="start"></button>
              <div class="time-pop" id="time-pop" role="dialog" aria-labelledby="time-h" hidden>
                <p class="time-h" id="time-h">${esc(t('start.durLegend'))} <span>${esc(t('start.optional'))}</span></p>
                <div class="dur-row">
                  <div><input id="dur-h" class="num" type="number" inputmode="numeric" min="0" max="23" value="${esc(state.hours)}" placeholder="0"><label for="dur-h">${esc(t('start.hours'))}</label></div>
                  <div><input id="dur-m" class="num" type="number" inputmode="numeric" min="0" max="59" value="${esc(state.minutes)}" placeholder="0"><label for="dur-m">${esc(t('start.minutes'))}</label></div>
                </div>
                <div class="dialog-actions">
                  <button class="btn" data-action="time-clear">${esc(t('start.noRemind'))}</button>
                  <button class="btn btn--ok" data-action="time-done">${esc(t('start.done'))}</button>
                </div>
              </div>
            </div>
          </div>
        </section>
      </div>
      <section class="card col-side" id="sum-card" aria-labelledby="sum-h"></section>
    </div>
    <section class="card" id="tl-card" aria-labelledby="tl-h"></section>`;
}

function renderNotif() {
  const slot = document.getElementById('notif-slot');
  if (!slot) return;
  const supported = 'Notification' in window;
  const dismissed = store.get('tt_notif_dismissed') === '1';
  if (!supported || dismissed || Notification.permission === 'granted') {
    slot.innerHTML = '';
    return;
  }
  const denied = Notification.permission === 'denied';
  slot.innerHTML = `
    <div class="banner">
      <span class="banner-ico">${svg('bell')}</span>
      <p>${esc(t(denied ? 'notif.denied' : 'notif.ask'))}</p>
      ${denied ? '' : `<button class="btn btn--primary" data-action="notif-enable">${esc(t('notif.enable'))}</button>`}
      <button class="btn" data-action="notif-dismiss">${esc(t('notif.dismiss'))}</button>
    </div>`;
}

function renderCurrent() {
  const card = document.getElementById('cur-card');
  if (!card) return;
  const cur = state.current;
  document.querySelector('.greet--below')?.classList.toggle('is-counting', !!cur);
  // Nothing is being timed: the start card below says what to do, so this card stays out of the way.
  card.hidden = !cur;
  if (!cur) {
    card.innerHTML = '';
    radialOpen = false;
    return;
  }
  const info = actInfo(cur.activity);
  const nameHtml = `${icoHtml(info, 'ico--lg')}<span class="cur-title">${esc(info.name)}</span>`;
  card.innerHTML = `
    <div class="cur">
      <div class="ring">
        <svg width="220" height="220" viewBox="0 0 200 200" aria-hidden="true">
          <defs><linearGradient id="ring-grad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" data-style="stop-color: var(--gold-hi)"/><stop offset="0.5" data-style="stop-color: var(--gold)"/><stop offset="1" data-style="stop-color: var(--ring-end)"/>
          </linearGradient></defs>
          <circle cx="100" cy="100" r="88" fill="none" stroke-width="14" data-style="stroke: var(--surface2)"/>
          <circle class="prog" id="ring-prog" cx="100" cy="100" r="88" fill="none" stroke-width="14" stroke-linecap="round" transform="rotate(-90 100 100)" stroke-dasharray="0 ${RING}"/>
        </svg>
        <div class="ring-center">
          <span class="ring-time" id="elapsed">00:00:00</span>
          <span class="ring-cap" id="ring-cap"></span>
        </div>
      </div>
      <div class="cur-info">
        <div>
          <button class="cur-name cur-name--btn" data-action="relabel" data-id="${esc(cur.id)}" aria-label="${esc(t('cur.relabel'))}: ${esc(info.name)}">${nameHtml}<span class="cur-edit">${svg('pencil')}</span></button>
          <div class="card-sub">${esc(t('cur.startedAt', { time: clock(Date.parse(cur.startedAt)) }))}</div>
        </div>
        ${cur.targetSeconds
          ? `<p class="cur-remain">${esc(t('cur.target', { target: formatDuration(cur.targetSeconds / 60) }))} · <strong id="remain"></strong></p>`
          : `<p class="muted">${esc(t('cur.noTarget'))}</p>`}
        <div class="reminder" id="reminder" role="status" hidden>
          ${svg('bell', 'data-style="flex:none;margin-top:2px"')}
          <div><strong>${esc(t('cur.reminderTitle'))}</strong><p>${esc(t('cur.reminderBody'))}</p></div>
        </div>
      </div>
      <div class="cur-actions">
        <button class="icon-bare" data-action="stop" aria-label="${esc(t('cur.stop'))}" title="${esc(t('cur.stop'))}"><svg width="26" height="26" viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="3.5" fill="currentColor"/></svg></button>
        <div class="radial-wrap">
          <button class="icon-bare icon-bare--accent radial-plus" id="plus-btn" data-action="new-activity" aria-haspopup="menu" aria-expanded="false" aria-controls="radial" aria-label="${esc(t('cur.new'))}" title="${esc(t('cur.new'))}"><svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" aria-hidden="true"><path d="M12 4v16M4 12h16"/></svg></button>
          <div class="radial" id="radial" role="menu" aria-label="${esc(t('radial.label'))}" hidden></div>
        </div>
      </div>
    </div>`;
  radialOpen = false;
  tickCurrent();
}

// ---------- radial menu ----------

// The first press on + fans the other activities out around it as icons: a
// press on an icon switches to that activity at once, a second press on +
// opens the full start card as before. Built-in activities come first, and the
// ring holds RADIAL_MAX icons; the rest stay behind the +.
let radialOpen = false;

function openRadial() {
  const el = document.getElementById('radial');
  const cur = state.current;
  if (!el || !cur) return;
  const items = pickable().filter((a) => a.key !== cur.activity).slice(0, RADIAL_MAX);
  const r = Math.max(78, Math.round((items.length * 58) / (2 * Math.PI)));
  const size = 2 * (r + 34);
  // The glass: a layer that bends what is behind it, a faint tint, a shine on the rim, then the icons.
  el.innerHTML = `<div class="radial-fx"></div><div class="radial-tint"></div><div class="radial-shine"></div>${items
    .map((a, i) => {
      const angle = (i / items.length) * 2 * Math.PI;
      const left = size / 2 + r * Math.sin(angle) - 22;
      const top = size / 2 - r * Math.cos(angle) - 22;
      const label = t('radial.switch', { name: a.name });
      return `<button class="radial-item" role="menuitem" data-action="radial-pick" data-key="${esc(a.key)}" aria-label="${esc(label)}" title="${esc(a.name)}" data-style="left:${left.toFixed(1)}px;top:${top.toFixed(1)}px;--c:${esc(a.color)};--ink:${esc(a.ink)}">${svg(a.icon)}</button>`;
    })
    .join('')}`;
  el.style.setProperty('--size', `${size}px`);
  el.hidden = false;
  radialOpen = true;
  const plus = document.getElementById('plus-btn');
  plus.setAttribute('aria-expanded', 'true');
  plus.setAttribute('aria-label', t('radial.all'));
  plus.title = t('radial.all');
}

function closeRadial(focusButton = true) {
  if (!radialOpen) return;
  radialOpen = false;
  const el = document.getElementById('radial');
  if (el) {
    el.hidden = true;
    el.innerHTML = '';
  }
  const plus = document.getElementById('plus-btn');
  if (!plus) return;
  plus.setAttribute('aria-expanded', 'false');
  plus.setAttribute('aria-label', t('cur.new'));
  plus.title = t('cur.new');
  if (focusButton) plus.focus();
}

// A press on an icon: switch now, with no reminder, as the ring has nowhere to set one.
async function radialPick(key) {
  closeRadial(false);
  closeTimePop(false);
  await api('POST', '/api/sessions/start', { activity: key, targetMinutes: null });
  state.picked = null;
  state.pickerOpen = false;
  state.day = startOfDay(new Date(now()));
  await refresh();
  document.getElementById('plus-btn')?.focus({ preventScroll: true });
}

function renderChips() {
  const el = document.getElementById('chips');
  if (!el) return;
  const list = pickable();
  if (state.picked && !list.some((a) => a.key === state.picked)) state.picked = null;
  el.innerHTML = list
    .map((a) => `<button class="chip" data-action="pick" data-key="${esc(a.key)}" aria-pressed="${state.picked === a.key}">${icoHtml(a)}<span>${esc(a.name)}</span></button>`)
    .join('');
  renderStartButton();
}

// The clock button and the start button appear only once an activity is picked.
function renderStartButton() {
  const btn = document.getElementById('start-btn');
  if (!btn) return;
  document.getElementById('start-go').hidden = !state.picked;
  if (!state.picked) return closeTimePop(false);
  const play = `<svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5c0-.8.9-1.3 1.6-.9l11 7a1.1 1.1 0 0 1 0 1.8l-11 7c-.7.4-1.6-.1-1.6-.9z" fill="currentColor"/></svg>`;
  const name = actInfo(state.picked).name;
  btn.innerHTML = `${play}<span>${esc(t(state.current ? 'start.switch' : 'start.go', { name }))}</span>`;
  renderTimeButton();
}

function reminderMinutes() {
  const h = parseInt(state.hours || '0', 10) || 0;
  const m = parseInt(state.minutes || '0', 10) || 0;
  return Math.min(24 * 60, Math.max(0, h * 60 + m));
}

// The clock icon turns purple once a reminder time is set.
function renderTimeButton() {
  const btn = document.getElementById('time-btn');
  if (!btn) return;
  const total = reminderMinutes();
  btn.classList.toggle('icon-bare--accent', total > 0);
  btn.setAttribute('aria-label', total ? t('start.timeSet', { d: formatDuration(total) }) : t('start.timeNone'));
  btn.title = total ? t('start.timeTitle', { d: formatDuration(total) }) : t('start.time');
}

function closeTimePop(focusButton = true) {
  const pop = document.getElementById('time-pop');
  if (!pop || pop.hidden) return;
  pop.hidden = true;
  const btn = document.getElementById('time-btn');
  btn?.setAttribute('aria-expanded', 'false');
  if (focusButton) btn?.focus();
}

function toggleTimePop() {
  const pop = document.getElementById('time-pop');
  if (!pop) return;
  if (!pop.hidden) return closeTimePop();
  pop.hidden = false;
  document.getElementById('time-btn').setAttribute('aria-expanded', 'true');
  document.getElementById('dur-h').focus();
}

// While a timer runs the start card stays hidden until + on the timer card
// opens it; with nothing running it is the only way forward, so it always shows.
function renderStartCard() {
  const card = document.getElementById('start-card');
  if (!card) return;
  if (!state.current) state.pickerOpen = false;
  const open = !state.current || state.pickerOpen;
  if (!open) closeTimePop(false);
  card.hidden = !open;
  document.getElementById('picker-close').hidden = !state.current;
}

function scrollToCard(id) {
  const smooth = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  document.getElementById(id)?.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto', block: 'start' });
}

// Sessions of the viewed day, clipped to it; the running one ends at "now".
function daySegments() {
  const dayStart = state.day.getTime();
  const dayEnd = addDays(state.day, 1).getTime();
  const n = now();
  const list = [...state.sessions];
  const cur = state.current;
  if (cur && !list.some((s) => s.id === cur.id) && Date.parse(cur.startedAt) < dayEnd && n > dayStart) list.push(cur);
  return list
    .map((s) => {
      const start = Date.parse(s.startedAt);
      const end = s.endedAt ? Date.parse(s.endedAt) : n;
      return { ...s, start, end, from: Math.max(dayStart, start), to: Math.min(dayEnd, end), live: !s.endedAt };
    })
    .filter((s) => s.to > s.from)
    .sort((a, b) => a.from - b.from);
}

function renderSummary() {
  const card = document.getElementById('sum-card');
  if (!card) return;
  const segs = daySegments();
  const totals = new Map();
  for (const s of segs) totals.set(s.activity, (totals.get(s.activity) || 0) + (s.to - s.from) / 60e3);
  const tracked = [...totals.values()].reduce((a, b) => a + b, 0);
  const max = Math.max(1, ...totals.values());
  const dayStart = state.day.getTime();
  const span = Math.max(0, Math.min(now(), addDays(state.day, 1).getTime()) - dayStart) / 60e3;
  const rows = [...totals.entries()]
    .filter(([, m]) => m >= 1)
    .sort((a, b) => b[1] - a[1])
    .map(([key, m]) => {
      const info = actInfo(key);
      return `<li class="sum-row">${icoHtml(info)}<div class="sum-body"><div class="sum-top"><span>${esc(info.name)}</span><span>${esc(formatDuration(m))}</span></div><div class="bar"><div data-style="--c:${esc(info.color)};width:${(m / max) * 100}%"></div></div></div></li>`;
    })
    .join('');
  card.innerHTML = `
    <div>
      <h2 id="sum-h">${esc(t('sum.heading'))}</h2>
      <p class="card-sub">${esc(t('sum.tracked', { d: formatDuration(tracked) }))}</p>
    </div>
    ${rows ? `<ul class="sum-list">${rows}</ul>` : `<p class="muted">${esc(t('sum.empty'))}</p>`}
    <p class="note-box">${esc(t('sum.untracked', { d: formatDuration(Math.max(0, span - tracked)) }))}</p>`;
}

// The day as a 24-hour dial: midnight at the bottom, midday at the top, time
// running clockwise (06:00 on the left, 18:00 on the right).
const DIAL = { c: 150, r: 100, w: 26, label: 134 };

function dialPoint(minutes, r) {
  const a = (minutes / 1440) * 2 * Math.PI + Math.PI;
  return [DIAL.c + r * Math.sin(a), DIAL.c - r * Math.cos(a)];
}

function dialArc(from, to) {
  const [x1, y1] = dialPoint(from, DIAL.r);
  const [x2, y2] = dialPoint(to, DIAL.r);
  return `M${x1.toFixed(2)} ${y1.toFixed(2)}A${DIAL.r} ${DIAL.r} 0 ${to - from > 720 ? 1 : 0} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

function dialHtml(segs, dayStart, today, trackedMin) {
  const min = (ms) => Math.min(1440, Math.max(0, (ms - dayStart) / 60e3));
  const arcs = segs
    .map((s) => {
      const info = actInfo(s.activity);
      const from = min(s.from);
      // At least ~1° so a short entry still shows; a whole day stops just short of closing the circle.
      const to = Math.min(from + 1439.5, Math.max(min(s.to), from + 4));
      return `<path class="dial-seg" d="${dialArc(from, to)}" stroke="${esc(info.color)}"><title>${esc(`${info.name} ${clock(s.from)}–${clock(s.to)}`)}</title></path>`;
    })
    .join('');
  const ticks = [0, 3, 6, 9, 12, 15, 18, 21]
    .map((h) => {
      const [x1, y1] = dialPoint(h * 60, DIAL.r - DIAL.w / 2 - 4);
      const [x2, y2] = dialPoint(h * 60, DIAL.r - DIAL.w / 2 - (h % 6 ? 8 : 12));
      return `<line class="dial-tick" x1="${x1.toFixed(2)}" y1="${y1.toFixed(2)}" x2="${x2.toFixed(2)}" y2="${y2.toFixed(2)}"/>`;
    })
    .join('');
  const labels = [0, 6, 12, 18]
    .map((h) => {
      const [x, y] = dialPoint(h * 60, DIAL.label);
      return `<text class="dial-label" x="${x.toFixed(2)}" y="${y.toFixed(2)}">${pad(h)}:00</text>`;
    })
    .join('');
  let nowMark = '';
  if (today) {
    const m = min(now());
    const [x1, y1] = dialPoint(m, DIAL.r - DIAL.w / 2 - 5);
    const [x2, y2] = dialPoint(m, DIAL.r + DIAL.w / 2 + 5);
    nowMark = `<line class="dial-now" x1="${x1.toFixed(2)}" y1="${y1.toFixed(2)}" x2="${x2.toFixed(2)}" y2="${y2.toFixed(2)}"/>`;
  }
  return `
    <svg class="dial" viewBox="0 0 300 300" role="img" aria-label="${esc(t('tl.dialLabel'))}">
      <circle class="dial-track" cx="${DIAL.c}" cy="${DIAL.c}" r="${DIAL.r}"/>
      ${arcs}${ticks}${nowMark}${labels}
      <text class="dial-cap" x="${DIAL.c}" y="${DIAL.c - 14}">${esc(t('tl.tracked'))}</text>
      <text class="dial-total" x="${DIAL.c}" y="${DIAL.c + 16}">${esc(formatDuration(trackedMin))}</text>
    </svg>`;
}

function renderTimeline() {
  const card = document.getElementById('tl-card');
  if (!card) return;
  const dayStart = state.day.getTime();
  const segs = daySegments();
  const today = isToday();
  const trackedMin = segs.reduce((sum, s) => sum + (s.to - s.from) / 60e3, 0);
  const entries = segs
    .slice()
    .reverse()
    .map((s) => {
      const info = actInfo(s.activity);
      const range = `${clock(s.start)}–${s.live ? t('tl.now') : clock(s.end)}`;
      const name = info.deleted ? `${info.name} ${t('act.deleted')}` : info.name;
      const open = entryMenu?.id === s.id;
      const more = `<button class="entry-more" data-action="entry-menu" data-id="${esc(s.id)}" aria-haspopup="menu" aria-expanded="${open}" aria-label="${esc(t('tl.more'))}: ${esc(info.name)} ${esc(range)}">${KEBAB}</button>`;
      return `<li class="entry">${icoHtml(info, 'ico--sm')}<span class="entry-name">${esc(name)}</span>${s.live ? `<span class="tag">${esc(t('tl.live'))}</span>` : ''}<span class="entry-time">${esc(range)}</span><span class="entry-dur">${esc(formatDuration((s.end - s.start) / 60e3))}</span>${more}</li>`;
    })
    .join('');
  card.innerHTML = `
    <div class="card-head">
      <h2 id="tl-h">${esc(t('tl.heading'))}</h2>
      <div class="day-nav">
        <button class="icon-btn" data-action="day-prev" aria-label="${esc(t('tl.prev'))}">${svg('left')}</button>
        <span class="day-label">${esc(today ? t('nav.today') : longDate(state.day))}</span>
        <button class="icon-btn" data-action="day-next" aria-label="${esc(t('tl.next'))}" ${today ? 'disabled' : ''}>${svg('right')}</button>
        ${today ? '' : `<button class="btn btn--link" data-action="day-today">${esc(t('tl.today'))}</button>`}
      </div>
    </div>
    <div class="tl-body">
      ${dialHtml(segs, dayStart, today, trackedMin)}
      ${entries ? `<ol class="entries">${entries}</ol>` : `<p class="muted">${esc(t('tl.empty'))}</p>`}
    </div>`;
  if (entryMenu && !entryButton(entryMenu.id)) closeEntryMenu(false);
}

// ---------- entry menu ----------

// The ⋮ button on a timeline entry opens a small menu: change activity, and
// delete for entries that have ended. It hangs off <body> so the timeline can
// redraw underneath without closing it.
const KEBAB = `<svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>`;
const CLOCK = `<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="13" r="8"/><path d="M12 9v4l2.5 1.5M9 2.5h6"/></svg>`;
let entryMenu = null; // { id, el }

const entryButton = (id) => [...document.querySelectorAll('[data-action="entry-menu"]')].find((b) => b.dataset.id === id);

function closeEntryMenu(focusButton = true) {
  if (!entryMenu) return;
  const { id, el } = entryMenu;
  entryMenu = null;
  el.remove();
  const btn = entryButton(id);
  btn?.setAttribute('aria-expanded', 'false');
  if (focusButton) btn?.focus();
}

function openEntryMenu(btn) {
  const id = btn.dataset.id;
  const wasOpen = entryMenu?.id === id;
  closeEntryMenu(false);
  if (wasOpen) return;
  const session = state.sessions.find((s) => s.id === id) || (state.current?.id === id ? state.current : null);
  if (!session) return;
  const el = document.createElement('div');
  el.className = 'menu';
  el.id = 'entry-menu';
  el.setAttribute('role', 'menu');
  el.innerHTML = `
    <button class="menu-item" role="menuitem" data-action="relabel" data-id="${esc(id)}">${svg('pencil')}${esc(t('tl.edit'))}</button>
    ${session.endedAt ? `<button class="menu-item menu-item--danger" role="menuitem" data-action="delete-session" data-id="${esc(id)}">${svg('trash')}${esc(t('tl.menuDelete'))}</button>` : ''}`;
  document.body.append(el);
  entryMenu = { id, el };
  btn.setAttribute('aria-expanded', 'true');
  // Below the button, right edges lined up; above it when the bottom (or the
  // phone's bottom bar) would cut it off.
  const r = btn.getBoundingClientRect();
  const bar = document.querySelector('.shell > .hdr');
  const floor = bar && getComputedStyle(bar).position === 'fixed' ? bar.getBoundingClientRect().top : window.innerHeight;
  const below = r.bottom + 6 + el.offsetHeight <= floor - 8;
  el.style.top = `${window.scrollY + (below ? r.bottom + 6 : r.top - 6 - el.offsetHeight)}px`;
  el.style.left = `${window.scrollX + Math.max(8, r.right - el.offsetWidth)}px`;
  el.querySelector('.menu-item').focus();
}

// ---------- account menu (phones) ----------

// On phones the avatar opens this menu in place of the tabs and the language
// switch. Like the entry menu it hangs off <body>, here above the bottom bar.
let acctMenu = null;

function closeAcctMenu(focusButton = true) {
  if (!acctMenu) return;
  acctMenu.remove();
  acctMenu = null;
  const btn = document.getElementById('acct-btn');
  btn?.setAttribute('aria-expanded', 'false');
  if (focusButton) btn?.focus();
}

function openAcctMenu(btn) {
  if (acctMenu) return closeAcctMenu();
  const pages = [['today', '#/', 'today'], ['activities', '#/activities', 'grid'], ['account', '#/account', 'user']];
  const el = document.createElement('div');
  el.className = 'menu acct-menu';
  el.id = 'acct-menu';
  el.setAttribute('role', 'dialog');
  el.setAttribute('aria-label', t('nav.menu'));
  el.innerHTML = `
    <nav aria-label="${esc(t('nav.label'))}">
      ${pages.map(([r, href, icon]) => `<a class="menu-item" href="${href}" ${state.route === r ? 'aria-current="page"' : ''}>${svg(icon)}${esc(t('nav.' + r))}</a>`).join('')}
    </nav>
    <div class="acct-lang">
      <span id="acct-lang-h">${esc(t('lang.label'))}</span>
      <div class="seg" role="group" aria-labelledby="acct-lang-h">
        <button data-action="lang" data-lang="th" aria-pressed="${getLang() === 'th'}">TH</button>
        <button data-action="lang" data-lang="en" aria-pressed="${getLang() === 'en'}">EN</button>
      </div>
    </div>`;
  document.body.append(el);
  acctMenu = el;
  btn.setAttribute('aria-expanded', 'true');
  const r = btn.getBoundingClientRect();
  el.style.bottom = `${window.innerHeight - r.top + 10}px`;
  el.style.right = `${Math.max(8, window.innerWidth - r.right)}px`;
  el.querySelector('a').focus();
}

// Updates only the parts that move every second.
function tickCurrent() {
  const nowText = t('today.now', { time: clock(now()) });
  document.querySelectorAll('.now-label').forEach((el) => { el.textContent = nowText; });
  const cur = state.current;
  if (!cur) {
    document.title = 'Time Traveler';
    return;
  }
  const el = elapsedSec(cur);
  const over = !!cur.targetSeconds && el >= cur.targetSeconds;
  const elapsed = document.getElementById('elapsed');
  if (elapsed) elapsed.textContent = hms(el);
  const prog = document.getElementById('ring-prog');
  if (prog) {
    const fraction = cur.targetSeconds ? Math.min(1, el / cur.targetSeconds) : (el % 3600) / 3600;
    prog.setAttribute('stroke-dasharray', `${RING * fraction} ${RING}`);
    prog.style.stroke = over ? 'var(--purple)' : 'url(#ring-grad)';
  }
  const cap = document.getElementById('ring-cap');
  if (cap) cap.textContent = t(cur.targetSeconds ? (over ? 'cur.ring.over' : 'cur.ring.elapsed') : 'cur.ring.free');
  const remain = document.getElementById('remain');
  if (remain) {
    const left = (cur.targetSeconds - el) / 60;
    remain.textContent = over ? t('cur.over', { d: formatDuration(Math.ceil(-left)) }) : t('cur.left', { d: formatDuration(Math.ceil(left)) });
    remain.classList.toggle('over', over);
  }
  const reminder = document.getElementById('reminder');
  if (reminder) reminder.hidden = !over;
  document.title = `${hms(el)} · ${actInfo(cur.activity).name} — Time Traveler`;
}

function renderLive() {
  if (state.route !== 'today') return;
  renderCurrent();
  renderStartCard();
  renderChips();
  renderIconButton();
  renderSummary();
  renderTimeline();
}

// ---------- activities page ----------

function activitiesHtml() {
  const builtins = BUILTIN.map((a) => {
    const info = actInfo(a.key);
    return `<li class="act-row">${icoHtml(info)}<span class="entry-name">${esc(info.name)}</span></li>`;
  }).join('');
  const customs = state.custom.filter((a) => !a.deleted).map((a) => {
    const info = actInfo(a.key);
    return `<li class="act-row">${icoHtml(info)}<span class="entry-name">${esc(info.name)}</span>
      <button class="btn btn--danger-outline" data-action="delete-custom" data-key="${esc(a.key)}">${esc(t('acts.delete'))}</button></li>`;
  }).join('');
  const swatches = CUSTOM_COLORS.map((c) => `<button class="swatch" data-style="--c:${c}" data-action="swatch" data-color="${c}" aria-pressed="${state.newColor === c}" aria-label="${esc(t('acts.color'))} ${c}"></button>`).join('');
  return `
    <section class="card" aria-labelledby="acts-h">
      <h1 id="acts-h" data-style="font-size:28px">${esc(t('acts.heading'))}</h1>
      <div class="add-row">
        ${iconPickHtml(true)}
        <div class="field">
          <label for="new-name">${esc(t('start.newLabel'))}</label>
          <input id="new-name" class="input" type="text" maxlength="40" autocomplete="off" placeholder="${esc(t('start.newPlaceholder'))}" value="${esc(state.newName)}">
        </div>
        <button class="btn btn--soft" data-action="add-custom">${svg('plus')}${esc(t('start.add'))}</button>
      </div>
      <div role="group" aria-label="${esc(t('acts.color'))}" class="swatches">${swatches}</div>
    </section>
    <section class="card" aria-labelledby="custom-h">
      <div><h2 id="custom-h">${esc(t('acts.custom'))}</h2><p class="card-sub">${esc(t('acts.customNote'))}</p></div>
      ${customs ? `<ul class="act-list">${customs}</ul>` : `<p class="muted">${esc(t('acts.noCustom'))}</p>`}
    </section>
    <section class="card" aria-labelledby="builtin-h">
      <div><h2 id="builtin-h">${esc(t('acts.builtins'))}</h2><p class="card-sub">${esc(t('acts.builtinsNote'))}</p></div>
      <ul class="act-list">${builtins}</ul>
    </section>`;
}

// ---------- account page ----------

function accountHtml() {
  const me = state.me;
  return `
    <section class="card" aria-labelledby="acct-h">
      <h1 id="acct-h" data-style="font-size:28px">${esc(t('acct.heading'))}</h1>
      <div><p class="card-sub">${esc(t('acct.signedInAs'))}</p><p data-style="font-family:var(--font-head);font-size:20px">${esc(me.name || me.email || '—')}</p>${me.name && me.email ? `<p class="muted">${esc(me.email)}</p>` : ''}</div>
      <div class="btn-row"><button class="btn" data-action="logout">${esc(t('acct.logout'))}</button></div>
    </section>
    <section class="card" aria-labelledby="del-h">
      <h2 id="del-h">${esc(t('acct.deleteHeading'))}</h2>
      <p class="muted">${esc(t('acct.deleteNote'))}</p>
      <div class="btn-row"><button class="btn btn--danger-outline" data-action="delete-account">${esc(t('acct.delete'))}</button></div>
    </section>`;
}

// ---------- render ----------

function routeFromHash() {
  const h = location.hash.replace(/^#\/?/, '');
  return h === 'activities' || h === 'account' ? h : 'today';
}

function render() {
  closeEntryMenu(false);
  closeAcctMenu(false);
  if (!state.me) return renderLogin();
  state.route = routeFromHash();
  const body = state.route === 'activities' ? activitiesHtml() : state.route === 'account' ? accountHtml() : todayHtml();
  document.getElementById('app').innerHTML = `<div class="wrap shell">${headerHtml(true)}<main class="wrap" data-style="padding:0">${body}</main></div>`;
  if (state.route === 'today') {
    renderNotif();
    renderLive();
  }
}

// ---------- actions ----------

async function addCustom() {
  const name = state.newName.trim();
  if (!name) {
    document.getElementById('new-name')?.focus();
    return;
  }
  const body = { name, icon: state.newIcon };
  if (state.route === 'activities') body.color = state.newColor;
  const { activity } = await api('POST', '/api/activities', body);
  state.newName = '';
  state.newIcon = CUSTOM_ICONS[0];
  state.custom.push(activity);
  state.picked = activity.key;
  render();
}

function clearReminder() {
  state.hours = '';
  state.minutes = '';
  const hEl = document.getElementById('dur-h');
  const mEl = document.getElementById('dur-m');
  if (hEl) hEl.value = '';
  if (mEl) mEl.value = '';
  renderTimeButton();
}

// Starting (or switching) hides the start card and goes back to the timer card.
async function start() {
  if (!state.picked) return;
  closeTimePop(false);
  const total = reminderMinutes();
  await api('POST', '/api/sessions/start', { activity: state.picked, targetMinutes: total > 0 ? total : null });
  state.picked = null;
  state.pickerOpen = false;
  state.day = startOfDay(new Date(now()));
  clearReminder();
  await refresh();
  scrollToCard('cur-card');
  document.querySelector('[data-action="new-activity"]')?.focus({ preventScroll: true });
}

async function relabel(id) {
  const session = state.sessions.find((s) => s.id === id) || (state.current?.id === id ? state.current : null);
  if (!session) return;
  const options = pickable();
  if (!options.some((a) => a.key === session.activity)) options.unshift(actInfo(session.activity));
  const value = await openDialog(`
    <form method="dialog">
      <h2 id="dlg-title">${esc(t('relabel.title'))}</h2>
      <p class="muted">${esc(t('relabel.note'))}</p>
      <div class="radio-list">
        ${options.map((a) => `<label><input type="radio" name="act" value="${esc(a.key)}" ${a.key === session.activity ? 'checked' : ''} ${a.deleted ? 'disabled' : ''}>${icoHtml(a, 'ico--sm')}<span>${esc(a.name)}</span></label>`).join('')}
      </div>
      <div class="dialog-actions">
        <button class="btn" value="cancel">${esc(t('cancel'))}</button>
        <button class="btn btn--primary" value="save" id="relabel-save">${esc(t('relabel.save'))}</button>
      </div>
    </form>`);
  if (value !== 'save') return;
  const chosen = document.querySelector('#dlg input[name="act"]:checked')?.value;
  if (!chosen || chosen === session.activity) return;
  await api('PATCH', `/api/sessions/${encodeURIComponent(id)}`, { activity: chosen });
  await refresh();
}

const actions = {
  lang: (el) => switchLang(el.dataset.lang),
  theme: () => toggleTheme(),
  'dev-login': async () => {
    try {
      await api('POST', '/auth/dev-login');
    } catch (err) {
      if (err.message !== 'disabled') throw err;
      history.replaceState(null, '', '/?login_error=disabled');
    }
    await boot();
  },
  pick: (el) => {
    state.picked = el.dataset.key;
    document.querySelectorAll('#chips .chip').forEach((c) => c.setAttribute('aria-pressed', String(c.dataset.key === state.picked)));
    renderStartButton();
  },
  'add-custom': addCustom,
  start,
  stop: async () => {
    await api('POST', '/api/sessions/stop');
    state.pickerOpen = false;
    await refresh();
  },
  time: () => toggleTimePop(),
  'time-done': () => closeTimePop(),
  'time-clear': () => {
    clearReminder();
    closeTimePop();
  },
  'acct-menu': (el) => openAcctMenu(el),
  'entry-menu': (el) => openEntryMenu(el),
  relabel: (el) => relabel(el.dataset.id),
  'delete-session': async (el) => {
    const session = state.sessions.find((s) => s.id === el.dataset.id);
    if (!session || !session.endedAt) return;
    const info = actInfo(session.activity);
    const range = `${clock(Date.parse(session.startedAt))}–${clock(Date.parse(session.endedAt))}`;
    const ok = await confirmDialog({
      title: t('tl.deleteTitle'),
      body: t('tl.deleteBody', { name: info.name, range }),
      confirm: t('tl.delete'),
      danger: true,
    });
    if (!ok) return;
    await api('DELETE', `/api/sessions/${encodeURIComponent(session.id)}`);
    await refresh();
  },
  'radial-pick': (el) => radialPick(el.dataset.key),
  'new-activity': () => {
    if (!radialOpen) return openRadial();
    closeRadial(false);
    state.pickerOpen = true;
    renderStartCard();
    scrollToCard('start-card');
    document.querySelector('#chips .chip')?.focus({ preventScroll: true });
  },
  'close-picker': () => {
    state.pickerOpen = false;
    renderStartCard();
    scrollToCard('cur-card');
    document.querySelector('[data-action="new-activity"]')?.focus({ preventScroll: true });
  },
  'day-prev': async () => {
    state.day = addDays(state.day, -1);
    await loadDay();
    renderLive();
  },
  'day-next': async () => {
    if (isToday()) return;
    state.day = addDays(state.day, 1);
    await loadDay();
    renderLive();
  },
  'day-today': async () => {
    state.day = startOfDay(new Date(now()));
    await loadDay();
    renderLive();
  },
  'notif-enable': async () => {
    await Notification.requestPermission();
    renderNotif();
  },
  'notif-dismiss': () => {
    store.set('tt_notif_dismissed', '1');
    renderNotif();
  },
  swatch: (el) => {
    state.newColor = el.dataset.color;
    document.querySelectorAll('.swatch').forEach((s) => s.setAttribute('aria-pressed', String(s.dataset.color === state.newColor)));
    renderIconButton();
  },
  icons: () => toggleIconPop(),
  icon: (el) => {
    state.newIcon = el.dataset.icon;
    renderIconButton();
    closeIconPop();
  },
  'delete-custom': async (el) => {
    const info = actInfo(el.dataset.key);
    const ok = await confirmDialog({ title: t('acts.deleteTitle', { name: info.name }), body: t('acts.deleteBody'), confirm: t('acts.delete'), danger: true });
    if (!ok) return;
    await api('DELETE', `/api/activities/${encodeURIComponent(el.dataset.key)}`);
    await loadState();
    render();
  },
  logout: async () => {
    await api('POST', '/auth/logout');
    state.me = null;
    render();
  },
  'delete-account': async () => {
    const ok = await confirmDialog({ title: t('acct.deleteTitle'), body: t('acct.deleteBody'), confirm: t('acct.deleteConfirm'), danger: true });
    if (!ok) return;
    await api('DELETE', '/api/account');
    state.me = null;
    render();
  },
};

document.addEventListener('click', (event) => {
  unlockAudio();
  const el = event.target.closest('[data-action]');
  // Any click outside the menu closes it; picking an item closes it before acting.
  if (entryMenu && !event.target.closest('#entry-menu, [data-action="entry-menu"]')) closeEntryMenu(false);
  if (el?.closest('#entry-menu')) closeEntryMenu(true);
  if (acctMenu && (!event.target.closest('#acct-menu, #acct-btn') || event.target.closest('#acct-menu a'))) closeAcctMenu(false);
  const pop = document.getElementById('time-pop');
  if (pop && !pop.hidden && !event.target.closest('#time-pop, #time-btn')) closeTimePop(false);
  if (!event.target.closest('.icon-pick')) closeIconPop(false);
  if (radialOpen && !event.target.closest('.radial-wrap')) closeRadial(false);
  if (!el || el.disabled) return;
  const fn = actions[el.dataset.action];
  if (!fn) return;
  event.preventDefault();
  Promise.resolve(fn(el)).catch(failed);
});

document.addEventListener('input', (event) => {
  const { id, value } = event.target;
  if (id === 'new-name') state.newName = value;
  if (id === 'dur-h') state.hours = value;
  if (id === 'dur-m') state.minutes = value;
  if (id === 'dur-h' || id === 'dur-m') renderTimeButton();
});

// Tabbing out of the account menu closes it.
document.addEventListener('focusout', (event) => {
  const to = event.relatedTarget;
  if (acctMenu && to && !acctMenu.contains(to) && to.id !== 'acct-btn') closeAcctMenu(false);
});

document.addEventListener('keydown', (event) => {
  if (entryMenu) {
    const items = [...entryMenu.el.querySelectorAll('.menu-item')];
    const at = items.indexOf(document.activeElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      closeEntryMenu(true);
    } else if (event.key === 'Tab') {
      closeEntryMenu(true);
    } else if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && at >= 0) {
      event.preventDefault();
      items[(at + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length].focus();
    }
  }
  if (acctMenu && event.key === 'Escape') {
    event.preventDefault();
    closeAcctMenu(true);
  }
  if (radialOpen) {
    const items = [...document.querySelectorAll('.radial-item')];
    const at = items.indexOf(document.activeElement);
    const forward = event.key === 'ArrowRight' || event.key === 'ArrowDown';
    if (event.key === 'Escape') {
      event.preventDefault();
      closeRadial(true);
    } else if ((forward || event.key === 'ArrowLeft' || event.key === 'ArrowUp') && items.length) {
      // Arrows walk the ring, from the + onto the first icon.
      event.preventDefault();
      items[at < 0 ? 0 : (at + (forward ? 1 : items.length - 1)) % items.length].focus();
    }
  }
  if (event.key === 'Escape' && event.target.closest?.('.icon-pick')) {
    event.preventDefault();
    closeIconPop(true);
  }
  if (event.target.closest?.('#time-pop') && (event.key === 'Escape' || (event.key === 'Enter' && event.target.classList.contains('num')))) {
    event.preventDefault();
    closeTimePop(true);
  }
  if (event.key === 'Enter' && event.target.id === 'new-name') {
    event.preventDefault();
    addCustom().catch(failed);
  }
});

window.addEventListener('hashchange', () => {
  if (state.me) render();
});

// Only a width change moves the entries; phones resize the height while scrolling.
let lastWidth = window.innerWidth;
window.addEventListener('resize', () => {
  if (window.innerWidth === lastWidth) return;
  lastWidth = window.innerWidth;
  closeEntryMenu(false);
  closeAcctMenu(false);
});

matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  syncThemeColor();
  if (!document.documentElement.hasAttribute('data-theme')) render();
});

// ---------- timers ----------

let lastSlow = 0;
setInterval(() => {
  if (!state.me) return;
  checkReminder();
  if (state.route !== 'today') return;
  tickCurrent();
  // Totals and the timeline move by the minute; redraw them every 30 seconds.
  if (Date.now() - lastSlow > 30e3) {
    lastSlow = Date.now();
    if (state.current && isToday()) {
      renderSummary();
      renderTimeline();
    }
  }
}, 1000);

// Another device may have switched activity: re-read on return and every minute.
async function quietRefresh() {
  if (!state.me || document.hidden) return;
  try {
    const before = JSON.stringify([state.current, state.custom]);
    if (!isToday() && state.day > startOfDay(new Date(now()))) state.day = startOfDay(new Date(now()));
    await Promise.all([loadState(), loadDay()]);
    if (JSON.stringify([state.current, state.custom]) !== before) {
      if (state.route === 'today') renderLive();
      else render();
    }
  } catch {
    // The next tick tries again.
  }
}
setInterval(quietRefresh, 60e3);
document.addEventListener('visibilitychange', quietRefresh);

// ---------- boot ----------

async function boot() {
  try {
    state.config = await (await fetch('/api/config')).json();
  } catch {
    // Keep the defaults: no providers.
  }
  try {
    const me = await api('GET', '/api/me');
    csrf = me.csrf;
    state.me = me;
  } catch {
    state.me = null;
  }
  render();
  if (state.me) await refresh().catch(failed);
}

const savedLang = store.get('tt_lang');
setLang(savedLang || (navigator.language?.toLowerCase().startsWith('th') ? 'th' : 'en'));
syncThemeColor();
boot();
