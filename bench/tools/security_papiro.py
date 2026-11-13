#!/usr/bin/env python3
"""Pruebas activas de seguridad contra un Papiro local (puerto 8023). Solo stdlib.
Uso: security_papiro.py <contraseña_admin>"""
import http.client, json, sys, threading, time, uuid

ADMIN_PW, PORT, out = sys.argv[1], 8023, {}
X = {"X-Requested-With": "papiro"}


def req(method, path, body=None, headers=None, raw=False):
    c = http.client.HTTPConnection("127.0.0.1", PORT, timeout=120)
    h = dict(headers or {})
    if isinstance(body, (dict, list)):
        body = json.dumps(body); h.setdefault("Content-Type", "application/json")
    c.request(method, path, body=body, headers=h)
    r = c.getresponse(); data = r.read()
    if raw:
        return r, data
    try:
        return r.status, json.loads(data) if data else None
    except ValueError:
        return r.status, data[:200]


def login(u, p, ip="198.51.100.7"):
    r, _ = req("POST", "/api/auth/login", {"username": u, "password": p}, {"X-Forwarded-For": ip}, raw=True)
    return r.status, (r.getheader("Set-Cookie") or "").split(";")[0]


def token(cookie, name, **kw):
    return {"Authorization": "Bearer " + req("POST", "/api/tokens", {"name": name, **kw}, {"Cookie": cookie, **X})[1]["token"]}


def upload(h, name, private=False):
    b = uuid.uuid4().hex
    body = f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: text/plain\r\n\r\nsecreto {uuid.uuid4().hex} nomina confidencial\r\n'
    if private:
        body += f'--{b}\r\nContent-Disposition: form-data; name="private"\r\n\r\ntrue\r\n'
    s, r = req("POST", "/api/documents?wait=true", (body + f"--{b}--\r\n").encode(), {**h, "Content-Type": f"multipart/form-data; boundary={b}"})
    return s, (r.get("documents") or [{}])[0].get("id") if isinstance(r, dict) else None


_, admin_ck = login("admin", ADMIN_PW)
A = token(admin_ck, "admin")
for u, role in (("eva", "editor"), ("max", "editor"), ("leo", "viewer")):
    req("POST", "/api/users", {"username": u, "password": f"contraseña-{u}-123", "role": role}, A)
E, M, L = (token(login(u, f"contraseña-{u}-123")[1], u) for u in ("eva", "max", "leo"))
RO = token(admin_ck, "solo-lectura", read_only=True)
docs = [d["id"] for d in req("GET", "/api/documents?limit=5", headers=A)[1]["items"]]

out["editor"] = {"ver": req("GET", f"/api/documents/{docs[0]}", headers=E)[0], "editar": req("PATCH", f"/api/documents/{docs[0]}", {"title": "editado por eva"}, E)[0],
                 "papelera": req("DELETE", f"/api/documents/{docs[1]}", headers=E)[0], "borrar definitivamente": req("DELETE", f"/api/documents/{docs[1]}?permanent=true", headers=E)[0],
                 "purga masiva": req("POST", "/api/documents/bulk", {"ids": [docs[2]], "action": "purge"}, E)[0], "usuarios": req("GET", "/api/users", headers=E)[0],
                 "auditoría": req("GET", "/api/audit", headers=E)[0], "estado": req("GET", "/api/status", headers=E)[0]}
out["documento_sigue_existiendo_tras_intento_de_purga"] = req("GET", f"/api/documents/{docs[1]}", headers=A)[0] == 200
out["lector"] = {"ver": req("GET", f"/api/documents/{docs[0]}", headers=L)[0], "buscar": req("GET", "/api/documents?q=a", headers=L)[0], "descargar": req("GET", f"/api/documents/{docs[0]}/file", headers=L)[0],
                 "editar": req("PATCH", f"/api/documents/{docs[0]}", {"title": "x"}, L)[0], "subir": upload(L, "l.txt")[0], "papelera": req("DELETE", f"/api/documents/{docs[0]}", headers=L)[0],
                 "crear etiqueta": req("POST", "/api/tags", {"name": "leo"}, L)[0], "nota": req("POST", f"/api/documents/{docs[0]}/notes", {"text": "x"}, L)[0]}
