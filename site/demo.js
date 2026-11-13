// Papiro live demo: the real UI (web/app.js) running against an in-browser fake of the API.
// Loaded as a classic script before app.js. Everything lives in memory: reload to reset.
(() => {
  const realFetch = window.fetch.bind(window);
  const dataP = realFetch('/demo/data.json').then((r) => r.json());
  const NO = 'This is a read-only demo, so uploads and account changes are off. Run your own Papiro with one docker command (see the README).';
  const now = () => new Date().toISOString().slice(0, 19).replace('T', ' ');
  const norm = (s) => (s || '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  let nextId = 1000;

  const json = (body, status = 200) => new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
  const fail = (status, error) => json({ error }, status);

  function folderPaths(db) {
    const byId = Object.fromEntries(db.folders.map((f) => [f.id, f]));
    const path = (f) => { const parts = []; for (let x = f, i = 0; x && i < 64; x = byId[x.parent_id], i++) parts.unshift(x.name); return parts.join('/'); };
    const subtree = (id) => { const out = [id]; for (let i = 0; i < out.length; i++) db.folders.forEach((f) => f.parent_id === out[i] && out.push(f.id)); return out; };
    return { path, subtree };
  }
  const live = (db) => db.documents.filter((d) => !d.deleted_at);
  function labels(db, kind) {
    const col = { tag: null, correspondent: 'correspondent_id', doctype: 'doctype_id' }[kind];
    return db[kind === 'tag' ? 'tags' : kind === 'correspondent' ? 'correspondents' : 'doctypes'].map((l) => ({
      ...l, document_count: live(db).filter((d) => (col ? d[col] === l.id : d.tags.includes(l.id))).length,
    }));
  }
  function folders(db) {
    const { path, subtree } = folderPaths(db);
    return db.folders.map((f) => {
      const ids = subtree(f.id);
      return { ...f, path: path(f), document_count: live(db).filter((d) => ids.includes(d.folder_id)).length };
    }).sort((a, b) => a.path.localeCompare(b.path));
  }
  const inboxIds = (db) => db.tags.filter((t) => t.is_inbox).map((t) => t.id);
  const soon = () => new Date(Date.now() + 60 * 864e5).toISOString().slice(0, 10);
  const brief = (d) => { const { content, notes, ...rest } = d; return rest; };

  function search(db, p) {
    const { subtree } = folderPaths(db);
    let docs = db.documents.filter((d) => (p.get('trash') === 'true' ? d.deleted_at : !d.deleted_at));
    const words = norm(p.get('q')).split(/\s+/).filter(Boolean);
    if (words.length) docs = docs.filter((d) => { const h = norm(d.title + ' ' + d.content); return words.every((w) => h.includes(w)); });
    const tags = (p.get('tags') || '').split(',').filter(Boolean).map(Number);
    if (tags.length) docs = docs.filter((d) => tags.every((t) => d.tags.includes(t)));
    if (p.get('correspondent')) docs = docs.filter((d) => d.correspondent_id === Number(p.get('correspondent')));
    if (p.get('doctype')) docs = docs.filter((d) => d.doctype_id === Number(p.get('doctype')));
    const f = Number(p.get('folder'));
    if (p.has('folder')) docs = docs.filter((d) => (f === 0 ? d.folder_id == null : subtree(f).includes(d.folder_id)));
    if (p.get('from')) docs = docs.filter((d) => d.created >= p.get('from'));
    if (p.get('to')) docs = docs.filter((d) => d.created <= p.get('to'));
    if (p.get('inbox') === 'true') docs = docs.filter((d) => d.tags.some((t) => inboxIds(db).includes(t)));
    if (p.get('expiring') === 'true') docs = docs.filter((d) => d.expires && d.expires <= soon());
    const sort = p.get('sort') || (words.length ? 'relevance' : p.get('expiring') === 'true' ? 'expires' : 'created');
    const dir = p.get('order') === 'asc' ? 1 : -1;
    const key = { created: (d) => d.created + d.id, added: (d) => d.added, title: (d) => norm(d.title), expires: (d) => d.expires || '9999' }[sort];
    if (key) docs = [...docs].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0) * (sort === 'expires' && !p.get('order') ? 1 : dir));
    else docs = [...docs].sort((a, b) => words.filter((w) => norm(b.title).includes(w)).length - words.filter((w) => norm(a.title).includes(w)).length);
    const items = docs.slice(Number(p.get('offset') || 0), Number(p.get('offset') || 0) + Number(p.get('limit') || 50)).map((d) => {
      const out = brief(d);
      if (words.length) {
        const h = norm(d.content), at = Math.max(0, h.indexOf(words[0]) - 50), seg = d.content.slice(at, at + 160);
        out.snippet = (at ? '…' : '') + seg.replace(new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi'), '\u0001$1\u0002') + '…';
      }
      return out;
    });
    const amounts = docs.filter((d) => d.amount != null);
    return { total: docs.length, amount_total: amounts.length ? Math.round(amounts.reduce((s, d) => s + d.amount, 0) * 100) / 100 : null, items };
  }

  function patchDoc(db, d, p) {
    for (const k of ['title', 'created', 'correspondent_id', 'doctype_id', 'asn', 'folder_id', 'expires', 'amount', 'is_private', 'summary']) {
      if (k in p) { d[k] = p[k]; if (d.suggest && d.suggest[k]) delete d.suggest[k]; }
    }
    if (p.tags) d.tags = [...p.tags];
    if (p.add_tags) d.tags = [...new Set([...d.tags, ...p.add_tags])];
    if (p.remove_tags) d.tags = d.tags.filter((t) => !p.remove_tags.includes(t));
    d.modified = now();
  }
  function suggest(d, action, fields) {
    const s = d.suggest || {}, want = (k) => !fields || fields.includes(k);
    for (const k of ['folder_id', 'doctype_id', 'correspondent_id']) {
      if (s[k] && !s[k].applied && want(k)) { if (action === 'accept') d[k] = s[k].value; delete s[k]; }
    }
    if (s.tags) {
      s.tags = s.tags.filter((t) => { if (!want(`tag:${t.value}`)) return true; if (action === 'accept' && !d.tags.includes(t.value)) d.tags.push(t.value); return false; });
      if (!s.tags.length) delete s.tags;
    }
    d.suggest = s;
  }

  async function route(method, url, body) {
    const db = await dataP, path = url.pathname.replace(/^\/api/, ''), p = url.searchParams, m = method.toUpperCase();
    const doc = (id) => db.documents.find((d) => d.id === Number(id));
    let x;
    if (path === '/auth/status') return json({ setup_required: false, user: db.user, llm: false });
    if (path === '/auth/logout') return json(null, 204);
    if (path === '/tags' && m === 'GET') return json(labels(db, 'tag'));
    if (path === '/correspondents' && m === 'GET') return json(labels(db, 'correspondent'));
    if (path === '/doctypes' && m === 'GET') return json(labels(db, 'doctype'));
    if ((x = path.match(/^\/(tags|correspondents|doctypes)$/)) && m === 'POST') {
      const l = { id: nextId++, name: body.name, color: body.color || '#64748b', is_inbox: false, parent_id: null, match: '', algo: 'none', insensitive: true };
      db[x[1]].push(l); return json({ ...l, document_count: 0 }, 201);
    }
    if ((x = path.match(/^\/(tags|correspondents|doctypes)\/(\d+)$/))) {
      const list = db[x[1]], l = list.find((y) => y.id === Number(x[2]));
      if (!l) return fail(404, 'not found');
      if (m === 'DELETE') { list.splice(list.indexOf(l), 1); return json(null, 204); }
      Object.assign(l, body); return json(l);
    }
    if (path === '/folders' && m === 'GET') return json(folders(db));
    if (path === '/folders' && m === 'POST') {
      if (db.folders.some((f) => (f.parent_id ?? null) === (body.parent_id ?? null) && norm(f.name) === norm(body.name))) return fail(409, 'a folder with that name already exists here');
      const f = { id: nextId++, name: body.name.trim(), parent_id: body.parent_id ?? null, created: now() };
      db.folders.push(f); return json(folders(db).find((y) => y.id === f.id), 201);
    }
    if ((x = path.match(/^\/folders\/(\d+)$/))) {
      const f = db.folders.find((y) => y.id === Number(x[1]));
      if (!f) return fail(404, 'not found');
      if (m === 'DELETE') {
        const ids = folderPaths(db).subtree(f.id);
        db.documents.forEach((d) => { if (ids.includes(d.folder_id)) d.folder_id = f.parent_id; });
        db.folders = db.folders.filter((y) => !ids.includes(y.id)); return json(null, 204);
      }
      if ('name' in body) f.name = body.name; if ('parent_id' in body) f.parent_id = body.parent_id;
      return json(folders(db).find((y) => y.id === f.id));
    }
    if (path === '/stats') {
      const docs = live(db);
      return json({ documents: docs.length, inbox: docs.filter((d) => d.tags.some((t) => inboxIds(db).includes(t))).length, trash: db.documents.length - docs.length,
        pages: docs.reduce((s, d) => s + d.pages, 0), bytes: docs.reduce((s, d) => s + d.size, 0), queued: 0, failed: 0,
        expiring: docs.filter((d) => d.expires && d.expires <= soon()).length, version: db.status.version });
    }
    if (path === '/documents' && m === 'GET') return json(search(db, p));
    if (path === '/documents' && m === 'POST') return fail(403, NO);
    if (path === '/documents/bulk') {
      for (const id of body.ids) {
        const d = doc(id); if (!d) continue;
        if (body.action === 'trash') d.deleted_at = now();
        else if (body.action === 'restore') d.deleted_at = null;
        else if (body.action === 'purge') db.documents.splice(db.documents.indexOf(d), 1);
        else patchDoc(db, d, body);
      }
      return json({ affected: body.ids.length });
    }
    if ((x = path.match(/^\/documents\/(\d+)$/))) {
      const d = doc(x[1]); if (!d) return fail(404, 'not found');
      if (m === 'GET') return json(d);
      if (m === 'PATCH') { patchDoc(db, d, body); return json(d); }
      if (m === 'DELETE') { if (p.get('permanent') === 'true') db.documents.splice(db.documents.indexOf(d), 1); else d.deleted_at = now(); return json(null, 204); }
    }
    if ((x = path.match(/^\/documents\/(\d+)\/restore$/))) { const d = doc(x[1]); d.deleted_at = null; return json(d); }
    if ((x = path.match(/^\/documents\/(\d+)\/suggest$/))) { const d = doc(x[1]); suggest(d, body.action, body.fields); return json(d); }
    if ((x = path.match(/^\/documents\/(\d+)\/notes$/))) { const d = doc(x[1]), n = { id: nextId++, text: body.text, created: now() }; d.notes.push(n); return json(n, 201); }
    if ((x = path.match(/^\/notes\/(\d+)$/))) { db.documents.forEach((d) => { d.notes = d.notes.filter((n) => n.id !== Number(x[1])); }); return json(null, 204); }
    if (/^\/documents\/\d+\/ai$/.test(path)) return fail(501, 'no LLM is configured (PAPIRO_LLM_API_KEY)');
    if (path === '/jobs' && m === 'GET') return json(db.jobs);
    if (path === '/status') return json(db.status);
    if (path === '/audit') return json(db.audit);
    if (path === '/openapi.json') return realFetch('/demo/openapi.json');
    if (path === '/users' && m === 'GET') return json(db.users);
    if (path === '/tokens' && m === 'GET') return json(db.tokens);
    if (path === '/webhooks' && m === 'GET') return json(db.webhooks);
    if (path === '/verify') return json(db.status.last_verify);
    return fail(403, NO);
  }

  window.fetch = async (input, init = {}) => {
    const url = new URL(typeof input === 'string' ? input : input.url, location.href);
    if (url.origin !== location.origin || !url.pathname.startsWith('/api/')) return realFetch(input, init);
    let body = {};
    if (typeof init.body === 'string') body = JSON.parse(init.body);
    await new Promise((r) => setTimeout(r, 60)); // a touch of latency, so it feels like a real server
    return route(init.method || (init.body ? 'POST' : 'GET'), url, body);
  };
  window.EventSource = class { constructor() {} close() {} };
})();
