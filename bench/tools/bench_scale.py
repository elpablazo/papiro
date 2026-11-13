#!/usr/bin/env python3
"""Pruebas de escala idénticas para Paperless y Papiro.

  bench_scale.py <sistema> ingest <carpeta>   sube todos los PDF (4 a la vez) y espera a que estén indexados
  bench_scale.py <sistema> probe <etiqueta>   RAM en reposo, latencias de búsqueda/listado/filtro y carga concurrente
Resultados en results/scale-<sistema>.json. Memoria y CPU se leen del cgroup de cada contenedor del stack.
"""
import json, random, statistics, sys, threading, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).parent))
from bench import ROOT, SYSTEMS, Client, Sampler, cgroup, pct

sysname, cmd, arg = sys.argv[1:4]
P = sysname == "paperless"
cl = Client(sysname)
OUT = ROOT / "results" / f"scale-{sysname}.json"
res = json.loads(OUT.read_text()) if OUT.exists() else {}


def cpu_s():
    return sum(int(dict(l.split() for l in (cgroup(c) / "cpu.stat").read_text().splitlines())["usage_usec"]) for c in SYSTEMS[sysname]["containers"]) / 1e6


def count():
    return cl.req("GET", "/api/documents/?page_size=1")["count"] if P else cl.req("GET", "/api/stats")["documents"]


def failures():
    if P:
        return cl.req("GET", "/api/tasks/?status=failure&page_size=1")["count"]
    return sum(1 for j in cl.req("GET", "/api/jobs") if j["status"] == "failed")


def search(q, extra=""):
    if P:
        return cl.req("GET", f"/api/documents/?page_size=25{'&query=' + quote(q) if q else ''}{extra}")
    return cl.req("GET", f"/api/documents?limit=25{'&q=' + quote(q) if q else ''}{extra}")


def timed(fn, n):
    lat = []
    for _ in range(n):
        t = time.perf_counter(); fn(); lat.append((time.perf_counter() - t) * 1000)
    return dict(n=n, p50_ms=round(pct(lat, 50), 2), p95_ms=round(pct(lat, 95), 2), max_ms=round(max(lat), 1))


if cmd == "ingest":
    files = sorted(p for p in Path(arg).iterdir() if p.suffix == ".pdf")
    base, f0 = count(), failures()
    with Sampler(sysname) as s:
        c0, t0 = cpu_s(), time.time()
        with ThreadPoolExecutor(4) as ex:
            list(ex.map(cl.upload, files))
        t_up = time.time() - t0
        last, last_change = base, time.time()
        while True:
            n = count()
            if n != last:
                last, last_change = n, time.time()
            if n - base + (failures() - f0) >= len(files) or time.time() - last_change > 600:
                break
            time.sleep(2)
        total, cpu = time.time() - t0, cpu_s() - c0
    r = dict(files=len(files), created=count() - base, failed=failures() - f0, upload_s=round(t_up, 1), total_s=round(total, 1),
             docs_per_min=round(len(files) / total * 60, 1), cpu_s=round(cpu, 1), avg_cores=round(cpu / total, 2), mem=s.summary())
    res[f"ingest:{Path(arg).name}"] = r
    print(json.dumps(r), flush=True)

if cmd == "probe":
    truth = [m for d in ("scan100", "digital1000", "export10k") if (ROOT / "scale" / d / "truth.json").exists()
             for m in json.loads((ROOT / "scale" / d / "truth.json").read_text())]
    n = count()
    time.sleep(20)
    with Sampler(sysname) as s:
        time.sleep(20)
    idle = s.summary()
    rnd = random.Random(1)
    # Solo folios de documentos que realmente están cargados en esta etapa
    # Solo folios de documentos realmente cargados en esta etapa (100 escaneos → +1 000 digitales → export de 10 000)
    tiers = ["export10k"] if n >= 10000 else ["scan100", "digital1000"] if n > 100 else ["scan100"]
    present = [m for d in tiers for m in json.loads((ROOT / "scale" / d / "truth.json").read_text())]
    sample = rnd.sample(present, min(150, len(present)))
    search("calentamiento")
    hits = sum(1 for m in sample if any(m["ref"] in d["title"] for d in (search(m["ref"])["results" if P else "items"])))
    words = ["electricidad", "contrato", "hospitalización", "payment invoice", "arrendatario renta", "depósitos retiros", "Telmex", "comisiones intereses"]
    by_ref = timed(lambda: search(rnd.choice(sample)["ref"]), 150)
    by_word = timed(lambda: search(rnd.choice(words)), 150)
    listing = timed(lambda: search(""), 50)
    flt = "&created__date__gte=2020-01-01&created__date__lte=2022-12-31" if P else "&from=2020-01-01&to=2022-12-31"
    filtered = timed(lambda: search("contrato", flt), 50)
    last = max(1, n // 25)   # última página completa
    deep = timed(lambda: search("", f"&page={last}" if P else f"&offset={(last - 1) * 25}"), 20)
    # Carga: 8 clientes buscando a la vez durante 20 s
    stop, lat, errors = time.time() + 20, [], [0]

    def worker():
        c = Client(sysname)
        while time.time() < stop:
            q = rnd.choice(words + [m["ref"] for m in sample[:20]])
            t = time.perf_counter()
            try:
                (c.req("GET", f"/api/documents/?page_size=25&query={quote(q)}") if P else c.req("GET", f"/api/documents?limit=25&q={quote(q)}"))
                lat.append((time.perf_counter() - t) * 1000)
            except Exception:
                errors[0] += 1
    with Sampler(sysname) as s2:
        c0 = cpu_s()
        ths = [threading.Thread(target=worker) for _ in range(8)]
        [t.start() for t in ths]; [t.join() for t in ths]
        cpu = cpu_s() - c0
    res[f"probe:{arg}"] = dict(documents=n, idle=idle, folio_hits=f"{hits}/{len(sample)}", search_folio=by_ref, search_words=by_word, list_first_page=listing,
                               search_filtered=filtered, list_deep_page=deep,
                               load=dict(clients=8, seconds=20, requests=len(lat), rps=round(len(lat) / 20, 1), p50_ms=round(pct(lat, 50), 1),
                                         p95_ms=round(pct(lat, 95), 1), errors=errors[0], avg_cores=round(cpu / 20, 2), mem=s2.summary()))
    print(json.dumps(res[f"probe:{arg}"]), flush=True)

OUT.write_text(json.dumps(res, indent=1, ensure_ascii=False))
