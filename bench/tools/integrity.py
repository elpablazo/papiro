#!/usr/bin/env python3
"""Revisa la coherencia entre la base de Papiro y los archivos. Solo lectura.
Uso (dentro de `podman unshare`): integrity.py <carpeta de datos de Papiro>"""
import hashlib, json, os, sqlite3, sys
from pathlib import Path

data = Path(sys.argv[1])
db = sqlite3.connect(f"file:{data / 'papiro.db'}?mode=ro", uri=True)
rep = dict(integrity_check=db.execute("PRAGMA integrity_check").fetchone()[0], fts_ok=None, problems=[])
try:
    db.execute("INSERT INTO docs_fts(docs_fts) VALUES('integrity-check')")
    rep["fts_ok"] = True
except sqlite3.Error as e:
    rep["fts_ok"] = str(e)
rows = db.execute("SELECT id, sha256, ext, has_archive, has_thumb, size FROM documents").fetchall()
rep["documents"] = len(rows)
for id_, sha, ext, arch, thumb, size in rows:
    d = data / "docs" / str(id_)
    served, ingested = d / "original", d / f"original.{ext}"   # la API lee «original»; la ingesta escribe «original.<ext>»
    f = served if served.exists() else ingested if ingested.exists() else None
    if f is None:
        rep["problems"].append(dict(id=id_, problem="sin archivo original")); continue
    got = hashlib.sha256(f.read_bytes()).hexdigest()
    if got != sha:
        rep["problems"].append(dict(id=id_, problem=f"original dañado ({f.stat().st_size} de {size} bytes)"))
    if not served.exists():
        rep["problems"].append(dict(id=id_, problem="original no accesible por la API (guardado como original.%s)" % ext))
    if arch and not (d / "archive.pdf").exists():
        rep["problems"].append(dict(id=id_, problem="falta archive.pdf"))
    if thumb and not (d / "thumb.jpg").exists():
        rep["problems"].append(dict(id=id_, problem="falta miniatura"))
ids = {str(r[0]) for r in rows}
rep["orphan_dirs"] = sorted(x for x in os.listdir(data / "docs") if x not in ids) if (data / "docs").exists() else []
rep["tmp_files"] = sorted(os.listdir(data / "tmp")) if (data / "tmp").exists() else []
rep["jobs"] = dict(db.execute("SELECT status, count(*) FROM jobs GROUP BY status").fetchall())
rep["jobs_detail"] = [dict(zip(("id", "filename", "status", "error", "doc_id"), r)) for r in db.execute("SELECT id, filename, status, error, doc_id FROM jobs ORDER BY id")]
print(json.dumps(rep, ensure_ascii=False, indent=1))
