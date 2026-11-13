#!/usr/bin/env python3
"""Pone los mismos títulos, corresponsales, tipos y etiquetas en ambos sistemas (para las capturas).
Uso: seed.py <paperless|papiro>"""
import json, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from bench import ROOT, Client

sysname = sys.argv[1]
cl = Client(sysname)
P = sysname == "paperless"
manifest = [m for d in ("docs", "docs2") for m in json.loads((ROOT / d / "manifest.json").read_text())]
TAGS = {"Factura": "Finanzas", "Invoice": "Finanzas", "Recibo": "Finanzas", "Receipt": "Finanzas", "Estado de cuenta": "Banco",
        "Contrato": "Legal", "Póliza": "Seguros", "Carta": "Correspondencia"}
COLORS = {"Finanzas": "#16a34a", "Banco": "#2563eb", "Legal": "#9333ea", "Seguros": "#0891b2", "Correspondencia": "#d97706"}
paths = dict(tag="/api/tags/", correspondent="/api/correspondents/", doctype="/api/document_types/") if P else \
        dict(tag="/api/tags", correspondent="/api/correspondents", doctype="/api/doctypes")
cache = {k: {} for k in paths}


def label(kind, name):
    if not cache[kind]:
        r = cl.req("GET", paths[kind] + ("?page_size=1000" if P else ""))
        cache[kind] = {x["name"]: x["id"] for x in (r["results"] if P else r)}
    if name not in cache[kind]:
        body = dict(name=name)
        if kind == "tag":
            body["color"] = COLORS[name]
        cache[kind][name] = cl.req("POST", paths[kind], json_body=body)["id"]
    return cache[kind][name]


from urllib.parse import quote

n = 0
for m in manifest:  # cada documento se localiza por su folio único (los nombres de archivo se repiten entre sets)
    r = cl.req("GET", f"/api/documents/?query={quote('"' + m['ref'] + '"')}&page_size=3" if P else f"/api/documents?q={quote('"' + m['ref'] + '"')}&limit=3")
    hits = r["results"] if P else r["items"]
    if not hits:
        print("  sin coincidencia:", m["file"], m["ref"], len(hits))
        continue
    d = hits[0]
    title = f"{m['tipo']} {m['emisor']} {m['ref']}"
    c, t, tag = label("correspondent", m["emisor"]), label("doctype", m["tipo"]), label("tag", TAGS[m["tipo"]])
    if P:
        cl.req("PATCH", f"/api/documents/{d['id']}/", json_body=dict(title=title, correspondent=c, document_type=t, tags=[tag]))
    else:
        cl.req("PATCH", f"/api/documents/{d['id']}", json_body=dict(title=title, correspondent_id=c, doctype_id=t, tags=[tag]))
    n += 1
print(n, "documentos actualizados")
