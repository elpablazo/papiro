#!/usr/bin/env python3
"""Benchmark idéntico para Paperless-ngx y Papiro. Solo stdlib.

Uso: bench.py <paperless|papiro> <etiqueta> [fases...]   (fases: idle batch seq search big restart)
Escribe results/<sistema>-<etiqueta>.json. La memoria se lee del cgroup de cada contenedor:
working set = memory.current - inactive_file + swap (lo de `docker stats` más lo enviado a swap) y anon (RSS anónima + swap).
"""
import http.client, json, os, statistics, subprocess, sys, threading, time, uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SECRETS = dict(l.strip().split("=", 1) for l in (ROOT / ".secrets").read_text().splitlines() if "=" in l)
SYSTEMS = {
    "paperless": dict(port=8010, main="pl-web", containers=["pl-web", "pl-broker", "pl-db", "pl-gotenberg", "pl-tika"]),
    "papiro": dict(port=8020, main="papiro", containers=["papiro"]),
}
# Otra instancia: POD=pl10 PORT=8011 (Paperless) o PAPIRO_NAME=pscale PORT=8029 TOKEN=… (Papiro)
if os.environ.get("POD"):
    pod = os.environ["POD"]
    SYSTEMS["paperless"] = dict(port=int(os.environ.get("PORT", 8010)), main=f"{pod}-web", containers=[f"{pod}-{c}" for c in ("web", "broker", "db", "gotenberg", "tika")])
if os.environ.get("PAPIRO_NAME"):
    SYSTEMS["papiro"] = dict(port=int(os.environ.get("PORT", 8020)), main=os.environ["PAPIRO_NAME"], containers=[os.environ["PAPIRO_NAME"]])
if os.environ.get("TOKEN"):
    SECRETS["PAPIRO_TOKEN"] = os.environ["TOKEN"]


_TOKENS, _TOKENS_LOCK = {}, threading.Lock()


def cgroup(container):
    p = subprocess.check_output(["podman", "inspect", "--format", "{{.State.CgroupPath}}", container], text=True).strip()
    return Path("/sys/fs/cgroup") / p.lstrip("/")


def mem(cg):
    """(working_set_MB, anon_MB) de un cgroup, sumando sus hijos."""
    stat = dict(l.split() for l in (cg / "memory.stat").read_text().splitlines())
    cur = int((cg / "memory.current").read_text())
    # Lo que el host mandó a swap sigue siendo memoria que el contenedor necesita: se suma.
    swap = int((cg / "memory.swap.current").read_text()) if (cg / "memory.swap.current").exists() else 0
    return (cur - int(stat["inactive_file"]) + swap) / 2**20, (int(stat["anon"]) + int(stat.get("shmem", 0)) + swap) / 2**20


class Sampler:
    """Muestrea memoria ~5 veces por segundo y guarda el máximo por contenedor y del total."""

    def __init__(self, sysname):
        self.cgs = {c: cgroup(c) for c in SYSTEMS[sysname]["containers"]}
        self.main = SYSTEMS[sysname]["main"]
        self.samples = []

    def __enter__(self):
        self.stop = False
        self.t = threading.Thread(target=self._run, daemon=True)
        self.t.start()
        return self

    def _run(self):
        while not self.stop:
            try:
                self.samples.append({c: mem(cg) for c, cg in self.cgs.items()})
            except (OSError, KeyError, ValueError):
                pass
            time.sleep(0.2)

    def __exit__(self, *a):
        self.stop = True
        self.t.join()

    def summary(self):
        ws_main = [s[self.main][0] for s in self.samples]
        ws_all = [sum(v[0] for v in s.values()) for s in self.samples]
        an_main = [s[self.main][1] for s in self.samples]
        an_all = [sum(v[1] for v in s.values()) for s in self.samples]
        r = lambda x: round(x, 1)
        return dict(main_ws_avg=r(statistics.mean(ws_main)), main_ws_peak=r(max(ws_main)), stack_ws_avg=r(statistics.mean(ws_all)),
                    stack_ws_peak=r(max(ws_all)), main_anon_avg=r(statistics.mean(an_main)), main_anon_peak=r(max(an_main)),
                    stack_anon_peak=r(max(an_all)), samples=len(self.samples))


