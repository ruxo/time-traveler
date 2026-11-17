// Admin console: first-admin setup, sign-in and user management (Thai only).

const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const dateTime = (iso) =>
  iso ? new Intl.DateTimeFormat('th-TH', { day: 'numeric', month: 'short', year: '2-digit', hour: '2-digit', minute: '2-digit' }).format(new Date(iso)) : '—';

const state = { status: null, users: [], filter: '' };
let setupToken = null;

// ---------- server ----------

async function api(method, path, body) {
  const headers = {};
  if (method !== 'GET') headers['x-csrf-token'] = state.status?.csrf || '';
  if (body !== undefined) headers['content-type'] = 'application/json';
  let res;
  try {
    res = await fetch('/admin/api' + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
  } catch {
    toast('ติดต่อเซิร์ฟเวอร์ไม่ได้ ลองใหม่อีกครั้ง');
    throw new Error('offline');
  }
  const data = await res.json().catch(() => null);
  if (!res.ok) {
    const err = new Error(data?.error || `http_${res.status}`);
    err.status = res.status;
    err.data = data;
    if (res.status === 401 && path !== '/login') {
      await boot();
    }
    throw err;
  }
  return data;
}

// ---------- ui helpers ----------

let toastTimer;
function toast(message) {
  const el = document.getElementById('toast');
  el.textContent = message;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, 4000);
}

function confirmDialog({ title, body, confirm }) {
  const dlg = document.getElementById('dlg');
  dlg.innerHTML = `
    <form method="dialog">
      <h2 id="dlg-title">${esc(title)}</h2>
      <p>${esc(body)}</p>
      <div class="dialog-actions">
        <button class="btn" value="cancel">ยกเลิก</button>
        <button class="btn btn--danger" value="ok">${esc(confirm)}</button>
      </div>
    </form>`;
  dlg.returnValue = '';
  dlg.showModal();
  return new Promise((resolve) => dlg.addEventListener('close', () => resolve(dlg.returnValue === 'ok'), { once: true }));
}

function shell(body, signedIn) {
  return `
    <div class="wrap">
      <header class="hdr">
        <a class="brand" href="/admin/"><img src="/icon.svg" width="44" height="44" alt=""><span class="brand-name">Time Traveler <span class="muted">· หลังบ้าน</span></span></a>
        ${signedIn ? `<div class="hdr-tools"><span class="muted">${esc(state.status.username)}</span><button class="btn" data-action="logout">ออกจากระบบ</button></div>` : ''}
      </header>
      <main class="wrap" data-style="padding:0">${body}</main>
    </div>`;
}

function render(html) {
  document.getElementById('app').innerHTML = html;
}

const passwordRules = 'ชื่อผู้ใช้ 3–32 ตัว ใช้ a–z, 0–9, จุด, ขีดล่าง หรือขีดกลาง · รหัสผ่านอย่างน้อย 10 ตัว';

function credentialsForm({ id, title, intro, submit, confirmPassword }) {
  return `
    <div class="login">
      <section class="card card--hero" aria-labelledby="${id}-h">
        <h1 id="${id}-h">${esc(title)}</h1>
        ${intro ? `<p class="muted">${esc(intro)}</p>` : ''}
        <form id="${id}" class="admin-form" novalidate>
          <div class="field"><label for="${id}-user">ชื่อผู้ใช้</label><input id="${id}-user" name="username" class="input" autocomplete="username" autocapitalize="none" spellcheck="false" required></div>
          <div class="field"><label for="${id}-pass">รหัสผ่าน</label><input id="${id}-pass" name="password" class="input" type="password" autocomplete="${confirmPassword ? 'new-password' : 'current-password'}" required></div>
          ${confirmPassword ? `<div class="field"><label for="${id}-pass2">ยืนยันรหัสผ่าน</label><input id="${id}-pass2" name="password2" class="input" type="password" autocomplete="new-password" required></div><p class="card-sub">${esc(passwordRules)}</p>` : ''}
          <p class="form-error" id="${id}-err" role="alert" hidden></p>
          <button class="btn btn--primary" type="submit">${esc(submit)}</button>
        </form>
      </section>
    </div>`;
}

function showFormError(formId, message) {
  const el = document.getElementById(`${formId}-err`);
  el.textContent = message;
  el.hidden = false;
}

// ---------- views ----------

