// Service worker de Papiro: deja la app instalable, abre la interfaz sin conexión, recibe
// archivos compartidos desde otras apps y muestra las notificaciones push. Nunca guarda documentos
// ni respuestas de la API.
const SHELL = 'papiro-shell-v3', SHARE = 'papiro-share', OFFLINE = 'papiro-offline';
const FILES = ['/', '/assets/app.js', '/assets/app.css', '/assets/preact.js', '/assets/icon.svg'];

self.addEventListener('install', (e) => e.waitUntil(caches.open(SHELL).then((c) => c.addAll(FILES)).then(() => self.skipWaiting())));
self.addEventListener('activate', (e) => e.waitUntil(
  caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== SHELL && k !== SHARE && k !== OFFLINE).map((k) => caches.delete(k)))).then(() => self.clients.claim())));

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (url.origin !== location.origin) return;
  // «Compartir con Papiro»: los archivos se guardan un momento y la app los sube al abrirse.
  if (e.request.method === 'POST' && url.pathname === '/share') {
    return e.respondWith((async () => {
      const form = await e.request.formData(), cache = await caches.open(SHARE);
      let i = Date.now();
      for (const f of form.getAll('files')) {
        if (f instanceof File) await cache.put(`/shared/${i++}`, new Response(f, { headers: { 'Content-Type': f.type || 'application/octet-stream', 'X-Name': encodeURIComponent(f.name || 'documento') } }));
      }
      return Response.redirect('/', 303);
    })());
  }
  // Documentos guardados «sin conexión»: siempre se pide al servidor; si no contesta, la copia del teléfono.
  if (e.request.method === 'GET' && url.pathname.startsWith('/api/documents/')) {
    return e.respondWith(fetch(e.request).catch(() => caches.open(OFFLINE).then((c) => c.match(e.request)).then((r) => r || Response.error())));
  }
  if (e.request.method !== 'GET' || url.pathname.startsWith('/api/') || url.pathname.startsWith('/mcp') || url.pathname.startsWith('/oauth/') || url.pathname.startsWith('/.well-known/')) return;
  // Interfaz: primero la red (siempre la versión nueva); sin conexión, la copia guardada.
  const key = e.request.mode === 'navigate' ? '/' : e.request;
  e.respondWith(fetch(e.request).then((r) => {
    if (r.ok && (key === '/' || url.pathname.startsWith('/assets/'))) { const copy = r.clone(); caches.open(SHELL).then((c) => c.put(key, copy)); }
    return r;
  }).catch(() => caches.match(key).then((r) => r || Response.error())));
});

// Notificación push: el servidor manda {title, body, url, tag}, cifrado de extremo a extremo con este navegador.
self.addEventListener('push', (e) => {
  let m = {};
  try { m = e.data ? e.data.json() : {}; } catch { m = { body: e.data?.text() }; }
  // El número del ícono: documentos que esperan revisión.
  if (typeof m.badge === 'number' && self.navigator.setAppBadge) (m.badge ? self.navigator.setAppBadge(m.badge) : self.navigator.clearAppBadge()).catch(() => {});
  e.waitUntil(self.registration.showNotification(m.title || 'Papiro', {
    body: m.body || '', tag: m.tag, data: { url: m.url || '/' },
    icon: '/assets/icon-192.png', badge: '/assets/icon-192.png', renotify: !!m.tag, timestamp: Date.now(),
  }));
});
// Al tocarla: si Papiro ya está abierta, se enfoca y navega al documento; si no, se abre.
self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  const url = new URL(e.notification.data?.url || '/', self.location.origin).href;
  e.waitUntil((async () => {
    const wins = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
    const w = wins.find((c) => new URL(c.url).origin === self.location.origin);
    if (w) { await w.focus(); return w.navigate ? w.navigate(url) : w.postMessage({ open: url }); }
    return self.clients.openWindow(url);
  })());
});