class Client:
    def __init__(self, sysname):
        self.sys, self.port = sysname, SYSTEMS[sysname]["port"]
        self.local = threading.local()
        self.headers = {}
        if sysname == "paperless":
            # Un solo inicio de sesión por proceso: Paperless limita /api/token/ a 5 por minuto.
            key = (self.port,)
            if key not in _TOKENS:
                with _TOKENS_LOCK:
                    if key not in _TOKENS:
                        _TOKENS[key] = self.req("POST", "/api/token/", json_body=dict(username="admin", password=SECRETS["ADMIN_PASSWORD"]))["token"]
            tok = _TOKENS[key]
            self.headers = {"Authorization": f"Token {tok}", "Accept": "application/json; version=10"}
        else:
            self.headers = {"Authorization": f"Bearer {SECRETS['PAPIRO_TOKEN']}"}

    def conn(self):
        if not getattr(self.local, "c", None):
            self.local.c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=900)
        return self.local.c

    def req(self, method, path, json_body=None, body=None, ctype=None):
        h = dict(self.headers)
        if json_body is not None:
            body, ctype = json.dumps(json_body), "application/json"
        if ctype:
            h["Content-Type"] = ctype
        for attempt in (0, 1):
            try:
                c = self.conn()
                c.request(method, path, body=body, headers=h)
                r = c.getresponse()
                data = r.read()
                break
            except (http.client.HTTPException, OSError):
                self.local.c = None
                if attempt:
                    raise
        if r.status >= 400:
            raise RuntimeError(f"{method} {path} -> {r.status} {data[:300]!r}")
        return json.loads(data) if data else None

    def upload(self, path):
        field = "document" if self.sys == "paperless" else "file"
        b = uuid.uuid4().hex
        body = (f'--{b}\r\nContent-Disposition: form-data; name="{field}"; filename="{path.name}"\r\n'
                f"Content-Type: application/octet-stream\r\n\r\n").encode() + path.read_bytes() + f"\r\n--{b}--\r\n".encode()
        url = "/api/documents/post_document/" if self.sys == "paperless" else "/api/documents"
        r = self.req("POST", url, body=body, ctype=f"multipart/form-data; boundary={b}")
        return r if self.sys == "paperless" else r["jobs"][0]["id"]

    def status(self, handle):
        """-> 'pending' | 'done' | 'failed:<motivo>'"""
        if self.sys == "paperless":
            r = self.req("GET", f"/api/tasks/?task_id={handle}")
            r = r.get("results", r) if isinstance(r, dict) else r
            if not r:
                return "pending"
            st = r[0]["status"].upper()
            return "done" if st == "SUCCESS" else f"failed:{str(r[0].get('result'))[:150]}" if st in ("FAILURE", "REVOKED") else "pending"
        j = self.req("GET", f"/api/jobs/{handle}")
        return "done" if j["status"] == "done" else "pending" if j["status"] in ("queued", "running") else f"failed:{j['status']} {j.get('error')}"

    def wait(self, handles, poll=0.05):
        pending, failed = set(handles), {}
        while pending:
            for h in list(pending):
                s = self.status(h)
                if s != "pending":
                    pending.discard(h)
                    if s != "done":
                        failed[h] = s
            if pending:
                time.sleep(poll)
        return failed

    def search(self, q):
        """-> (total, títulos)"""
        from urllib.parse import quote
        if self.sys == "paperless":
            r = self.req("GET", f"/api/documents/?query={quote(q)}&page_size=10")
            return r["count"], [d["title"] for d in r["results"]]
        r = self.req("GET", f"/api/documents?q={quote(q)}&limit=10")
        return r["total"], [d["title"] for d in r["items"]]

    def count(self):
        return self.search("")[0] if self.sys == "papiro" else self.req("GET", "/api/documents/?page_size=1")["count"]


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))]


