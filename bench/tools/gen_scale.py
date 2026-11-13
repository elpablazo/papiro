#!/usr/bin/env python3
"""Sets para las pruebas de escala (deterministas).

  scale/scan100/        100 escaneos de 1 página (200 dpi) → mide OCR sostenido
  scale/digital1000/    1 000 PDF digitales de 1 página   → mide ingesta sin OCR
  scale/export10k/      export con formato de Paperless (manifest.json + 10 000 PDF)
                        → se carga en ambos con su importador nativo
Cada documento lleva un folio único SC-xxxxxxx; scale/*/truth.json guarda folio y texto.
"""
import hashlib, io, json, random, sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

import img2pdf, numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parent.parent / "scale"
SERIF = "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Regular.ttf"
EMISORES = ["Comisión Federal de Electricidad", "Telmex", "BBVA México", "Seguros Monterrey", "Notaría Pública 47",
            "Amazon Web Services", "Hospital Ángeles", "SAT", "Liverpool", "Totalplay"]
TIPOS = ["Factura", "Recibo", "Contrato", "Estado de cuenta", "Póliza", "Carta", "Invoice", "Receipt"]
TAGS = ["Finanzas", "Hogar", "Salud", "Legal", "Impuestos"]
PARRAFOS = [
    "Por medio de la presente se hace constar que el titular de la cuenta ha cubierto el importe total correspondiente al periodo facturado.",
    "El arrendatario se obliga a pagar la renta mensual dentro de los primeros cinco días naturales de cada mes.",
    "This agreement shall be governed by and construed in accordance with the laws of Mexico.",
    "El consumo registrado durante el bimestre fue de 412 kWh, un incremento del ocho por ciento.",
    "Payment is due within thirty days of the invoice date. Late payments are subject to a finance charge.",
    "La cobertura de gastos médicos mayores incluye hospitalización, honorarios quirúrgicos y medicamentos.",
    "Se anexa el desglose de movimientos del periodo, incluyendo depósitos, retiros, comisiones e intereses.",
    "The customer acknowledges receipt of the goods described above in good condition.",
]


def meta(rnd, n):
    d = date(2015, 1, 1) + timedelta(days=rnd.randrange(0, 4300))
    return dict(ref=f"SC-{n:07d}", emisor=rnd.randrange(len(EMISORES)), tipo=rnd.randrange(len(TIPOS)),
                tags=sorted(rnd.sample(range(len(TAGS)), rnd.randrange(0, 3))), date=d.isoformat(),
                lines=[f"{TIPOS[0]}", ""] and None)


def lines_for(m, rnd):
    return [f"{TIPOS[m['tipo']]} — {EMISORES[m['emisor']]}", f"Folio: {m['ref']}  Fecha: {m['date'][8:]}/{m['date'][5:7]}/{m['date'][:4]}",
            *rnd.sample(PARRAFOS, 4), f"Total a pagar: ${rnd.randrange(150, 98000):,}.00 MXN"]


def digital(path, lines):
    c = canvas.Canvas(str(path), pagesize=A4)
    y = 800
    for i, ln in enumerate(lines):
        c.setFont("Helvetica-Bold" if i == 0 else "Helvetica", 15 if i == 0 else 10)
        for chunk in [ln[k:k + 95] for k in range(0, len(ln), 95)] or [""]:
            c.drawString(50, y, chunk); y -= 16
    c.save()


def scan(path, lines, seed, dpi=200):
    w, h = int(8.27 * dpi), int(11.69 * dpi)
    img = Image.new("L", (w, h), 250); d = ImageDraw.Draw(img)
    big, small = ImageFont.truetype(SERIF, int(dpi * 18 / 72)), ImageFont.truetype(SERIF, int(dpi * 11 / 72))
    y = int(dpi * 0.8)
    for i, ln in enumerate(lines):
        for chunk in [ln[k:k + 85] for k in range(0, len(ln), 85)] or [""]:
            d.text((int(dpi * 0.9), y), chunk, font=big if i == 0 else small, fill=25); y += int(dpi * (0.36 if i == 0 else 0.2))
    img = img.rotate(random.Random(seed).uniform(-0.8, 0.8), fillcolor=250, resample=Image.BICUBIC).filter(ImageFilter.GaussianBlur(0.6))
    noise = np.random.default_rng(seed).normal(0, 7, (h, w))
    img = Image.fromarray(np.clip(np.asarray(img, dtype=np.float32) + noise, 0, 255).astype(np.uint8))
    b = io.BytesIO(); img.save(b, "JPEG", quality=72, dpi=(dpi, dpi))
    path.write_bytes(img2pdf.convert([b.getvalue()]))


