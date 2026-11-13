#!/usr/bin/env python3
"""Sube el set de OCR difícil a un sistema y mide tiempo, exactitud del texto y el PDF resultante.
Uso: ocr_hard.py <paperless|papiro> [puerto]   (TOKEN=… en el entorno para un Papiro que no sea el principal)"""
import json, os, re, subprocess, sys, tempfile, time
from collections import Counter
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).parent))
import bench
from bench import ROOT, Client

if os.environ.get("TOKEN"):
    bench.SECRETS["PAPIRO_TOKEN"] = os.environ["TOKEN"]
sysname = sys.argv[1]; P = sysname == "paperless"
if len(sys.argv) > 2:
    bench.SYSTEMS[sysname]["port"] = int(sys.argv[2])
cl = Client(sysname)
truth = json.loads((ROOT / "ocr" / "truth.json").read_text())
words = lambda t: Counter(re.findall(r"[0-9a-záéíóúüñäöüßàâçèéêëîïôûù]+", t.lower()))


def recall(want, got):
    w = words("\n".join(want)); g = words(got)
    return round(sum((w & g).values()) / sum(w.values()), 4)


def pdf_info(data):
    with tempfile.NamedTemporaryFile(suffix=".pdf") as f:
        f.write(data); f.flush()
        info = subprocess.run(["pdfinfo", f.name], capture_output=True, text=True).stdout
        meta = subprocess.run(["pdfinfo", "-meta", f.name], capture_output=True, text=True).stdout
        text = subprocess.run(["pdftotext", f.name, "-"], capture_output=True, text=True).stdout
        chk = subprocess.run(["qpdf", "--check", f.name], capture_output=True, text=True) if subprocess.run(["which", "qpdf"], capture_output=True).returncode == 0 else None
        rot = re.findall(r"Page rot:\s+(\d+)", subprocess.run(["pdfinfo", "-f", "1", "-l", "9", f.name], capture_output=True, text=True).stdout)
        sizes = re.findall(r"Page\s+\d+ size:\s+([\d.]+) x ([\d.]+)", subprocess.run(["pdfinfo", "-f", "1", "-l", "9", f.name], capture_output=True, text=True).stdout)
    return dict(kb=len(data) // 1024, pdfa=("pdfaid" in meta) and re.search(r"pdfaid:part[^0-9]*(\d)", meta).group(1) if "pdfaid" in meta else None,
                producer=(re.search(r"Producer:\s+(.*)", info) or [None, None])[1], page_rot=rot,
                portrait=[float(w) < float(h) for w, h in sizes], text_layer_recall=None, _text=text,
                qpdf_check=None if chk is None else ("ok" if chk.returncode == 0 else chk.stdout.strip()[-80:]))


out = {}
for name, t in truth.items():
    path = ROOT / "ocr" / name
    t0 = time.time()
    if P:
        task = cl.upload(path)
        for _ in range(900):
            st = cl.status(task)
            if st != "pending": break
            time.sleep(0.5)
        r = cl.req("GET", f"/api/tasks/?task_id={task}")
        r = r.get("results", r)[0]
        doc_id = (r.get("related_document_ids") or [None])[0] if isinstance(r.get("related_document_ids"), list) else r.get("related_document")
        dt = time.time() - t0
        if not doc_id:
            out[name] = dict(case=t["case"], error=str(r.get("result"))[:200], s=round(dt, 1)); continue
        doc = cl.req("GET", f"/api/documents/{doc_id}/")
        content = doc["content"]
        c = cl.conn(); c.request("GET", f"/api/documents/{doc_id}/download/", headers=cl.headers); resp = c.getresponse(); pdf = resp.read()
    else:
        b = __import__("uuid").uuid4().hex
        body = (f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: application/octet-stream\r\n\r\n').encode() + path.read_bytes() + f"\r\n--{b}--\r\n".encode()
        r = cl.req("POST", "/api/documents?wait=true", body=body, ctype=f"multipart/form-data; boundary={b}")
        dt = time.time() - t0
        j = r["jobs"][0]
        if not j.get("doc_id") or j["status"] not in ("done",):
            out[name] = dict(case=t["case"], error=f"{j['status']}: {j.get('error')}", s=round(dt, 1)); continue
        doc = cl.req("GET", f"/api/documents/{j['doc_id']}")
        content = doc["content"]
        c = cl.conn(); c.request("GET", f"/api/documents/{j['doc_id']}/file", headers=cl.headers); resp = c.getresponse(); pdf = resp.read()
    info = pdf_info(pdf) if resp.status == 200 and pdf[:4] == b"%PDF" else dict(error=f"descarga {resp.status}")
    if "_text" in info:
        info["text_layer_recall"] = recall(t["text"], info.pop("_text"))
    out[name] = dict(case=t["case"], s=round(dt, 1), recall=recall(t["text"], content), pdf=info)
    print(name, json.dumps(out[name], ensure_ascii=False), flush=True)
(ROOT / "results" / f"ocr-hard-{sysname}.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))
