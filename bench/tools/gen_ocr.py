#!/usr/bin/env python3
"""Set de OCR difícil (determinista): orientación, inclinación, idiomas, baja resolución, PDF mixto y foto.
Escribe bench/ocr/*.pdf|jpg y ocr/truth.json con el texto exacto de cada uno."""
import io, json
from pathlib import Path

import img2pdf, numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from pypdf import PdfReader, PdfWriter
from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

OUT = Path(__file__).resolve().parent.parent / "ocr"; OUT.mkdir(exist_ok=True)
SERIF = "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Regular.ttf"
TXT = {
    "spa": ["Contrato de arrendamiento OCR-ES-4471", "El arrendatario pagará la renta mensual de doce mil quinientos pesos",
            "dentro de los primeros cinco días naturales de cada mes, mediante transferencia",
            "electrónica a la cuenta señalada. La vigencia será de doce meses a partir de la firma.",
            "Ambas partes se someten a la jurisdicción de los tribunales de la Ciudad de México."],
    "deu": ["Mietvertrag Nummer OCR-DE-9921", "Der Mieter zahlt die monatliche Miete in Höhe von zwölfhundert Euro",
            "spätestens am dritten Werktag eines jeden Monats. Für Schäden, die durch",
            "unsachgemäße Nutzung entstehen, haftet der Mieter. Gerichtsstand ist München.",
            "Änderungen bedürfen der Schriftform. Größere Umbauten sind genehmigungspflichtig."],
    "fra": ["Contrat de location numéro OCR-FR-3318", "Le locataire s'engage à payer un loyer mensuel de mille deux cents euros",
            "au plus tard le cinquième jour de chaque mois. Les charges sont réglées",
            "séparément. Toute modification doit être approuvée par écrit par le propriétaire.",
            "Le présent contrat est régi par le droit français. Fait à Lyon, en deux exemplaires."],
}


def page(lines, dpi=200, size=11, noise=7, blur=0.6, contrast=25):
    w, h = int(8.27 * dpi), int(11.69 * dpi)
    img = Image.new("L", (w, h), 250); d = ImageDraw.Draw(img)
    f = ImageFont.truetype(SERIF, int(dpi * size / 72)); y = int(dpi * 0.9)
    for ln in lines:
        d.text((int(dpi * 0.9), y), ln, font=f, fill=contrast); y += int(dpi * size * 1.8 / 72)
    img = img.filter(ImageFilter.GaussianBlur(blur))
    arr = np.asarray(img, dtype=np.float32) + np.random.default_rng(len(lines[0])).normal(0, noise, (h, w))
    return Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))


def to_pdf(img, path, dpi=200):
    b = io.BytesIO(); img.save(b, "JPEG", quality=75, dpi=(dpi, dpi)); path.write_bytes(img2pdf.convert(b.getvalue()))


truth = {}
def add(name, img_or_path, lines, dpi=200, **extra):
    if isinstance(img_or_path, Image.Image):
        to_pdf(img_or_path, OUT / name, dpi)
    truth[name] = dict(text=lines, **extra)

es = TXT["spa"]
add("recto.pdf", page(es), es, case="control: escaneo derecho")
add("girado-90.pdf", page(es).rotate(90, expand=True, fillcolor=250), es, case="página girada 90°")
add("girado-180.pdf", page(es).rotate(180, fillcolor=250), es, case="página de cabeza (180°)")
add("inclinado-4.pdf", page(es).rotate(4, expand=False, fillcolor=250, resample=Image.BICUBIC), es, case="inclinada 4°")
add("aleman.pdf", page(TXT["deu"]), TXT["deu"], case="alemán (no configurado en ninguno)")
add("frances.pdf", page(TXT["fra"]), TXT["fra"], case="francés (no configurado en ninguno)")
add("baja-150dpi-9pt.pdf", page(es, dpi=150, size=9, blur=0.8), es, dpi=150, case="150 dpi, letra de 9 pt")
# Foto con poca luz: contraste bajo y sombra en degradado
img = page(es, noise=12, contrast=95)
grad = np.linspace(1.0, 0.55, img.width)[None, :]
add("foto-sombra.pdf", Image.fromarray((np.asarray(img, dtype=np.float32) * grad).astype(np.uint8)), es, case="contraste bajo y sombra")
# PDF mixto: página 1 con texto digital, página 2 escaneada
c = canvas.Canvas(str(OUT / "_p1.pdf"), pagesize=A4); y = 780
for ln in TXT["fra"]:
    c.setFont("Helvetica", 11); c.drawString(50, y, ln); y -= 20
c.save(); to_pdf(page(es), OUT / "_p2.pdf")
wr = PdfWriter(); [wr.add_page(p) for f in ("_p1.pdf", "_p2.pdf") for p in PdfReader(OUT / f).pages]; wr.write(OUT / "mixto.pdf")
(OUT / "_p1.pdf").unlink(); (OUT / "_p2.pdf").unlink()
truth["mixto.pdf"] = dict(text=TXT["fra"] + es, case="pág. 1 digital (francés) + pág. 2 escaneada (español)")
(OUT / "truth.json").write_text(json.dumps(truth, ensure_ascii=False, indent=1))
print(len(truth), "documentos de OCR difícil")