def make(kind, count, start, seed):
    out = ROOT / kind; out.mkdir(parents=True, exist_ok=True)
    rnd, truth = random.Random(seed), []
    for i in range(count):
        n = start + i
        m = meta(rnd, n); m["text"] = lines_for(m, rnd); m["file"] = f"{m['ref']}.pdf"
        (scan if kind == "scan100" else digital)(*([out / m["file"], m["text"], n] if kind == "scan100" else [out / m["file"], m["text"]]))
        truth.append(m)
    (out / "truth.json").write_text(json.dumps(truth, ensure_ascii=False))
    return out, truth


def export10k():
    out, truth = make("export10k", 10000, 1_000_000, 10)
    now = datetime(2026, 11, 1, tzinfo=timezone.utc).isoformat().replace("+00:00", "Z")
    lab = lambda model, pk, name, **extra: dict(model=model, pk=pk, fields=dict(owner=None, name=name, match="", matching_algorithm=0, is_insensitive=True, **extra))
    tagx = dict(tn_ancestors_pks="", tn_ancestors_count=0, tn_children_pks="", tn_children_count=0, tn_depth=0, tn_descendants_pks="", tn_descendants_count=0,
                tn_index=0, tn_level=1, tn_parent=None, tn_priority=0, tn_order=0, tn_siblings_pks="", tn_siblings_count=0, is_inbox_tag=False)
    recs = [lab("documents.correspondent", i + 1, n) for i, n in enumerate(EMISORES)]
    recs += [lab("documents.documenttype", i + 1, n) for i, n in enumerate(TIPOS)]
    recs += [lab("documents.tag", i + 1, n, color="#16a34a", **{**tagx, "tn_index": i, "tn_order": i}) for i, n in enumerate(TAGS)]
    for pk, m in enumerate(truth, 1):
        data = (out / m["file"]).read_bytes()
        recs.append(dict(model="documents.document", pk=pk, __exported_file_name__=m["file"], __exported_thumbnail_name__=None,
                         fields=dict(deleted_at=None, restored_at=None, transaction_id=None, owner=None, correspondent=m["emisor"] + 1, storage_path=None,
                                     title=f"{TIPOS[m['tipo']]} {EMISORES[m['emisor']]} {m['ref']}", document_type=m["tipo"] + 1, mime_type="application/pdf",
                                     checksum=hashlib.sha256(data).hexdigest(), archive_checksum=None, page_count=1, created=m["date"], modified=now, added=now,
                                     filename=f"{pk:07d}.pdf", archive_filename=None, original_filename=m["file"], archive_serial_number=None,
                                     root_document=None, version_index=None, version_label=None, content="\n".join(m["text"]),
                                     tags=[t + 1 for t in m["tags"]])))
    (out / "manifest.json").write_text(json.dumps(recs, ensure_ascii=False))
    (out / "metadata.json").write_text(json.dumps({"version": "3.4.2"}))


if __name__ != "__main__":
    raise SystemExit
which = sys.argv[1:] or ["scan100", "digital1000", "export10k"]
if "scan100" in which: make("scan100", 100, 100_000, 100)
if "digital1000" in which: make("digital1000", 1000, 200_000, 1000)
if "export10k" in which: export10k()
if "conc" in which:   # 40 documentos únicos para la prueba de concurrencia (30 digitales + 10 escaneos)
    make("conc", 30, 300_000, 31); out = ROOT / "conc"
    rnd = random.Random(32)
    for i in range(10):
        m = meta(rnd, 300_100 + i); scan(out / f"{m['ref']}.pdf", lines_for(m, rnd), 300_100 + i)
for k in which:
    p = ROOT / k
    print(k, sum(1 for f in p.iterdir() if f.suffix == ".pdf"), "PDF,", round(sum(f.stat().st_size for f in p.iterdir()) / 2**20, 1), "MB")
