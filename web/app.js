import { html, render, useState, useEffect, useRef, useMemo } from './preact.js';

// ───────────── utilidades ─────────────
const ICONS = {
  inbox: 'M22 12h-6l-2 3h-4l-2-3H2M5.5 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1z',
  files: 'M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7zM14 2v5h6M9 13h6M9 17h6',
  trash: 'M3 6h18M8 6V4h8v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6',
  search: 'M21 21l-4.3-4.3M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16z',
  upload: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.8 1.2V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-2.8-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0-1.2-2.8H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.2-2.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 2.8-1.2V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 2.8 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0 1.2 2.8h.1a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z',
  x: 'M18 6 6 18M6 6l12 12', check: 'M20 6 9 17l-5-5', plus: 'M12 5v14M5 12h14', back: 'M19 12H5M12 19l-7-7 7-7',
  down: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3',
  moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z', sun: 'M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4',
  tag: 'M20.6 13.4 13.4 20.6a2 2 0 0 1-2.8 0L2 12V2h10l8.6 8.6a2 2 0 0 1 0 2.8zM7 7h.01',
  user: 'M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z',
  type: 'M4 4h16v16H4zM9 9h6M9 13h6M9 17h3', grid: 'M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z', rows: 'M3 6h18M3 12h18M3 18h18',
  spark: 'M12 3l1.9 5.6L19.5 10l-5.6 1.9L12 17.5l-1.9-5.6L4.5 10l5.6-1.4zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z',
  key: 'M21 2l-2 2m-7.6 7.6a5.5 5.5 0 1 1-7.8 7.8 5.5 5.5 0 0 1 7.8-7.8zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4',
  hook: 'M18 16.98h-5.99c-1.1 0-1.95.94-2.48 1.9A4 4 0 0 1 2 17c.01-.7.2-1.4.57-2M6 17l3.13-5.78c.53-.97.1-2.18-.5-3.1a4 4 0 1 1 6.89-4.06M12 6l3.13 5.73C15.66 12.7 16.9 13 18 13a4 4 0 0 1 0 8',
  menu: 'M3 6h18M3 12h18M3 18h18', help: 'M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z', code: 'M16 18l6-6-6-6M8 6l-6 6 6 6',
  folder: 'M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z', chev: 'M9 18l6-6-6-6',
  dots: 'M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM19 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z', edit: 'M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z',
  camera: 'M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3zM12 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z', ext: 'M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6',
  lock: 'M7 11V7a5 5 0 0 1 10 0v4M6 11h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1z', cal: 'M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z',
  install: 'M12 3v12M7 10l5 5 5-5M5 21h14', clock: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 6v6l4 2', cash: 'M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6',
  restore: 'M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5', logout: 'M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9', activity: 'M22 12h-4l-3 9L9 3l-3 9H2',
};
const Icon = ({ n }) => html`<svg class="i" viewBox="0 0 24 24"><path d=${ICONS[n]} /></svg>`;
const fmtDate = (d) => d ? new Date(d.length === 10 ? d + 'T12:00' : d.replace(' ', 'T') + 'Z').toLocaleDateString('es-MX', { day: 'numeric', month: 'short', year: 'numeric' }) : '';
const fmtSize = (b) => b > 1e6 ? (b / 1048576).toFixed(1) + ' MB' : Math.max(1, Math.round(b / 1024)) + ' KB';
const cls = (...a) => a.filter(Boolean).join(' ');
const fmtMoney = (n) => n.toLocaleString('es-MX', { style: 'currency', currency: 'MXN' });
/** «Vencido», «Vence en 12 días»… o null si falta más de 60 días. */
function dueLabel(d) {
  const n = Math.round((new Date(d + 'T12:00') - new Date(new Date().toDateString() + ' 12:00')) / 864e5);
  return n < 0 ? ['bad', 'Vencido'] : n === 0 ? ['bad', 'Vence hoy'] : n <= 60 ? ['warn', `Vence en ${n} ${n === 1 ? 'día' : 'días'}`] : null;
}
const debounce = (fn, ms) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };

// ───────────── estado global ─────────────
const parseHash = () => {
  const p = new URLSearchParams(location.hash.replace(/^#\/?\??/, ''));
  const num = (k) => (p.get(k) ? Number(p.get(k)) : null);
  return { view: p.get('view') || 'all', q: p.get('q') || '', tags: (p.get('tags') || '').split(',').filter(Boolean).map(Number), c: num('c'), t: num('t'), f: num('f'),
    from: p.get('from') || '', to: p.get('to') || '', doc: num('doc'), sort: p.get('sort') || '' };
};
/** Lo que se limpia al cambiar de vista. */
const HOME = { doc: null, tags: [], c: null, t: null, q: '', from: '', to: '', f: null };
const touch = matchMedia('(hover: none)').matches;
const store = {
  state: { ready: false, setup: false, user: null, llm: false, labels: { tag: [], correspondent: [], doctype: [] }, folders: [], meta: false, stats: {}, route: parseHash(), docs: [], total: 0,
    loading: false, amountTotal: null, cur: 0, picked: new Set(), jobs: [], toasts: [], modal: null, palette: false, grid: localStorage.grid === '1', menu: false, jobsOpen: false,
    open: new Set(JSON.parse(localStorage.open || '[]')), dialog: null, sheet: null, install: null, online: navigator.onLine },
  subs: new Set(),
  set(patch) { Object.assign(this.state, typeof patch === 'function' ? patch(this.state) : patch); this.subs.forEach((f) => f()); },
};
const S = () => store.state;
function useStore() {
  const [, tick] = useState(0);
  useEffect(() => { const f = () => tick((x) => x + 1); store.subs.add(f); return () => store.subs.delete(f); }, []);
  return store.state;
}
function nav(patch, replace) {
  const r = { ...S().route, ...patch }, p = new URLSearchParams();
  if (r.view !== 'all') { p.set('view', r.view); r.f = null; }
  for (const k of ['q', 'c', 't', 'f', 'from', 'to', 'doc', 'sort']) if (r[k]) p.set(k, r[k]);
  if (r.tags.length) p.set('tags', r.tags.join(','));
  const h = '#/' + (p.toString() ? '?' + p : '');
  replace ? history.replaceState(null, '', h) : history.pushState(null, '', h);
  onRoute();
}
function onRoute() {
  const prev = S().route, route = parseHash();
  store.set({ route, menu: false });
  if (['view', 'q', 'tags', 'c', 't', 'f', 'from', 'to', 'sort'].some((k) => String(prev[k]) !== String(route[k]))) { store.set({ picked: new Set(), cur: 0 }); loadDocs(); }
}
addEventListener('hashchange', onRoute); addEventListener('popstate', onRoute);

async function api(path, o = {}) {
  const init = { method: o.method || (o.json !== undefined || o.body ? 'POST' : 'GET'), headers: { 'X-Requested-With': 'papiro' }, credentials: 'same-origin' };
  if (o.json !== undefined) { init.headers['Content-Type'] = 'application/json'; init.body = JSON.stringify(o.json); }
  if (o.body) init.body = o.body;
  const r = await fetch('/api' + path, init).catch(() => { throw new Error(o.body && navigator.onLine ? 'La subida se cortó antes de terminar. Inténtalo de nuevo.' : 'Sin conexión con Papiro'); });
  if (r.status === 401 && !o.quiet) store.set({ user: null });
  const data = r.status === 204 ? null : await r.json().catch(() => null);
  if (!r.ok) throw new Error(data?.error || `Error ${r.status}`);
  return data;
}
function toast(text, kind = 'ok', ms = 4200) {
  const id = Math.random();
  store.set((s) => ({ toasts: [...s.toasts, { id, text, kind }] }));
  setTimeout(() => store.set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })), ms);
}
const attempt = (p, okMsg) => p.then((v) => { okMsg && toast(okMsg); return v; }).catch((e) => { toast(e.message, 'bad', 6500); });
/** Diálogo propio en lugar de confirm()/prompt(): devuelve true, el texto escrito o null si se cancela. */
const ask = (d) => new Promise((resolve) => store.set({ dialog: { ...d, resolve, id: Math.random() } }));

const PAGE = 60;
let seq = 0;
async function loadDocs(more) {
  const { route: r, docs } = S(), my = ++seq, p = new URLSearchParams({ limit: PAGE, offset: more ? docs.length : 0 });
  if (r.q) p.set('q', r.q);
  if (r.tags.length) p.set('tags', r.tags.join(','));
  if (r.c) p.set('correspondent', r.c);
  if (r.t) p.set('doctype', r.t);
  if (r.f && r.view === 'all') p.set('folder', r.f);
  if (r.from) p.set('from', r.from);
  if (r.to) p.set('to', r.to);
  if (r.view === 'inbox') p.set('inbox', 'true');
  if (r.view === 'trash') p.set('trash', 'true');
  if (r.view === 'expiring') p.set('expiring', 'true');
  if (r.sort) { const [s, o] = r.sort.split(':'); p.set('sort', s); p.set('order', o || 'desc'); }
  store.set({ loading: true });
  try {
    const res = await api('/documents?' + p);
    if (my === seq) store.set({ docs: more ? [...docs, ...res.items] : res.items, total: res.total, amountTotal: res.amount_total, loading: false });
  } catch { if (my === seq) store.set({ loading: false }); }
}
async function loadMeta() {
  const [tag, correspondent, doctype, folders, stats] = await Promise.all([api('/tags'), api('/correspondents'), api('/doctypes'), api('/folders'), api('/stats')]);
  store.set({ labels: { tag, correspondent, doctype }, folders, stats, meta: true });
}
const refresh = debounce(() => { loadDocs(); loadMeta(); }, 250);
const labelOf = (kind, id) => S().labels[kind].find((l) => l.id === id);
const folderOf = (id) => (id ? S().folders.find((f) => f.id === id) : undefined);
const kids = (id) => S().folders.filter((f) => (f.parent_id ?? null) === (id ?? null));
const here = () => { const r = S().route; return r.view === 'all' ? folderOf(r.f) : undefined; };
/** Sugerencias del análisis local: `pend` son las que esperan respuesta; `auto`, las que se aplicaron solas. */
function suggestions(d) {
  const s = d.suggest || {}, pend = [], auto = [];
  const add = (key, icon, label, kind, x) => {
    const name = kind === 'folder' ? folderOf(x.value)?.path.replaceAll('/', ' › ') : labelOf(kind, x.value)?.name;
    if (name) (x.applied ? auto : pend).push({ key, icon, label, name, similar: x.similar });
  };
  if (s.folder_id) add('folder_id', 'folder', 'Carpeta', 'folder', s.folder_id);
  if (s.doctype_id) add('doctype_id', 'type', 'Tipo', 'doctype', s.doctype_id);
  if (s.correspondent_id) add('correspondent_id', 'user', 'Corresponsal', 'correspondent', s.correspondent_id);
  for (const t of s.tags || []) add(`tag:${t.value}`, 'tag', 'Etiqueta', 'tag', t);
  return { pend, auto };
}

async function boot() {
  // Sin conexión (o con el servidor caído) la app instalada abre igual y lo dice, en vez de quedarse cargando.
  const st = await api('/auth/status', { quiet: true }).catch(() => null);
  if (!st) return store.set({ ready: true, down: true });
  store.set({ ready: true, down: false, setup: st.setup_required, user: st.user, llm: st.llm });
  if (st.user) start();
}
let es;
function start() {
  loadMeta(); loadDocs();
  api('/jobs').then((jobs) => store.set({ jobs }));
  es?.close();
  es = new EventSource('/api/events');
  es.onmessage = (m) => {
    const ev = JSON.parse(m.data);
    if (ev.type === 'job') {
      const j = ev.job;
      store.set((s) => ({ jobs: [j, ...s.jobs.filter((x) => x.id !== j.id)].slice(0, 100) }));
      if (j.status === 'failed') toast(`${j.filename}: ${j.error}`, 'bad', 9000);
      if (j.status === 'duplicate') toast(`${j.filename} ya estaba en el archivo`, 'warn');
    } else { refresh(); window.dispatchEvent(new CustomEvent('doc-changed', { detail: ev })); }
  };
  takeShared();
  if (location.hash.includes('focus=1')) setTimeout(() => document.getElementById('q')?.focus(), 300);
}

