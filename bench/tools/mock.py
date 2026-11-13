#!/usr/bin/env python3
"""Servidor falso para probar integraciones de Papiro sin tocar servicios reales.

  POST /hook         recibe webhooks, verifica la firma HMAC (secreto "s3creto") y los guarda en mock.log
  POST /v1/messages  imita la API de Anthropic Messages y devuelve una clasificación fija
Uso: mock.py [puerto]
"""
import hashlib, hmac, json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

LOG = Path(__file__).resolve().parent.parent / "results" / "mock.log"


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        entry = dict(path=self.path)
        if self.path == "/hook":
            sig = "sha256=" + hmac.new(b"s3creto", body, hashlib.sha256).hexdigest()
            entry.update(signature_ok=hmac.compare_digest(sig, self.headers.get("X-Papiro-Signature", "")), body=json.loads(body))
            out = {"ok": True}
        else:
            req = json.loads(body)
            entry.update(headers={k.lower(): ("<presente>" if k.lower() == "x-api-key" else v) for k, v in self.headers.items()
                                  if k.lower() in ("x-api-key", "anthropic-version", "anthropic-beta")},
                         model=req.get("model"), keys=sorted(req), output_format=req.get("output_config", {}).get("format", {}).get("type"),
                         user_chars=len(req["messages"][0]["content"]))
            answer = dict(title="Recibo de luz CFE marzo 2024", summary="Recibo bimestral de la CFE por 412 kWh. Sin adeudos.",
                          correspondent="Comisión Federal de Electricidad", document_type="Recibo", tags=["Hogar", "Servicios"], date="2024-03-15")
            out = dict(id="msg_mock", type="message", role="assistant", model=req.get("model"), stop_reason="end_turn",
                       content=[{"type": "thinking", "thinking": ""}, {"type": "text", "text": json.dumps(answer, ensure_ascii=False)}],
                       usage=dict(input_tokens=1, output_tokens=1))
        with LOG.open("a") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        data = json.dumps(out).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


HTTPServer(("0.0.0.0", int(sys.argv[1]) if len(sys.argv) > 1 else 8099), H).serve_forever()