out["token_solo_lectura_de_admin"] = {"leer": req("GET", "/api/documents?limit=1", headers=RO)[0], "editar": req("PATCH", f"/api/documents/{docs[0]}", {"title": "x"}, RO)[0],
                                      "usuarios": req("GET", "/api/users", headers=RO)[0], "crear token": req("POST", "/api/tokens", {"name": "x"}, RO)[0],
                                      "mcp escribir": json.loads(req("POST", "/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "update_document", "arguments": {"id": docs[0], "title": "x"}}}, RO)[1]["result"]["content"][0]["text"] if False else "{}") or req("POST", "/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "update_document", "arguments": {"id": docs[0], "title": "x"}}}, RO)[1]["result"]["isError"]}

# documentos privados: eva sube uno privado
_, priv = upload(E, "nomina-eva.txt", private=True)
out["privado"] = {"dueña lo ve": req("GET", f"/api/documents/{priv}", headers=E)[0], "admin lo ve": req("GET", f"/api/documents/{priv}", headers=A)[0],
                  "otro editor lo ve": req("GET", f"/api/documents/{priv}", headers=M)[0], "otro editor lo descarga": req("GET", f"/api/documents/{priv}/file", headers=M)[0],
                  "otro editor lo edita": req("PATCH", f"/api/documents/{priv}", {"title": "x"}, M)[0], "otro editor lo manda a papelera": req("DELETE", f"/api/documents/{priv}", headers=M)[0],
                  "otro editor lo encuentra buscando": req("GET", "/api/documents?q=nomina+confidencial", headers=M)[1]["total"],
                  "la dueña lo encuentra": req("GET", "/api/documents?q=nomina+confidencial", headers=E)[1]["total"],
                  "otro editor por MCP": req("POST", "/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_document", "arguments": {"id": priv}}}, M)[1]["result"]["isError"],
                  "otro editor lo hace público": req("PATCH", f"/api/documents/{docs[3]}", {"is_private": True}, M)[0]}

out["sin_credenciales"] = {p: req("GET", p)[0] for p in ["/api/documents", f"/api/documents/{docs[0]}/file", "/api/stats", "/api/openapi.json", "/api/auth/status", "/api/health"]}
out["mcp_sin_token"] = req("POST", "/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/list"})[0]
out["csrf"] = {"cookie sin cabecera propia": req("POST", "/api/tags", {"name": "c1"}, {"Cookie": admin_ck})[0],
               "cookie + cabecera + origen ajeno": req("POST", "/api/tags", {"name": "c2"}, {"Cookie": admin_ck, **X, "Origin": "https://evil.example"})[0],
               "cookie + cabecera (interfaz propia)": req("POST", "/api/tags", {"name": "c3"}, {"Cookie": admin_ck, **X})[0]}
out["webhook_a_direccion_interna"] = [req("POST", "/api/webhooks", {"url": u}, A)[0] for u in ("http://127.0.0.1:9/x", "http://169.254.169.254/latest", "http://10.0.0.1/x")]

# bloqueo: un atacante falla 12 veces la contraseña del admin desde su IP
codes = [req("POST", "/api/auth/login", {"username": "admin", "password": f"x{i}"}, {"X-Forwarded-For": "203.0.113.5"})[0] for i in range(12)]
out["atacante_12_intentos"] = codes
out["admin_legitimo_desde_otra_ip"] = login("admin", ADMIN_PW, "198.51.100.99")[0]
out["atacante_con_la_contraseña_correcta_sigue_frenado"] = login("admin", ADMIN_PW, "203.0.113.5")[0]
out["último_administrador_no_se_puede_degradar"] = req("PATCH", "/api/users/1", {"role": "viewer"}, A)[0]
audit = req("GET", "/api/audit?limit=200", headers=A)[1]
out["auditoría_registra"] = sorted({a["action"] for a in audit})[:14]
print(json.dumps(out, ensure_ascii=False, indent=1))