async function upload(files) {
  files = [...files];
  if (!files.length) return;
  if (!canWrite()) return toast('Tu usuario es de solo lectura', 'warn');
  const folder = here();
  toast((files.length === 1 ? `Subiendo ${files[0].name || 'documento'}` : `Subiendo ${files.length} archivos`) + (folder ? ` a «${folder.name}»…` : '…'));
  // De tres en tres: el servidor los encola y procesa a su ritmo.
  const queue = files.slice();
  await Promise.all([0, 1, 2].map(async () => {
    for (let f; (f = queue.shift());) {
      const name = f.name || `documento.${(f.type.split('/')[1] || 'bin').replace('jpeg', 'jpg')}`;
      // Un archivo arrastrado desde otra aplicación (o movido tras elegirlo) puede no ser legible: se avisa antes de enviar nada.
      if (!f.size || !await f.slice(0, 1).arrayBuffer().then(() => true, () => false)) { toast(`El navegador no pudo leer «${name}». Prueba con el botón Subir o copia el archivo a otra carpeta.`, 'bad', 9000); continue; }
      const body = new FormData(); body.append('file', f, name); if (folder) body.append('folder_id', folder.id); await attempt(api('/documents', { body }));
    }
  }));
}
/** Abre el selector de archivos; con `camera`, la cámara del teléfono. */
function pickFiles(camera) {
  const i = Object.assign(document.createElement('input'), { type: 'file', multiple: !camera });
  if (camera) { i.accept = 'image/*'; i.capture = 'environment'; }
  i.onchange = () => upload(i.files); i.click();
}
const addDocs = () => (touch ? store.set({ sheet: 'upload', menu: false }) : pickFiles());
async function bulk(ids, json, msg) { await attempt(api('/documents/bulk', { json: { ids: [...ids], ...json } }), msg); store.set({ picked: new Set() }); refresh(); }
function setTheme(t) { t ? (localStorage.theme = t) : delete localStorage.theme; document.documentElement.dataset.theme = t || ''; themeColor(); store.set({}); }
/** La barra del sistema (y la del navegador en móvil) toma el color de fondo de la app. */
function themeColor() { const bg = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim(); document.querySelectorAll('meta[name=theme-color]').forEach((m) => { m.content = bg; m.removeAttribute('media'); }); }
if (localStorage.theme) { document.documentElement.dataset.theme = localStorage.theme; themeColor(); }
const isDark = () => (document.documentElement.dataset.theme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
const inboxIds = () => S().labels.tag.filter((t) => t.is_inbox).map((t) => t.id);
const canWrite = () => S().user?.role !== 'viewer';
const isAdmin = () => S().user?.role === 'admin';
const openJobs = () => { api('/jobs').then((jobs) => store.set({ jobs })); store.set({ jobsOpen: true, menu: false }); };

// carpetas
function toggleOpen(id, on) { const open = new Set(S().open); (on ?? !open.has(id)) ? open.add(id) : open.delete(id); localStorage.open = JSON.stringify([...open]); store.set({ open }); }
async function newFolder(parent_id = null) {
  const parent = folderOf(parent_id);
  const name = await ask({ title: parent ? `Nueva carpeta en «${parent.name}»` : 'Nueva carpeta', input: true, placeholder: 'Nombre de la carpeta', ok: 'Crear' });
  const f = name && await attempt(api('/folders', { json: { name, parent_id } }));
  if (f) { await loadMeta(); parent_id && toggleOpen(parent_id, true); }
  return f;
}
async function renameFolder(f) {
  const name = await ask({ title: 'Cambiar nombre', input: true, value: f.name, ok: 'Guardar' });
  if (name && name !== f.name) { await attempt(api(`/folders/${f.id}`, { method: 'PATCH', json: { name } })); loadMeta(); }
}
async function deleteFolder(f) {
  const sub = kids(f.id).length;
  if (!await ask({ title: `¿Borrar la carpeta «${f.name}»?`, text: `Sus documentos no se borran: ${folderOf(f.parent_id) ? `pasan a «${folderOf(f.parent_id).name}»` : 'se quedan sin carpeta'}.${sub ? ' Sus subcarpetas también se borran.' : ''}`, ok: 'Borrar carpeta', danger: true })) return;
  await attempt(api(`/folders/${f.id}`, { method: 'DELETE' }), 'Carpeta borrada');
  if (S().route.f === f.id) nav({ f: f.parent_id || null }, true);
  refresh();
}
const folderMenu = (f) => [['folder', 'Nueva subcarpeta', () => newFolder(f.id)], ['edit', 'Cambiar nombre', () => renameFolder(f)], ['trash', 'Borrar carpeta', () => deleteFolder(f), true]];
const moveTo = (ids, folder_id) => bulk(ids, { folder_id }, folder_id ? `Movido a «${folderOf(folder_id)?.name}»` : 'Sacado de su carpeta');
/** Arrastrar documentos de la lista y soltarlos sobre una carpeta los mueve. */
const DND = 'application/x-papiro-docs';
const dropOn = (folder_id) => canWrite() && {
  onDragOver: (e) => { if ([...e.dataTransfer.types].includes(DND)) { e.preventDefault(); e.currentTarget.classList.add('drop-on'); } },
  onDragLeave: (e) => e.currentTarget.classList.remove('drop-on'),
  onDrop: (e) => { e.currentTarget.classList.remove('drop-on'); const ids = e.dataTransfer.getData(DND); if (ids) { e.preventDefault(); moveTo(ids.split(',').map(Number), folder_id); } },
};

// aplicación instalable
if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); store.set({ install: e }); });
addEventListener('appinstalled', () => store.set({ install: null }));
addEventListener('online', () => { store.set({ online: true }); S().down ? boot() : S().user && refresh(); });
addEventListener('offline', () => store.set({ online: false }));
/** Archivos que llegaron con «Compartir → Papiro»: el service worker los guardó y aquí se suben. */
async function takeShared() {
  if (!('caches' in window)) return;
  const cache = await caches.open('papiro-share'), files = [];
  for (const k of await cache.keys()) {
    const r = await cache.match(k);
    files.push(new File([await r.blob()], decodeURIComponent(r.headers.get('X-Name') || 'documento'), { type: r.headers.get('Content-Type') || '' }));
    await cache.delete(k);
  }
  upload(files);
}

// ───────────── piezas comunes ─────────────
const Tag = ({ l, onRemove }) => html`<span class="tag" style=${`--c:${l.color}`}>${l.name}${onRemove && html`<button onClick=${(e) => { e.stopPropagation(); onRemove(); }} title="Quitar">×</button>`}</span>`;

/** Cierra un desplegable al tocar fuera de él. */
function usePop(onClose) {
  const ref = useRef();
  useEffect(() => {
    const out = (e) => ref.current && !ref.current.contains(e.target) && onClose();
    const t = setTimeout(() => addEventListener('pointerdown', out));
    return () => { clearTimeout(t); removeEventListener('pointerdown', out); };
  }, []);
  return ref;
}
const listKeys = (n, hi, setHi, choose, onClose) => (e) => {
  if (e.key === 'ArrowDown') { setHi((hi + 1) % Math.max(n, 1)); e.preventDefault(); }
  else if (e.key === 'ArrowUp') { setHi((hi - 1 + n) % Math.max(n, 1)); e.preventDefault(); }
  else if (e.key === 'Enter') { choose(hi); e.preventDefault(); }
  else if (e.key === 'Escape') { onClose(); e.stopPropagation(); }
};

/** Selector con filtro y «crear»: sirve para etiquetas (multi), corresponsal y tipo. */
function Picker({ kind, value, multi, onChange, onClose }) {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), input = useRef(), ref = usePop(onClose);
  const sel = new Set(multi ? value : value ? [value] : []);
  const list = s.labels[kind].filter((l) => l.name.toLowerCase().includes(q.trim().toLowerCase()));
  const canCreate = canWrite() && q.trim() && !s.labels[kind].some((l) => l.name.toLowerCase() === q.trim().toLowerCase());
  const n = list.length + (canCreate ? 1 : 0);
  useEffect(() => { input.current.focus(); }, []);
  const choose = async (i) => {
    let l = list[i];
    if (!l && canCreate) { l = await attempt(api('/' + ({ tag: 'tags', correspondent: 'correspondents', doctype: 'doctypes' })[kind], { json: { name: q.trim() } })); if (!l) return; await loadMeta(); }
    if (!l) return;
    if (multi) { sel.has(l.id) ? sel.delete(l.id) : sel.add(l.id); onChange([...sel], l); setQ(''); } else { onChange(sel.has(l.id) ? null : l.id, l); onClose(); }
  };
  return html`<div class="pop-scrim" /><div class="pop" ref=${ref} onClick=${(e) => e.stopPropagation()}>
    <input ref=${input} value=${q} placeholder=${canWrite() ? 'Buscar o crear…' : 'Buscar…'} onInput=${(e) => { setQ(e.target.value); setHi(0); }} onKeyDown=${listKeys(n, hi, setHi, choose, onClose)} />
    <div class="opts scroll">
      ${list.map((l, i) => html`<button class=${cls('opt', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => choose(i)}>
        <i class="dot" style=${`background:${l.color}`} /><span>${l.name}</span>${sel.has(l.id) && html`<span class="tick"><${Icon} n="check" /></span>`}</button>`)}
      ${canCreate && html`<button class=${cls('opt', hi === list.length && 'hi')} onMouseEnter=${() => setHi(list.length)} onClick=${() => choose(list.length)}><${Icon} n="plus" /><span>Crear «${q.trim()}»</span></button>`}
      ${!n && html`<div class="opt muted">Sin resultados</div>`}
    </div></div>`;
}

/** Selector de carpeta: filtra por ruta, permite sacar el documento de su carpeta y crear una nueva. */
function FolderPicker({ value, onChange, onClose }) {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), input = useRef(), ref = usePop(onClose), needle = q.trim().toLowerCase();
  const list = s.folders.filter((f) => f.path.toLowerCase().includes(needle));
  const items = [...(value && !needle ? [{ id: null, none: true }] : []), ...list];
  const canCreate = needle && !q.includes('/') && !s.folders.some((f) => !f.parent_id && f.name.toLowerCase() === needle);
  const n = items.length + (canCreate ? 1 : 0);
  useEffect(() => { input.current.focus(); }, []);
  const choose = async (i) => {
    let f = items[i];
    if (!f && canCreate) { f = await attempt(api('/folders', { json: { name: q.trim() } })); if (!f) return; await loadMeta(); }
    if (f) { onChange(f.id, f); onClose(); }
  };
  return html`<div class="pop-scrim" /><div class="pop" ref=${ref} onClick=${(e) => e.stopPropagation()}>
    <input ref=${input} value=${q} placeholder="Buscar o crear carpeta…" onInput=${(e) => { setQ(e.target.value); setHi(0); }} onKeyDown=${listKeys(n, hi, setHi, choose, onClose)} />
    <div class="opts scroll">
      ${items.map((f, i) => html`<button class=${cls('opt fold', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => choose(i)}>
        ${f.none ? html`<${Icon} n="x" /><span>Sin carpeta</span>` : html`<${Icon} n="folder" /><span>${f.path.replaceAll('/', ' › ')}</span>${f.id === value && html`<span class="tick"><${Icon} n="check" /></span>`}`}</button>`)}
      ${canCreate && html`<button class=${cls('opt', hi === items.length && 'hi')} onMouseEnter=${() => setHi(items.length)} onClick=${() => choose(items.length)}><${Icon} n="plus" /><span>Crear carpeta «${q.trim()}»</span></button>`}
      ${!n && html`<div class="opt muted">Aún no hay carpetas: escribe un nombre para crear la primera</div>`}
    </div></div>`;
}

/** Menú de acciones: [icono, texto, acción, peligrosa]. */
function Menu({ items, onClose, right }) {
  const ref = usePop(onClose);
  return html`<div class="pop-scrim" /><div class=${cls('pop menu', right && 'right')} ref=${ref} onClick=${(e) => e.stopPropagation()}>${items.filter(Boolean).map(([icon, label, fn, danger]) =>
    html`<button class=${cls('opt', danger && 'danger')} onClick=${() => { onClose(); fn(); }}><${Icon} n=${icon} /><span>${label}</span></button>`)}</div>`;
}
/** Botón que abre un menú. */
function MenuButton({ items, icon = 'dots', title, right, cls: c = 'btn icon ghost sm', children }) {
  const [open, setOpen] = useState(false);
  return html`<span class="rel"><button class=${c} title=${title} onClick=${(e) => { e.stopPropagation(); setOpen(!open); }}><${Icon} n=${icon} />${children}</button>
    ${open && html`<${Menu} items=${items} right=${right} onClose=${() => setOpen(false)} />`}</span>`;
}
function Dialog({ d }) {
  const [v, setV] = useState(d.value || '');
  const done = (r) => { store.set({ dialog: null }); d.resolve(r); };
  return html`<div class="veil center" onMouseDown=${(e) => e.target === e.currentTarget && done(null)}><form class="dialog" onSubmit=${(e) => { e.preventDefault(); done(d.input ? v.trim() || null : true); }}>
    <h3>${d.title}</h3>${d.text && html`<p>${d.text}</p>`}
    ${d.input && html`<input class="input" ref=${(el) => el && !el.dataset.f && (el.dataset.f = 1, el.focus(), el.select())} type=${d.type || 'text'} minlength=${d.minlength} value=${v} placeholder=${d.placeholder || ''} onInput=${(e) => setV(e.target.value)} required />`}
    <div class="row end"><button type="button" class="btn" onClick=${() => done(null)}>Cancelar</button><button class=${cls('btn primary', d.danger && 'danger')}>${d.ok || 'Aceptar'}</button></div></form></div>`;
}

