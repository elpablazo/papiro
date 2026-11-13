# Benchmarks: Papiro vs Paperless-ngx

Every number in the README comes from this page, and every number on this page comes from a script in [`bench/tools/`](../bench/tools/) with its raw output in [`docs/bench/`](bench/). Nothing here is estimated.

## Setup

- **Machine:** 8 cores, 16 GB RAM, Linux, rootless podman. It was not idle (a desktop session was running), so timings carry some noise. The two apps were measured **one after the other, never at the same time**, with the other one stopped.
- **Paperless-ngx 3.4.2** with its official `docker-compose.postgres-tika.yml`: Valkey, PostgreSQL, Tika and Gotenberg. `PAPERLESS_OCR_LANGUAGE=spa+eng`, everything else default.
- **Papiro** with `PAPIRO_OCR_LANGS=spa+eng`, everything else default. Each row says which version was measured (0.1 to 0.4).
- **Memory** is read from each container's cgroup five times per second: `memory.current − inactive_file + swap`, which is what `docker stats` shows plus anything pushed to swap. Papiro's idle figure by that method is 5 MB; the process RSS is about 9 MB because it also counts binary pages backed by the file.
- **Documents** are synthetic, generated with fixed seeds by `gen_docs.py` and `gen_scale.py`: digital PDFs, scans at 200–300 dpi, photos and Office files, in Spanish and English.

## Footprint

| | Paperless-ngx | Papiro | |
|---|---:|---:|---:|
| RAM at idle, whole stack, 10,000 documents | 931 MB (5 containers) | 5 MB (1 container) | **182× less** (0.2) |
| RAM at idle, main container only, 10,000 documents | 659 MB | 5 MB | 129× less (0.2) |
| Images to pull | 5.16 GB | 149 MB | **35× smaller** (0.4) |
| Restart to serving requests | 20.4 s | 0.33 s | **62× faster** (0.4) |
| First boot (creates the database) | 35.5 s | 0.1 s | (0.1) |

## Ingest

| | Paperless-ngx | Papiro | |
|---|---:|---:|---:|
| 100 scanned PDFs, OCR (1–3 pages each) | 1,394 s (23 min) | 142.5 s | **9.8× faster** (0.2) |
| 1,000 digital PDFs (text already embedded) | 4,308 s (72 min) | 88.5 s | **48.7× faster** (0.4) |
| Peak RAM during the 100 scans | 835 MB (stack 1,082 MB) | 112 MB | 7.5× less (0.2) |
| One digital PDF | 4.26 s | 0.05 s | (0.1) |
| One scanned PDF | 14.4 s | 2.26 s | (0.1) |
| One photo (PNG/JPG) | 14.0 s | 2.03 s | (0.1) |
| A 40-page scan at 300 dpi, defaults | 108.1 s (8 OCR threads) | 50.5 s (2 OCR threads) | (0.1) |

Why Papiro 0.4 ingests 1,000 PDFs slower than 0.1 did (36 s): since 0.2 every file is copied to a staging area, verified against its SHA-256, `fsync`ed and only then moved into place inside the same database transaction, and since 0.4 every document is also analysed for auto-filing. That's the price of never losing a file, and it's still 49× faster than Paperless.

**Same number of OCR threads.** By default Paperless uses as many OCR threads as cores (8 here) and Papiro uses 2. Matching them on the 40-page scan: at 2 threads Paperless takes 206 s and peaks at 830 MB, Papiro 50.5 s and 218 MB; at 8 threads Paperless takes 108 s and 1,334 MB, Papiro 23.4 s and 839 MB. So Papiro is about 4× faster at equal threads, and **OCR memory costs the same for both**: each Tesseract process uses ~100 MB per page at 300 dpi, whoever launches it. Papiro stays lighter under load because it caps how many run at once.

## Search and concurrency (10,000 documents)

| | Paperless-ngx | Papiro | |
|---|---:|---:|---:|
| Find a document by its number, median | 143 ms | 0.73 ms | **196× faster** (0.2) |
| Search by words, median | 135 ms | 11.8 ms | 11× faster (0.2) |
| 8 concurrent clients: requests per second | 5.7 | 536 | **94× more** (0.2) |
| 8 concurrent clients: p95 response | 1,031 ms | 32 ms | **32× faster** (0.2) |
| RAM under that load | 689 MB | 9 MB | (0.2) |
| Mixed load, 8 users × 60 s (read, search, edit, upload, bulk edit) | 750 operations | 39,267 operations | 52× more (0.2) |

## OCR accuracy

Share of the original words found in the extracted text (`ocr_quality.py`, against the generator's ground truth):

| Clean documents | Paperless-ngx | Papiro |
|---|---:|---:|
| Digital PDF (20) | 100.00 % | 100.00 % |
| DOCX (6) | 100.00 % | 100.00 % |
| Scanned PDF (20) | 99.90 % | 99.76 % |
| Photo (10) | 99.81 % | 99.43 % |
| 40-page scan | 99.84 % | 99.95 % |

| Hard cases (`ocr_hard.py`) | Paperless-ngx | Papiro (0.2) |
|---|---:|---:|
| Page rotated 90° | 100 % | 100 % |
| Page upside down | 100 % | 100 % |
| Page skewed 4° | **100 %** | 96.7 % |
| Low contrast with a shadow | **61.7 %** | 53.3 % |
| PDF with one digital page and one scanned page | 50 % | **100 %** |
| 150 dpi, 9 pt text | 100 % | 100 % |

Paperless cleans and deskews each page before OCR, and that shows on skewed and shadowed pages. Papiro skips those passes, which is a big part of why it's faster. On the mixed PDF, Paperless's extracted text was missing the scanned page.

## Not losing data

`crash_papiro.sh` and `crash_paperless.sh` kill the app with SIGKILL at a chosen moment; `integrity.py` then checks every row against its file and SHA-256.

| Killed during… | Paperless-ngx | Papiro (0.2+) |
|---|---|---|
| OCR of a document uploaded through the API | **The upload is lost.** The task stays in "started" forever and is never retried | The job goes back to the queue and finishes on restart |
| Copy of a 161 MB original (3 different moments) | not reproduced | The job finishes on restart; the file downloads byte-identical; no leftovers |
| The same file uploaded 10 times at once | **10 documents** (duplicates are accepted by default) | 1 document |

## What was not measured

- Real power loss (only SIGKILL), weeks-long stability, and real-world archives with messy scans. All test documents are synthetic.
- Paperless at 10,000 documents was measured for search and memory; its importer timing (44 s for 10,000) is not comparable because the synthetic export had no thumbnails.
- Paperless-ngx is no longer installed here, so its numbers can't be re-run against Papiro 0.4. Rows mark which Papiro version each number comes from.