def main():
    sysname, label, phases = sys.argv[1], sys.argv[2], sys.argv[3:] or ["idle", "batch", "seq", "search", "big", "restart"]
    out_path = ROOT / "results" / f"{sysname}-{label}.json"
    out_path.parent.mkdir(exist_ok=True)
    res = json.loads(out_path.read_text()) if out_path.exists() else {}
    cl = Client(sysname)
    manifest = json.loads((ROOT / "docs" / "manifest.json").read_text())
    small = [m for m in manifest if m["kind"] != "bigscan"]

    if "idle" in phases:
        with Sampler(sysname) as s:
            time.sleep(20)
        res["idle"] = s.summary()
        print("idle", res["idle"])

    if "batch" in phases:  # ráfaga: los 28 documentos del set B de golpe
        files = sorted(p for p in (ROOT / "docs2").iterdir() if p.suffix != ".json" and "big_scan" not in p.name)
        with Sampler(sysname) as s:
            t0 = time.time()
            with ThreadPoolExecutor(4) as ex:
                handles = list(ex.map(cl.upload, files))
            t_up = time.time() - t0
            failed = cl.wait(handles, poll=0.25)
            total = time.time() - t0
        res["batch"] = dict(docs=len(files), pages=sum(m["pages"] for m in json.loads((ROOT / "docs2" / "manifest.json").read_text()) if m["kind"] != "bigscan"),
                            upload_s=round(t_up, 2), total_s=round(total, 2), failed=list(failed.values()), mem=s.summary())
        print("batch", res["batch"])

    if "seq" in phases:  # uno por uno: tiempo desde que empieza la subida hasta que el documento está listo
        per = {}
        for m in small:
            t0 = time.time()
            failed = cl.wait([cl.upload(ROOT / "docs" / m["file"])])
            dt = time.time() - t0
            per.setdefault(m["kind"], []).append(dict(file=m["file"], pages=m["pages"], s=round(dt, 3), failed=list(failed.values())))
        res["seq"] = {k: dict(n=len(v), pages=sum(x["pages"] for x in v), avg_s=round(statistics.mean(x["s"] for x in v), 3),
                              s_per_page=round(sum(x["s"] for x in v) / sum(x["pages"] for x in v), 3),
                              failed=[x for x in v if x["failed"]], items=v) for k, v in per.items()}
        for k, v in res["seq"].items():
            print("seq", k, {a: b for a, b in v.items() if a != "items"})

    if "search" in phases:
        time.sleep(3)
        # Exactitud: buscar el folio único de cada documento debe devolver ese documento.
        hits, misses = 0, []
        for m in small:
            total, titles = cl.search(m["ref"])
            stem = m["file"].rsplit(".", 1)[0]
            if any(stem in t for t in titles):
                hits += 1
            else:
                misses.append(dict(file=m["file"], ref=m["ref"], total=total, got=titles[:3]))
        queries = [m["ref"] for m in small] + ["electricidad", "payment invoice", "contrato", "comisión federal", "hospitalización medicamentos",
                                               "warranty manufacturer", "arrendatario renta mensual", "estado de cuenta", "Telmex", "movimientos depósitos"]
        cl.search("calentamiento")
        lat = []
        for _ in range(15):
            for q in queries:
                t0 = time.perf_counter()
                cl.search(q)
                lat.append((time.perf_counter() - t0) * 1000)
        res["search"] = dict(ref_hits=hits, ref_total=len(small), misses=misses, queries=len(lat), p50_ms=round(pct(lat, 50), 2),
                             p95_ms=round(pct(lat, 95), 2), mean_ms=round(statistics.mean(lat), 2), docs_in_index=cl.count())
        print("search", {k: v for k, v in res["search"].items() if k != "misses"}, "misses:", misses)

    if "big" in phases:  # un escaneo de 40 páginas a 300 dpi
        with Sampler(sysname) as s:
            t0 = time.time()
            failed = cl.wait([cl.upload(ROOT / "docs" / "big_scan_40p.pdf")], poll=0.25)
            dt = time.time() - t0
        ref = next(m["ref"] for m in manifest if m["kind"] == "bigscan")
        time.sleep(2)
        res["big"] = dict(pages=40, total_s=round(dt, 2), s_per_page=round(dt / 40, 3), failed=list(failed.values()), searchable=cl.search(ref)[0] >= 1, mem=s.summary())
        print("big", res["big"])

    if "restart" in phases:  # arranque en caliente: reinicio del contenedor hasta que responde la API
        main_c, times = SYSTEMS[sysname]["main"], []
        probe = "/api/documents/?page_size=1" if sysname == "paperless" else "/api/stats"
        for _ in range(3):
            subprocess.run(["podman", "stop", "-t", "20", main_c], check=True, capture_output=True)
            t0 = time.time()
            subprocess.run(["podman", "start", main_c], check=True, capture_output=True)
            while True:
                try:
                    cl.local.c = None
                    cl.req("GET", probe)
                    break
                except Exception:
                    time.sleep(0.02)
            times.append(round(time.time() - t0, 2))
        res["restart_s"] = times
        print("restart", times)

    out_path.write_text(json.dumps(res, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    main()