function renderSetup() {
  if (!setupToken) {
    render(shell(`<div class="login"><section class="card"><h1>ยังไม่มีแอดมิน</h1><p class="muted">เปิดลิงก์ตั้งค่าครั้งแรกที่ได้รับ เพื่อสร้างบัญชีแอดมินคนแรก</p></section></div>`, false));
    return;
  }
  render(shell(credentialsForm({ id: 'setup', title: 'สร้างบัญชีแอดมินคนแรก', intro: 'ลิงก์นี้ใช้ได้ครั้งเดียว ตั้งชื่อผู้ใช้และรหัสผ่านที่จะใช้เข้าหลังบ้าน', submit: 'สร้างบัญชีแอดมิน', confirmPassword: true }), false));
}

function renderLogin() {
  render(shell(credentialsForm({ id: 'login', title: 'เข้าสู่หลังบ้าน', submit: 'เข้าสู่ระบบ' }), false));
}

function userRow(u) {
  const status = u.disabled
    ? `<span class="tag tag--off">ปิดใช้งาน</span>`
    : `<span class="tag">ใช้งานอยู่</span>`;
  return `
    <li class="user-row" data-id="${esc(u.id)}">
      <div class="user-main">
        <div class="user-name">${esc(u.name || '(ไม่มีชื่อ)')} ${status}</div>
        <div class="muted user-email">${esc(u.email || '—')}</div>
        <dl class="user-meta">
          <div><dt>สมัคร</dt><dd>${esc(dateTime(u.createdAt))}</dd></div>
          <div><dt>ใช้ล่าสุด</dt><dd>${esc(dateTime(u.lastActive))}</dd></div>
          <div><dt>รายการ</dt><dd>${u.sessionCount}</dd></div>
        </dl>
      </div>
      <div class="btn-row">
        ${u.disabled
          ? `<button class="btn" data-action="enable" data-id="${esc(u.id)}">เปิดใช้งาน</button>`
          : `<button class="btn" data-action="disable" data-id="${esc(u.id)}">ปิดใช้งาน</button>`}
        <button class="btn btn--danger-outline" data-action="delete" data-id="${esc(u.id)}">ลบ</button>
      </div>
    </li>`;
}

function renderUsers() {
  const q = state.filter.trim().toLowerCase();
  const list = q ? state.users.filter((u) => `${u.name ?? ''} ${u.email ?? ''}`.toLowerCase().includes(q)) : state.users;
  const active = state.users.filter((u) => !u.disabled).length;
  const listEl = document.getElementById('user-list');
  const countEl = document.getElementById('user-count');
  if (listEl && countEl) {
    countEl.textContent = `ทั้งหมด ${state.users.length} คน · ใช้งานอยู่ ${active} · ปิดใช้งาน ${state.users.length - active}`;
    listEl.innerHTML = list.length ? list.map(userRow).join('') : `<li class="muted">${q ? 'ไม่พบผู้ใช้ที่ค้นหา' : 'ยังไม่มีผู้ใช้'}</li>`;
    return;
  }
  render(shell(`
    <section class="card" aria-labelledby="users-h">
      <div class="card-head">
        <div><h1 id="users-h" class="admin-title">ผู้ใช้</h1><p class="card-sub" id="user-count"></p></div>
        <button class="btn" data-action="refresh">โหลดใหม่</button>
      </div>
      <div class="field"><label for="user-filter">ค้นหาชื่อหรืออีเมล</label><input id="user-filter" class="input" type="search" autocomplete="off" value="${esc(state.filter)}"></div>
      <ul class="user-list" id="user-list"></ul>
    </section>`, true));
  renderUsers();
}

async function loadUsers() {
  const { users } = await api('GET', '/users');
  state.users = users;
  renderUsers();
}

function replaceUser(user) {
  state.users = state.users.map((u) => (u.id === user.id ? user : u));
  renderUsers();
}

async function showView() {
  render(shell('', true));
  renderUsers();
  await loadUsers();
}

// ---------- events ----------

