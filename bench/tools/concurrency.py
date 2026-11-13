#!/usr/bin/env python3
"""Concurrencia y varios usuarios. Uso: concurrency.py <paperless|papiro> <puerto> <etiqueta>
  1. 10 subidas simultáneas del MISMO archivo           → ¿cuántos documentos se crean?
  2. 10 clientes añaden cada uno una etiqueta distinta al mismo documento, a la vez → ¿se pierde alguna?
     10 clientes añaden una nota al mismo documento a la vez                      → ¿se pierde alguna?
  3. 8 usuarios durante 60 s: búsquedas, lecturas, ediciones, subidas y ediciones masivas mezcladas
     → errores por código, latencias por operación y, al final, ¿están todos los documentos subidos?
"""
import json, random, statistics, sys, threading, time, uuid
from collections import Counter, defaultdict
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).parent))
import bench
from bench import ROOT, Client, pct

import os
if os.environ.get("TOKEN"):
    bench.SECRETS["PAPIRO_TOKEN"] = os.environ["TOKEN"]
sysname, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]
bench.SYSTEMS[sysname]["port"] = port
P = sysname == "paperless"
res = {}


def count():
    c = Client(sysname)
    return c.req("GET", "/api/documents/?page_size=1")["count"] if P else c.req("GET", "/api/stats")["documents"]


def idle():
    c = Client(sysname)
    for _ in range(900):
        busy = (c.req("GET", "/api/tasks/?status=started&page_size=1")["count"] + c.req("GET", "/api/tasks/?status=pending&page_size=1")["count"]) if P \
            else c.req("GET", "/api/stats")["queued"]
        if not busy:
            return
        time.sleep(1)


def doc_ids(n):
    c = Client(sysname)
    r = c.req("GET", f"/api/documents/?page_size={n}") if P else c.req("GET", f"/api/documents?limit={n}")
    return [d["id"] for d in (r["results"] if P else r["items"])]


# --- 1. mismo archivo, 10 veces a la vez
src = ROOT / "scale" / "concurrency-same.pdf"
if not src.exists():
    from reportlab.pdfgen import canvas
    cv = canvas.Canvas(str(src)); cv.drawString(50, 800, f"Documento de carrera {uuid.uuid4().hex}"); cv.save()
before = count()
ths = [threading.Thread(target=lambda: Client(sysname).upload(src)) for _ in range(10)]
[t.start() for t in ths]; [t.join() for t in ths]
time.sleep(2); idle(); time.sleep(3)
res["mismo_archivo_x10_documentos_creados"] = count() - before

# --- 2. actualizaciones simultáneas sobre un mismo documento
target = doc_ids(1)[0]
c0 = Client(sysname)
tags = []
for i in range(10):
    name = f"conc-{label}-{i}-{uuid.uuid4().hex[:4]}"
    tags.append(c0.req("POST", "/api/tags/" if P else "/api/tags", json_body={"name": name})["id"])
start = threading.Barrier(10)


def add_tag(t):
    c = Client(sysname); start.wait()
    if P:   # en Paperless, PATCH reemplaza la lista: la forma segura es bulk_edit add_tag
        c.req("POST", "/api/documents/bulk_edit/", json_body={"documents": [target], "method": "add_tag", "parameters": {"tag": t}})
    else:
        c.req("PATCH", f"/api/documents/{target}", json_body={"add_tags": [t]})
ths = [threading.Thread(target=add_tag, args=(t,)) for t in tags]; [t.start() for t in ths]; [t.join() for t in ths]
time.sleep(1); idle(); time.sleep(2)
have = set(c0.req("GET", f"/api/documents/{target}/" if P else f"/api/documents/{target}")["tags"])
res["etiquetas_simultaneas_aplicadas"] = f"{len(set(tags) & have)}/10"

start = threading.Barrier(10)
def add_note(i):
    c = Client(sysname); start.wait()
    c.req("POST", f"/api/documents/{target}/notes/" if P else f"/api/documents/{target}/notes", json_body={"note": f"nota {i}"} if P else {"text": f"nota {i}"})