// ───────────── acceso ─────────────
function Auth({ setup }) {
  const [u, setU] = useState(''), [p, setP] = useState(''), [err, setErr] = useState(''), [busy, setBusy] = useState(false);
  const go = async (e) => {
    e.preventDefault(); setBusy(true); setErr('');
    try { const user = await api(setup ? '/auth/setup' : '/auth/login', { json: { username: u, password: p }, quiet: true }); store.set({ user, setup: false }); start(); }
    catch (x) { setErr(x.message); } setBusy(false);
  };
  return html`<div class="auth"><form onSubmit=${go}>
    <div class="brand"><img class="logo" src="/assets/icon.svg" alt="" />Papiro</div>
    <h1>${setup ? 'Crea tu cuenta de administrador' : 'Inicia sesión'}</h1>
    ${setup && html`<div class="muted">Es la primera vez que se abre esta instalación. Esta cuenta tendrá control total.</div>`}
    <label class="field">Usuario<input class="input" value=${u} onInput=${(e) => setU(e.target.value)} autocomplete="username" autofocus required /></label>
    <label class="field">Contraseña<input class="input" type="password" value=${p} onInput=${(e) => setP(e.target.value)} autocomplete=${setup ? 'new-password' : 'current-password'} minlength=${setup ? 8 : 1} required /></label>
    ${err && html`<div class="err">${err}</div>`}
    <button class="btn primary big" disabled=${busy}>${setup ? 'Crear cuenta' : 'Entrar'}</button>
  </form></div>`;
}


// ───────────── barra lateral ─────────────
function FolderNode({ f, depth }) {
  const s = S(), sub = kids(f.id), on = here()?.id === f.id, open = s.open.has(f.id), [menu, setMenu] = useState(false);
  return html`<div class="rel"><div class=${cls('nav folder', on && 'on')} style=${`padding-left:${10 + depth * 16}px`} role="button" tabindex="0" ...${dropOn(f.id)}
      onClick=${() => { nav({ ...HOME, view: 'all', f: f.id }); sub.length && toggleOpen(f.id, true); }} onKeyDown=${(e) => e.key === 'Enter' && e.target === e.currentTarget && nav({ ...HOME, view: 'all', f: f.id })}>
      <button class=${cls('twist', open && 'open', !sub.length && 'none')} title=${open ? 'Contraer' : 'Desplegar'} onClick=${(e) => { e.stopPropagation(); toggleOpen(f.id); }}><svg class="i t" viewBox="0 0 24 24"><path d=${ICONS.chev} /></svg></button>
      <${Icon} n="folder" /><span class="name">${f.name}</span><span class="count">${f.document_count || ''}</span>
      ${canWrite() && html`<button class="more" title="Opciones de la carpeta" onClick=${(e) => { e.stopPropagation(); setMenu(!menu); }}><${Icon} n="dots" /></button>`}</div>
    ${menu && html`<${Menu} items=${folderMenu(f)} onClose=${() => setMenu(false)} />`}
    ${open && sub.map((c) => html`<${FolderNode} f=${c} depth=${depth + 1} key=${c.id} />`)}</div>`;
}
function Sidebar() {
  const s = useStore(), r = s.route, active = s.jobs.filter((j) => j.status === 'queued' || j.status === 'running').length, roots = kids(null);
  const group = (title, kind, key, icon) => {
    const items = s.labels[kind].filter((l) => l.document_count > 0 || kind === 'tag');
    return items.length > 0 && html`<details open=${kind === 'tag' || items.length <= 6}><summary>${title}</summary>${items.map((l) => {
      const on = kind === 'tag' ? r.tags.includes(l.id) : r[key] === l.id;
      const go = () => nav(kind === 'tag' ? { tags: on ? r.tags.filter((x) => x !== l.id) : [...r.tags, l.id], doc: null } : { [key]: on ? null : l.id, doc: null });
      return html`<button class=${cls('nav', on && 'on')} onClick=${go}>${kind === 'tag' ? html`<i class="dot" style=${`background:${l.color}`} />` : html`<${Icon} n=${icon} />`}
        <span class="name">${l.name}</span><span class="count">${l.document_count || ''}</span></button>`;
    })}</details>`;
  };
  const view = (v, icon, label, count) => html`<button class=${cls('nav', r.view === v && !here() && 'on')} onClick=${() => nav({ ...HOME, view: v })} ...${v === 'all' ? dropOn(null) : {}}>
    <${Icon} n=${icon} /><span class="name">${label}</span><span class="count">${count || ''}</span></button>`;
  return html`<nav class=${cls('side', s.menu && 'open')}>
    <div class="side-head"><div class="brand"><img class="logo" src="/assets/icon.svg" alt="" />Papiro</div>
      <button class="btn icon ghost only-narrow" title="Cerrar" onClick=${() => store.set({ menu: false })}><${Icon} n="x" /></button></div>
    ${canWrite() && html`<button class="btn primary big only-wide" onClick=${() => pickFiles()} title="Subir documentos (U)"><${Icon} n="upload" />Subir documentos<kbd>U</kbd></button>`}
    <div class="side-body scroll">
      ${view('inbox', 'inbox', 'Bandeja de entrada', s.stats.inbox)}${view('all', 'files', 'Todos los documentos', s.stats.documents)}
      ${(s.stats.expiring > 0 || r.view === 'expiring') && view('expiring', 'clock', 'Vencimientos', s.stats.expiring)}${view('trash', 'trash', 'Papelera', s.stats.trash)}
      <h4>Carpetas${canWrite() && html`<button title="Nueva carpeta" onClick=${() => newFolder()}><${Icon} n="plus" /></button>`}</h4>
      ${roots.map((f) => html`<${FolderNode} f=${f} depth=${0} key=${f.id} />`)}
      ${!roots.length && s.meta && html`<button class="nav hint" disabled=${!canWrite()} onClick=${() => newFolder()}><${Icon} n="folder" /><span class="name">${canWrite() ? 'Crea tu primera carpeta' : 'Aún no hay carpetas'}</span></button>`}
      ${group('Etiquetas', 'tag')}${group('Corresponsales', 'correspondent', 'c', 'user')}${group('Tipos', 'doctype', 't', 'type')}
    </div>
    <div class="side-foot">
      <button class="nav only-wide" onClick=${openJobs}>${active ? html`<span class="spinner" />` : html`<${Icon} n="activity" />`}
        <span class="name">${active ? `Procesando ${active}…` : 'Actividad'}</span>${s.stats.failed > 0 && html`<span class="count" style="color:var(--danger)" title="Trabajos fallidos">${s.stats.failed}</span>`}</button>
      ${s.install && html`<button class="nav" onClick=${() => { s.install.prompt(); store.set({ install: null, menu: false }); }}><${Icon} n="install" /><span class="name">Instalar Papiro</span></button>`}
      <button class="nav" onClick=${() => store.set({ modal: 'settings', menu: false })}><${Icon} n="settings" /><span class="name">Ajustes</span><span class="count only-wide"><kbd>,</kbd></span></button>
      <button class="nav" onClick=${() => setTheme(isDark() ? 'light' : 'dark')}><${Icon} n=${isDark() ? 'sun' : 'moon'} /><span class="name">${isDark() ? 'Modo claro' : 'Modo oscuro'}</span></button>
    </div></nav>`;
}

/** Barra inferior del móvil. */
function Tabbar() {
  const s = useStore(), r = s.route, active = s.jobs.filter((j) => j.status === 'queued' || j.status === 'running').length;
  const go = (v) => { store.set({ jobsOpen: false }); nav({ ...HOME, view: v }); };
  return html`<nav class="tabbar only-narrow">
    <button class=${cls('tab', r.view === 'all' && !s.jobsOpen && !s.menu && 'on')} onClick=${() => go('all')}><${Icon} n="files" /><span>Archivo</span></button>
    <button class=${cls('tab', r.view === 'inbox' && !s.jobsOpen && !s.menu && 'on')} onClick=${() => go('inbox')}><${Icon} n="inbox" /><span>Bandeja</span>${s.stats.inbox > 0 && html`<i class="badge">${s.stats.inbox}</i>`}</button>
    ${canWrite() ? html`<button class="fab" title="Añadir documentos" onClick=${() => store.set({ sheet: 'upload' })}><${Icon} n="plus" /></button>` : html`<span />`}
    <button class=${cls('tab', s.jobsOpen && 'on')} onClick=${openJobs}>${active ? html`<span class="spinner" />` : html`<${Icon} n="activity" />`}<span>Actividad</span>${s.stats.failed > 0 && html`<i class="badge bad">${s.stats.failed}</i>`}</button>
    <button class=${cls('tab', s.menu && 'on')} onClick=${() => store.set({ menu: true })}><${Icon} n="menu" /><span>Más</span></button></nav>`;
}
function UploadSheet() {
  const close = () => store.set({ sheet: null }), folder = here();
  return html`<div class="veil bottom" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="sheet"><div class="grabber" />
    <h3>Añadir a ${folder ? `«${folder.name}»` : 'Papiro'}</h3>
    <button class="big-opt" onClick=${() => { close(); pickFiles(true); }}><span class="ico"><${Icon} n="camera" /></span><span><b>Tomar una foto</b><small>Fotografía un documento en papel; Papiro extrae el texto.</small></span></button>
    <button class="big-opt" onClick=${() => { close(); pickFiles(); }}><span class="ico"><${Icon} n="upload" /></span><span><b>Elegir archivos</b><small>PDF, fotos, Word, Excel, PowerPoint o texto.</small></span></button>
    <p class="hint">Con Papiro instalada en el teléfono también puedes usar «Compartir» desde cualquier otra app.</p></div></div>`;
}

function Jobs() {
  const s = useStore();
  const icon = { queued: html`<span class="faint">◦</span>`, running: html`<span class="spinner" />`, done: html`<span class="ok"><${Icon} n="check" /></span>`, duplicate: html`<span class="warn">⧉</span>`, failed: html`<span class="bad"><${Icon} n="x" /></span>` };
  const label = { queued: 'En cola', running: 'Extrayendo texto…', done: 'Listo', duplicate: 'Ya estaba en el archivo', failed: 'Falló' };
  const reload = () => api('/jobs').then((jobs) => store.set({ jobs }));
  const close = () => store.set({ jobsOpen: false });
  return html`<div class="veil bottom" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="jobs scroll"><div class="grabber" />
    <div class="jobs-head"><b>Actividad</b><button class="btn icon ghost sm" onClick=${close} title="Cerrar"><${Icon} n="x" /></button></div>
    ${!s.jobs.length && html`<div class="job muted"><span /><div>Aún no se ha procesado nada. Sube un archivo para empezar.</div></div>`}
    ${s.jobs.slice(0, 40).map((j) => html`<div class="job" key=${j.id}>${icon[j.status]}<div class="grow"><div class="name">${j.filename}</div>
      <small>${label[j.status]}${j.error && j.status === 'failed' ? ` — ${j.error.split('\n')[0]}` : ''}
        ${j.doc_id ? html` · <a href="#" onClick=${(e) => { e.preventDefault(); nav({ doc: j.doc_id }); close(); }}>abrir</a>` : ''}</small>
      ${j.status === 'failed' && canWrite() && html`<div class="row wrap" style="margin-top:7px">
        ${j.can_retry && html`<button class="btn sm" onClick=${() => attempt(api(`/jobs/${j.id}/retry`, { method: 'POST' }), 'Reintentando…').then(reload)}>Reintentar</button>`}
        <button class="btn ghost sm" onClick=${() => attempt(api(`/jobs/${j.id}`, { method: 'DELETE' })).then(reload)}>Descartar</button>
        ${j.can_retry && html`<span class="faint">El archivo se conservó</span>`}</div>`}
    </div></div>`)}
  </div></div>`;
}

