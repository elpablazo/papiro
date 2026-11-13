//! Servidor MCP (Model Context Protocol) sobre Streamable HTTP en POST /mcp.
//! Sin estado: cada petición JSON-RPC se responde con JSON. Autenticación: el mismo token Bearer de la API.
use crate::{auth::AuthUser, cfg, ingest, random_hex, store};
use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use base64::Engine;
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

fn tools() -> Value {
    let s = |d: &str| json!({"type": "string", "description": d});
    let names = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    json!([
        {"name": "search_documents",
         "description": "Full-text search over the document archive, with optional filters. Returns matching documents with a text snippet. Call list_metadata first to learn the exact tag/correspondent/type names.",
         "inputSchema": {"type": "object", "properties": {
            "query": s("Words to search in title and content. Use \"quotes\" for an exact phrase. Omit to list by date."),
            "tags": names("Tag names; documents must have all of them"),
            "correspondent": s("Correspondent name (who issued/sent the document)"),
            "document_type": s("Document type name"),
            "folder": s("Folder path, e.g. Casa/Recibos; includes its subfolders"),
            "from": s("Earliest document date, YYYY-MM-DD"), "to": s("Latest document date, YYYY-MM-DD"),
            "inbox": {"type": "boolean", "description": "Only documents still in the inbox (not yet filed)"},
            "limit": {"type": "integer", "description": "Max results (default 20, max 100)"}}}},
        {"name": "get_document",
         "description": "Read one document: metadata, summary, notes and its full text (OCR). Long texts are paged with offset/max_chars.",
         "inputSchema": {"type": "object", "required": ["id"], "properties": {
            "id": {"type": "integer"},
            "offset": {"type": "integer", "description": "Character offset into the text (default 0)"},
            "max_chars": {"type": "integer", "description": "Max characters of text to return (default 20000)"}}}},
        {"name": "list_metadata",
         "description": "List every tag, correspondent, document type and folder with its document count.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "update_document",
         "description": "Edit a document's metadata: retitle, set date, file it under a correspondent/type, add or remove tags, write a summary or add a note. Tags, correspondents and types are given by name and created if missing. Removing the inbox tag marks the document as filed.",
         "inputSchema": {"type": "object", "required": ["id"], "properties": {
            "id": {"type": "integer"}, "title": s("New title"), "date": s("Document date, YYYY-MM-DD"),
            "correspondent": s("Correspondent name; empty string clears it"),
            "document_type": s("Document type name; empty string clears it"),
            "folder": s("Folder path to move the document into, e.g. Casa/Recibos (created if missing); empty string removes it from its folder"),
            "add_tags": names("Tag names to add"), "remove_tags": names("Tag names to remove"),
            "summary": s("Short summary to store with the document"), "note": s("Text of a note to append")}}},
        {"name": "upload_document",
         "description": "Add a document to the archive and wait until it is processed (text extraction/OCR). Give either content_base64 (PDF, image, Office file) or text (plain text/markdown). Returns the new document, or the existing one if the same file was already archived.",
         "inputSchema": {"type": "object", "required": ["filename"], "properties": {
            "filename": s("File name with extension, e.g. invoice.pdf or notes.md"),
            "content_base64": s("File bytes, base64-encoded"), "text": s("Plain-text content (alternative to content_base64)"),
            "title": s("Title (defaults to the file name)"), "tags": names("Tag names to assign"),
            "correspondent": s("Correspondent name"), "document_type": s("Document type name"),
            "folder": s("Folder path to file it under, e.g. Casa/Recibos (created if missing)")}}}
    ])
}

struct Names(HashMap<i64, String>, HashMap<i64, String>);
impl Names {
    fn load(u: &AuthUser) -> Self {
        Names(
            store::KINDS.iter().flat_map(|k| store::list_labels(k).unwrap_or_default()).map(|l| (l.id, l.name)).collect(),
            store::list_folders(u).unwrap_or_default().into_iter().map(|f| (f.id, f.path)).collect(),
        )
    }
    fn doc(&self, d: &store::Document) -> Value {
        let name = |id: Option<i64>| id.and_then(|i| self.0.get(&i).cloned());
        json!({
            "id": d.id, "title": d.title, "date": d.created, "added": d.added, "pages": d.pages, "file_name": d.original_name,
            "correspondent": name(d.correspondent_id), "document_type": name(d.doctype_id), "folder": d.folder_id.and_then(|f| self.1.get(&f)),
            "tags": d.tags.iter().filter_map(|t| self.0.get(t)).collect::<Vec<_>>(),
            "summary": d.summary, "expires": d.expires, "amount": d.amount,
            "snippet": d.snippet.as_ref().map(|s| s.replace(['\u{1}', '\u{2}'], "**")),
        })
    }
}