n0 = len(c0.req("GET", f"/api/documents/{target}/notes/") if P else c0.req("GET", f"/api/documents/{target}")["notes"])
ths = [threading.Thread(target=add_note, args=(i,)) for i in range(10)]; [t.start() for t in ths]; [t.join() for t in ths]
n1 = len(c0.req("GET", f"/api/documents/{target}/notes/") if P else c0.req("GET", f"/api/documents/{target}")["notes"])
res["notas_simultaneas_guardadas"] = f"{n1 - n0}/10"

# Lectura-modificación-escritura: dos clientes cambian cosas distintas del mismo documento con PATCH completo
# (como hace la interfaz de Paperless al guardar el formulario)
if P:
    a, b = Client(sysname), Client(sysname)
    da, db = a.req("GET", f"/api/documents/{target}/"), b.req("GET", f"/api/documents/{target}/")
    a.req("PATCH", f"/api/documents/{target}/", json_body={"title": "título de A", "tags": da["tags"] + [tags[0]]})
    b.req("PATCH", f"/api/documents/{target}/", json_body={"title": db["title"], "tags": [t for t in db["tags"] if t != tags[1]]})
    fin = a.req("GET", f"/api/documents/{target}/")
    res["formulario_simultaneo_paperless"] = dict(titulo_de_A_se_conserva=fin["title"] == "título de A")

# --- 3. carga mixta de 8 usuarios durante 60 s
pool = sorted((ROOT / "scale" / "conc").glob("*.pdf"))
uploads, stats, errors, lock = [], defaultdict(list), Counter(), threading.Lock()
ids = doc_ids(100)
words = ["contrato", "electricidad", "payment", "hospitalización", "Telmex", "depósitos"]
stop = time.time() + 60


def user(k):
    c, rnd = Client(sysname), random.Random(k)
    while time.time() < stop:
        op = rnd.choices(["buscar", "leer", "editar", "subir", "masiva"], [60, 20, 12, 4, 4])[0]
        t = time.perf_counter()
        try:
            if op == "buscar":
                q = quote(rnd.choice(words)); c.req("GET", f"/api/documents/?page_size=25&query={q}" if P else f"/api/documents?limit=25&q={q}")
            elif op == "leer":
                c.req("GET", f"/api/documents/{rnd.choice(ids)}/" if P else f"/api/documents/{rnd.choice(ids)}")
            elif op == "editar":
                c.req("PATCH", f"/api/documents/{rnd.choice(ids)}/" if P else f"/api/documents/{rnd.choice(ids)}", json_body={"title": f"edit {k}-{uuid.uuid4().hex[:6]}"})
            elif op == "subir":
                with lock:
                    if not pool: continue
                    f = pool.pop()
                c.upload(f); uploads.append(f.name)
            else:
                sel = rnd.sample(ids, 20)
                if P: c.req("POST", "/api/documents/bulk_edit/", json_body={"documents": sel, "method": "add_tag", "parameters": {"tag": rnd.choice(tags)}})
                else: c.req("POST", "/api/documents/bulk", json_body={"ids": sel, "add_tags": [rnd.choice(tags)]})
            stats[op].append((time.perf_counter() - t) * 1000)
        except Exception as e:
            errors[f"{op}: {str(e)[:90]}"] += 1


before = count()
ths = [threading.Thread(target=user, args=(k,)) for k in range(8)]; [t.start() for t in ths]; [t.join() for t in ths]
t_end = time.time(); idle(); drain = time.time() - t_end; time.sleep(3)
res["carga_mixta"] = dict(usuarios=8, segundos=60, operaciones={k: dict(n=len(v), p50_ms=round(pct(v, 50), 1), p95_ms=round(pct(v, 95), 1)) for k, v in stats.items()},
                          errores=dict(errors), subidas=len(uploads), documentos_nuevos=count() - before, cola_vaciada_en_s=round(drain, 1))
print(json.dumps(res, ensure_ascii=False, indent=1))
out = ROOT / "results" / f"concurrency-{sysname}-{label}.json"; out.write_text(json.dumps(res, ensure_ascii=False, indent=1))
