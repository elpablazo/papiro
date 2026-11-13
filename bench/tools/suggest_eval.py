#!/usr/bin/env python3
"""Mide las sugerencias por parecido de Papiro dejando un documento fuera cada vez: se le quita el tipo y el
corresponsal, se pide un análisis nuevo y se compara lo propuesto con lo que tenía. Solo stdlib.
Uso: suggest_eval.py <url> <token>"""
import json, sys, time, urllib.request
URL, TOK = sys.argv[1:3]
def call(method, path, body=None):
    r = urllib.request.Request(URL + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                               headers={'Authorization': 'Bearer ' + TOK, 'Content-Type': 'application/json'})
    return json.load(urllib.request.urlopen(r))
docs = call('GET', '/api/documents?limit=200')['items']
out, t_total = {}, 0.0
for field in ('doctype_id', 'correspondent_id'):
    s = dict(con_valor=0, sugeridos=0, sugeridos_bien=0, aplicados_solos=0, aplicados_bien=0)
    for d in docs:
        truth = d[field]
        if truth is None: continue
        s['con_valor'] += 1
        call('PATCH', f"/api/documents/{d['id']}", {field: None})
        t0 = time.time(); got = call('POST', f"/api/documents/{d['id']}/suggest", {'action': 'refresh'}); t_total += time.time() - t0
        sug = got['suggest'].get(field)
        if sug:
            k = 'aplicados' if sug['applied'] else 'sugeridos'
            s[k + ('_solos' if sug['applied'] else '')] += 1
            s[k + '_bien'] += sug['value'] == truth
        call('PATCH', f"/api/documents/{d['id']}", {field: truth})
        call('POST', f"/api/documents/{d['id']}/suggest", {'action': 'dismiss'})
    s['sin_propuesta'] = s['con_valor'] - s['sugeridos'] - s['aplicados_solos']
    out[field] = s
out['documentos'] = len(docs); out['ms_por_analisis'] = round(t_total / max(1, sum(v['con_valor'] for v in out.values() if isinstance(v, dict))) * 1000, 1)
print(json.dumps(out, ensure_ascii=False, indent=1))