// ───────────── lista ─────────────
function Snippet({ text }) {
  return text.split(/\u0001(.*?)\u0002/s).map((part, i) => (i % 2 ? html`<mark>${part}</mark>` : part));
}
let pressTimer = 0, pressed = false;
function DocRow({ d, i }) {
  const s = S(), open = s.route.doc === d.id, picked = s.picked.has(d.id);
  const corr = labelOf('correspondent', d.correspondent_id), type = labelOf('doctype', d.doctype_id), folder = d.folder_id !== here()?.id && folderOf(d.folder_id);
  const click = (e) => {
    if (pressed) { pressed = false; return; }
    if (e.metaKey || e.ctrlKey || e.target.closest('.check') || (touch && s.picked.size)) return togglePick(d.id, i, e.shiftKey);
    if (e.shiftKey) return togglePick(d.id, i, true);
    store.set({ cur: i }); nav({ doc: d.id });
  };
  // En pantallas táctiles, mantener pulsado selecciona; después cada toque añade o quita.
  const down = (e) => { if (e.pointerType !== 'mouse' && canWrite()) pressTimer = setTimeout(() => { pressed = true; navigator.vibrate?.(10); togglePick(d.id, i); }, 480); };
  const up = () => clearTimeout(pressTimer);
  const drag = (e) => { const ids = picked ? [...s.picked] : [d.id]; e.dataTransfer.setData(DND, ids.join(',')); e.dataTransfer.effectAllowed = 'move'; };
  return html`<div class=${cls('doc', i === s.cur && 'cur', open && 'open', picked && 'picked')} data-i=${i} onClick=${click} draggable=${!touch && canWrite()} onDragStart=${drag}
      onPointerDown=${down} onPointerUp=${up} onPointerCancel=${up} onPointerLeave=${up} onContextMenu=${(e) => touch && e.preventDefault()}>
    ${d.has_thumb ? html`<img class="thumb" loading="lazy" draggable=${false} src=${`/api/documents/${d.id}/thumb?v=2`} alt="" />` : html`<div class="thumb">${d.original_name.split('.').pop()}</div>`}
    <span class="check"><${Icon} n="check" /></span>
    <div class="body"><div class="title">${d.is_private && html`<${Icon} n="lock" />`}${d.title}${suggestions(d).pend.length > 0 && html`<span class="hint-dot" title="Tiene sugerencias"><${Icon} n="spark" /></span>`}</div>
      ${d.snippet ? html`<div class="snip"><${Snippet} text=${d.snippet} /></div>`
        : html`<div class="sub">${folder && html`<${Icon} n="folder" />`}${[folder?.name, corr?.name, type?.name, d.pages > 1 && `${d.pages} páginas`].filter(Boolean).join(' · ') || d.original_name}</div>`}
      ${d.tags.length > 0 && html`<div class="tags">${d.tags.map((t) => labelOf('tag', t)).filter(Boolean).map((l) => html`<${Tag} l=${l} key=${l.id} />`)}</div>`}
    </div><div class="date">${fmtDate(d.created)}${d.expires && dueLabel(d.expires) && html`<span class=${`due ${dueLabel(d.expires)[0]}`}>${dueLabel(d.expires)[1]}</span>`}${d.amount != null && html`<span class="amt">${fmtMoney(d.amount)}</span>`}</div></div>`;
}
let anchor = 0;
function togglePick(id, i, range) {
  const { picked, docs } = S(), next = new Set(picked);
  if (range) for (let k = Math.min(anchor, i); k <= Math.max(anchor, i); k++) next.add(docs[k].id);
  else { next.has(id) ? next.delete(id) : next.add(id); anchor = i; }
  store.set({ picked: next, cur: i });
}
function Crumbs({ f }) {
  const chain = [];
  for (let x = f; x && chain.length < 64; x = folderOf(x.parent_id)) chain.unshift(x);
  return html`<div class="crumbs"><button onClick=${() => nav({ f: null, doc: null })} ...${dropOn(null)}>Archivo</button>
    ${chain.slice(0, -1).map((x) => html`<${Icon} n="chev" /><button onClick=${() => nav({ f: x.id, doc: null })} ...${dropOn(x.id)}>${x.name}</button>`)}<${Icon} n="chev" /></div>`;
}

function Main() {
  const s = useStore(), r = s.route, input = useRef(), [pop, setPop] = useState(null), folder = here();
  const setQ = useMemo(() => debounce((q) => nav({ q, doc: S().route.doc }, true), 110), []);
  useEffect(() => { if (input.current && input.current.value !== r.q && document.activeElement !== input.current) input.current.value = r.q; }, [r.q]);
  useEffect(() => { !touch && document.querySelector(`.doc[data-i="${s.cur}"]`)?.scrollIntoView({ block: 'nearest' }); }, [s.cur]);
  // Una carpeta borrada (aquí o en otro dispositivo) deja de existir: se vuelve a la raíz.
  // (Se lee el estado vivo: el efecto corre después del render y las carpetas pueden haber llegado entretanto.)
  useEffect(() => { const now = S().route; if (S().meta && now.f && now.view === 'all' && !folderOf(now.f)) nav({ f: null }, true); });
  const more = useRef();
  useEffect(() => { const io = new IntersectionObserver((e) => e[0].isIntersecting && !S().loading && S().docs.length < S().total && loadDocs(true)); more.current && io.observe(more.current); return () => io.disconnect(); });
  const title = folder ? folder.name : { inbox: 'Bandeja de entrada', all: 'Todos los documentos', trash: 'Papelera', expiring: 'Vencimientos' }[r.view];
  const filtered = r.q || r.tags.length || r.c || r.t || r.from || r.to;
  const browsing = r.view === 'all' && !filtered, tiles = browsing ? kids(r.f) : [];
  const corr = labelOf('correspondent', r.c), type = labelOf('doctype', r.t);
  const sorts = [['', r.q ? 'Relevancia' : r.view === 'expiring' ? 'Vence antes' : 'Más recientes'], ['created:asc', 'Más antiguos'], ['added:desc', 'Recién añadidos'], ['title:asc', 'Título A–Z']];
  const filter = (key, kind, icon, label, value, onChange, multi) => html`<span class="rel"><button class="chip" onClick=${() => setPop(pop === key ? null : key)}><${Icon} n=${icon} />${label}</button>
    ${pop === key && html`<${Picker} kind=${kind} multi=${multi} value=${value} onChange=${onChange} onClose=${() => setPop(null)} />`}</span>`;
  const empty = filtered ? ['search', 'Nada coincide con tu búsqueda', 'Prueba con otras palabras o quita algún filtro.']
    : folder ? ['folder', 'Esta carpeta está vacía', canWrite() ? 'Lo que subas estando aquí se guarda en esta carpeta. También puedes mover documentos desde su detalle.' : 'Todavía no hay documentos aquí.']
    : r.view === 'inbox' ? ['inbox', 'Bandeja al día', 'Aquí aparecerá lo nuevo que aún no has revisado.']
    : r.view === 'expiring' ? ['clock', 'Nada vence pronto', 'Aquí aparecen los documentos vencidos o que vencen en los próximos 60 días.']
    : r.view === 'trash' ? ['trash', 'La papelera está vacía', 'Lo que borres se queda aquí hasta que lo restaures o lo borres del todo.']
    : ['upload', 'Tu archivo está vacío', touch ? 'Toca + para fotografiar un documento o elegir archivos.' : 'Arrastra aquí tus PDF, fotos o documentos de Office para empezar.'];
  return html`<main class="main">
    <div class="topbar">
      <div class="row"><div class="search grow"><${Icon} n="search" />
          <input ref=${input} id="q" type="search" enterkeyhint="search" autocomplete="off" placeholder=${folder ? `Buscar en ${folder.name}…` : 'Buscar documentos…'} defaultValue=${r.q} onInput=${(e) => setQ(e.target.value)}
            onKeyDown=${(e) => { if (e.key === 'Escape' || e.key === 'ArrowDown' || e.key === 'Enter') { e.target.blur(); e.preventDefault(); } }} />
          <span class="hint"><kbd>/</kbd><kbd>⌘K</kbd></span></div>
        <button class="avatar only-narrow" title=${`${s.user.username}: ajustes`} onClick=${() => store.set({ modal: 'settings:account' })}>${s.user.username[0]}</button></div>
      <div class="filters">
        ${r.tags.map((id) => labelOf('tag', id)).filter(Boolean).map((l) => html`<${Tag} l=${l} onRemove=${() => nav({ tags: r.tags.filter((x) => x !== l.id) })} />`)}
        ${corr && html`<button class="chip on" onClick=${() => nav({ c: null })}><${Icon} n="user" /><span>${corr.name}</span>×</button>`}
        ${type && html`<button class="chip on" onClick=${() => nav({ t: null })}><${Icon} n="type" /><span>${type.name}</span>×</button>`}
        ${filter('tag', 'tag', 'tag', 'Etiqueta', r.tags, (tags) => nav({ tags }), true)}
        ${filter('c', 'correspondent', 'user', 'Corresponsal', r.c, (c) => nav({ c }))}
        ${filter('t', 'doctype', 'type', 'Tipo', r.t, (t) => nav({ t }))}
        <label class=${cls('chip', r.from && 'on')}>Desde<input type="date" value=${r.from} onChange=${(e) => nav({ from: e.target.value })} /></label>
        <label class=${cls('chip', r.to && 'on')}>Hasta<input type="date" value=${r.to} onChange=${(e) => nav({ to: e.target.value })} /></label>
        ${filtered && html`<button class="btn ghost sm" onClick=${() => { input.current.value = ''; nav({ q: '', tags: [], c: null, t: null, from: '', to: '' }); }}>Limpiar</button>`}
      </div>
    </div>
    <div class=${cls('list scroll', s.picked.size && 'selecting')}>
      <div class="list-head">${folder && html`<${Crumbs} f=${folder} />`}
        <div class="row"><h1 class="grow">${title}</h1>${folder && canWrite() && html`<${MenuButton} items=${folderMenu(folder)} title="Opciones de la carpeta" right cls="btn icon ghost" />`}</div>
        <div class="row"><span class="sub grow">${s.total} ${s.total === 1 ? 'documento' : 'documentos'}${s.amountTotal != null ? ` · suman ${fmtMoney(s.amountTotal)}` : ''}</span>
          <select title="Ordenar" value=${r.sort} onChange=${(e) => nav({ sort: e.target.value })}>${sorts.map(([v, l]) => html`<option value=${v}>${l}</option>`)}</select>
          <button class="btn icon ghost sm" title="Cambiar vista (V)" onClick=${toggleGrid}><${Icon} n=${s.grid ? 'rows' : 'grid'} /></button></div></div>
      ${browsing && (tiles.length > 0 || canWrite()) && html`<div class="tiles">
        ${tiles.map((f) => html`<button class="tile" key=${f.id} onClick=${() => nav({ f: f.id, doc: null })} ...${dropOn(f.id)}><${Icon} n="folder" /><span class="grow"><span class="name">${f.name}</span>
          <small>${f.document_count} ${f.document_count === 1 ? 'documento' : 'documentos'}</small></span></button>`)}
        ${canWrite() && html`<button class="tile new" onClick=${() => newFolder(r.f)}><${Icon} n="plus" /><span class="name">Nueva carpeta</span></button>`}</div>`}
      ${!s.docs.length && !s.loading ? html`<div class="empty"><div><div class="big"><${Icon} n=${empty[0]} /></div><h3>${empty[1]}</h3><div>${empty[2]}</div>
          ${!filtered && r.view === 'all' && canWrite() && html`<button class="btn primary" onClick=${addDocs}><${Icon} n="upload" />Añadir documentos</button>`}</div></div>`
        : html`<div class=${s.grid ? 'grid' : ''}>${s.docs.map((d, i) => html`<${DocRow} d=${d} i=${i} key=${d.id} />`)}</div><div ref=${more} style="height:1px" />`}
    </div></main>`;
}
function toggleGrid() { localStorage.grid = S().grid ? '0' : '1'; store.set({ grid: !S().grid }); }

