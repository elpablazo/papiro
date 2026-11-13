// Service worker de Papiro: deja la app instalable, abre la interfaz sin conexión y recibe
// archivos compartidos desde otras apps. Nunca guarda documentos ni respuestas de la API.
const SHELL = 'papiro-shell-v1', SHARE = 'papiro-share';
const FILES = ['/', '/assets/app.js', '/assets/app.css', '/assets/preact.js', '/assets/icon.svg'];

self.addEventListener('install', (e) => e.waitUntil(caches.open(SHELL).then((c) => c.addAll(FILES)).then(() => self.skipWaiting())));
self.addEventListener('activate', (e) => e.waitUntil(
  caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== SHELL && k !== SHARE).map((k) => caches.delete(k)))).then(() => self.clients.claim())));

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
  if (e.request.method !== 'GET' || url.pathname.startsWith('/api/') || url.pathname.startsWith('/mcp')) return;
  // Interfaz: primero la red (siempre la versión nueva); sin conexión, la copia guardada.
  const key = e.request.mode === 'navigate' ? '/' : e.request;
  e.respondWith(fetch(e.request).then((r) => {
    if (r.ok && (key === '/' || url.pathname.startsWith('/assets/'))) { const copy = r.clone(); caches.open(SHELL).then((c) => c.put(key, copy)); }
    return r;
  }).catch(() => caches.match(key).then((r) => r || Response.error())));
});