fn label_id(kind: &str, name: &str) -> Result<i64, String> {
    store::list_labels(kind).map_err(|e| e.to_string())?.iter().find(|l| l.name.eq_ignore_ascii_case(name.trim())).map(|l| l.id)
        .ok_or_else(|| format!("unknown {kind} «{name}»; call list_metadata to see the valid names"))
}

fn strs(v: &Value) -> Vec<&str> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

async fn call_tool(u: &AuthUser, name: &str, a: &Value) -> Result<Value, String> {
    let e = |e: store::Error| e.to_string();
    if matches!(name, "update_document" | "upload_document") && !u.can_write() {
        return Err("this token is read-only".into());
    }
    match name {
        "search_documents" => {
            let tags = strs(&a["tags"]).iter().map(|t| label_id("tag", t).map(|i| i.to_string())).collect::<Result<Vec<_>, _>>()?;
            let opt = |kind, v: &Value| v.as_str().filter(|s| !s.is_empty()).map(|n| label_id(kind, n)).transpose();
            let p = store::SearchParams {
                q: a["query"].as_str().map(String::from),
                tags: (!tags.is_empty()).then(|| tags.join(",")),
                correspondent: opt("correspondent", &a["correspondent"])?,
                doctype: opt("doctype", &a["document_type"])?,
                folder: match a["folder"].as_str().map(|f| f.trim().trim_matches('/')).filter(|f| !f.is_empty()) {
                    Some(path) => Some(store::list_folders(u).map_err(e)?.iter().find(|f| f.path.eq_ignore_ascii_case(path)).map(|f| f.id)
                        .ok_or_else(|| format!("unknown folder «{path}»; call list_metadata to see the valid paths"))?),
                    None => None,
                },
                from: a["from"].as_str().map(String::from),
                to: a["to"].as_str().map(String::from),
                inbox: a["inbox"].as_bool(),
                limit: Some(a["limit"].as_i64().unwrap_or(20).min(100)),
                ..Default::default()
            };
            let (r, n) = (store::search(&p, u).map_err(e)?, Names::load(u));
            Ok(json!({"total": r.total, "documents": r.items.iter().map(|d| n.doc(d)).collect::<Vec<_>>()}))
        }
        "get_document" => {
            let id = a["id"].as_i64().ok_or("id is required")?;
            store::authorize(u, id).map_err(e)?;
            let d = store::get_doc(id).map_err(e)?;
            let text = d.content.clone().unwrap_or_default();
            let (total, offset, max) = (text.chars().count(), a["offset"].as_u64().unwrap_or(0) as usize, a["max_chars"].as_u64().unwrap_or(20_000) as usize);
            let mut v = Names::load(u).doc(&d);
            v["text"] = text.chars().skip(offset).take(max).collect::<String>().into();
            v["text_total_chars"] = total.into();
            v["text_has_more"] = (offset + max < total).into();
            v["notes"] = json!(d.notes);
            v["custom_fields"] = d.custom.clone();
            Ok(v)
        }
        "list_metadata" => {
            let list = |k| store::list_labels(k).map(|l| l.iter().map(|x| json!({"name": x.name, "documents": x.document_count})).collect::<Vec<_>>()).map_err(e);
            let folders = store::list_folders(u).map_err(e)?.iter().map(|f| json!({"path": f.path, "documents": f.document_count})).collect::<Vec<_>>();
            Ok(json!({"tags": list("tag")?, "correspondents": list("correspondent")?, "document_types": list("doctype")?, "folders": folders}))
        }
        "update_document" => {
            let id = a["id"].as_i64().ok_or("id is required")?;
            store::authorize(u, id).map_err(e)?;
            let ensure = |kind, v: &Value| -> Result<Option<Option<i64>>, String> {
                match v.as_str() {
                    None => Ok(None),
                    Some("") => Ok(Some(None)),
                    Some(n) => store::ensure_label(kind, n).map(|i| Some(Some(i))).map_err(e),
                }
            };
            let patch = store::DocPatch {
                title: a["title"].as_str().map(String::from),
                created: a["date"].as_str().map(String::from),
                correspondent_id: ensure("correspondent", &a["correspondent"])?,
                doctype_id: ensure("doctype", &a["document_type"])?,
                folder_id: a["folder"].as_str().map(|p| store::ensure_folder(p)).transpose().map_err(e)?,
                add_tags: Some(strs(&a["add_tags"]).iter().map(|t| store::ensure_label("tag", t)).collect::<Result<_, _>>().map_err(e)?),
                remove_tags: Some(strs(&a["remove_tags"]).iter().filter_map(|t| label_id("tag", t).ok()).collect()),
                summary: a["summary"].as_str().map(|s| Some(s.to_string())),
                ..Default::default()
            };
            let d = store::update_doc(id, &patch).map_err(e)?;
            store::audit(Some(u), "documento editado (MCP)", &format!("documento {id}"), &d.title);
            if let Some(note) = a["note"].as_str().filter(|n| !n.trim().is_empty()) {
                store::add_note(id, note).map_err(e)?;
            }
            Ok(Names::load(u).doc(&d))
        }
        "upload_document" => {
            let filename = a["filename"].as_str().ok_or("filename is required")?.rsplit(['/', '\\']).next().unwrap_or("document").to_string();
            let bytes = match (a["content_base64"].as_str(), a["text"].as_str()) {
                (Some(b), _) => base64::engine::general_purpose::STANDARD.decode(b.trim()).map_err(|e| format!("content_base64 is not valid base64: {e}"))?,
                (None, Some(t)) => t.as_bytes().to_vec(),
                _ => return Err("give content_base64 or text".into()),
            };
            let ids = |kind, v: &Value| strs(v).iter().map(|n| store::ensure_label(kind, n)).collect::<Result<Vec<_>, _>>().map_err(e);
            let one = |kind, v: &Value| v.as_str().filter(|s| !s.is_empty()).map(|n| store::ensure_label(kind, n)).transpose().map_err(e);
            let meta = json!({"title": a["title"], "tags": ids("tag", &a["tags"])?,
                "correspondent_id": one("correspondent", &a["correspondent"])?, "doctype_id": one("doctype", &a["document_type"])?, "folder": a["folder"]});
            let path = cfg().data.join("tmp").join(format!("up-{}", random_hex(8)));
            tokio::fs::write(&path, bytes).await.map_err(|e| e.to_string())?;
            let job = ingest::enqueue(&path, &filename, "mcp", &meta, Some(u.id)).map_err(e)?;
            store::audit(Some(u), "documento subido (MCP)", &filename, "");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(cfg().cmd_timeout * 2);
            loop {
                let j = ingest::job(job).map_err(e)?;
                match (j.status.as_str(), j.doc_id) {
                    ("done", Some(id)) => return Ok(json!({"status": "created", "document": Names::load(u).doc(&store::get_doc_brief(id).map_err(e)?)})),
                    ("duplicate", Some(id)) => return Ok(json!({"status": "duplicate", "document": store::get_doc_brief(id).ok().map(|d| Names::load(u).doc(&d))})),
                    ("failed", _) => return Err(j.error.unwrap_or_else(|| "processing failed".into())),
                    _ if tokio::time::Instant::now() > deadline => return Err(format!("still processing (job {job}); search for it later")),
                    _ => tokio::time::sleep(Duration::from_millis(250)).await,
                }
            }
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

async fn dispatch(u: &AuthUser, msg: &Value) -> Option<Value> {
    let id = msg.get("id").filter(|i| !i.is_null())?.clone(); // sin id = notificación: no se responde
    let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    match msg["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let asked = msg["params"]["protocolVersion"].as_str().unwrap_or(VERSIONS[0]);
            ok(json!({
                "protocolVersion": if VERSIONS.contains(&asked) { asked } else { VERSIONS[0] },
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "papiro", "title": "Papiro", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Papiro is the user's document archive (scanned mail, invoices, contracts, receipts…) with OCR text. Search before answering questions about the user's paperwork, and read the document text rather than guessing from titles."
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let res = call_tool(u, msg["params"]["name"].as_str().unwrap_or_default(), &msg["params"]["arguments"]).await;
            let (text, is_error) = match res {
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap_or_default(), false),
                Err(e) => (e, true),
            };
            ok(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        m => Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {m}")}})),
    }
}

pub async fn handle(user: AuthUser, Json(msg): Json<Value>) -> Response {
    let reply = match &msg {
        Value::Array(batch) => {
            let mut out = vec![];
            for m in batch {
                out.extend(dispatch(&user, m).await);
            }
            (!out.is_empty()).then(|| Value::Array(out))
        }
        m => dispatch(&user, m).await,
    };
    match reply {
        Some(v) => Json(v).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}
