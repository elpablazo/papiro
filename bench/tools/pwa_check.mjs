// Comprueba la PWA de un Papiro local con Chrome headless (CDP): service worker, manifiesto instalable,
// arranque sin conexión y «Compartir con Papiro». Uso: node pwa_check.mjs <url> <usuario> <contraseña> <puerto-cdp>
const [url, user, pass, port] = process.argv.slice(2), out = {};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let target;
for (let i = 0; i < 50 && !target; i++) { target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).then((l) => l.find((t) => t.type === 'page')).catch(() => null); await sleep(200); }
const ws = new WebSocket(target.webSocketDebuggerUrl); await new Promise((r) => (ws.onopen = r));
let id = 0; const waiting = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && waiting.has(d.id)) { waiting.get(d.id)(d); waiting.delete(d.id); } };
const cdp = (method, params = {}) => new Promise((res) => { waiting.set(++id, res); ws.send(JSON.stringify({ id, method, params })); }).then((d) => d.result ?? d.error);
const js = async (expression) => { const r = await cdp('Runtime.evaluate', { expression: `(async () => { ${expression} })()`, awaitPromise: true, returnByValue: true }); return r.exceptionDetails ? 'EXCEPCIÓN: ' + (r.exceptionDetails.exception?.description || r.exceptionDetails.text) : r.result.value; };
const go = async (u) => { await cdp('Page.navigate', { url: u }); await sleep(1500); };

await cdp('Page.enable'); await cdp('Network.enable');
await go(url);
out.login = await js(`return (await fetch('/api/auth/login', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'papiro' }, body: JSON.stringify({ username: ${JSON.stringify(user)}, password: ${JSON.stringify(pass)} }) })).status`);
await go(url);
out.service_worker = await js(`const r = await navigator.serviceWorker.ready; return { estado: r.active.state, alcance: r.scope, controla_la_pagina: !!navigator.serviceWorker.controller }`);
out.cache_de_interfaz = await js(`return (await (await caches.open('papiro-shell-v1')).keys()).map((r) => new URL(r.url).pathname)`);
const man = await cdp('Page.getAppManifest');
out.manifiesto = { url: man.url, errores: man.errors, nombre: JSON.parse(man.data || '{}').name, iconos: JSON.parse(man.data || '{}').icons?.length, share_target: !!JSON.parse(man.data || '{}').share_target };
out.errores_de_instalabilidad = (await cdp('Page.getInstallabilityErrors')).installabilityErrors;
out.iconos = await js(`const o = {}; for (const p of ['/assets/icon-192.png', '/assets/icon-512.png', '/assets/icon-maskable.png', '/assets/apple-touch-icon.png']) { const b = await createImageBitmap(await (await fetch(p)).blob()); o[p] = b.width + 'x' + b.height; } return o`);

// sin conexión: la interfaz abre desde la copia local
await cdp('Network.emulateNetworkConditions', { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
await go(url);
out.sin_conexion = await js(`return { interfaz_cargada: !!document.querySelector('#app > *'), aviso: document.querySelector('.offline')?.textContent || null, mensaje: document.querySelector('.empty h3')?.textContent || null, cargando_sin_fin: !!document.querySelector('.auth > .spinner') }`);
await cdp('Network.emulateNetworkConditions', { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
await js(`dispatchEvent(new Event('online'))`); await sleep(1500);
out.al_volver_la_conexion = await js(`return document.querySelector('.shell') ? 'la app se recupera sola' : 'sigue en la pantalla de sin conexión'`);
await go(url);

// compartir: el service worker recibe el POST de /share, guarda el archivo y la app lo sube al abrirse
const marca = 'COMPARTIDO' + Date.now();
out.compartir = await js(`const fd = new FormData(); fd.append('files', new File(['Nota compartida desde otra app ${marca}'], 'nota-compartida.txt', { type: 'text/plain' }));
  const r = await fetch('/share', { method: 'POST', body: fd }); return { respuesta: r.status, redirigido_a: new URL(r.url).pathname, en_espera: (await (await caches.open('papiro-share')).keys()).length }`);
await go(url); await sleep(3500);
out.compartido_subido = await js(`const d = await (await fetch('/api/documents?q=${marca}', { headers: { 'X-Requested-With': 'papiro' } })).json(); return { documentos: d.total, titulo: d.items[0]?.title, quedan_en_espera: (await (await caches.open('papiro-share')).keys()).length }`);
out.assets_con_etag = await js(`const a = await fetch('/assets/app.css', { cache: 'no-store' }); const b = await fetch('/assets/app.css', { cache: 'no-store', headers: { 'If-None-Match': a.headers.get('ETag') || 'x' } }); return { etag: a.headers.get('ETag'), revalidacion: b.status }`);
console.log(JSON.stringify(out, null, 1));
ws.close();
