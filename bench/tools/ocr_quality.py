#!/usr/bin/env python3
"""Exactitud del texto extraído frente a la verdad de referencia (recall de palabras), por sistema y tipo."""
import json, re, statistics, sys
from collections import Counter
from pathlib import Path
from urllib.parse import quote
sys.path.insert(0, str(Path(__file__).parent))
from bench import ROOT, Client

words = lambda t: Counter(re.findall(r"[0-9a-záéíóúüñ]+", t.lower()))
gt = json.loads((ROOT / "results" / "ground_truth.json").read_text())
paperless = {}
for r in json.loads((ROOT / "export" / "manifest.json").read_text()):
    if r["model"] == "documents.document":
        paperless[r["fields"]["title"].split()[-1]] = r["fields"]["content"]
cl = Client("papiro")
out = {}
for m in gt:
    want = words("\n".join(m["text"]))
    hit = cl.req("GET", f"/api/documents?q={quote(chr(34) + m['ref'] + chr(34))}&limit=1")["items"]
    texts = dict(papiro=cl.req("GET", f"/api/documents/{hit[0]['id']}")["content"] if hit else None, paperless=paperless.get(m["ref"]))
    for name, text in texts.items():
        if text is None:
            continue
        got = words(text)
        out.setdefault((m["kind"], name), []).append(sum((want & got).values()) / sum(want.values()))
for (kind, name), v in sorted(out.items()):
    print(f"{kind:8} {name:10} docs={len(v):2}  recall medio={statistics.mean(v):.4f}  mínimo={min(v):.4f}")
