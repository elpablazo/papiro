"""Genera el set de documentos de prueba (determinista). Uso: gen_docs.py <dir_salida>

Cada documento lleva un código único REF-xxxxxx para verificar que el OCR/extracción
y la búsqueda encuentran exactamente ese documento.
"""
import io, json, random, sys, textwrap
from pathlib import Path

import img2pdf
import numpy as np
from docx import Document as Docx
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

out = Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=True)
rnd = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 2026)
SANS = "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf"
SERIF = "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Regular.ttf"

EMISORES = ["Comisión Federal de Electricidad", "Telmex", "BBVA México", "Seguros Monterrey",
            "Notaría Pública 47", "Amazon Web Services", "Hospital Ángeles", "SAT", "Liverpool", "Totalplay"]
TIPOS = ["Factura", "Recibo", "Contrato", "Estado de cuenta", "Póliza", "Carta", "Invoice", "Receipt"]
PARRAFOS = [
    "Por medio de la presente se hace constar que el titular de la cuenta ha cubierto el importe total correspondiente al periodo facturado, sin adeudos pendientes a la fecha de emisión de este documento.",
    "El arrendatario se obliga a pagar la renta mensual dentro de los primeros cinco días naturales de cada mes, mediante transferencia electrónica a la cuenta señalada por el arrendador.",
    "This agreement shall be governed by and construed in accordance with the laws of Mexico. Any dispute arising out of this agreement shall be submitted to the competent courts of Mexico City.",
    "El consumo registrado durante el bimestre fue de 412 kWh, lo cual representa un incremento del ocho por ciento respecto al mismo periodo del año anterior.",
    "Payment is due within thirty days of the invoice date. Late payments are subject to a monthly finance charge of one and a half percent on the outstanding balance.",
    "La cobertura de gastos médicos mayores incluye hospitalización, honorarios quirúrgicos, estudios de laboratorio y medicamentos prescritos durante la estancia hospitalaria.",
    "Se anexa el desglose de movimientos del periodo, incluyendo depósitos, retiros, comisiones e intereses generados conforme al contrato de apertura.",
    "The customer acknowledges receipt of the goods described above in good condition and accepts the terms of the limited warranty provided by the manufacturer.",
]
manifest = []


def meta(i, kind):
    ref = f"REF-{rnd.randrange(100000, 999999)}"
    em, tp = rnd.choice(EMISORES), rnd.choice(TIPOS)
    fecha = f"{rnd.randrange(1, 28):02d}/{rnd.randrange(1, 13):02d}/{rnd.randrange(2021, 2027)}"
    total = f"${rnd.randrange(150, 98000):,}.{rnd.randrange(0, 99):02d} MXN"
    return dict(ref=ref, emisor=em, tipo=tp, fecha=fecha, total=total, kind=kind, n=i)


def page_lines(m, page, pages):
    lines = [f"{m['tipo']} — {m['emisor']}", f"Folio: {m['ref']}    Fecha: {m['fecha']}    Página {page} de {pages}", ""]
    for p in rnd.sample(PARRAFOS, 5):
        lines += textwrap.wrap(p, 86) + [""]
    lines += [f"Total a pagar: {m['total']}", f"Referencia de documento {m['ref']}"]
    m.setdefault("text", []).extend(lines)  # verdad de referencia para medir la exactitud del OCR
    return lines


def digital_pdf(path, m, pages):
    c = canvas.Canvas(str(path), pagesize=A4)
    for p in range(1, pages + 1):
        y = 800
        for i, ln in enumerate(page_lines(m, p, pages)):
            c.setFont("Helvetica-Bold" if i == 0 else "Helvetica", 16 if i == 0 else 11)
            c.drawString(50, y, ln); y -= 24 if i == 0 else 15
        c.showPage()
    c.save()


def scan_image(m, page, pages, dpi=200):
    w, h = int(8.27 * dpi), int(11.69 * dpi)
    img = Image.new("L", (w, h), 250)
    d = ImageDraw.Draw(img)
    big = ImageFont.truetype(SERIF, int(dpi * 18 / 72)); small = ImageFont.truetype(SERIF, int(dpi * 11 / 72))  # 18 pt y 11 pt reales
    y = int(dpi * 0.8)
    for i, ln in enumerate(page_lines(m, page, pages)):
        d.text((int(dpi * 0.9), y), ln, font=big if i == 0 else small, fill=25)
        y += int(dpi * (0.36 if i == 0 else 0.2))
    img = img.rotate(rnd.uniform(-0.8, 0.8), fillcolor=250, resample=Image.BICUBIC)
    img = img.filter(ImageFilter.GaussianBlur(0.6))
    noise = np.random.default_rng(m["n"] * 100 + page).normal(0, 7, (h, w))
    return Image.fromarray(np.clip(np.asarray(img, dtype=np.float32) + noise, 0, 255).astype(np.uint8))


def jpeg_bytes(img, dpi=200):
    b = io.BytesIO(); img.save(b, "JPEG", quality=72, dpi=(dpi, dpi)); return b.getvalue()


def scan_pdf(path, m, pages, dpi=200):
    path.write_bytes(img2pdf.convert([jpeg_bytes(scan_image(m, p, pages, dpi), dpi) for p in range(1, pages + 1)]))


def docx(path, m):
    d = Docx(); d.add_heading(f"{m['tipo']} — {m['emisor']}", 1)
    d.add_paragraph(f"Folio: {m['ref']}    Fecha: {m['fecha']}")
    for p in rnd.sample(PARRAFOS, 6):
        d.add_paragraph(p); m.setdefault("text", []).append(p)
    d.add_paragraph(f"Total a pagar: {m['total']}"); d.save(str(path))


def add(name, m, pages):
    m.update(file=name, pages=pages); manifest.append(m)


for i in range(1, 11):
    m = meta(i, "digital"); n = rnd.choice([1, 1, 2, 3, 5]); digital_pdf(out / f"digital_{i:02d}.pdf", m, n); add(f"digital_{i:02d}.pdf", m, n)
for i in range(1, 11):
    m = meta(i, "scan"); n = rnd.choice([1, 1, 2, 3]); scan_pdf(out / f"scan_{i:02d}.pdf", m, n); add(f"scan_{i:02d}.pdf", m, n)
for i in range(1, 6):
    m = meta(i, "image"); ext = "png" if i % 2 else "jpg"; im = scan_image(m, 1, 1)
    im.save(out / f"img_{i:02d}.{ext}", dpi=(200, 200), **({"quality": 75} if ext == "jpg" else {})); add(f"img_{i:02d}.{ext}", m, 1)
for i in range(1, 4):
    m = meta(i, "office"); docx(out / f"office_{i:02d}.docx", m); add(f"office_{i:02d}.docx", m, 1)
m = meta(1, "bigscan"); scan_pdf(out / "big_scan_40p.pdf", m, 40, dpi=300); add("big_scan_40p.pdf", m, 40)

(out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1))
print(len(manifest), "docs,", sum(x["pages"] for x in manifest), "páginas")