// ───────────── detalle ─────────────
function PageImg({ id, n }) {
  const [bad, setBad] = useState(false);
  if (bad) return html`<div class="page-error"><${Icon} n="help" /><b>No se pudo mostrar la página ${n}</b>
    <span>El texto extraído sigue disponible en la pestaña Texto. Si el problema continúa, un administrador puede revisarlo en Ajustes → Salud.</span>
    <button class="btn sm" onClick=${() => setBad(false)}>Reintentar</button></div>`;
  return html`<img loading=${n > 1 ? 'lazy' : 'eager'} src=${`/api/documents/${id}/pages/${n}?v=2`} alt=${`Página ${n}`} onError=${() => setBad(true)} />`;
}
function Prop({ k, icon, children, picker }) {
  const [open, setOpen] = useState(false);
  useEffect(() => { const f = (e) => e.detail === k && setOpen(true); addEventListener('open-prop', f); return () => removeEventListener('open-prop', f); }, []);
  return html`<div class="prop"><div class="k"><${Icon} n=${icon} />${k}</div>
    ${picker && canWrite() ? html`<button class="v" onClick=${() => setOpen(true)}>${children}</button>${open && picker(() => setOpen(false))}` : picker ? html`<div class="v">${children}</div>` : children}</div>`;
}
function Detail({ id }) {
  const s = useStore(), [d, setD] = useState(null), [tab, setTab] = useState('doc'), [note, setNote] = useState(''), [busy, setBusy] = useState(false);
  const load = () => api(`/documents/${id}`).then(setD).catch(() => { toast('Ese documento ya no existe', 'bad'); nav({ doc: null }, true); });
  useEffect(() => { setD(null); setTab('doc'); load(); const f = (e) => e.detail.id === id && e.detail.type === 'document.updated' && load(); addEventListener('doc-changed', f); return () => removeEventListener('doc-changed', f); }, [id]);
  if (!d) return html`<section class="detail"><div class="viewer"><div class="viewer-bar"><button class="btn icon ghost" onClick=${() => nav({ doc: null })} title="Volver"><${Icon} n="back" /></button></div><div class="empty"><span class="spinner" /></div></div></section>`;
  const save = async (patch) => { const nd = await attempt(api(`/documents/${id}`, { method: 'PATCH', json: patch })); if (nd) { setD(nd); refresh(); } };
  const pdf = (d.has_archive || d.mime === 'application/pdf') && d.pages > 0, trashed = !!d.deleted_at, ro = !canWrite();
  const mine = isAdmin() || d.owner_id === s.user.id;
  const corr = labelOf('correspondent', d.correspondent_id), type = labelOf('doctype', d.doctype_id), folder = folderOf(d.folder_id);
  const inbox = d.tags.filter((t) => inboxIds().includes(t));
  const words = s.route.q.replace(/"/g, ' ').split(/\s+/).filter((w) => w.length > 1).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const text = () => words.length ? d.content.split(new RegExp(`(${words.join('|')})`, 'gi')).map((p, i) => (i % 2 ? html`<mark>${p}</mark>` : p)) : d.content;
  const ai = async () => { setBusy(true); const nd = await attempt(api(`/documents/${id}/ai`, { method: 'POST' }), 'Clasificado con IA'); nd && setD(nd); setBusy(false); refresh(); };
  const addNote = async () => { if (note.trim() && await attempt(api(`/documents/${id}/notes`, { json: { text: note } }))) { setNote(''); load(); } };
  const del = async () => {
    if (trashed && !await ask({ title: '¿Borrar definitivamente?', text: `«${d.title}» y sus archivos se eliminarán del servidor. No se puede deshacer.`, ok: 'Borrar para siempre', danger: true })) return;
    await attempt(api(`/documents/${id}${trashed ? '?permanent=true' : ''}`, { method: 'DELETE' }), trashed ? 'Borrado definitivamente' : 'Enviado a la papelera'); nav({ doc: null }, true); refresh();
  };
  const sg = suggestions(d);
  const suggest = async (action, fields) => {
    const nd = await attempt(api(`/documents/${id}/suggest`, { json: { action, fields } }));
    if (!nd) return;
    setD(nd); refresh();
    if (action === 'refresh' && !suggestions(nd).pend.length) toast(suggestions(nd).auto.length ? 'Se aplicó lo que estaba claro; no quedan sugerencias' : 'Sin sugerencias: aún no hay documentos parecidos ya archivados', 'warn');
  };
  const file = `/api/documents/${id}/file`, view = tab === 'meta' ? 'doc' : tab;
  const files = [pdf && ['ext', 'Abrir el PDF en otra pestaña', () => window.open(file, '_blank')], ['down', `Descargar el original (${fmtSize(d.size)})`, () => { location.href = `${file}?variant=original&download=true`; }],
    d.has_archive && ['down', 'Descargar el PDF con texto (OCR)', () => { location.href = `${file}?variant=archive&download=true`; }]];
  return html`<section class=${`detail tab-${tab}`}>
    <div class="viewer">
      <div class="viewer-bar"><button class="btn icon ghost" onClick=${() => nav({ doc: null })} title="Volver (Esc)"><${Icon} n="back" /></button>
        <div class=${cls('seg', !pdf && 'only-narrow')}>${pdf && html`<button class=${tab === 'doc' ? 'on' : ''} onClick=${() => setTab('doc')}>Páginas</button>`}
          <button class=${tab === 'text' || (!pdf && tab === 'doc') ? 'on' : ''} onClick=${() => setTab(pdf ? 'text' : 'doc')}>Texto</button>
          <button class=${cls('only-narrow', tab === 'meta' && 'on')} onClick=${() => setTab('meta')}>Detalles</button></div>
        <span class="grow" /><${MenuButton} items=${files} icon="down" title="Abrir o descargar" right cls="btn ghost sm"><span class="only-wide">Descargar</span><//></div>
      ${view === 'text' || !pdf ? html`<div class="textview scroll">${d.content ? text() : html`<span class="muted">Este documento no tiene texto extraído.</span>`}</div>`
        : html`<div class="pages scroll" key=${id}>${Array.from({ length: Math.max(d.pages, 1) }, (_, i) => html`<${PageImg} id=${id} n=${i + 1} />`)}</div>`}
    </div>
    <div class="meta scroll">
      <textarea class="title-input" rows="1" key=${id} defaultValue=${d.title} title="Título (toca para editar)" readonly=${ro} enterkeyhint="done"
        onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); e.target.blur(); } }} onBlur=${(e) => !ro && e.target.value.trim() && e.target.value !== d.title && save({ title: e.target.value.trim() })} />
      ${trashed && html`<div class="row"><span class="chip on">En la papelera</span>${!ro && html`<button class="btn sm" onClick=${async () => { await attempt(api(`/documents/${id}/restore`, { method: 'POST' }), 'Restaurado'); load(); refresh(); }}><${Icon} n="restore" />Restaurar</button>`}</div>`}
      ${inbox.length > 0 && !trashed && !ro && html`<button class="btn primary" onClick=${() => save({ remove_tags: inbox })}><${Icon} n="check" />Marcar como revisado <kbd class="only-wide" style="background:none;border-color:currentColor;color:inherit;opacity:.7">E</kbd></button>`}
      ${sg.pend.length > 0 && !ro && !trashed && html`<div class="suggest"><h5><span class="row"><${Icon} n="spark" />Sugerencias</span>
          ${sg.pend.length > 1 && html`<button class="btn primary sm" onClick=${() => suggest('accept')}>Aceptar todas</button>`}</h5>
        ${sg.pend.map((x) => html`<div class="sg" key=${x.key}><${Icon} n=${x.icon} /><span class="grow"><small>${x.label} · ${x.similar} ${x.similar === 1 ? 'parecido' : 'parecidos'}</small><b>${x.name}</b></span>
          <button class="btn icon sm ok" title="Aceptar" onClick=${() => suggest('accept', [x.key])}><${Icon} n="check" /></button>
          <button class="btn icon ghost sm" title="Descartar" onClick=${() => suggest('dismiss', [x.key])}><${Icon} n="x" /></button></div>`)}
        <small class="faint">Por parecido con documentos que ya archivaste. Se calcula en tu servidor.</small></div>`}
      <div class="props">
        <${Prop} k="Carpeta" icon="folder" picker=${(close) => html`<${FolderPicker} value=${d.folder_id} onChange=${(v) => save({ folder_id: v })} onClose=${close} />`}>${folder ? folder.path.replaceAll('/', ' › ') : html`<span class="faint">Sin carpeta</span>`}<//>
        <${Prop} k="Fecha" icon="cal"><input class="v" type="date" value=${d.created} disabled=${ro} onChange=${(e) => e.target.value && save({ created: e.target.value })} /><//>
        <${Prop} k="Corresponsal" icon="user" picker=${(close) => html`<${Picker} kind="correspondent" value=${d.correspondent_id} onChange=${(v) => save({ correspondent_id: v })} onClose=${close} />`}>${corr ? corr.name : html`<span class="faint">Sin asignar</span>`}<//>
        <${Prop} k="Tipo" icon="type" picker=${(close) => html`<${Picker} kind="doctype" value=${d.doctype_id} onChange=${(v) => save({ doctype_id: v })} onClose=${close} />`}>${type ? type.name : html`<span class="faint">Sin asignar</span>`}<//>
        <${Prop} k="Etiquetas" icon="tag" picker=${(close) => html`<${Picker} kind="tag" multi value=${d.tags} onChange=${(tags) => save({ tags })} onClose=${close} />`}>
          ${d.tags.length ? d.tags.map((t) => labelOf('tag', t)).filter(Boolean).map((l) => html`<${Tag} l=${l} key=${l.id} onRemove=${!ro && (() => save({ remove_tags: [l.id] }))} />`) : html`<span class="faint">${ro ? 'Ninguna' : 'Añadir…'}</span>`}<//>
        <${Prop} k="Vence" icon="clock"><input class="v" type="date" value=${d.expires || ''} disabled=${ro} onChange=${(e) => save({ expires: e.target.value || null })} /><//>
        <${Prop} k="Importe" icon="cash"><input class="v" type="number" inputmode="decimal" step="0.01" min="0" placeholder="—" key=${`${id}-${d.amount}`} disabled=${ro} defaultValue=${d.amount ?? ''} onBlur=${(e) => String(d.amount ?? '') !== e.target.value && save({ amount: e.target.value ? Number(e.target.value) : null })} /><//>
        <${Prop} k="N.º archivo" icon="key"><input class="v" type="number" inputmode="numeric" min="0" placeholder="—" key=${id} disabled=${ro} defaultValue=${d.asn ?? ''} onBlur=${(e) => String(d.asn ?? '') !== e.target.value && save({ asn: e.target.value ? Number(e.target.value) : null })} /><//>
        ${(d.is_private || (mine && !ro)) && html`<${Prop} k="Privado" icon="lock"><label class="v" title="Solo lo ven quien lo subió y los administradores"><input type="checkbox" checked=${d.is_private} disabled=${!mine || ro} onChange=${(e) => save({ is_private: e.target.checked })} /> ${d.is_private ? 'Solo tú y los administradores' : 'Visible para todos los usuarios'}</label><//>`}
      </div>
      ${!ro && !trashed && html`<div class="row wrap auto-note">${sg.auto.length > 0 && html`<small class="faint grow">Asignado solo por parecido: ${sg.auto.map((x) => `${x.label.toLowerCase()} «${x.name}»`).join(', ')}.</small>`}
        ${!sg.pend.length && html`<button class="btn ghost sm" onClick=${() => suggest('refresh')}><${Icon} n="spark" />Buscar sugerencias</button>`}</div>`}
      ${(d.summary || (s.llm && !ro)) && html`<div class="section"><h5>Resumen ${s.llm && !ro && html`<button class="btn ghost sm" disabled=${busy} onClick=${ai}>${busy ? html`<span class="spinner" />` : html`<${Icon} n="spark" />`}${d.summary ? 'Regenerar' : 'Clasificar con IA'}</button>`}</h5>
        ${d.summary && html`<div class="summary">${d.summary}</div>`}</div>`}
      ${Object.keys(d.custom).length > 0 && html`<div class="section"><h5>Campos</h5><div class="facts">${Object.entries(d.custom).map(([k, v]) => html`<div>${k}: <b>${String(v)}</b></div>`)}</div></div>`}
      <div class="section"><h5>Notas</h5>
        ${d.notes.map((n) => html`<div class="note" key=${n.id}>${n.text}<small>${fmtDate(n.created)}</small>${!ro && html`<button title="Borrar nota" onClick=${async () => { await attempt(api(`/notes/${n.id}`, { method: 'DELETE' })); load(); }}><${Icon} n="x" /></button>`}</div>`)}
        ${!ro && html`<textarea class="input" rows="2" placeholder="Escribe una nota…" value=${note} onInput=${(e) => setNote(e.target.value)} onKeyDown=${(e) => (e.metaKey || e.ctrlKey) && e.key === 'Enter' && addNote()} />`}
        ${ro && !d.notes.length && html`<span class="faint">Sin notas</span>`}
        ${note.trim() && html`<button class="btn sm" style="justify-self:start" onClick=${addNote}>Guardar nota</button>`}</div>
      <div class="section"><h5>Archivo</h5><div class="facts">
        <div><b>${d.original_name}</b></div><div>${fmtSize(d.size)} · ${d.pages} ${d.pages === 1 ? 'página' : 'páginas'} · ${d.mime}</div>
        <div>Añadido el ${fmtDate(d.added)}</div><div class="faint" title="SHA-256 del original">${d.sha256.slice(0, 16)}…</div></div>
        ${!ro && (!trashed || isAdmin()) && html`<button class="btn danger sm" style="justify-self:start" onClick=${del}><${Icon} n="trash" />${trashed ? 'Borrar definitivamente' : 'Enviar a la papelera'}</button>`}</div>
    </div></section>`;
}

// ───────────── edición masiva ─────────────
function BulkBar() {
  const s = useStore(), [pop, setPop] = useState(null), ids = s.picked, trash = s.route.view === 'trash';
  const btn = (key, icon, label, kind, apply) => html`<span class="rel"><button class="btn sm" onClick=${() => setPop(pop === key ? null : key)}><${Icon} n=${icon} />${label}</button>
    ${pop === key && (kind === 'folder' ? html`<${FolderPicker} value=${-1} onClose=${() => setPop(null)} onChange=${(v) => moveTo(ids, v)} />`
      : html`<${Picker} kind=${kind} multi=${kind === 'tag'} value=${kind === 'tag' ? [] : null} onClose=${() => setPop(null)} onChange=${(v, l) => { apply(v, l); setPop(null); }} />`)}</span>`;
  const withSug = s.docs.filter((d) => ids.has(d.id) && suggestions(d).pend.length);
  const acceptAll = async () => { await Promise.all(withSug.map((d) => attempt(api(`/documents/${d.id}/suggest`, { json: { action: 'accept' } })))); toast(`Sugerencias aceptadas en ${withSug.length} ${withSug.length === 1 ? 'documento' : 'documentos'}`); store.set({ picked: new Set() }); refresh(); };
  const purge = async () => await ask({ title: `¿Borrar definitivamente ${ids.size} ${ids.size === 1 ? 'documento' : 'documentos'}?`, text: 'Se eliminarán del servidor con sus archivos. No se puede deshacer.', ok: 'Borrar para siempre', danger: true }) && bulk(ids, { action: 'purge' }, 'Borrados definitivamente');
  return html`<div class="bulk"><span>${ids.size} ${ids.size === 1 ? 'seleccionado' : 'seleccionados'}</span><span class="sep" />
    ${trash ? html`<button class="btn sm" onClick=${() => bulk(ids, { action: 'restore' }, 'Restaurados')}><${Icon} n="restore" />Restaurar</button>
      ${isAdmin() && html`<button class="btn sm" onClick=${purge}><${Icon} n="trash" />Borrar para siempre</button>`}`
    : html`${withSug.length > 0 && html`<button class="btn sm" onClick=${acceptAll}><${Icon} n="spark" />Aceptar sugerencias</button>`}${btn('f', 'folder', 'Mover', 'folder')}
      ${btn('tag', 'tag', 'Etiquetar', 'tag', (v, l) => bulk(ids, { add_tags: [l.id] }, `Etiqueta «${l.name}» añadida`))}
      ${btn('c', 'user', 'Corresponsal', 'correspondent', (v, l) => bulk(ids, { correspondent_id: l.id }, `Corresponsal: ${l.name}`))}
      ${btn('t', 'type', 'Tipo', 'doctype', (v, l) => bulk(ids, { doctype_id: l.id }, `Tipo: ${l.name}`))}
      ${inboxIds().length > 0 && html`<button class="btn sm" onClick=${() => bulk(ids, { remove_tags: inboxIds() }, 'Marcados como revisados')}><${Icon} n="check" />Revisado</button>`}
      <button class="btn sm" onClick=${() => bulk(ids, { action: 'trash' }, 'Enviados a la papelera')}><${Icon} n="trash" />Papelera</button>`}
    <span class="sep" /><button class="btn icon sm" title="Quitar selección (Esc)" onClick=${() => store.set({ picked: new Set() })}><${Icon} n="x" /></button></div>`;
}

// ───────────── paleta de comandos ─────────────
function Palette() {
  const s = useStore(), [q, setQ] = useState(''), [hi, setHi] = useState(0), [found, setFound] = useState([]);
  const close = () => store.set({ palette: false });
  const search = useMemo(() => debounce(async (q) => setFound(q.trim().length > 1 ? (await api('/documents?limit=6&q=' + encodeURIComponent(q))).items : []), 90), []);
  const actions = [
    ['Subir documentos', 'upload', () => pickFiles(), 'U'], ['Ir a la bandeja de entrada', 'inbox', () => nav({ ...HOME, view: 'inbox' }), 'G I'], ['Ir a todos los documentos', 'files', () => nav({ ...HOME, view: 'all' }), 'G A'],
    ['Ir a la papelera', 'trash', () => nav({ ...HOME, view: 'trash' }), 'G T'], ['Ir a vencimientos', 'clock', () => nav({ ...HOME, view: 'expiring' })], ...(canWrite() ? [['Nueva carpeta', 'folder', () => newFolder(here()?.id), 'N']] : []), [isDark() ? 'Cambiar a modo claro' : 'Cambiar a modo oscuro', isDark() ? 'sun' : 'moon', () => setTheme(isDark() ? 'light' : 'dark'), 'D'],
    ['Ajustes', 'settings', () => store.set({ modal: 'settings' }), ','], ['Tokens de API y MCP', 'key', () => store.set({ modal: 'settings:tokens' })], ...(isAdmin() ? [['Salud del archivo', 'activity', () => store.set({ modal: 'settings:health' })]] : []), ['Documentación de la API', 'code', () => store.set({ modal: 'settings:api' })],
    ['Atajos de teclado', 'help', () => store.set({ modal: 'help' }), '?'], ['Cerrar sesión', 'logout', () => api('/auth/logout', { method: 'POST' }).then(() => location.reload())],
    ...s.folders.map((f) => [`Carpeta: ${f.path.replaceAll('/', ' › ')}`, 'folder', () => nav({ ...HOME, view: 'all', f: f.id })]),
    ...s.labels.tag.map((l) => [`Etiqueta: ${l.name}`, 'tag', () => nav({ tags: [l.id], doc: null })]),
    ...s.labels.correspondent.map((l) => [`Corresponsal: ${l.name}`, 'user', () => nav({ c: l.id, doc: null })]),
    ...s.labels.doctype.map((l) => [`Tipo: ${l.name}`, 'type', () => nav({ t: l.id, doc: null })]),
  ].filter(([l]) => l.toLowerCase().includes(q.trim().toLowerCase())).slice(0, q ? 7 : 9);
  const items = [...found.map((d) => [d.title, 'files', () => nav({ doc: d.id }), fmtDate(d.created)]), ...actions];
  const run = (i) => { close(); items[i]?.[2](); };
  const key = (e) => {
    if (e.key === 'ArrowDown') { setHi((hi + 1) % items.length); e.preventDefault(); } else if (e.key === 'ArrowUp') { setHi((hi - 1 + items.length) % items.length); e.preventDefault(); }
    else if (e.key === 'Enter') run(hi); else if (e.key === 'Escape') close();
  };
  return html`<div class="veil" onMouseDown=${(e) => e.target === e.currentTarget && close()}><div class="palette">
    <input autofocus ref=${(el) => el?.focus()} placeholder="Busca un documento o escribe una acción…" value=${q} onInput=${(e) => { setQ(e.target.value); setHi(0); search(e.target.value); }} onKeyDown=${key} />
    <div class="opts scroll">${items.map(([label, icon, , hint], i) => html`<button class=${cls('opt', i === hi && 'hi')} onMouseEnter=${() => setHi(i)} onClick=${() => run(i)}><${Icon} n=${icon} /><span>${label}</span>${hint && html`<small>${hint}</small>`}</button>`)}
      ${!items.length && html`<div class="opt muted">Nada coincide</div>`}</div></div></div>`;
}


// ───────────── ajustes ─────────────
const ALGOS = [['none', 'Sin regla'], ['any', 'Alguna palabra'], ['all', 'Todas las palabras'], ['literal', 'Frase exacta'], ['regex', 'Expresión regular']];
function Labels({ kind, path, title, blurb }) {
  const s = useStore(), [name, setName] = useState('');
  const patch = (l, json) => attempt(api(`/${path}/${l.id}`, { method: 'PATCH', json })).then(loadMeta);
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/' + path, { json: { name } }))) { setName(''); loadMeta(); } };
  return html`<h2>${title}</h2><p>${blurb}</p>
    <form class="row" style="margin-bottom:12px" onSubmit=${add}><input class="input" placeholder="Nombre nuevo…" value=${name} onInput=${(e) => setName(e.target.value)} required /><button class="btn primary">Añadir</button></form>
    <table class="table"><tr><th></th><th>Nombre</th><th>Asignar si el texto contiene</th><th>Regla</th>${kind === 'tag' && html`<th title="Se asigna a todo documento nuevo">Bandeja</th>`}<th>Docs</th><th></th></tr>
    ${s.labels[kind].map((l) => html`<tr key=${l.id}>
      <td><input type="color" value=${l.color} onChange=${(e) => patch(l, { color: e.target.value })} /></td>
      <td><input class="input" defaultValue=${l.name} onBlur=${(e) => e.target.value.trim() && e.target.value !== l.name && patch(l, { name: e.target.value.trim() })} /></td>
      <td><input class="input" defaultValue=${l.match} placeholder="p. ej. CFE electricidad" onBlur=${(e) => e.target.value !== l.match && patch(l, { match: e.target.value, algo: l.algo === 'none' && e.target.value ? 'any' : l.algo })} /></td>
      <td><select class="input" value=${ALGOS.some(([v]) => v === l.algo) ? l.algo : 'all'} onChange=${(e) => patch(l, { algo: e.target.value })}>${ALGOS.map(([v, t]) => html`<option value=${v}>${t}</option>`)}</select></td>
      ${kind === 'tag' && html`<td style="text-align:center"><input type="checkbox" checked=${l.is_inbox} onChange=${(e) => patch(l, { is_inbox: e.target.checked })} /></td>`}
      <td class="muted">${l.document_count}</td>
      <td><button class="btn icon ghost sm" title="Borrar" onClick=${async () => await ask({ title: `¿Borrar «${l.name}»?`, text: 'Los documentos se conservan; solo pierden esta asignación.', ok: 'Borrar', danger: true }) && attempt(api(`/${path}/${l.id}`, { method: 'DELETE' })).then(refresh)}><${Icon} n="trash" /></button></td></tr>`)}</table>`;
}
function Tokens() {
  const [list, setList] = useState([]), [f, setF] = useState({ name: '', read_only: false, expires_days: '' }), [fresh, setFresh] = useState(null);
  const load = () => api('/tokens').then(setList); useEffect(() => { load(); }, []);
  const add = async (e) => { e.preventDefault(); const t = await attempt(api('/tokens', { json: { ...f, expires_days: f.expires_days ? Number(f.expires_days) : null } })); if (t) { setFresh(t.token); setF({ name: '', read_only: false, expires_days: '' }); load(); } };
  const tok = fresh || 'ppr_TU_TOKEN';
  return html`<h2>Tokens de API y MCP</h2><p>Un token da acceso a tu archivo desde scripts, automatizaciones o asistentes de IA. Solo se muestra una vez; en el servidor se guarda cifrado con hash.</p>
    <form style="display:grid;gap:8px;margin-bottom:12px" onSubmit=${add}><input class="input" placeholder="¿Para qué es? (p. ej. n8n, Claude)" value=${f.name} onInput=${(e) => setF({ ...f, name: e.target.value })} required />
      <div class="row" style="flex-wrap:wrap"><label class="row"><input type="checkbox" checked=${f.read_only} onChange=${(e) => setF({ ...f, read_only: e.target.checked })} />Solo lectura</label>
        <select class="input" style="width:auto" value=${f.expires_days} onChange=${(e) => setF({ ...f, expires_days: e.target.value })}><option value="">No caduca</option><option value="30">Caduca en 30 días</option><option value="90">Caduca en 90 días</option><option value="365">Caduca en 1 año</option></select>
        <span class="grow" /><button class="btn primary">Crear token</button></div></form>
    ${fresh && html`<div class="section" style="border:0;padding:0;margin-bottom:14px"><b>Copia tu token ahora; no se volverá a mostrar:</b><div class="code">${fresh}</div></div>`}
    <table class="table"><tr><th>Nombre</th><th>Permiso</th><th>Caduca</th><th>Último uso</th><th></th></tr>${list.map((t) => html`<tr><td>${t.name}</td><td class="muted">${t.read_only ? 'Solo lectura' : 'Completo'}</td><td class="muted">${t.expires ? fmtDate(t.expires) : 'Nunca'}</td><td class="muted">${t.last_used ? fmtDate(t.last_used) : 'Nunca'}</td>
      <td><button class="btn ghost sm danger" onClick=${() => attempt(api(`/tokens/${t.id}`, { method: 'DELETE' }), 'Token revocado').then(load)}>Revocar</button></td></tr>`)}</table>
    <h2 style="margin-top:26px">Conectar una IA (MCP)</h2><p>Papiro incluye un servidor MCP para que un asistente busque, lea, suba y etiquete documentos. Con un token de solo lectura, solo puede buscar y leer.</p>
    <div class="code">claude mcp add --transport http papiro ${location.origin}/mcp --header "Authorization: Bearer ${tok}"</div>
    <p style="margin:14px 0 6px" class="muted">O por la API REST:</p>
    <div class="code">curl -H "Authorization: Bearer ${tok}" "${location.origin}/api/documents?q=factura"</div>`;
}
function Webhooks() {
  const [list, setList] = useState([]), [f, setF] = useState({ url: '', events: '*', secret: '' });
  const load = () => api('/webhooks').then(setList).catch(() => {}); useEffect(() => { load(); }, []);
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/webhooks', { json: f }), 'Webhook creado')) { setF({ url: '', events: '*', secret: '' }); load(); } };
  return html`<h2>Webhooks</h2><p>Papiro avisa por HTTP POST cuando un documento se crea, cambia o se borra. Con secreto, cada aviso va firmado (cabecera X-Papiro-Signature, HMAC-SHA256).</p>
    <form style="display:grid;gap:8px;margin-bottom:14px" onSubmit=${add}><input class="input" type="url" placeholder="https://tu-servidor/webhook" value=${f.url} onInput=${(e) => setF({ ...f, url: e.target.value })} required />
      <div class="row"><select class="input" value=${f.events} onChange=${(e) => setF({ ...f, events: e.target.value })}><option value="*">Todos los eventos</option><option value="document.created">Solo documentos nuevos</option><option value="document.created,document.updated">Nuevos y modificados</option></select>
        <input class="input" placeholder="Secreto de firma (opcional)" value=${f.secret} onInput=${(e) => setF({ ...f, secret: e.target.value })} /><button class="btn primary">Añadir</button></div></form>
    <table class="table"><tr><th>URL</th><th>Eventos</th><th>Última entrega</th><th></th></tr>${list.map((w) => html`<tr><td style="overflow-wrap:anywhere">${w.url}${w.signed && html` <span class="chip">firmado</span>`}</td><td class="muted">${w.events === '*' ? 'todos' : w.events}</td>
      <td class="muted">${w.last_status ? `${w.last_status} · ${fmtDate(w.last_at)}` : '—'}</td>
      <td class="row"><button class="btn ghost sm" onClick=${() => attempt(api(`/webhooks/${w.id}/test`, { method: 'POST' })).then((r) => { r && toast(`Respuesta: ${r.status}`, r.status.startsWith('2') ? 'ok' : 'bad'); load(); })}>Probar</button>
        <button class="btn icon ghost sm" onClick=${() => attempt(api(`/webhooks/${w.id}`, { method: 'DELETE' })).then(load)}><${Icon} n="trash" /></button></td></tr>`)}</table>`;
}
const ROLES = [['admin', 'Administrador', 'todo, incluido borrar definitivamente y gestionar usuarios'], ['editor', 'Editor', 'sube y edita; no borra definitivamente'], ['viewer', 'Lector', 'solo busca, lee y descarga']];
function Account() {
  const s = useStore(), [users, setUsers] = useState([]), [pw, setPw] = useState({ current: '', new: '' }), [nu, setNu] = useState({ username: '', password: '', role: 'editor' });
  const load = () => isAdmin() && api('/users').then(setUsers); useEffect(() => { load(); }, []);
  const change = async (e) => { e.preventDefault(); await attempt(api('/auth/password', { json: pw }).then(() => setPw({ current: '', new: '' })), 'Contraseña actualizada'); };
  const add = async (e) => { e.preventDefault(); if (await attempt(api('/users', { json: nu }), 'Usuario creado')) { setNu({ username: '', password: '', role: 'editor' }); load(); } };
  const reset = async (u) => { const p = await ask({ title: `Nueva contraseña para ${u.username}`, text: 'Mínimo 8 caracteres. Sus sesiones abiertas se cierran.', input: true, type: 'password', minlength: 8, ok: 'Restablecer' }); if (p) await attempt(api(`/users/${u.id}`, { method: 'PATCH', json: { password: p } }), 'Contraseña restablecida'); };
  return html`<h2>Cuenta</h2><p>Sesión iniciada como <b>${s.user.username}</b> (${ROLES.find((r) => r[0] === s.user.role)?.[1]}). <a href="#" onClick=${(e) => { e.preventDefault(); api('/auth/logout', { method: 'POST' }).then(() => location.reload()); }}>Cerrar sesión</a></p>
    <form class="row" style="flex-wrap:wrap" onSubmit=${change}><input class="input" style="flex:1;min-width:150px" type="password" placeholder="Contraseña actual" value=${pw.current} onInput=${(e) => setPw({ ...pw, current: e.target.value })} required />
      <input class="input" style="flex:1;min-width:150px" type="password" placeholder="Nueva (mín. 8)" minlength="8" value=${pw.new} onInput=${(e) => setPw({ ...pw, new: e.target.value })} required /><button class="btn">Cambiar</button></form>
    ${isAdmin() && html`<h2 style="margin-top:26px">Usuarios</h2><p>${ROLES.map(([, n, d]) => html`<b>${n}</b>: ${d}. `)}Un documento marcado como privado solo lo ven quien lo subió y los administradores.</p>
      <form class="row" style="margin-bottom:12px;flex-wrap:wrap" onSubmit=${add}><input class="input" style="flex:1;min-width:120px" placeholder="Usuario" value=${nu.username} onInput=${(e) => setNu({ ...nu, username: e.target.value })} required />
        <input class="input" style="flex:1;min-width:120px" type="password" placeholder="Contraseña" minlength="8" value=${nu.password} onInput=${(e) => setNu({ ...nu, password: e.target.value })} required />
        <select class="input" style="width:auto" value=${nu.role} onChange=${(e) => setNu({ ...nu, role: e.target.value })}>${ROLES.map(([v, n]) => html`<option value=${v}>${n}</option>`)}</select><button class="btn primary">Añadir</button></form>
      <table class="table">${users.map((u) => html`<tr><td>${u.username}</td>
        <td><select class="input" style="width:auto" value=${u.role} disabled=${u.id === s.user.id} onChange=${(e) => attempt(api(`/users/${u.id}`, { method: 'PATCH', json: { role: e.target.value } }), 'Rol actualizado').then(load)}>${ROLES.map(([v, n]) => html`<option value=${v}>${n}</option>`)}</select></td>
        <td style="text-align:right;white-space:nowrap"><button class="btn ghost sm" onClick=${() => reset(u)}>Restablecer contraseña</button>
          ${u.id !== s.user.id && html`<button class="btn ghost sm danger" onClick=${async () => await ask({ title: `¿Borrar al usuario ${u.username}?`, text: 'Sus documentos se conservan.', ok: 'Borrar usuario', danger: true }) && attempt(api(`/users/${u.id}`, { method: 'DELETE' })).then(load)}>Borrar</button>`}</td></tr>`)}</table>`}`;
}
function Health() {
  const [st, setSt] = useState(null), [busy, setBusy] = useState(false);
  const load = () => api('/status').then(setSt); useEffect(() => { load(); }, []);
  if (!st) return html`<span class="spinner" />`;
  const v = st.last_verify, b = st.last_backup, ok = v && !v.problems.length && v.database === 'ok' && v.search_index_ok;
  const verify = async () => { setBusy(true); await attempt(api('/verify', { method: 'POST' }), 'Verificación terminada'); setBusy(false); load(); };
  return html`<h2>Salud del archivo</h2><p>Papiro ${st.version} · esquema ${st.schema} · OCR ${st.ocr_langs}${st.ocr_rotate ? ' con corrección de páginas giradas' : ''}.</p>
    <div class="health">
      <div class=${cls('card', v ? (ok ? 'good' : 'bad') : '')}><b>Integridad</b>
        ${v ? html`<span>${ok ? `Los ${v.documents} documentos tienen su archivo y su huella SHA-256 coincide.` : `${v.problems.length} documento(s) con problemas.`}</span>
          <small>Última verificación: ${fmtDate(v.at)} (${v.seconds} s) · base de datos: ${v.database} · índice de búsqueda: ${v.search_index_ok ? 'ok' : 'dañado'}</small>`
          : html`<span>Aún no se ha verificado. Se hace sola cada día; también puedes lanzarla ahora.</span>`}
        <button class="btn sm" style="justify-self:start" disabled=${busy} onClick=${verify}>${busy ? html`<span class="spinner" />` : null}Verificar ahora</button></div>
      <div class=${cls('card', st.backup_every_hours ? (b?.ok === false ? 'bad' : b ? 'good' : '') : 'warn')}><b>Respaldo</b>
        ${st.backup_every_hours ? html`<span>Automático cada ${st.backup_every_hours} h.</span>` : html`<span>El respaldo automático está <b>desactivado</b>. Define <code>PAPIRO_BACKUP_DIR</code> apuntando a otro volumen o disco, o ejecuta <code>papiro backup</code> desde tu propio programador.</span>`}
        ${b && html`<small>${b.ok === false ? `Último intento fallido: ${b.error}` : `Último respaldo: ${fmtDate(b.at)} · ${b.documents} documentos · ${fmtSize(b.bytes)} en ${b.dest}${b.history ? ` · ${b.history} ${b.history === 1 ? 'copia diaria' : 'copias diarias'} de la base` : ''}`}</small>`}</div>
      <div class=${cls('card', st.failed_jobs ? 'warn' : 'good')}><b>Ingesta</b><span>${st.queued} en cola · ${st.failed_jobs} fallidos${st.failed_jobs ? ' (su archivo se conservó: revísalos en Actividad)' : ''} · ${st.pending_webhooks} avisos de webhook pendientes.</span></div>
    </div>
    ${v?.problems.length > 0 && html`<h2 style="margin-top:22px">Documentos con problemas</h2><p>Si conservas el archivo, vuelve a subirlo: Papiro reconoce que es el mismo y repone la copia dañada.</p>
      <table class="table">${v.problems.map((p) => html`<tr><td><a href="#" onClick=${(e) => { e.preventDefault(); store.set({ modal: null }); nav({ doc: p.document_id }); }}>${p.title}</a></td><td class="err">${p.problem}</td></tr>`)}</table>`}`;
}
function Audit() {
  const [rows, setRows] = useState(null); useEffect(() => { api('/audit?limit=200').then(setRows); }, []);
  if (!rows) return html`<span class="spinner" />`;
  return html`<h2>Auditoría</h2><p>Quién hizo qué y desde dónde: inicios de sesión, subidas, ediciones, borrados y cambios de administración.</p>
    <table class="table"><tr><th>Cuándo</th><th>Quién</th><th>Qué</th><th>Sobre</th><th>IP</th></tr>
    ${rows.map((r) => html`<tr><td class="muted" style="white-space:nowrap">${new Date(r.at.replace(' ', 'T') + 'Z').toLocaleString('es-MX', { dateStyle: 'short', timeStyle: 'short' })}</td><td>${r.username}</td><td>${r.action}</td>
      <td class="muted" style="overflow-wrap:anywhere">${r.target}${r.detail ? ` — ${r.detail}` : ''}</td><td class="faint">${r.ip}</td></tr>`)}</table>`;
}
function ApiDocs() {
  const [spec, setSpec] = useState(null); useEffect(() => { api('/openapi.json').then(setSpec); }, []);
  if (!spec) return html`<span class="spinner" />`;
  const groups = {};
  for (const [path, ops] of Object.entries(spec.paths)) for (const [m, op] of Object.entries(ops)) (groups[op.tags?.[0] || 'otros'] ||= []).push({ path, m, op });
  const schema = (c) => { const sc = c && Object.values(c)[0]?.schema; return sc ? JSON.stringify(deref(sc), null, 1) : null; };
  const deref = (sc, depth = 0) => { if (!sc || typeof sc !== 'object' || depth > 4) return sc; if (sc.$ref) return deref(spec.components.schemas[sc.$ref.split('/').pop()], depth + 1);
    return Array.isArray(sc) ? sc.map((x) => deref(x, depth)) : Object.fromEntries(Object.entries(sc).map(([k, v]) => [k, deref(v, depth + 1)])); };
  return html`<div class="apidoc"><h2>${spec.info.title} <small class="muted">v${spec.info.version}</small></h2><p style="white-space:pre-wrap">${spec.info.description}</p>
    <p>Especificación OpenAPI 3.1: <a href="/api/openapi.json" target="_blank">/api/openapi.json</a></p>
    ${Object.entries(groups).map(([tag, ops]) => html`<h3>${tag}</h3>${ops.map(({ path, m, op }) => html`<details class="op"><summary><span class=${`method ${m}`}>${m}</span><span class="path">${path}</span><span class="muted grow">${op.summary}</span></summary>
      <div class="opbody">${op.description && html`<p>${op.description}</p>`}
        ${op.parameters?.length > 0 && html`<table class="table">${op.parameters.map((p) => html`<tr><td><code>${p.name}</code></td><td class="muted">${p.in}${p.required ? ' · obligatorio' : ''}</td><td>${p.description || ''}</td></tr>`)}</table>`}
        ${op.requestBody && html`<p><b>Cuerpo</b> (${Object.keys(op.requestBody.content)[0]})</p><div class="code">${schema(op.requestBody.content)}</div>`}
        <p><b>Respuestas</b></p>${Object.entries(op.responses).map(([code, r]) => html`<details><summary><span class="method">${code}</span><span class="muted">${r.description || (r.content ? Object.keys(r.content)[0] : '')}</span></summary>${schema(r.content) && html`<div class="code">${schema(r.content)}</div>`}</details>`)}
        <p><b>Ejemplo</b></p><div class="code">curl -X ${m.toUpperCase()} -H "Authorization: Bearer $PAPIRO_TOKEN" "${location.origin}${path}"</div></div></details>`)}`)}</div>`;
}
function Settings({ tab }) {
  const [t, setT] = useState(tab || 'tags'), s = useStore();
  useEffect(() => { document.querySelector('.modal aside .nav.on')?.scrollIntoView({ inline: 'center', block: 'nearest' }); }, [t]);
  const tabs = [['tags', 'tag', 'Etiquetas'], ['correspondents', 'user', 'Corresponsales'], ['doctypes', 'type', 'Tipos'], ['tokens', 'key', 'Tokens y MCP'], ...(isAdmin() ? [['webhooks', 'hook', 'Webhooks'], ['health', 'activity', 'Salud'], ['audit', 'rows', 'Auditoría']] : []), ['api', 'code', 'API'], ['account', 'settings', 'Cuenta']];
  return html`<div class="veil full" onMouseDown=${(e) => e.target === e.currentTarget && store.set({ modal: null })}><div class="modal">
    <aside><button class="btn icon ghost close" title="Cerrar" onClick=${() => store.set({ modal: null })}><${Icon} n="back" /></button><h3>Ajustes</h3>${tabs.map(([k, icon, label]) => html`<button class=${cls('nav', t === k && 'on')} onClick=${() => setT(k)}><${Icon} n=${icon} /><span class="name">${label}</span></button>`)}</aside>
    <section class="scroll">${t === 'tags' ? html`<${Labels} kind="tag" path="tags" title="Etiquetas" blurb="Las etiquetas marcadas como «Bandeja» se ponen solas a cada documento nuevo; quítalas al revisarlo." />`
      : t === 'correspondents' ? html`<${Labels} kind="correspondent" path="correspondents" title="Corresponsales" blurb="Quién emite o envía el documento: empresas, bancos, dependencias, personas." />`
      : t === 'doctypes' ? html`<${Labels} kind="doctype" path="doctypes" title="Tipos de documento" blurb="Factura, contrato, recibo, póliza… Las reglas asignan el tipo al ingerir." />`
      : t === 'tokens' ? html`<${Tokens} />` : t === 'webhooks' ? html`<${Webhooks} />` : t === 'health' ? html`<${Health} />` : t === 'audit' ? html`<${Audit} />` : t === 'api' ? html`<${ApiDocs} />` : html`<${Account} />`}</section></div></div>`;
}
function Help() {
  const keys = [['/', 'Buscar'], ['⌘K · Ctrl K', 'Paleta de comandos'], ['J · ↓', 'Siguiente documento'], ['K · ↑', 'Documento anterior'], ['↵ · O', 'Abrir vista previa'], ['Esc', 'Cerrar / quitar selección'],
    ['X', 'Seleccionar'], ['Shift + clic', 'Seleccionar rango'], ['⌘A · Ctrl A', 'Seleccionar todo'], ['E', 'Marcar como revisado'], ['T', 'Etiquetar'], ['C', 'Corresponsal'], ['M', 'Mover a una carpeta'], ['N', 'Nueva carpeta'],
    ['#  ·  Supr', 'Enviar a la papelera'], ['U', 'Subir documentos'], ['V', 'Lista / cuadrícula'], ['D', 'Modo claro / oscuro'], ['G luego I · A · T', 'Bandeja · Todos · Papelera'], [',', 'Ajustes'], ['?', 'Esta ayuda']];
  return html`<div class="veil center" onMouseDown=${(e) => e.target === e.currentTarget && store.set({ modal: null })}><div class="modal narrow scroll" style="width:min(640px,100%)"><div class="row"><h2 class="grow">Atajos de teclado</h2><button class="btn icon ghost sm" title="Cerrar" onClick=${() => store.set({ modal: null })}><${Icon} n="x" /></button></div><p class="muted">Todo se puede hacer sin soltar el teclado.</p>
    <div class="keys">${keys.map(([k, l]) => html`<div><span>${l}</span><kbd>${k}</kbd></div>`)}</div></div></div>`;
}


// ───────────── raíz y teclado ─────────────
let gPending = 0;
function onKey(e) {
  const s = S(), el = e.target, typing = /INPUT|TEXTAREA|SELECT/.test(el.tagName) || el.isContentEditable, mod = e.metaKey || e.ctrlKey;
  if (s.dialog) { if (e.key === 'Escape') { s.dialog.resolve(null); store.set({ dialog: null }); } return; }
  if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); return store.set({ palette: !s.palette }); }
  if (e.key === 'Escape') {
    if (s.palette || s.modal || s.jobsOpen || s.sheet || s.menu) return store.set({ palette: false, modal: null, jobsOpen: false, sheet: null, menu: false });
    if (typing) return el.blur();
    if (s.picked.size) return store.set({ picked: new Set() });
    if (s.route.doc) return nav({ doc: null });
    return;
  }
  if (typing || s.palette || s.modal || !s.user || e.altKey) return;
  const cur = s.docs[s.cur], targets = s.picked.size ? s.picked : s.route.doc ? new Set([s.route.doc]) : cur ? new Set([cur.id]) : new Set();
  const move = (d) => { const i = Math.max(0, Math.min(s.docs.length - 1, s.cur + d)); store.set({ cur: i }); if (s.route.doc && s.docs[i]) nav({ doc: s.docs[i].id }, true); };
  if (mod && e.key.toLowerCase() === 'a') { e.preventDefault(); return store.set({ picked: new Set(s.docs.map((d) => d.id)) }); }
  if (mod) return;
  if (gPending && Date.now() - gPending < 900) { gPending = 0; const v = { i: 'inbox', a: 'all', t: 'trash' }[e.key.toLowerCase()]; if (v) { e.preventDefault(); return nav({ ...HOME, view: v }); } }
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key, prop = (name) => () => s.route.doc && dispatchEvent(new CustomEvent('open-prop', { detail: name }));
  const map = {
    '/': () => document.getElementById('q')?.focus(), j: () => move(1), ArrowDown: () => move(1), k: () => move(-1), ArrowUp: () => move(-1),
    Enter: () => cur && nav({ doc: cur.id }), o: () => cur && nav({ doc: cur.id }), x: () => cur && togglePick(cur.id, s.cur), g: () => { gPending = Date.now(); },
    u: () => pickFiles(), v: toggleGrid, d: () => setTheme(isDark() ? 'light' : 'dark'), ',': () => store.set({ modal: 'settings' }), '?': () => store.set({ modal: 'help' }),
    n: () => canWrite() && s.route.view === 'all' && newFolder(s.route.f),
    e: () => targets.size && inboxIds().length && bulk(targets, { remove_tags: inboxIds() }, 'Marcado como revisado'),
    '#': () => targets.size && s.route.view !== 'trash' && bulk(targets, { action: 'trash' }, 'Enviado a la papelera'), Delete: () => map['#'](),
    t: prop('Etiquetas'), c: prop('Corresponsal'), m: prop('Carpeta'),
  };
  if (map[k]) { e.preventDefault(); map[k](); }
}
function App() {
  const s = useStore(), [drag, setDrag] = useState(false);
  useEffect(() => {
    let depth = 0;
    const has = (e) => [...(e.dataTransfer?.types || [])].includes('Files');
    const enter = (e) => { if (has(e)) { depth++; setDrag(true); e.preventDefault(); } }, leave = () => { if (--depth <= 0) { depth = 0; setDrag(false); } };
    const over = (e) => has(e) && e.preventDefault(), drop = (e) => { if (has(e)) { e.preventDefault(); depth = 0; setDrag(false); S().user && upload(e.dataTransfer.files); } };
    addEventListener('keydown', onKey); addEventListener('dragenter', enter); addEventListener('dragleave', leave); addEventListener('dragover', over); addEventListener('drop', drop);
    addEventListener('paste', (e) => S().user && !/INPUT|TEXTAREA/.test(e.target.tagName) && e.clipboardData.files.length && upload(e.clipboardData.files));
    boot();
  }, []);
  if (!s.ready) return html`<div class="auth"><span class="spinner" /></div>`;
  if (s.down) return html`<div class="auth"><div class="empty"><div><div class="big"><${Icon} n="activity" /></div><h3>Sin conexión con Papiro</h3>
    <div>Tus documentos están en tu servidor: hace falta conexión para verlos. Se reintenta solo en cuanto vuelva.</div>
    <button class="btn primary" onClick=${() => { store.set({ ready: false }); boot(); }}>Reintentar</button></div></div></div>`;
  if (s.setup || !s.user) return html`<${Auth} setup=${s.setup} /><${Toasts} />`;
  const [modal, tab] = (s.modal || '').split(':'), folder = here();
  return html`${!s.online && html`<div class="offline">Sin conexión: verás lo último que se cargó</div>`}
    <div class=${cls('shell', s.route.doc && 'has-doc')}>
    <${Sidebar} /><${Main} />${s.route.doc && html`<${Detail} id=${s.route.doc} key=${s.route.doc} />`}</div>
    ${s.menu && html`<div class="scrim only-narrow" onClick=${() => store.set({ menu: false })} />`}
    ${!s.route.doc && html`<${Tabbar} />`}
    ${s.picked.size > 0 && canWrite() && html`<${BulkBar} />`}${s.jobsOpen && html`<${Jobs} />`}${s.sheet === 'upload' && html`<${UploadSheet} />`}${s.palette && html`<${Palette} />`}
    ${modal === 'settings' && html`<${Settings} tab=${tab} />`}${modal === 'help' && html`<${Help} />`}${s.dialog && html`<${Dialog} d=${s.dialog} key=${s.dialog.id} />`}
    ${drag && html`<div class="drop"><div>Suelta para añadir a ${folder ? `«${folder.name}»` : 'Papiro'}<small>PDF, imágenes, Word, Excel, PowerPoint, texto</small></div></div>`}<${Toasts} />`;
}
function Toasts() {
  const s = useStore(), icon = { ok: 'check', bad: 'x', warn: 'help' };
  return html`<div class="toasts">${s.toasts.map((t) => html`<div class=${cls('toast', t.kind === 'bad' && 'bad')} key=${t.id}><span class=${t.kind}><${Icon} n=${icon[t.kind]} /></span><span>${t.text}</span></div>`)}</div>`;
}
render(html`<${App} />`, document.getElementById('app'));