document.addEventListener('submit', async (event) => {
  const form = event.target;
  if (form.id !== 'setup' && form.id !== 'login') return;
  event.preventDefault();
  const data = new FormData(form);
  const username = String(data.get('username') || '').trim().toLowerCase();
  const password = String(data.get('password') || '');
  const button = form.querySelector('button[type="submit"]');
  try {
    button.disabled = true;
    if (form.id === 'setup') {
      if (!/^[a-z0-9._-]{3,32}$/.test(username)) return showFormError('setup', 'ชื่อผู้ใช้ไม่ตรงเงื่อนไข');
      if (password.length < 10) return showFormError('setup', 'รหัสผ่านต้องยาวอย่างน้อย 10 ตัว');
      if (password !== String(data.get('password2') || '')) return showFormError('setup', 'รหัสผ่านสองช่องไม่ตรงกัน');
      await api('POST', '/setup', { token: setupToken, username, password });
      setupToken = null;
      history.replaceState(null, '', '/admin/');
    } else {
      await api('POST', '/login', { username, password });
    }
    await boot();
  } catch (err) {
    const messages = {
      bad_credentials: 'ชื่อผู้ใช้หรือรหัสผ่านไม่ถูกต้อง',
      locked: `ลองผิดหลายครั้งเกินไป รออีก ${Math.ceil((err.data?.retryAfterSeconds ?? 900) / 60)} นาที`,
      bad_token: 'ลิงก์นี้ใช้ไปแล้วหรือหมดอายุแล้ว',
      already_set_up: 'มีบัญชีแอดมินแล้ว ให้เข้าสู่ระบบแทน',
      bad_username: 'ชื่อผู้ใช้ไม่ตรงเงื่อนไข',
      bad_password: 'รหัสผ่านต้องยาว 10–200 ตัว',
    };
    if (err.message !== 'offline') showFormError(form.id, messages[err.message] || 'มีบางอย่างผิดพลาด ลองอีกครั้ง');
  } finally {
    button.disabled = false;
  }
});

document.addEventListener('input', (event) => {
  if (event.target.id === 'user-filter') {
    state.filter = event.target.value;
    renderUsers();
  }
});

const actions = {
  refresh: () => loadUsers(),
  logout: async () => {
    await api('POST', '/logout');
    await boot();
  },
  disable: async (el) => {
    const { user } = await api('POST', `/users/${el.dataset.id}/disable`);
    replaceUser(user);
    toast('ปิดใช้งานแล้ว ผู้ใช้ถูกออกจากระบบทุกเครื่อง');
  },
  enable: async (el) => {
    const { user } = await api('POST', `/users/${el.dataset.id}/enable`);
    replaceUser(user);
    toast('เปิดใช้งานแล้ว');
  },
  delete: async (el) => {
    const u = state.users.find((x) => x.id === el.dataset.id);
    const ok = await confirmDialog({
      title: `ลบ ${u?.name || u?.email || 'ผู้ใช้นี้'} ถาวร?`,
      body: 'บัญชี กิจกรรม และประวัติการจับเวลาทั้งหมดของผู้ใช้นี้จะถูกลบ และกู้คืนไม่ได้',
      confirm: 'ลบถาวร',
    });
    if (!ok) return;
    await api('DELETE', `/users/${el.dataset.id}`);
    state.users = state.users.filter((x) => x.id !== el.dataset.id);
    renderUsers();
    toast('ลบผู้ใช้แล้ว');
  },
};

document.addEventListener('click', (event) => {
  const el = event.target.closest('[data-action]');
  if (!el || el.disabled) return;
  const fn = actions[el.dataset.action];
  if (!fn) return;
  event.preventDefault();
  Promise.resolve(fn(el)).catch((err) => {
    if (err.status === 404) {
      toast('ไม่พบผู้ใช้นี้แล้ว');
      loadUsers().catch(() => {});
    } else if (err.message !== 'offline' && err.status !== 401) {
      toast('มีบางอย่างผิดพลาด ลองอีกครั้ง');
    }
  });
});

// Keep the list current when coming back to the tab.
document.addEventListener('visibilitychange', () => {
  if (!document.hidden && state.status?.signedIn) loadUsers().catch(() => {});
});

// The CSP forbids inline style attributes; copy data-style into element.style.
new MutationObserver((mutations) => {
  for (const m of mutations) {
    m.addedNodes.forEach((n) => {
      if (n.nodeType !== 1) return;
      [n, ...n.querySelectorAll('[data-style]')].forEach((el) => {
        if (el.dataset?.style) el.style.cssText = el.dataset.style;
      });
    });
  }
}).observe(document.documentElement, { childList: true, subtree: true });

// ---------- boot ----------

async function boot() {
  const res = await fetch('/admin/api/status', { credentials: 'same-origin' });
  state.status = await res.json();
  if (state.status.signedIn) {
    state.users = [];
    await showView();
  } else if (state.status.needsSetup) {
    renderSetup();
  } else {
    renderLogin();
  }
}

// The setup token travels in the URL fragment, which the browser never sends to the server.
const match = location.hash.match(/setup=([A-Za-z0-9_-]+)/);
if (match) {
  setupToken = match[1];
  history.replaceState(null, '', '/admin/');
}
boot().catch(() => toast('โหลดหน้าไม่สำเร็จ ลองรีเฟรช'));
