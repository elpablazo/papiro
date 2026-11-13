# Benchmarks

The scripts used to measure Paperless-ngx and Papiro side by side. Results and method: [docs/BENCHMARKS.md](../docs/BENCHMARKS.md); raw JSON in [docs/bench/](../docs/bench/). They need podman, Python 3 and `jq`.

```bash
python3 -m venv tools/venv && tools/venv/bin/pip install reportlab pillow python-docx img2pdf numpy
tools/venv/bin/python tools/gen_docs.py docs          # set A (includes a 40-page scan)
tools/venv/bin/python tools/gen_docs.py docs2 777     # set B, for the burst test
tools/paperless.sh up && tools/venv/bin/python tools/bench.py paperless default
tools/papiro.sh up    && tools/venv/bin/python tools/bench.py papiro default
python3 tools/gen_scale.py && python3 tools/bench_scale.py papiro ingest scale/scan100   # scale tests
```

| Script | What it measures |
|---|---|
| `bench.py` | RAM (from the cgroup, incl. swap), per-document ingest time, search latency, startup |
| `bench_scale.py` | 100 scans, 1,000 digital PDFs, search and concurrent load at 10,000 documents |
| `ocr_quality.py`, `ocr_hard.py` | OCR word recall: clean set, and rotated / skewed / shadowed / mixed pages |
| `concurrency.py` | 8 users for 60 s of mixed reads, searches, edits and uploads; duplicate uploads |
| `crash_papiro.sh`, `crash_paperless.sh`, `killer*.py`, `integrity.py` | SIGKILL at chosen moments, then every row checked against its file and SHA-256 |
| `security_papiro.py` | Roles, private documents, read-only tokens, CSRF, SSRF, login throttling |
| `suggest_eval.py` | Accuracy of the local auto-filing suggestions (leave-one-out) |
| `pwa_check.mjs` | Installability, service worker, offline start and share target, via Chrome DevTools |

`.secrets` (test password and tokens) is generated on the fly and never committed. Script comments are in Spanish.
