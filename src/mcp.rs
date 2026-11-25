//! Servidor MCP (Model Context Protocol) sobre Streamable HTTP en POST /mcp.
//! Sin estado: cada petición JSON-RPC se responde con JSON.
//!
//! Autenticación: token Bearer. Puede ser un token creado a mano (Ajustes → Tokens) o uno emitido por
//! el inicio de sesión OAuth (`oauth.rs`), que es lo que usan Claude, ChatGPT y compañía: sin token,
//! el 401 lleva la cabecera `WWW-Authenticate` que les dice dónde iniciar sesión. Un token puede ser
//! de solo lectura y estar limitado a ciertas carpetas: las herramientas lo respetan siempre.
use crate::{api::ApiError, auth::AuthUser, cfg, ingest, oauth, random_hex, store};
use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// Archivos más grandes no se devuelven enteros al modelo: se le sugiere leer el texto o las páginas.
const MAX_FILE: u64 = 10 << 20;
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn tools(u: &AuthUser) -> Value {
    let s = |d: &str| json!({"type": "string", "description": d});
    let int = |d: &str| json!({"type": "integer", "description": d});
    let names = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    let read = |title: &str| json!({"title": title, "readOnlyHint": true, "openWorldHint": false});
    let write = |title: &str, idem: bool| json!({"title": title, "readOnlyHint": false, "destructiveHint": false, "idempotentHint": idem, "openWorldHint": false});
    let mut list = vec![
        json!({"name": "search_documents", "annotations": read("Search documents"),
         "description": "Full-text search over the document archive, with optional filters. Returns matching documents with a text snippet. Call list_metadata first to learn the exact tag/correspondent/type names.",
         "inputSchema": {"type": "object", "properties": {
            "query": s("Words to search in title and content. Use \"quotes\" for an exact phrase. Omit to list by date."),
            "tags": names("Tag names; documents must have all of them"),
            "correspondent": s("Correspondent name (who issued/sent the document)"),
            "document_type": s("Document type name"),
            "folder": s("Folder path, e.g. Home/Receipts; includes its subfolders"),
            "from": s("Earliest document date, YYYY-MM-DD"), "to": s("Latest document date, YYYY-MM-DD"),
            "inbox": {"type": "boolean", "description": "Only documents still in the inbox (not yet filed)"},
            "limit": int("Max results (default 20, max 100)")}}}),
        json!({"name": "browse_folder", "annotations": read("Browse a folder"),
         "description": "Open a folder like a file browser: lists its subfolders and the documents directly inside it, newest first. Omit folder to start at the top. Use this to explore how the archive is organized.",
         "inputSchema": {"type": "object", "properties": {
            "folder": s("Folder path, e.g. Home/Receipts. Omit for the top level"),
            "limit": int("Max documents (default 50, max 200)"), "offset": int("Skip this many documents (paging)")}}}),
        json!({"name": "get_document", "annotations": read("Read a document"),
         "description": "Read one document: metadata, summary, notes and its full text (OCR). Long texts are paged with offset/max_chars.",
         "inputSchema": {"type": "object", "required": ["id"], "properties": {
            "id": {"type": "integer"},
            "offset": int("Character offset into the text (default 0)"),
            "max_chars": int("Max characters of text to return (default 20000)")}}}),
        json!({"name": "get_document_file", "annotations": read("Get the original file"),
         "description": "Get the document's actual file: images come back as images you can see, PDFs and other files as an embedded file (base64), text files as text. Up to 10 MB; for longer documents read the text with get_document or look at pages with get_page_image.",
         "inputSchema": {"type": "object", "required": ["id"], "properties": {
            "id": {"type": "integer"},
            "variant": {"type": "string", "enum": ["original", "archive"], "description": "original = the uploaded file (default); archive = PDF with an OCR text layer, when it exists"}}}}),
        json!({"name": "get_page_image", "annotations": read("Look at a page"),
         "description": "Render one page of a PDF or scanned document as an image, to look at layout, handwriting, stamps, tables or photos that the OCR text doesn't capture.",
         "inputSchema": {"type": "object", "required": ["id"], "properties": {
            "id": {"type": "integer"}, "page": int("Page number, starting at 1 (default 1)")}}}),
        json!({"name": "list_metadata", "annotations": read("List tags, people and folders"),
         "description": "List every tag, correspondent, document type and folder with its document count.",
         "inputSchema": {"type": "object", "properties": {}}}),
    ];
    if u.can_write() {
        list.extend([
            json!({"name": "update_document", "annotations": write("Edit a document", true),
             "description": "Edit a document's metadata: retitle, set date, move it to a folder, file it under a correspondent/type, add or remove tags, write a summary or add a note. Tags, correspondents and types are given by name and created if missing. Removing the inbox tag marks the document as filed.",
             "inputSchema": {"type": "object", "required": ["id"], "properties": {
                "id": {"type": "integer"}, "title": s("New title"), "date": s("Document date, YYYY-MM-DD"),
                "correspondent": s("Correspondent name; empty string clears it"),
                "document_type": s("Document type name; empty string clears it"),
                "folder": s("Folder path to move the document into, e.g. Home/Receipts (created if missing); empty string removes it from its folder"),
                "add_tags": names("Tag names to add"), "remove_tags": names("Tag names to remove"),
                "due_date": s("Expiry/due date YYYY-MM-DD; empty string clears it"),
                "amount": {"type": "number", "description": "Amount (total of an invoice or receipt)"},
                "summary": s("Short summary to store with the document"), "note": s("Text of a note to append")}}}),
            json!({"name": "upload_document", "annotations": write("Upload a document", false),
             "description": "Add a document to the archive and wait until it is processed (text extraction/OCR). Give exactly one of: content_base64 (PDF, image, Office file), text (plain text/markdown) or source_url (a public https link to download). Returns the new document, or the existing one if the same file was already archived.",
             "inputSchema": {"type": "object", "required": ["filename"], "properties": {
                "filename": s("File name with extension, e.g. invoice.pdf or notes.md"),
                "content_base64": s("File bytes, base64-encoded"), "text": s("Plain-text content"),
                "source_url": s("Public http(s) URL to download the file from"),
                "title": s("Title (defaults to the file name)"), "tags": names("Tag names to assign"),
                "correspondent": s("Correspondent name"), "document_type": s("Document type name"),
                "folder": s("Folder path to file it under, e.g. Home/Receipts (created if missing)")}}}),
            json!({"name": "create_folder", "annotations": write("Create a folder", true),
             "description": "Create a folder (and any missing parent folders) from a path like Home/Receipts/2026. Returns the folder; if it already exists, returns it unchanged.",
             "inputSchema": {"type": "object", "required": ["path"], "properties": {"path": s("Folder path, e.g. Home/Receipts/2026")}}}),
        ]);
    }
    Value::Array(list)
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
            "id": d.id, "title": d.title, "date": d.created, "added": d.added, "pages": d.pages, "file_name": d.original_name, "mime_type": d.mime,
            "correspondent": name(d.correspondent_id), "document_type": name(d.doctype_id), "folder": d.folder_id.and_then(|f| self.1.get(&f)),
            "tags": d.tags.iter().filter_map(|t| self.0.get(t)).collect::<Vec<_>>(),
            "summary": d.summary, "due_date": d.expires, "amount": d.amount,
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

fn clean_path(p: &str) -> String {
    p.split('/').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/")
}

/// Carpeta existente (y visible para este token) por su ruta.
fn find_folder(u: &AuthUser, path: &str) -> Result<store::Folder, String> {
    let path = clean_path(path);
    store::list_folders(u).map_err(|e| e.to_string())?.into_iter().find(|f| f.path.eq_ignore_ascii_case(&path))
        .ok_or_else(|| format!("unknown folder «{path}»; call browse_folder or list_metadata to see the valid paths"))
}

/// Carpeta por ruta, creándola si falta. Un token limitado solo puede crear dentro de sus carpetas.
fn ensure_folder(u: &AuthUser, path: &str) -> Result<Option<i64>, String> {
    let path = clean_path(path);
    if let Some(scope) = store::scope_ids(u) {
        let inside = store::list_folders(u).map_err(|e| e.to_string())?.iter().filter(|f| scope.contains(&f.id)).any(|f| {
            let lp = f.path.to_lowercase();
            let p = path.to_lowercase();
            p == lp || p.starts_with(&format!("{lp}/"))
        });
        if !inside {
            return Err(format!("this connection can only use these folders: {}", allowed_paths(u).join(", ")));
        }
    }
    store::ensure_folder(&path).map_err(|e| e.to_string())
}

fn allowed_paths(u: &AuthUser) -> Vec<String> {
    let roots = u.folders.clone().unwrap_or_default();
    store::list_folders(u).unwrap_or_default().into_iter().filter(|f| roots.contains(&f.id)).map(|f| f.path).collect()
}

/// Lo que devuelve una herramienta: JSON (se envía como texto) o bloques de contenido ya armados.
enum Out {
    Json(Value),
    Blocks(Vec<Value>),
}

async fn download(url: &str) -> Result<Vec<u8>, String> {
    store::webhook_target_ok(url)?;
    let (url, max) = (url.to_string(), cfg().max_upload as u64);
    tokio::task::spawn_blocking(move || {
        let mut r = crate::http().get(&url).config().timeout_global(Some(Duration::from_secs(60))).build().call().map_err(|e| format!("could not download it: {e}"))?;
        r.body_mut().with_config().limit(max).read_to_vec().map_err(|e| format!("could not download it: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn call_tool(u: &AuthUser, name: &str, a: &Value) -> Result<Out, String> {
    let e = |e: store::Error| e.to_string();
    if matches!(name, "update_document" | "upload_document" | "create_folder") && !u.can_write() {
        return Err("this connection is read-only".into());
    }
    let n = || Names::load(u);
    match name {
        "search_documents" => {
            let tags = strs(&a["tags"]).iter().map(|t| label_id("tag", t).map(|i| i.to_string())).collect::<Result<Vec<_>, _>>()?;
            let opt = |kind, v: &Value| v.as_str().filter(|s| !s.is_empty()).map(|n| label_id(kind, n)).transpose();
            let p = store::SearchParams {
                q: a["query"].as_str().map(String::from),
                tags: (!tags.is_empty()).then(|| tags.join(",")),
                correspondent: opt("correspondent", &a["correspondent"])?,
                doctype: opt("doctype", &a["document_type"])?,
                folder: match a["folder"].as_str().map(clean_path).filter(|f| !f.is_empty()) {
                    Some(path) => Some(find_folder(u, &path)?.id),
                    None => None,
                },
                from: a["from"].as_str().map(String::from),
                to: a["to"].as_str().map(String::from),
                inbox: a["inbox"].as_bool(),
                limit: Some(a["limit"].as_i64().unwrap_or(20).min(100)),
                ..Default::default()
            };
            let (r, n) = (store::search(&p, u).map_err(e)?, n());
            Ok(Out::Json(json!({"total": r.total, "documents": r.items.iter().map(|d| n.doc(d)).collect::<Vec<_>>()})))
        }
        "browse_folder" => {
            let all = store::list_folders(u).map_err(e)?;
            let here = match a["folder"].as_str().map(clean_path).filter(|f| !f.is_empty()) {
                Some(path) => Some(find_folder(u, &path)?),
                None => None,
            };
            let sub = |f: &store::Folder| json!({"path": f.path, "name": f.name, "documents": f.document_count});
            let subfolders: Vec<Value> = match (&here, &u.folders) {
                (Some(h), _) => all.iter().filter(|f| f.parent_id == Some(h.id)).map(sub).collect(),
                (None, Some(roots)) => all.iter().filter(|f| roots.contains(&f.id)).map(sub).collect(),
                (None, None) => all.iter().filter(|f| f.parent_id.is_none()).map(sub).collect(),
            };
            let (limit, offset) = (a["limit"].as_i64().unwrap_or(50).clamp(1, 200), a["offset"].as_i64().unwrap_or(0).max(0));
            // En la raíz, un token limitado no ve documentos sueltos: solo sus carpetas.
            let docs = match (&here, &u.folders) {
                (None, Some(_)) => None,
                (h, _) => Some(store::search(
                    &store::SearchParams { folder: Some(h.as_ref().map_or(0, |f| f.id)), direct: Some(true), limit: Some(limit), offset: Some(offset), ..Default::default() },
                    u,
                ).map_err(e)?),
            };
            let n = n();
            Ok(Out::Json(json!({
                "folder": here.as_ref().map(|f| &f.path), "parent": here.as_ref().and_then(|h| h.parent_id).and_then(|p| all.iter().find(|f| f.id == p)).map(|f| &f.path),
                "subfolders": subfolders,
                "documents": docs.as_ref().map(|d| d.items.iter().map(|x| n.doc(x)).collect::<Vec<_>>()).unwrap_or_default(),
                "documents_total": docs.as_ref().map_or(0, |d| d.total),
                "documents_have_more": docs.as_ref().is_some_and(|d| offset + (d.items.len() as i64) < d.total),
            })))
        }
        "get_document" => {
            let id = a["id"].as_i64().ok_or("id is required")?;
            store::authorize(u, id).map_err(e)?;
            let d = store::get_doc(id).map_err(e)?;
            let text = d.content.clone().unwrap_or_default();
            let (total, offset, max) = (text.chars().count(), a["offset"].as_u64().unwrap_or(0) as usize, a["max_chars"].as_u64().unwrap_or(20_000) as usize);
            let mut v = n().doc(&d);
            v["text"] = text.chars().skip(offset).take(max).collect::<String>().into();
            v["text_total_chars"] = total.into();
            v["text_has_more"] = (offset + max < total).into();
            v["notes"] = json!(d.notes);
            v["custom_fields"] = d.custom.clone();
            Ok(Out::Json(v))
        }
        "get_document_file" => {
            let id = a["id"].as_i64().ok_or("id is required")?;
            store::authorize(u, id).map_err(e)?;
            let d = store::get_doc_brief(id).map_err(e)?;
            let archive = a["variant"].as_str() == Some("archive") && d.has_archive;
            let (file, mime, fname) = if archive {
                ("archive.pdf", "application/pdf".to_string(), format!("{}.pdf", d.title))
            } else {
                ("original", d.mime.clone(), d.original_name.clone())
            };
            let path = store::doc_dir(id).join(file);
            let size = tokio::fs::metadata(&path).await.map_err(|_| "this document's file is not on disk".to_string())?.len();
            if size > MAX_FILE {
                return Err(format!("the file is {} MB, too big to send whole; read its text with get_document or look at pages with get_page_image", size >> 20));
            }
            let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
            let uri = format!("papiro://documents/{id}/{}", if archive { "archive" } else { "original" });
            let about = json!({"type": "text", "text": format!("{fname} · {mime} · {} KB · document #{id}", size.div_ceil(1024))});
            let block = if mime.starts_with("image/") && ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&mime.as_str()) {
                json!({"type": "image", "data": B64.encode(&bytes), "mimeType": mime})
            } else if mime.starts_with("text/") || mime == "application/json" {
                json!({"type": "resource", "resource": {"uri": uri, "mimeType": mime, "text": String::from_utf8_lossy(&bytes)}})
            } else {
                json!({"type": "resource", "resource": {"uri": uri, "mimeType": mime, "blob": B64.encode(&bytes)}})
            };
            Ok(Out::Blocks(vec![about, block]))
        }
        "get_page_image" => {
            let id = a["id"].as_i64().ok_or("id is required")?;
            store::authorize(u, id).map_err(e)?;
            let d = store::get_doc_brief(id).map_err(e)?;
            let page = a["page"].as_u64().unwrap_or(1).max(1) as u32;
            if d.mime.starts_with("image/") && page == 1 && !d.has_archive {
                return Box::pin(call_tool(u, "get_document_file", &json!({"id": id}))).await;
            }
            if !d.has_archive && d.mime != "application/pdf" {
                return Err("this document has no pages to render (it is not a PDF or scan); use get_document or get_document_file".into());
            }
            if i64::from(page) > d.pages.max(1) {
                return Err(format!("the document has {} page(s)", d.pages));
            }
            let file = store::doc_dir(id).join(if d.has_archive { "archive.pdf" } else { "original" });
            let _permit = crate::api::RENDERS.acquire().await.map_err(|e| e.to_string())?;
            let jpeg = tokio::task::spawn_blocking(move || ingest::render_page(&file, page, 1200)).await.map_err(|e| e.to_string())?.map_err(e)?;
            Ok(Out::Blocks(vec![
                json!({"type": "text", "text": format!("«{}», page {page} of {}", d.title, d.pages.max(1))}),
                json!({"type": "image", "data": B64.encode(jpeg), "mimeType": "image/jpeg"}),
            ]))
        }
        "list_metadata" => {
            let list = |k| store::list_labels(k).map(|l| l.iter().map(|x| json!({"name": x.name, "documents": x.document_count})).collect::<Vec<_>>()).map_err(e);
            let folders = store::list_folders(u).map_err(e)?.iter().map(|f| json!({"path": f.path, "documents": f.document_count})).collect::<Vec<_>>();
            Ok(Out::Json(json!({"tags": list("tag")?, "correspondents": list("correspondent")?, "document_types": list("doctype")?, "folders": folders})))
        }
        "create_folder" => {
            let id = ensure_folder(u, a["path"].as_str().ok_or("path is required")?)?.ok_or("the path is empty")?;
            store::notify(json!({"type": "folders"}));
            let f = store::list_folders(u).map_err(e)?.into_iter().find(|f| f.id == id).ok_or("folder not found")?;
            store::audit(Some(u), "folder created", &f.path, "MCP");
            Ok(Out::Json(json!({"path": f.path, "documents": f.document_count})))
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
            let folder_id = match a["folder"].as_str() {
                None => None,
                Some(p) if clean_path(p).is_empty() && u.folders.is_some() => return Err("this connection cannot take documents out of its folders".into()),
                Some(p) => Some(ensure_folder(u, p)?),
            };
            let patch = store::DocPatch {
                title: a["title"].as_str().map(String::from),
                created: a["date"].as_str().map(String::from),
                correspondent_id: ensure("correspondent", &a["correspondent"])?,
                doctype_id: ensure("doctype", &a["document_type"])?,
                folder_id,
                expires: a["due_date"].as_str().map(|d| Some(d.to_string()).filter(|d| !d.is_empty())),
                amount: a["amount"].as_f64().map(Some),
                add_tags: Some(strs(&a["add_tags"]).iter().map(|t| store::ensure_label("tag", t)).collect::<Result<_, _>>().map_err(e)?),
                remove_tags: Some(strs(&a["remove_tags"]).iter().filter_map(|t| label_id("tag", t).ok()).collect()),
                summary: a["summary"].as_str().map(|s| Some(s.to_string())),
                ..Default::default()
            };
            let d = store::update_doc(id, &patch).map_err(e)?;
            store::audit(Some(u), "document edited (MCP)", &format!("document {id}"), &d.title);
            if let Some(note) = a["note"].as_str().filter(|n| !n.trim().is_empty()) {
                store::add_note(id, note).map_err(e)?;
            }
            Ok(Out::Json(n().doc(&d)))
        }
        "upload_document" => {
            let filename = a["filename"].as_str().ok_or("filename is required")?.rsplit(['/', '\\']).next().unwrap_or("document").to_string();
            let bytes = match (a["content_base64"].as_str(), a["text"].as_str(), a["source_url"].as_str()) {
                (Some(b), _, _) => B64.decode(b.trim()).map_err(|e| format!("content_base64 is not valid base64: {e}"))?,
                (None, Some(t), _) => t.as_bytes().to_vec(),
                (None, None, Some(url)) => download(url).await?,
                _ => return Err("give content_base64, text or source_url".into()),
            };
            let folder = match a["folder"].as_str().map(clean_path).filter(|f| !f.is_empty()) {
                Some(p) => ensure_folder(u, &p)?,
                // Sin carpeta, un token limitado guarda en la primera de las suyas.
                None => match &u.folders {
                    Some(roots) => Some(*roots.iter().find(|f| store::folder_exists(**f)).ok_or("this connection's folders no longer exist")?),
                    None => None,
                },
            };
            let ids = |kind, v: &Value| strs(v).iter().map(|n| store::ensure_label(kind, n)).collect::<Result<Vec<_>, _>>().map_err(e);
            let one = |kind, v: &Value| v.as_str().filter(|s| !s.is_empty()).map(|n| store::ensure_label(kind, n)).transpose().map_err(e);
            let meta = json!({"title": a["title"], "tags": ids("tag", &a["tags"])?, "folder_id": folder,
                "correspondent_id": one("correspondent", &a["correspondent"])?, "doctype_id": one("doctype", &a["document_type"])?});
            let path = cfg().data.join("tmp").join(format!("up-{}", random_hex(8)));
            tokio::fs::write(&path, bytes).await.map_err(|e| e.to_string())?;
            if let Err(err) = ingest::supported(&path, &filename) {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(err);
            }
            let job = ingest::enqueue(&path, &filename, "mcp", &meta, Some(u.id)).map_err(e)?;
            store::audit(Some(u), "document uploaded (MCP)", &filename, "");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(cfg().cmd_timeout * 2);
            loop {
                let j = ingest::job(job).map_err(e)?;
                match (j.status.as_str(), j.doc_id) {
                    ("done", Some(id)) => return Ok(Out::Json(json!({"status": "created", "document": n().doc(&store::get_doc_brief(id).map_err(e)?)}))),
                    ("duplicate", Some(id)) => return Ok(Out::Json(json!({"status": "duplicate", "document": store::get_doc_brief(id).ok().map(|d| n().doc(&d))}))),
                    ("failed", _) => return Err(j.error.unwrap_or_else(|| "processing failed".into())),
                    _ if tokio::time::Instant::now() > deadline => return Err(format!("still processing (job {job}); search for it later")),
                    _ => tokio::time::sleep(Duration::from_millis(250)).await,
                }
            }
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn instructions(u: &AuthUser) -> String {
    let mut s = "Papiro is the user's document archive (scanned mail, invoices, contracts, receipts, manuals…) with OCR text, organized in folders. \
Search before answering questions about the user's paperwork, and read the document text rather than guessing from titles. \
Use browse_folder to navigate the folders like a file browser, get_page_image to look at a page, and get_document_file for the original file."
        .to_string();
    if !u.can_write() {
        s.push_str(" This connection is read-only.");
    }
    if u.folders.is_some() {
        s.push_str(&format!(" This connection can only see these folders (and their subfolders): {}.", allowed_paths(u).join(", ")));
    }
    s
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
                "instructions": instructions(u)
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools(u)})),
        "tools/call" => {
            let content = match call_tool(u, msg["params"]["name"].as_str().unwrap_or_default(), &msg["params"]["arguments"]).await {
                Ok(Out::Json(v)) => Ok(vec![json!({"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()})]),
                Ok(Out::Blocks(b)) => Ok(b),
                Err(e) => Err(e),
            };
            ok(match content {
                Ok(c) => json!({"content": c, "isError": false}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            })
        }
        m => Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {m}")}})),
    }
}

/// Sin token válido, un 401 con `WWW-Authenticate`: así el cliente MCP descubre el inicio de sesión
/// OAuth. La cookie de sesión del navegador no sirve aquí (solo tokens Bearer).
pub async fn handle(headers: HeaderMap, user: Result<AuthUser, ApiError>, Json(msg): Json<Value>) -> Response {
    let user = match user {
        Ok(u) if u.token => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, oauth::www_authenticate(&headers))],
                Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32001, "message": "sign in to Papiro to use this server"}})),
            )
                .into_response();
        }
    };
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

/// Un GET o una petición sin cuerpo a /mcp también debe invitar a iniciar sesión si no hay token.
pub async fn probe(headers: HeaderMap, user: Result<AuthUser, ApiError>) -> Response {
    match user {
        Ok(u) if u.token => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        _ => (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, oauth::www_authenticate(&headers))]).into_response(),
    }
}
