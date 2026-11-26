//! API REST. Cada handler lleva su anotación OpenAPI; el documento se sirve en /api/openapi.json.
//!
//! Permisos: leer exige sesión o token; modificar exige rol editor o admin (extractor `Editor`);
//! administrar y borrar definitivamente exige admin (`Admin`). Los documentos privados solo los ven
//! su dueño y los administradores (`store::authorize`).
use crate::{
    auth::{self, Admin, AuthUser, Editor},
    cfg, db,
    ingest::{self, Job},
    random_hex,
    store::{self, AuditEntry, BulkEdit, DocList, DocPatch, Document, Folder, FolderInput, Label, LabelInput, Note, SearchParams, Stats, VerifyReport},
};
use axum::{
    Json,
    extract::{Multipart, Path, Query, Request},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response, sse},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use tower::ServiceExt;
use utoipa::{IntoParams, Modify, OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub struct ApiError(pub StatusCode, pub String);
type Res<T> = Result<T, ApiError>;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<store::Error> for ApiError {
    fn from(e: store::Error) -> Self {
        let code = match e {
            store::Error::NotFound => StatusCode::NOT_FOUND,
            store::Error::Conflict(_) => StatusCode::CONFLICT,
            store::Error::Bad(_) => StatusCode::BAD_REQUEST,
            store::Error::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(code, e.to_string())
    }
}
fn bad(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}
fn forbidden(msg: &str) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, msg.into())
}

/// Error body shared by the whole API.
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Papiro API",
        description = "Papiro REST API. Authentication: an `Authorization: Bearer <token>` header with a token created in Settings → API tokens (or with `POST /api/tokens`). The web interface uses a session cookie.\n\nRoles: `viewer` can only read; `editor` uploads and edits; `admin` can also manage users and webhooks and delete permanently. A token can be read-only.\n\nErrors always return `{\"error\": \"message\"}` with the matching HTTP status code.\n\nFor AI agents there is also an MCP server (Streamable HTTP) at `/mcp` that accepts the same token."
    ),
    tags(
        (name = "documents", description = "Upload, search, read and edit documents"),
        (name = "folders", description = "Nested folders to organize documents"),
        (name = "labels", description = "Tags, correspondents and document types"),
        (name = "queue", description = "Ingest status and live events"),
        (name = "webhooks", description = "Signed HTTP notifications when a document changes"),
        (name = "notifications", description = "Push notifications to the browser or the installed app"),
        (name = "sharing", description = "Temporary links to a document, smart folders and the due-date calendar"),
        (name = "bundles", description = "Bundles: the checklist of documents a procedure asks for, exported as a .zip or a link"),
        (name = "account", description = "Session, users and API tokens"),
        (name = "system", description = "Health, integrity and audit log (administrators)")
    ),
    components(schemas(ErrorBody)),
    security(("token" = [])),
    modifiers(&Security)
)]
struct ApiDoc;

struct Security;
impl Modify for Security {
    fn modify(&self, api: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
        api.components.get_or_insert_default().add_security_scheme(
            "token",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).description(Some("API token (ppr_…)")).build()),
        );
    }
}

pub fn router() -> (axum::Router, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(list_documents, upload_documents))
        .routes(routes!(bulk_edit))
        .routes(routes!(get_document, update_document, delete_document))
        .routes(routes!(restore_document))
        .routes(routes!(document_file))
        .routes(routes!(document_thumb))
        .routes(routes!(document_page))
        .routes(routes!(document_ai))
        .routes(routes!(document_suggest))
        .routes(routes!(add_note))
        .routes(routes!(delete_note))
        .routes(routes!(list_tags, create_tag))
        .routes(routes!(update_tag, delete_tag))
        .routes(routes!(list_correspondents, create_correspondent))
        .routes(routes!(update_correspondent, delete_correspondent))
        .routes(routes!(list_doctypes, create_doctype))
        .routes(routes!(update_doctype, delete_doctype))
        .routes(routes!(list_folders, create_folder))
        .routes(routes!(update_folder, delete_folder))
        .routes(routes!(list_jobs))
        .routes(routes!(get_job, dismiss_job))
        .routes(routes!(retry_job))
        .routes(routes!(events))
        .routes(routes!(get_stats))
        .routes(routes!(get_status))
        .routes(routes!(run_verify))
        .routes(routes!(list_audit))
        .routes(routes!(list_webhooks, create_webhook))
        .routes(routes!(delete_webhook))
        .routes(routes!(test_webhook))
        .routes(routes!(auth_status))
        .routes(routes!(auth_setup))
        .routes(routes!(auth_login))
        .routes(routes!(auth_logout))
        .routes(routes!(auth_password))
        .routes(routes!(list_tokens, create_token))
        .routes(routes!(delete_token))
        .routes(routes!(list_versions, upload_version))
        .routes(routes!(version_file))
        .routes(routes!(list_reminders, create_reminder))
        .routes(routes!(upcoming_reminders))
        .routes(routes!(delete_reminder))
        .routes(routes!(get_titles, put_titles))
        .routes(routes!(preview_titles))
        .routes(routes!(list_bundles, create_bundle))
        .routes(routes!(get_bundle, update_bundle, delete_bundle))
        .routes(routes!(add_bundle_item))
        .routes(routes!(update_bundle_item, delete_bundle_item))
        .routes(routes!(bundle_zip))
        .routes(routes!(create_bundle_share))
        .routes(routes!(delete_bundle_share))
        .routes(routes!(list_views, create_view))
        .routes(routes!(delete_view))
        .routes(routes!(list_shares, create_share))
        .routes(routes!(delete_share))
        .routes(routes!(create_calendar, delete_calendar))
        .routes(routes!(push_status))
        .routes(routes!(push_subscribe))
        .routes(routes!(push_unsubscribe))
        .routes(routes!(push_test))
        .routes(routes!(list_users, create_user))
        .routes(routes!(update_user, delete_user))
        .split_for_parts()
}

/// Argon2 uses 19 MB per hash: at most two run at once and the rest wait their turn.
async fn hashing<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    let _permit = tokio::time::timeout(Duration::from_secs(8), auth::HASHING.acquire())
        .await
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "server busy; try again in a few seconds".into()))?
        .map_err(bad)?;
    tokio::task::spawn_blocking(f).await.map_err(bad)
}

// ───────────────────────── documentos ─────────────────────────

/// Search and list documents
///
/// Full-text search (SQLite FTS5, ranked by BM25 relevance) combined with filters.
/// Without `q` it returns the list sorted by date. Only includes what the user can see.
#[utoipa::path(get, path = "/api/documents", tag = "documents", params(SearchParams),
    responses((status = 200, body = DocList), (status = 401, body = ErrorBody)))]
async fn list_documents(u: AuthUser, Query(p): Query<SearchParams>) -> Res<Json<DocList>> {
    Ok(Json(store::search(&p, &u)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct UploadQuery {
    /// Wait until processing finishes and return the created documents
    wait: Option<bool>,
}

/// Multipart upload form
#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadForm {
    /// File to upload (repeat the field to upload several)
    #[schema(value_type = String, format = Binary)]
    file: String,
    /// Title (defaults to the file name)
    title: Option<String>,
    /// Document date YYYY-MM-DD (detected from the content by default)
    created: Option<String>,
    correspondent_id: Option<i64>,
    doctype_id: Option<i64>,
    /// Comma-separated tag IDs
    tags: Option<String>,
    /// `true` so that only the uploader and administrators can see it
    private: Option<bool>,
    /// Folder to store it in
    folder_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
struct UploadResult {
    /// One ingest job per file
    jobs: Vec<Job>,
    /// With `wait=true`: the resulting documents (including existing ones if the file was a duplicate)
    documents: Vec<Document>,
}

/// Upload documents
///
/// Stores the files and queues them for extraction/OCR. Responds `202` right away with the jobs;
/// with `?wait=true` it waits for the result and responds `201` with the documents. Duplicates (same
/// SHA-256) are not stored again: the job ends as `duplicate`, pointing at the existing document.
/// An unsupported file type is rejected with `415` and nothing is queued. If a job fails, the
/// file is kept and can be retried with `POST /api/jobs/{id}/retry`.
#[utoipa::path(post, path = "/api/documents", tag = "documents", params(UploadQuery),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 202, body = UploadResult, description = "Queued"), (status = 201, body = UploadResult, description = "Processed (wait=true)"),
              (status = 400, body = ErrorBody), (status = 415, body = ErrorBody), (status = 507, body = ErrorBody, description = "Out of disk space")))]
async fn upload_documents(Editor(u): Editor, Query(q): Query<UploadQuery>, mut mp: Multipart) -> Res<(StatusCode, Json<UploadResult>)> {
    let mut staged: Vec<(std::path::PathBuf, String)> = vec![];
    let received = receive_upload(&mut mp, &mut staged).await;
    let meta = match received.and_then(|mut meta| {
        // Un token limitado a carpetas sube siempre dentro de ellas (por defecto, a la primera).
        if let Some(roots) = &u.folders {
            match meta["folder_id"].as_i64() {
                None => meta["folder_id"] = roots.iter().copied().find(|f| store::folder_exists(*f)).ok_or_else(store::scope_error)?.into(),
                Some(f) if !store::folder_allowed(&u, Some(f)) => return Err(store::scope_error().into()),
                _ => {}
            }
        }
        Ok(meta)
    }) {
        Ok(meta) if !staged.is_empty() => meta,
        other => {
            // Nada queda a medias: si la subida falla, sus temporales se borran.
            for (p, _) in &staged {
                let _ = tokio::fs::remove_file(p).await;
            }
            let err = other.err().unwrap_or_else(|| bad("missing \"file\" field"));
            // El motivo queda en el log: una subida cortada por el navegador solo se ve desde aquí.
            println!("upload rejected ({}): {}", err.0.as_u16(), err.1);
            return Err(err);
        }
    };
    let mut rx = store::bus().subscribe();
    let mut ids = vec![];
    for (path, filename) in &staged {
        ids.push(ingest::enqueue(path, filename, if u.token { "api" } else { "upload" }, &meta, Some(u.id))?);
        store::audit(Some(&u), "document uploaded", filename, "");
    }
    let load = |ids: &[i64]| ids.iter().map(|id| ingest::job(*id)).collect::<store::Result<Vec<Job>>>();
    let mut jobs = load(&ids)?;
    if q.wait != Some(true) {
        return Ok((StatusCode::ACCEPTED, Json(UploadResult { jobs, documents: vec![] })));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(cfg().cmd_timeout * 2);
    while jobs.iter().any(|j| j.status == "queued" || j.status == "running") && tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
        jobs = load(&ids)?;
    }
    let documents = jobs.iter().filter_map(|j| j.doc_id).filter_map(|id| store::get_doc_brief(id).ok()).collect();
    Ok((StatusCode::CREATED, Json(UploadResult { jobs, documents })))
}

async fn receive_upload(mp: &mut Multipart, staged: &mut Vec<(std::path::PathBuf, String)>) -> Res<Value> {
    let io = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::StorageFull => ApiError(StatusCode::INSUFFICIENT_STORAGE, "no disk space left".into()),
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut meta = json!({});
    while let Some(mut field) = mp.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or_default().rsplit(['/', '\\']).next().filter(|n| !n.trim().is_empty()).unwrap_or("document").to_string();
            let path = cfg().data.join("tmp").join(format!("up-{}", random_hex(8)));
            staged.push((path.clone(), filename.clone()));
            let mut f = tokio::fs::File::create(&path).await.map_err(io)?;
            // Directo a disco por trozos: subir un archivo grande no ocupa RAM.
            while let Some(chunk) = field.chunk().await.map_err(bad)? {
                f.write_all(&chunk).await.map_err(io)?;
            }
            f.sync_all().await.map_err(io)?;
            if let Err(e) = ingest::supported(&path, &filename) {
                return Err(ApiError(StatusCode::UNSUPPORTED_MEDIA_TYPE, e));
            }
        } else {
            let v = field.text().await.map_err(bad)?;
            match name.as_str() {
                "tags" => meta["tags"] = v.split(',').filter_map(|t| t.trim().parse::<i64>().ok()).collect(),
                "folder_id" => match v.trim().parse::<i64>() {
                    Ok(f) if store::folder_exists(f) => meta["folder_id"] = f.into(),
                    _ => return Err(bad("that folder does not exist")),
                },
                "correspondent_id" | "doctype_id" => meta[&name] = v.trim().parse::<i64>().map(Value::from).unwrap_or(Value::Null),
                "private" => meta["is_private"] = Value::Bool(matches!(v.trim(), "true" | "1" | "on")),
                _ => meta[&name] = Value::from(v),
            }
        }
    }
    Ok(meta)
}

/// A token limited to some folders can only move documents between them.
fn check_scope(u: &AuthUser, p: &DocPatch) -> Res<()> {
    match p.folder_id {
        Some(f) if !store::folder_allowed(u, f) => Err(store::scope_error().into()),
        _ => Ok(()),
    }
}

/// Only a document's owner or an administrator can make it private or public.
fn check_privacy(u: &AuthUser, id: i64, p: &DocPatch) -> Res<()> {
    if p.is_private.is_some() && u.role != "admin" && store::get_doc_brief(id)?.owner_id != Some(u.id) {
        return Err(forbidden("only the uploader or an administrator can change a document's privacy"));
    }
    Ok(())
}

#[derive(Serialize, ToSchema)]
struct Affected {
    affected: usize,
}

/// Bulk edit
///
/// Applies the same fields as `PATCH /api/documents/{id}` to all the `ids`, or runs an action
/// (`trash`, `restore`, `purge`; the last one is for administrators only). All or nothing.
#[utoipa::path(post, path = "/api/documents/bulk", tag = "documents", request_body = BulkEdit,
    responses((status = 200, body = Affected), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn bulk_edit(Editor(u): Editor, Json(b): Json<BulkEdit>) -> Res<Json<Affected>> {
    if b.action.as_deref() == Some("purge") && !u.admin() {
        return Err(forbidden("only an administrator can delete permanently"));
    }
    check_scope(&u, &b.patch)?;
    for id in &b.ids {
        store::authorize(&u, *id)?;
        check_privacy(&u, *id, &b.patch)?;
    }
    let n = store::bulk(&b)?;
    store::audit(Some(&u), &format!("bulk edit{}", b.action.as_deref().map(|a| format!(" ({a})")).unwrap_or_default()), &format!("{n} documents"), &format!("{:?}", b.ids));
    Ok(Json(Affected { affected: n }))
}

/// Read a document (metadata, full text and notes)
#[utoipa::path(get, path = "/api/documents/{id}", tag = "documents", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 404, body = ErrorBody)))]
async fn get_document(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    Ok(Json(store::get_doc(id)?))
}

/// Edit a document
///
/// Only the fields sent are changed. `null` in `correspondent_id`, `doctype_id`, `asn` or `summary` clears them.
#[utoipa::path(patch, path = "/api/documents/{id}", tag = "documents", params(("id" = i64, Path,)), request_body = DocPatch,
    responses((status = 200, body = Document), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update_document(Editor(u): Editor, Path(id): Path<i64>, Json(p): Json<DocPatch>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    check_privacy(&u, id, &p)?;
    check_scope(&u, &p)?;
    let d = store::update_doc(id, &p)?;
    store::audit(Some(&u), "document edited", &format!("document {id}"), &d.title);
    Ok(Json(d))
}

#[derive(Deserialize, ToSchema)]
struct SuggestAction {
    /// `accept` applies the pending suggestions, `dismiss` discards them, `refresh` analyzes the document again
    action: String,
    /// Limits the action to certain suggestions: `folder_id`, `doctype_id`, `correspondent_id`, `tag:<id>`. Without it, all of them
    fields: Option<Vec<String>>,
}

/// Local analysis suggestions
///
/// Papiro analyzes every document without anything leaving the server: rules detect the expiry date, the amount,
/// the CURP and the RFC, and it proposes a folder, type, correspondent and tags by similarity to what is already filed.
/// This endpoint accepts or dismisses the pending suggestions, or requests a fresh analysis.
#[utoipa::path(post, path = "/api/documents/{id}/suggest", tag = "documents", params(("id" = i64, Path,)), request_body = SuggestAction,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn document_suggest(Editor(u): Editor, Path(id): Path<i64>, Json(a): Json<SuggestAction>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    match a.action.as_str() {
        "accept" => {
            crate::suggest::resolve(id, true, a.fields.as_deref())?;
            store::audit(Some(&u), "suggestions accepted", &format!("document {id}"), "");
        }
        "dismiss" => crate::suggest::resolve(id, false, a.fields.as_deref())?,
        "refresh" => crate::suggest::analyze(id, true)?,
        _ => return Err(bad("action must be accept, dismiss or refresh")),
    }
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct DeleteQuery {
    /// Delete permanently (row and files) instead of moving to the trash. Administrators only.
    permanent: Option<bool>,
}

/// Move to the trash (or delete permanently)
#[utoipa::path(delete, path = "/api/documents/{id}", tag = "documents", params(("id" = i64, Path,), DeleteQuery),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn delete_document(Editor(u): Editor, Path(id): Path<i64>, Query(q): Query<DeleteQuery>) -> Res<StatusCode> {
    store::authorize(&u, id)?;
    let title = store::get_doc_brief(id)?.title;
    if q.permanent == Some(true) {
        if !u.admin() {
            return Err(forbidden("only an administrator can delete permanently"));
        }
        store::purge(id)?;
        store::audit(Some(&u), "document permanently deleted", &format!("document {id}"), &title);
    } else {
        store::trash(id, true)?;
        store::audit(Some(&u), "document moved to trash", &format!("document {id}"), &title);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Restore from the trash
#[utoipa::path(post, path = "/api/documents/{id}/restore", tag = "documents", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 404, body = ErrorBody)))]
async fn restore_document(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    store::trash(id, false)?;
    store::audit(Some(&u), "document restored", &format!("document {id}"), "");
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileQuery {
    /// `original` (what was uploaded) or `archive` (PDF with a text layer). Default: archive if it exists.
    variant: Option<String>,
    /// Force a download instead of displaying it in the browser
    download: Option<bool>,
}

pub(crate) async fn serve_file(path: std::path::PathBuf, mime: &str, disposition: Option<String>, req: Request) -> Res<Response> {
    let mut res = tower_http::services::ServeFile::new(path).oneshot(req).await.map_err(bad)?.map(axum::body::Body::new);
    if res.status() == StatusCode::NOT_FOUND {
        return Err(ApiError(StatusCode::NOT_FOUND, "this document's file is not on disk; check Settings → Health".into()));
    }
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).map_err(bad)?);
    if let Some(d) = disposition {
        h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&d).map_err(bad)?);
    }
    Ok(res)
}

/// Download the file
///
/// Supports range requests (`Range`).
#[utoipa::path(get, path = "/api/documents/{id}/file", tag = "documents", params(("id" = i64, Path,), FileQuery),
    responses((status = 200, description = "File contents", content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = ErrorBody)))]
async fn document_file(u: AuthUser, Path(id): Path<i64>, Query(q): Query<FileQuery>, req: Request) -> Res<Response> {
    store::authorize(&u, id)?;
    let d = store::get_doc_brief(id)?;
    let archive = d.has_archive && q.variant.as_deref() != Some("original");
    let (file, mime, name) = if archive {
        ("archive.pdf", "application/pdf".to_string(), format!("{}.pdf", d.title))
    } else {
        ("original", d.mime.clone(), d.original_name.clone())
    };
    // Solo PDF e imágenes se muestran en línea; lo demás siempre se descarga.
    let inline = q.download != Some(true) && (mime == "application/pdf" || mime.starts_with("image/"));
    let ascii: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let disposition = format!("{}; filename=\"{ascii}\"", if inline { "inline" } else { "attachment" });
    serve_file(store::doc_dir(id).join(file), &mime, Some(disposition), req).await
}

// Como mucho dos renders a la vez: abrir un PDF de 300 páginas no dispara ni CPU ni RAM.
pub static RENDERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Thumbnail of the first page (JPEG)
///
/// If it does not exist yet (imported documents), it is generated on the first request and stored.
#[utoipa::path(get, path = "/api/documents/{id}/thumb", tag = "documents", params(("id" = i64, Path,)),
    responses((status = 200, description = "JPEG image", content_type = "image/jpeg", body = Vec<u8>), (status = 404, body = ErrorBody)))]
async fn document_thumb(u: AuthUser, Path(id): Path<i64>, req: Request) -> Res<Response> {
    store::authorize(&u, id)?;
    let d = store::get_doc_brief(id)?;
    let thumb = store::doc_dir(id).join("thumb.jpg");
    if !thumb.is_file() {
        let _permit = RENDERS.acquire().await.map_err(bad)?;
        tokio::task::spawn_blocking(move || ingest::ensure_thumb(id, d.has_archive, &d.mime)).await.map_err(bad)?.ok_or(store::Error::NotFound)?;
    }
    let mut res = serve_file(thumb, "image/jpeg", None, req).await?;
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    Ok(res)
}

/// Page rendered as an image (JPEG, 1400 px wide)
///
/// This is what the preview uses: it works the same in every browser, including on phones.
/// Available for PDFs and images (any document with an original or archive PDF).
#[utoipa::path(get, path = "/api/documents/{id}/pages/{page}", tag = "documents", params(("id" = i64, Path,), ("page" = u32, Path, description = "Page number, starting at 1")),
    responses((status = 200, description = "JPEG image", content_type = "image/jpeg", body = Vec<u8>), (status = 404, body = ErrorBody)))]
async fn document_page(u: AuthUser, Path((id, page)): Path<(i64, u32)>) -> Res<Response> {
    store::authorize(&u, id)?;
    let d = store::get_doc_brief(id)?;
    let file = store::doc_dir(id).join(if d.has_archive { "archive.pdf" } else { "original" });
    if (!d.has_archive && d.mime != "application/pdf") || page < 1 || i64::from(page) > d.pages.max(1) {
        return Err(store::Error::NotFound.into());
    }
    if !file.is_file() {
        return Err(ApiError(StatusCode::NOT_FOUND, "this document's file is not on disk; check Settings → Health".into()));
    }
    let _permit = RENDERS.acquire().await.map_err(bad)?;
    let jpeg = tokio::task::spawn_blocking(move || ingest::render_page(&file, page, 1400)).await.map_err(bad)??;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=604800")], jpeg).into_response())
}

/// Classify and summarize with AI
///
/// Sends the text to the configured LLM (`PAPIRO_LLM_*`) and applies the suggested title, summary, correspondent, type,
/// tags and date. Responds `501` if no LLM is configured.
#[utoipa::path(post, path = "/api/documents/{id}/ai", tag = "documents", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 501, body = ErrorBody), (status = 502, body = ErrorBody)))]
async fn document_ai(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Document>> {
    if cfg().llm.is_none() {
        return Err(ApiError(StatusCode::NOT_IMPLEMENTED, "no LLM is configured (PAPIRO_LLM_API_KEY)".into()));
    }
    store::authorize(&u, id)?;
    tokio::task::spawn_blocking(move || crate::llm::enrich(id))
        .await
        .map_err(bad)?
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    store::audit(Some(&u), "document classified with AI", &format!("document {id}"), "");
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, ToSchema)]
struct NoteInput {
    text: String,
}

/// Add a note
#[utoipa::path(post, path = "/api/documents/{id}/notes", tag = "documents", params(("id" = i64, Path,)), request_body = NoteInput,
    responses((status = 201, body = Note), (status = 404, body = ErrorBody)))]
async fn add_note(Editor(u): Editor, Path(id): Path<i64>, Json(n): Json<NoteInput>) -> Res<(StatusCode, Json<Note>)> {
    store::authorize(&u, id)?;
    Ok((StatusCode::CREATED, Json(store::add_note(id, n.text.trim())?)))
}

/// Delete a note
#[utoipa::path(delete, path = "/api/notes/{id}", tag = "documents", params(("id" = i64, Path,)), responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_note(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    store::authorize(&u, store::note_doc(id)?)?;
    store::delete_note(id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── etiquetas, corresponsales, tipos ─────────────────────────

macro_rules! label_routes {
    ($kind:literal, $path:literal, $path_id:literal, $what:literal, $one:literal, $list:ident, $create:ident, $update:ident, $delete:ident) => {
        #[doc = concat!("List ", $what, " (with document counts)")]
        #[utoipa::path(get, path = $path, tag = "labels", responses((status = 200, body = Vec<Label>)))]
        async fn $list(_u: AuthUser) -> Res<Json<Vec<Label>>> {
            Ok(Json(store::list_labels($kind)?))
        }

        #[doc = concat!("Create a ", $one)]
        ///
        /// `match` + `algo` define automatic assignment on ingest: `any` (any of the words),
        /// `all` (all of them), `literal` (exact phrase), `regex`; `none` turns it off.
        #[utoipa::path(post, path = $path, tag = "labels", request_body = LabelInput,
            responses((status = 201, body = Label), (status = 409, body = ErrorBody)))]
        async fn $create(_u: Editor, Json(i): Json<LabelInput>) -> Res<(StatusCode, Json<Label>)> {
            Ok((StatusCode::CREATED, Json(store::create_label($kind, &i)?)))
        }

        #[doc = concat!("Edit a ", $one)]
        #[utoipa::path(patch, path = $path_id, tag = "labels", params(("id" = i64, Path,)), request_body = LabelInput,
            responses((status = 200, body = Label), (status = 404, body = ErrorBody)))]
        async fn $update(_u: Editor, Path(id): Path<i64>, Json(i): Json<LabelInput>) -> Res<Json<Label>> {
            Ok(Json(store::update_label($kind, id, &i)?))
        }

        #[doc = concat!("Delete a ", $one, " (its documents are kept)")]
        #[utoipa::path(delete, path = $path_id, tag = "labels", params(("id" = i64, Path,)),
            responses((status = 204), (status = 404, body = ErrorBody)))]
        async fn $delete(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
            let name = store::get_label($kind, id)?.name;
            store::delete_label($kind, id)?;
            store::audit(Some(&u), concat!($one, " deleted"), &name, "");
            Ok(StatusCode::NO_CONTENT)
        }
    };
}
label_routes!("tag", "/api/tags", "/api/tags/{id}", "tags", "tag", list_tags, create_tag, update_tag, delete_tag);
label_routes!("correspondent", "/api/correspondents", "/api/correspondents/{id}", "correspondents", "correspondent", list_correspondents, create_correspondent, update_correspondent, delete_correspondent);
label_routes!("doctype", "/api/doctypes", "/api/doctypes/{id}", "document types", "document type", list_doctypes, create_doctype, update_doctype, delete_doctype);

// ───────────────────────── carpetas ─────────────────────────

fn folder(u: &AuthUser, id: i64) -> Res<Json<Folder>> {
    Ok(Json(store::list_folders(u)?.into_iter().find(|f| f.id == id).ok_or(store::Error::NotFound)?))
}

/// List folders
///
/// Every folder, sorted by path. `document_count` counts what is in the folder and in its subfolders.
#[utoipa::path(get, path = "/api/folders", tag = "folders", responses((status = 200, body = Vec<Folder>)))]
async fn list_folders(u: AuthUser) -> Res<Json<Vec<Folder>>> {
    Ok(Json(store::list_folders(&u)?))
}

/// Create a folder
#[utoipa::path(post, path = "/api/folders", tag = "folders", request_body = FolderInput,
    responses((status = 201, body = Folder), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn create_folder(Editor(u): Editor, Json(i): Json<FolderInput>) -> Res<(StatusCode, Json<Folder>)> {
    if !store::folder_allowed(&u, i.parent_id.flatten()) {
        return Err(store::scope_error().into());
    }
    let f = folder(&u, store::create_folder(&i)?)?;
    store::audit(Some(&u), "folder created", &f.path, "");
    store::notify(json!({"type": "folders"}));
    Ok((StatusCode::CREATED, f))
}

/// Rename or move a folder
#[utoipa::path(patch, path = "/api/folders/{id}", tag = "folders", params(("id" = i64, Path,)), request_body = FolderInput,
    responses((status = 200, body = Folder), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn update_folder(Editor(u): Editor, Path(id): Path<i64>, Json(i): Json<FolderInput>) -> Res<Json<Folder>> {
    scoped_folder(&u, id)?;
    if i.parent_id.is_some_and(|p| !store::folder_allowed(&u, p)) {
        return Err(store::scope_error().into());
    }
    store::update_folder(id, &i)?;
    store::notify(json!({"type": "folders"}));
    folder(&u, id)
}

/// Delete a folder
///
/// Deletes the folder and its subfolders. Documents are not deleted: they move to the parent folder
/// (or end up with no folder if it was at the root).
#[utoipa::path(delete, path = "/api/folders/{id}", tag = "folders", params(("id" = i64, Path,)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_folder(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    scoped_folder(&u, id)?;
    let path = folder(&u, id)?.0.path;
    let moved = store::delete_folder(id)?;
    store::audit(Some(&u), "folder deleted", &path, &format!("{moved} document(s) moved to the parent folder"));
    store::notify(json!({"type": "folders"}));
    Ok(StatusCode::NO_CONTENT)
}

/// A limited token can rename, move or delete the subfolders it reaches, but not the folders it was given.
fn scoped_folder(u: &AuthUser, id: i64) -> Res<()> {
    match &u.folders {
        Some(roots) if roots.contains(&id) || !store::folder_allowed(u, Some(id)) => Err(store::scope_error().into()),
        _ => Ok(()),
    }
}

// ───────────────────────── cola, eventos, estadísticas ─────────────────────────

/// Everyone sees their own uploads and whatever came in by folder or mail; administrators see everything.
fn job_visible(u: &AuthUser, j: &Job) -> bool {
    u.role == "admin" || j.user_id.is_none() || j.user_id == Some(u.id)
}

/// Last 100 ingest jobs
#[utoipa::path(get, path = "/api/jobs", tag = "queue", responses((status = 200, body = Vec<Job>)))]
async fn list_jobs(u: AuthUser) -> Res<Json<Vec<Job>>> {
    Ok(Json(ingest::jobs("id > ?", 0)?.into_iter().filter(|j| job_visible(&u, j)).collect()))
}

fn own_job(u: &AuthUser, id: i64) -> Res<Job> {
    Some(ingest::job(id)?).filter(|j| job_visible(u, j)).ok_or_else(|| store::Error::NotFound.into())
}

/// Status of an ingest job
#[utoipa::path(get, path = "/api/jobs/{id}", tag = "queue", params(("id" = i64, Path,)),
    responses((status = 200, body = Job), (status = 404, body = ErrorBody)))]
async fn get_job(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Job>> {
    Ok(Json(own_job(&u, id)?))
}

/// Retry a failed job
///
/// The file of a failed job is kept (`can_retry` field); this queues it again.
#[utoipa::path(post, path = "/api/jobs/{id}/retry", tag = "queue", params(("id" = i64, Path,)),
    responses((status = 200, body = Job), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn retry_job(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Job>> {
    own_job(&u, id)?;
    Ok(Json(ingest::retry(id)?))
}

/// Dismiss a finished job
///
/// If it had failed, the file that was kept is deleted too.
#[utoipa::path(delete, path = "/api/jobs/{id}", tag = "queue", params(("id" = i64, Path,)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn dismiss_job(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    let j = own_job(&u, id)?;
    ingest::dismiss(id)?;
    if j.status == "failed" {
        store::audit(Some(&u), "failed job dismissed", &j.filename, j.error.as_deref().unwrap_or(""));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Live events (Server-Sent Events)
///
/// Each message is JSON: `{"type":"job","job":{…}}` or `{"type":"document.created|updated|deleted|purged","id":N}`.
#[utoipa::path(get, path = "/api/events", tag = "queue", responses((status = 200, description = "SSE stream", content_type = "text/event-stream", body = String)))]
async fn events(u: AuthUser) -> impl IntoResponse {
    let stream = tokio_stream::wrappers::BroadcastStream::new(store::bus().subscribe()).filter_map(move |m| {
        let data = m.ok()?;
        // Los trabajos de otros usuarios (y sus nombres de archivo) no se anuncian a los demás.
        let owner = serde_json::from_str::<Value>(&data).ok().and_then(|v| v["user_id"].as_i64());
        (u.role == "admin" || owner.is_none() || owner == Some(u.id)).then(|| Ok::<_, std::convert::Infallible>(sse::Event::default().data(data)))
    });
    sse::Sse::new(stream).keep_alive(sse::KeepAlive::default())
}

/// Archive and queue totals
#[utoipa::path(get, path = "/api/stats", tag = "queue", responses((status = 200, body = Stats)))]
async fn get_stats(u: AuthUser) -> Res<Json<Stats>> {
    Ok(Json(store::stats(&u)?))
}

// ───────────────────────── sistema ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Status {
    version: &'static str,
    /// Database schema version
    schema: i64,
    /// Correction of rotated pages is enabled
    ocr_rotate: bool,
    ocr_langs: String,
    queued: i64,
    failed_jobs: i64,
    /// Webhook deliveries still waiting to be sent
    pending_webhooks: i64,
    /// Last integrity check (null if none has run yet)
    last_verify: Option<VerifyReport>,
    /// Last automatic or manual backup (null if there has never been one)
    last_backup: Option<Value>,
    /// Hours between automatic backups (null = disabled: PAPIRO_BACKUP_DIR is not set)
    backup_every_hours: Option<u64>,
}

/// System status (admin)
#[utoipa::path(get, path = "/api/status", tag = "system", responses((status = 200, body = Status)))]
async fn get_status(Admin(u): Admin) -> Res<Json<Status>> {
    let s = store::stats(&u)?;
    Ok(Json(Status {
        version: env!("CARGO_PKG_VERSION"),
        schema: db::schema_version(),
        ocr_rotate: ingest::rotate_on(),
        ocr_langs: cfg().ocr_langs.clone(),
        queued: s.queued,
        failed_jobs: s.failed,
        pending_webhooks: db::with(|c| c.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))).unwrap_or(0),
        last_verify: store::meta_get("verify").and_then(|v| serde_json::from_str(&v).ok()),
        last_backup: store::meta_get("backup").and_then(|v| serde_json::from_str(&v).ok()),
        backup_every_hours: cfg().backup_dir.as_ref().map(|_| cfg().backup_hours),
    }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct VerifyQuery {
    /// `false` to check only existence and size (fast); by default every SHA-256 is recomputed
    deep: Option<bool>,
}

/// Verify integrity now (admin)
///
/// Checks that every document has its files and that the original still matches its SHA-256, as well
/// as the integrity of the database and of the search index.
#[utoipa::path(post, path = "/api/verify", tag = "system", params(VerifyQuery), responses((status = 200, body = VerifyReport)))]
async fn run_verify(Admin(u): Admin, Query(q): Query<VerifyQuery>) -> Res<Json<VerifyReport>> {
    let report = tokio::task::spawn_blocking(move || store::verify(q.deep != Some(false))).await.map_err(bad)?;
    store::audit(Some(&u), "integrity check", &format!("{} documents", report.documents), &format!("{} problems", report.problems.len()));
    Ok(Json(report))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Audit log (admin)
///
/// Who did what and from which IP: logins, uploads, edits, deletions and administration changes.
#[utoipa::path(get, path = "/api/audit", tag = "system", params(PageQuery), responses((status = 200, body = Vec<AuditEntry>)))]
async fn list_audit(_a: Admin, Query(q): Query<PageQuery>) -> Res<Json<Vec<AuditEntry>>> {
    Ok(Json(store::list_audit(q.limit.unwrap_or(100), q.offset.unwrap_or(0))?))
}

// ───────────────────────── webhooks ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Webhook {
    id: i64,
    url: String,
    /// `*` or a comma-separated list: document.created, document.updated, document.deleted, document.purged
    events: String,
    /// Whether it has a signing secret (the secret itself is never returned)
    signed: bool,
    /// HTTP status code (or error) of the last delivery
    last_status: Option<String>,
    last_at: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct WebhookInput {
    url: String,
    events: Option<String>,
    /// If set, every delivery carries `X-Papiro-Signature: sha256=<HMAC-SHA256 of the body>`
    secret: Option<String>,
}

fn webhooks() -> store::Result<Vec<Webhook>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, url, events, secret != '', last_status, last_at FROM webhooks ORDER BY id")?
            .query_map([], |r| Ok(Webhook { id: r.get(0)?, url: r.get(1)?, events: r.get(2)?, signed: r.get(3)?, last_status: r.get(4)?, last_at: r.get(5)? }))?
            .collect()
    })?)
}

/// List webhooks (admin)
#[utoipa::path(get, path = "/api/webhooks", tag = "webhooks", responses((status = 200, body = Vec<Webhook>)))]
async fn list_webhooks(_a: Admin) -> Res<Json<Vec<Webhook>>> {
    Ok(Json(webhooks()?))
}

/// Create a webhook (admin)
///
/// On every event the URL receives a `POST` with `{"event", "document_id", "document"}`. Deliveries are
/// stored in a persistent queue and retried up to 6 times. Internal addresses are not allowed.
#[utoipa::path(post, path = "/api/webhooks", tag = "webhooks", request_body = WebhookInput,
    responses((status = 201, body = Webhook), (status = 400, body = ErrorBody)))]
async fn create_webhook(Admin(u): Admin, Json(w): Json<WebhookInput>) -> Res<(StatusCode, Json<Webhook>)> {
    let url = w.url.clone();
    tokio::task::spawn_blocking(move || store::webhook_target_ok(&url)).await.map_err(bad)?.map_err(bad)?;
    let id = db::with(|c| {
        c.execute("INSERT INTO webhooks(url, events, secret) VALUES(?,?,?)", params![w.url, w.events.unwrap_or("*".into()).replace(' ', ""), w.secret.unwrap_or_default()])?;
        Ok(c.last_insert_rowid())
    })
    .map_err(store::Error::from)?;
    let hook = webhooks()?.into_iter().find(|h| h.id == id).ok_or(store::Error::NotFound)?;
    store::audit(Some(&u), "webhook created", &hook.url, &hook.events);
    Ok((StatusCode::CREATED, Json(hook)))
}

/// Delete a webhook (admin)
#[utoipa::path(delete, path = "/api/webhooks/{id}", tag = "webhooks", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_webhook(Admin(u): Admin, Path(id): Path<i64>) -> Res<StatusCode> {
    db::with(|c| c.execute("DELETE FROM webhooks WHERE id=?", [id])).map_err(store::Error::from)?;
    store::audit(Some(&u), "webhook deleted", &format!("webhook {id}"), "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
struct WebhookTest {
    /// HTTP status code received, or a description of the error
    status: String,
}

/// Send a test event (`ping`) (admin)
#[utoipa::path(post, path = "/api/webhooks/{id}/test", tag = "webhooks", params(("id" = i64, Path,)),
    responses((status = 200, body = WebhookTest), (status = 404, body = ErrorBody)))]
async fn test_webhook(_a: Admin, Path(id): Path<i64>) -> Res<Json<WebhookTest>> {
    let (url, secret): (String, String) =
        db::with(|c| c.query_row("SELECT url, secret FROM webhooks WHERE id=?", [id], |r| Ok((r.get(0)?, r.get(1)?)))).map_err(store::Error::from)?;
    let body = json!({"event": "ping", "document_id": null, "document": null}).to_string();
    let status = tokio::task::spawn_blocking(move || store::deliver(id, &url, &secret, &body)).await.map_err(bad)?;
    Ok(Json(WebhookTest { status }))
}

// ───────────────────────── cuenta ─────────────────────────

#[derive(Serialize, ToSchema)]
struct AuthStatus {
    /// There are no users yet: the administrator must be created with `POST /api/auth/setup`
    setup_required: bool,
    user: Option<AuthUser>,
    /// An LLM is configured (enables "Classify with AI")
    llm: bool,
}

#[derive(Deserialize, ToSchema)]
struct Credentials {
    username: String,
    password: String,
}

fn is_https(h: &HeaderMap) -> bool {
    h.get("x-forwarded-proto").is_some_and(|v| v == "https")
}

/// Session status
#[utoipa::path(get, path = "/api/auth/status", tag = "account", security(()), responses((status = 200, body = AuthStatus)))]
async fn auth_status(req: Request) -> Json<AuthStatus> {
    use axum::extract::FromRequestParts;
    let user = AuthUser::from_request_parts(&mut req.into_parts().0, &()).await.ok();
    Json(AuthStatus { setup_required: auth::user_count() == 0, llm: user.is_some() && cfg().llm.is_some(), user })
}

fn session_response(user: AuthUser, headers: &HeaderMap) -> Res<Response> {
    let cookie = auth::new_session(user.id, is_https(headers))?;
    Ok(([(header::SET_COOKIE, cookie)], Json(user)).into_response())
}

fn attempt(parts: &Parts) -> Res<String> {
    let ip = auth::client_ip(parts);
    if auth::allow_attempt(&ip) {
        Ok(ip)
    } else {
        Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "too many attempts from your connection; wait a few minutes".into()))
    }
}

/// Create the initial administrator account
///
/// Only works while no user exists.
#[utoipa::path(post, path = "/api/auth/setup", tag = "account", security(()), request_body = Credentials,
    responses((status = 200, body = AuthUser), (status = 409, body = ErrorBody)))]
async fn auth_setup(parts: Parts, Json(c): Json<Credentials>) -> Res<Response> {
    attempt(&parts)?;
    if auth::user_count() > 0 {
        return Err(ApiError(StatusCode::CONFLICT, "this installation already has users".into()));
    }
    let user = hashing(move || auth::create_user(&c.username, &c.password, "admin")).await??;
    store::audit(Some(&user), "administrator account created", &user.username, "");
    session_response(user, &parts.headers)
}

/// Log in (cookie)
///
/// Limit: 10 attempts every 5 minutes per IP address (`429`). The limit is per connection, not per
/// user: nobody can lock someone else out by getting their password wrong.
#[utoipa::path(post, path = "/api/auth/login", tag = "account", security(()), request_body = Credentials,
    responses((status = 200, body = AuthUser), (status = 401, body = ErrorBody), (status = 429, body = ErrorBody), (status = 503, body = ErrorBody)))]
async fn auth_login(parts: Parts, Json(c): Json<Credentials>) -> Res<Response> {
    let ip = attempt(&parts)?;
    let user = hashing(move || auth::login(&c.username, &c.password, &ip)).await??;
    session_response(user, &parts.headers)
}

/// Log out
#[utoipa::path(post, path = "/api/auth/logout", tag = "account", responses((status = 204)))]
async fn auth_logout(_u: AuthUser, req: Request) -> Res<Response> {
    let (parts, _) = req.into_parts();
    if let Some(t) = auth::cookie_token(&parts) {
        db::with(|c| c.execute("DELETE FROM sessions WHERE token_hash=?", [auth::sha(t)])).map_err(store::Error::from)?;
    }
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, format!("{}=; Path=/; Max-Age=0", auth::COOKIE))]).into_response())
}

#[derive(Deserialize, ToSchema)]
struct PasswordChange {
    current: String,
    new: String,
}

/// Change your own password (signs out your other sessions)
#[utoipa::path(post, path = "/api/auth/password", tag = "account", request_body = PasswordChange,
    responses((status = 204), (status = 401, body = ErrorBody)))]
async fn auth_password(u: AuthUser, parts: Parts, Json(p): Json<PasswordChange>) -> Res<StatusCode> {
    if u.read_only || u.limited() {
        return Err(forbidden("a read-only token cannot change the password"));
    }
    let ip = attempt(&parts)?;
    let who = u.clone();
    hashing(move || {
        auth::login(&who.username, &p.current, &ip)?;
        Ok::<_, ApiError>(auth::set_password(&who.username, &p.new)?)
    })
    .await??;
    store::audit(Some(&u), "password changed", &u.username, "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
struct TokenInfo {
    id: i64,
    name: String,
    created: String,
    last_used: Option<String>,
    read_only: bool,
    /// Expiry date (null = never expires)
    expires: Option<String>,
    /// Folders it is limited to, with their subfolders (null = the whole archive)
    folders: Option<Vec<i64>>,
    /// Only returned once, when it is created
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct TokenInput {
    /// What it is used for (e.g. "n8n", "Claude MCP")
    name: String,
    /// The token can only read (search, view, download)
    #[serde(default)]
    read_only: bool,
    /// Days until it expires (omit = never expires)
    expires_days: Option<i64>,
    /// Limit the token to these folders and their subfolders (omit = the whole archive)
    folders: Option<Vec<i64>>,
}

fn tokens(user_id: i64) -> store::Result<Vec<TokenInfo>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, name, created, last_used, read_only, expires, folders FROM api_tokens WHERE user_id=? AND grant_id IS NULL ORDER BY id")?
            .query_map([user_id], |r| {
                Ok(TokenInfo {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created: r.get(2)?,
                    last_used: r.get(3)?,
                    read_only: r.get(4)?,
                    expires: r.get(5)?,
                    folders: auth::parse_folders(r.get(6)?),
                    token: None,
                })
            })?
            .collect()
    })?)
}

/// List your own API tokens
#[utoipa::path(get, path = "/api/tokens", tag = "account", responses((status = 200, body = Vec<TokenInfo>)))]
async fn list_tokens(u: AuthUser) -> Res<Json<Vec<TokenInfo>>> {
    Ok(Json(tokens(u.id)?))
}

/// Create an API token
///
/// The token value (`ppr_…`) only appears in this response; the server stores only its hash.
/// A viewer can only create read-only tokens.
#[utoipa::path(post, path = "/api/tokens", tag = "account", request_body = TokenInput, responses((status = 201, body = TokenInfo), (status = 403, body = ErrorBody)))]
async fn create_token(u: AuthUser, Json(t): Json<TokenInput>) -> Res<(StatusCode, Json<TokenInfo>)> {
    if u.read_only || u.limited() {
        return Err(forbidden("a read-only token cannot create other tokens"));
    }
    let folders = t.folders.map(|f| f.into_iter().filter(|id| store::folder_exists(*id)).collect::<Vec<_>>());
    if folders.as_ref().is_some_and(|f| f.is_empty()) {
        return Err(bad("choose at least one folder, or give the token access to the whole archive"));
    }
    let (id, token) = auth::new_api_token(u.id, t.name.trim(), t.read_only || u.role == "viewer", t.expires_days, folders.as_deref())?;
    store::audit(Some(&u), "token created", t.name.trim(), if t.read_only { "read-only" } else { "" });
    let mut info = tokens(u.id)?.into_iter().find(|x| x.id == id).ok_or(store::Error::NotFound)?;
    info.token = Some(token);
    Ok((StatusCode::CREATED, Json(info)))
}

/// Revoke an API token
#[utoipa::path(delete, path = "/api/tokens/{id}", tag = "account", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_token(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    if u.read_only || u.limited() {
        return Err(forbidden("a read-only token cannot revoke tokens"));
    }
    db::with(|c| c.execute("DELETE FROM api_tokens WHERE id=? AND user_id=?", [id, u.id])).map_err(store::Error::from)?;
    store::audit(Some(&u), "token revoked", &format!("token {id}"), "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
struct UserInput {
    username: String,
    password: String,
    /// admin | editor | viewer (editor by default)
    role: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct UserPatch {
    /// admin | editor | viewer
    role: Option<String>,
    /// New password (signs out that user's sessions)
    password: Option<String>,
}

fn users() -> store::Result<Vec<AuthUser>> {
    Ok(db::with(|c| c.prepare_cached("SELECT id, username, role FROM users ORDER BY id")?.query_map([], |r| Ok(AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())?)
}

/// List users (admin)
#[utoipa::path(get, path = "/api/users", tag = "account", responses((status = 200, body = Vec<AuthUser>)))]
async fn list_users(_a: Admin) -> Res<Json<Vec<AuthUser>>> {
    Ok(Json(users()?))
}

/// Create a user (admin)
#[utoipa::path(post, path = "/api/users", tag = "account", request_body = UserInput,
    responses((status = 201, body = AuthUser), (status = 409, body = ErrorBody)))]
async fn create_user(Admin(a): Admin, Json(i): Json<UserInput>) -> Res<(StatusCode, Json<AuthUser>)> {
    let user = hashing(move || auth::create_user(&i.username, &i.password, i.role.as_deref().unwrap_or("editor"))).await??;
    store::audit(Some(&a), "user created", &user.username, &user.role);
    Ok((StatusCode::CREATED, Json(user)))
}

/// There must always be at least one administrator left.
fn keep_an_admin(target: &AuthUser) -> Res<()> {
    if target.role == "admin" && users()?.iter().filter(|u| u.role == "admin").count() <= 1 {
        return Err(bad("this is the only administrator; make someone else an administrator first"));
    }
    Ok(())
}

/// Change a user's role or password (admin)
#[utoipa::path(patch, path = "/api/users/{id}", tag = "account", params(("id" = i64, Path,)), request_body = UserPatch,
    responses((status = 200, body = AuthUser), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update_user(Admin(a): Admin, Path(id): Path<i64>, Json(p): Json<UserPatch>) -> Res<Json<AuthUser>> {
    let target = users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?;
    if let Some(role) = p.role.as_deref().filter(|r| *r != target.role) {
        if !auth::ROLES.contains(&role) {
            return Err(bad("invalid role (admin, editor or viewer)"));
        }
        keep_an_admin(&target)?;
        db::with(|c| c.execute("UPDATE users SET role=?, is_admin=? WHERE id=?", params![role, role == "admin", id])).map_err(store::Error::from)?;
        store::audit(Some(&a), "role changed", &target.username, &format!("{} → {role}", target.role));
    }
    if let Some(pw) = p.password {
        let name = target.username.clone();
        hashing(move || auth::set_password(&name, &pw)).await??;
        store::audit(Some(&a), "password reset", &target.username, "");
    }
    Ok(Json(users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?))
}

/// Delete a user (admin)
///
/// Their documents are kept (left without an owner; private ones become visible to administrators only).
#[utoipa::path(delete, path = "/api/users/{id}", tag = "account", params(("id" = i64, Path,)),
    responses((status = 204), (status = 400, body = ErrorBody)))]
async fn delete_user(Admin(a): Admin, Path(id): Path<i64>) -> Res<StatusCode> {
    if a.id == id {
        return Err(bad("you cannot delete your own user"));
    }
    let target = users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?;
    keep_an_admin(&target)?;
    db::with(|c| c.execute("DELETE FROM users WHERE id=?", [id])).map_err(store::Error::from)?;
    store::audit(Some(&a), "user deleted", &target.username, "");
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── notificaciones push ─────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PushQuery {
    /// Endpoint of this browser's subscription, to know whether it is already registered
    endpoint: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct PushStatus {
    /// VAPID public key (base64url) for `pushManager.subscribe`
    public_key: String,
    /// Devices of yours that receive notifications
    devices: i64,
    /// This device is subscribed
    subscribed: bool,
    /// Folders this device is limited to (null = everything)
    folders: Option<Vec<i64>>,
    /// Result of the last delivery to this device (HTTP status of the push service)
    last_status: Option<String>,
    /// This device also gets the daily due-date reminder
    reminders: bool,
    /// A calendar feed exists for your due dates
    calendar: bool,
}

/// Notification status
#[utoipa::path(get, path = "/api/push", tag = "notifications", params(PushQuery), responses((status = 200, body = PushStatus)))]
async fn push_status(u: AuthUser, Query(q): Query<PushQuery>) -> Res<Json<PushStatus>> {
    let this = q.endpoint.as_deref().and_then(|e| crate::push::find(u.id, e));
    Ok(Json(PushStatus {
        public_key: crate::push::public_key(),
        devices: crate::push::count(u.id),
        subscribed: this.is_some(),
        last_status: this.as_ref().map(|t| t.1.clone()).filter(|s| !s.is_empty()),
        reminders: this.as_ref().is_none_or(|t| t.2),
        folders: this.and_then(|t| t.0),
        calendar: db::with(|c| c.query_row("SELECT calendar_hash IS NOT NULL FROM users WHERE id=?", [u.id], |r| r.get(0))).unwrap_or(false),
    }))
}

#[derive(Deserialize, ToSchema)]
struct PushKeys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize, ToSchema)]
struct PushSubscribe {
    /// `PushSubscription.endpoint`
    endpoint: String,
    keys: PushKeys,
    /// Only notify about documents in these folders and their subfolders (omit = everything)
    folders: Option<Vec<i64>>,
    /// Language of the notifications (`en` or `es`)
    lang: Option<String>,
    /// Daily reminder of documents due today, tomorrow, in 7 and in 30 days (default true)
    reminders: Option<bool>,
}

/// Subscribe this device to notifications
///
/// Sends a notification when a new document arrives (upload from another device or user, email, watched folder, API or MCP)
/// that the user can see. Calling it again with the same endpoint updates the folders.
#[utoipa::path(post, path = "/api/push/subscriptions", tag = "notifications", request_body = PushSubscribe, responses((status = 204), (status = 400, body = ErrorBody)))]
async fn push_subscribe(u: AuthUser, Json(p): Json<PushSubscribe>) -> Res<StatusCode> {
    if u.limited() {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    let folders = p.folders.filter(|f| !f.is_empty());
    let sub = crate::push::Subscription { endpoint: p.endpoint, p256dh: p.keys.p256dh, auth: p.keys.auth, folders, lang: p.lang.unwrap_or_else(|| "en".into()), reminders: p.reminders.unwrap_or(true) };
    tokio::task::spawn_blocking(move || crate::push::subscribe(u.id, &sub)).await.map_err(bad)??;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
struct PushEndpoint {
    endpoint: String,
}

/// Stop notifications on this device
#[utoipa::path(post, path = "/api/push/unsubscribe", tag = "notifications", request_body = PushEndpoint, responses((status = 204)))]
async fn push_unsubscribe(u: AuthUser, Json(p): Json<PushEndpoint>) -> Res<StatusCode> {
    crate::push::unsubscribe(u.id, &p.endpoint)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
struct PushTest {
    /// Devices the test notification was sent to
    devices: usize,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PushTestQuery {
    /// Send today's due-date reminder now instead of a test message
    reminders: Option<bool>,
}

/// Send a test notification to all your devices
#[utoipa::path(post, path = "/api/push/test", tag = "notifications", params(PushTestQuery), responses((status = 200, body = PushTest)))]
async fn push_test(u: AuthUser, Query(q): Query<PushTestQuery>) -> Res<Json<PushTest>> {
    if q.reminders == Some(true) {
        let today: String = db::with(|c| c.query_row("SELECT date('now','localtime')", [], |r| r.get(0))).map_err(store::Error::from)?;
        return Ok(Json(PushTest { devices: tokio::task::spawn_blocking(move || crate::push::send_reminders(&today, Some(u.id))).await.map_err(bad)? }));
    }
    Ok(Json(PushTest { devices: crate::push::test(u.id) }))
}

// ───────────────────────── vistas guardadas (carpetas inteligentes) ─────────────────────────

#[derive(Serialize, ToSchema)]
struct View {
    id: i64,
    name: String,
    /// Search parameters, same as `GET /api/documents` (e.g. `q=electricity&tags=3`)
    query: String,
    /// The interface's own route for this view
    route: String,
    /// Documents that match right now
    count: i64,
}

#[derive(Deserialize, ToSchema)]
struct ViewInput {
    name: String,
    query: String,
    #[serde(default)]
    route: String,
}

fn view_count(u: &AuthUser, query: &str) -> i64 {
    let uri: axum::http::Uri = match format!("/?{query}").parse() {
        Ok(u) => u,
        Err(_) => return 0,
    };
    Query::<SearchParams>::try_from_uri(&uri).ok().and_then(|Query(mut p)| {
        p.limit = Some(1);
        store::search(&p, u).ok()
    }).map_or(0, |r| r.total)
}

/// Your saved views, with how many documents each one matches now
#[utoipa::path(get, path = "/api/views", tag = "sharing", responses((status = 200, body = Vec<View>)))]
async fn list_views(u: AuthUser) -> Res<Json<Vec<View>>> {
    let rows: Vec<(i64, String, String, String)> = db::with(|c| {
        c.prepare_cached("SELECT id, name, query, route FROM views WHERE user_id=? ORDER BY name COLLATE NOCASE")?
            .query_map([u.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect()
    }).map_err(store::Error::from)?;
    Ok(Json(rows.into_iter().map(|(id, name, query, route)| View { count: view_count(&u, &query), id, name, query, route }).collect()))
}

/// Save a search as a view
#[utoipa::path(post, path = "/api/views", tag = "sharing", request_body = ViewInput, responses((status = 201, body = View), (status = 400, body = ErrorBody)))]
async fn create_view(u: AuthUser, Json(v): Json<ViewInput>) -> Res<(StatusCode, Json<View>)> {
    let name: String = v.name.trim().chars().take(80).collect();
    if name.is_empty() || v.query.len() > 2000 || v.route.len() > 2000 {
        return Err(bad("the view needs a name"));
    }
    let id = db::with(|c| {
        c.execute("INSERT INTO views(user_id, name, query, route) VALUES(?,?,?,?)", params![u.id, name, v.query, v.route])?;
        Ok(c.last_insert_rowid())
    }).map_err(store::Error::from)?;
    Ok((StatusCode::CREATED, Json(View { count: view_count(&u, &v.query), id, name, query: v.query, route: v.route })))
}

/// Delete a saved view
#[utoipa::path(delete, path = "/api/views/{id}", tag = "sharing", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_view(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    db::with(|c| c.execute("DELETE FROM views WHERE id=? AND user_id=?", [id, u.id])).map_err(store::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── enlaces para compartir ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Share {
    id: i64,
    created: String,
    expires: String,
    /// Times the link was opened
    views: i64,
    /// Only returned once, when it is created
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct ShareInput {
    /// Days until the link stops working (1 to 90, default 7)
    days: Option<i64>,
}

fn shares_of(doc: i64) -> store::Result<Vec<Share>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, created, expires, views FROM shares WHERE doc_id=? AND expires > datetime('now') ORDER BY id DESC")?
            .query_map([doc], |r| Ok(Share { id: r.get(0)?, created: r.get(1)?, expires: r.get(2)?, views: r.get(3)?, url: None }))?
            .collect()
    })?)
}

/// Active share links of a document
#[utoipa::path(get, path = "/api/documents/{id}/shares", tag = "sharing", params(("id" = i64, Path,)), responses((status = 200, body = Vec<Share>)))]
async fn list_shares(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Vec<Share>>> {
    store::authorize(&u, id)?;
    Ok(Json(shares_of(id)?))
}

/// Create a temporary share link
///
/// Anyone with the link can view and download this one document, without an account, until it expires.
/// The link is only shown in this response; the server stores a hash of it.
#[utoipa::path(post, path = "/api/documents/{id}/shares", tag = "sharing", params(("id" = i64, Path,)), request_body = ShareInput,
    responses((status = 201, body = Share), (status = 403, body = ErrorBody)))]
async fn create_share(Editor(u): Editor, headers: HeaderMap, Path(id): Path<i64>, Json(i): Json<ShareInput>) -> Res<(StatusCode, Json<Share>)> {
    if u.limited() {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    store::authorize(&u, id)?;
    let days = i.days.unwrap_or(7).clamp(1, 90);
    let token = random_hex(24);
    let sid = db::with(|c| {
        c.execute("INSERT INTO shares(doc_id, user_id, token_hash, expires) VALUES(?,?,?, datetime('now', '+' || ? || ' days'))", params![id, u.id, auth::sha(&token), days])?;
        Ok(c.last_insert_rowid())
    }).map_err(store::Error::from)?;
    store::audit(Some(&u), "share link created", &format!("document {id}"), &format!("{days} days"));
    let mut s = shares_of(id)?.into_iter().find(|s| s.id == sid).ok_or(store::Error::NotFound)?;
    s.url = Some(format!("{}/s/{token}", crate::oauth::origin(&headers)));
    Ok((StatusCode::CREATED, Json(s)))
}

/// Revoke a share link
#[utoipa::path(delete, path = "/api/shares/{id}", tag = "sharing", params(("id" = i64, Path,)), responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_share(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    let doc: i64 = db::with(|c| c.query_row("SELECT doc_id FROM shares WHERE id=?", [id], |r| r.get(0))).map_err(store::Error::from)?;
    store::authorize(&u, doc)?;
    db::with(|c| c.execute("DELETE FROM shares WHERE id=?", [id])).map_err(store::Error::from)?;
    store::audit(Some(&u), "share link revoked", &format!("document {doc}"), "");
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── calendario ─────────────────────────

#[derive(Serialize, ToSchema)]
struct CalendarLink {
    /// Subscribe to this URL from your calendar app (it is shown only once; creating a new one revokes the old)
    url: String,
}

/// Create the due-date calendar link
#[utoipa::path(post, path = "/api/calendar", tag = "sharing", responses((status = 201, body = CalendarLink)))]
async fn create_calendar(u: AuthUser, headers: HeaderMap) -> Res<(StatusCode, Json<CalendarLink>)> {
    if u.limited() || u.read_only {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    let token = crate::public::new_calendar(u.id)?;
    store::audit(Some(&u), "calendar link created", &u.username, "");
    Ok((StatusCode::CREATED, Json(CalendarLink { url: format!("{}/cal/{token}.ics", crate::oauth::origin(&headers)) })))
}

/// Revoke the due-date calendar link
#[utoipa::path(delete, path = "/api/calendar", tag = "sharing", responses((status = 204)))]
async fn delete_calendar(u: AuthUser) -> Res<StatusCode> {
    if u.limited() || u.read_only {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    db::with(|c| c.execute("UPDATE users SET calendar_hash=NULL WHERE id=?", [u.id])).map_err(store::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── versiones ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Version {
    /// Version number (1 = the first file this document had)
    n: i64,
    original_name: String,
    mime: String,
    size: i64,
    pages: i64,
    /// When this version became the current one
    since: String,
    /// When a newer version replaced it
    replaced: String,
}

/// Previous versions of a document
///
/// The current file is the document itself; this lists the ones it replaced, newest first.
#[utoipa::path(get, path = "/api/documents/{id}/versions", tag = "documents", params(("id" = i64, Path,)), responses((status = 200, body = Vec<Version>)))]
async fn list_versions(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Vec<Version>>> {
    store::authorize(&u, id)?;
    Ok(Json(db::with(|c| {
        c.prepare_cached("SELECT n, original_name, mime, size, pages, since, replaced FROM versions WHERE doc_id=? ORDER BY n DESC")?
            .query_map([id], |r| Ok(Version { n: r.get(0)?, original_name: r.get(1)?, mime: r.get(2)?, size: r.get(3)?, pages: r.get(4)?, since: r.get(5)?, replaced: r.get(6)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()
    }).map_err(store::Error::from)?))
}

/// Upload a new version
///
/// Replaces the document's file (a renewed contract, this year's policy) and keeps everything else: title, folder,
/// tags, notes, links. The previous file stays downloadable from `GET /api/documents/{id}/versions/{n}/file`.
#[utoipa::path(post, path = "/api/documents/{id}/versions", tag = "documents", params(("id" = i64, Path,), UploadQuery),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 202, body = UploadResult), (status = 201, body = UploadResult), (status = 415, body = ErrorBody)))]
async fn upload_version(Editor(u): Editor, Path(id): Path<i64>, Query(q): Query<UploadQuery>, mut mp: Multipart) -> Res<(StatusCode, Json<UploadResult>)> {
    store::authorize(&u, id)?;
    let mut staged: Vec<(std::path::PathBuf, String)> = vec![];
    let received = receive_upload(&mut mp, &mut staged).await;
    if received.is_err() || staged.len() != 1 {
        for (p, _) in &staged {
            let _ = tokio::fs::remove_file(p).await;
        }
        return Err(received.err().unwrap_or_else(|| bad("send exactly one \"file\"")));
    }
    let (path, filename) = &staged[0];
    let mut rx = store::bus().subscribe();
    let job = ingest::enqueue(path, filename, if u.token { "api" } else { "upload" }, &json!({"replace": id}), Some(u.id))?;
    store::audit(Some(&u), "new version uploaded", &format!("document {id}"), filename);
    let mut j = ingest::job(job)?;
    if q.wait != Some(true) {
        return Ok((StatusCode::ACCEPTED, Json(UploadResult { jobs: vec![j], documents: vec![] })));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(cfg().cmd_timeout * 2);
    while (j.status == "queued" || j.status == "running") && tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
        j = ingest::job(job)?;
    }
    let documents = j.doc_id.and_then(|d| store::get_doc_brief(d).ok()).into_iter().collect();
    Ok((StatusCode::CREATED, Json(UploadResult { jobs: vec![j], documents })))
}

/// Download a previous version
#[utoipa::path(get, path = "/api/documents/{id}/versions/{n}/file", tag = "documents", params(("id" = i64, Path,), ("n" = i64, Path,)),
    responses((status = 200, description = "File contents", content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = ErrorBody)))]
async fn version_file(u: AuthUser, Path((id, n)): Path<(i64, i64)>, req: Request) -> Res<Response> {
    store::authorize(&u, id)?;
    let (name, mime): (String, String) =
        db::with(|c| c.query_row("SELECT original_name, mime FROM versions WHERE doc_id=? AND n=?", [id, n], |r| Ok((r.get(0)?, r.get(1)?)))).map_err(store::Error::from)?;
    let stem = std::path::Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = std::path::Path::new(&name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let ascii: String = format!("{stem} (v{n}){ext}").chars().map(|c| if c.is_ascii_alphanumeric() || "._- ()".contains(c) { c } else { '_' }).collect();
    serve_file(store::doc_dir(id).join("versions").join(n.to_string()).join("original"), &mime, Some(format!("attachment; filename=\"{ascii}\"")), req).await
}

// ───────────────────────── recordatorios ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Reminder {
    id: i64,
    doc_id: i64,
    /// Local date and time, `YYYY-MM-DD HH:MM`
    at: String,
    note: String,
    /// Document title (in the upcoming list)
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct ReminderInput {
    /// `YYYY-MM-DD` (at 9:00) or `YYYY-MM-DD HH:MM`, local time
    at: String,
    #[serde(default)]
    note: String,
}

fn reminders(where_sql: &str, args: impl rusqlite::Params) -> store::Result<Vec<Reminder>> {
    Ok(db::with(|c| {
        c.prepare(&format!("SELECT r.id, r.doc_id, r.at, r.note, d.title FROM reminders r JOIN documents d ON d.id=r.doc_id WHERE r.fired IS NULL AND {where_sql} ORDER BY r.at"))?
            .query_map(args, |r| Ok(Reminder { id: r.get(0)?, doc_id: r.get(1)?, at: r.get(2)?, note: r.get(3)?, title: Some(r.get(4)?) }))?
            .collect()
    })?)
}

/// Your pending reminders on a document
#[utoipa::path(get, path = "/api/documents/{id}/reminders", tag = "documents", params(("id" = i64, Path,)), responses((status = 200, body = Vec<Reminder>)))]
async fn list_reminders(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Vec<Reminder>>> {
    store::authorize(&u, id)?;
    Ok(Json(reminders("r.doc_id=?1 AND r.user_id=?2", params![id, u.id])?))
}

/// Remind me about this document
///
/// At that moment the document goes back to the inbox and a notification reaches your devices.
#[utoipa::path(post, path = "/api/documents/{id}/reminders", tag = "documents", params(("id" = i64, Path,)), request_body = ReminderInput,
    responses((status = 201, body = Reminder), (status = 400, body = ErrorBody)))]
async fn create_reminder(u: AuthUser, Path(id): Path<i64>, Json(i): Json<ReminderInput>) -> Res<(StatusCode, Json<Reminder>)> {
    store::authorize(&u, id)?;
    let at = i.at.trim().replace('T', " ");
    let at = if at.len() == 10 { format!("{at} 09:00") } else { at.chars().take(16).collect() };
    let valid = at.len() == 16 && store::valid_date(&at[..10]) && at[11..13].parse::<u8>().is_ok_and(|h| h < 24) && &at[13..14] == ":" && at[14..16].parse::<u8>().is_ok_and(|m| m < 60);
    if !valid {
        return Err(bad("at must be YYYY-MM-DD or YYYY-MM-DD HH:MM"));
    }
    let note: String = i.note.trim().chars().take(300).collect();
    let rid = db::with(|c| {
        c.execute("INSERT INTO reminders(doc_id, user_id, at, note) VALUES(?,?,?,?)", params![id, u.id, at, note])?;
        Ok(c.last_insert_rowid())
    }).map_err(store::Error::from)?;
    Ok((StatusCode::CREATED, Json(Reminder { id: rid, doc_id: id, at, note, title: None })))
}

/// Your upcoming reminders
#[utoipa::path(get, path = "/api/reminders", tag = "documents", responses((status = 200, body = Vec<Reminder>)))]
async fn upcoming_reminders(u: AuthUser) -> Res<Json<Vec<Reminder>>> {
    Ok(Json(reminders("r.user_id=?1 AND d.deleted_at IS NULL", [u.id])?))
}

/// Cancel a reminder
#[utoipa::path(delete, path = "/api/reminders/{id}", tag = "documents", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_reminder(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    db::with(|c| c.execute("DELETE FROM reminders WHERE id=? AND user_id=?", [id, u.id])).map_err(store::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── títulos automáticos ─────────────────────────

/// Title template
#[utoipa::path(get, path = "/api/settings/titles", tag = "documents", responses((status = 200, body = store::Titles)))]
async fn get_titles(_u: AuthUser) -> Json<store::Titles> {
    Json(store::titles())
}

/// Save the title template
#[utoipa::path(put, path = "/api/settings/titles", tag = "documents", request_body = store::Titles, responses((status = 200, body = store::Titles)))]
async fn put_titles(Editor(u): Editor, Json(t): Json<store::Titles>) -> Res<Json<store::Titles>> {
    if u.limited() {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    let t = store::Titles { template: t.template.trim().chars().take(200).collect(), ..t };
    store::set_titles(&t);
    store::audit(Some(&u), "title template changed", &t.template, if t.auto { "automatic" } else { "" });
    Ok(Json(t))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct TitlePreviewQuery {
    template: String,
    lang: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct TitlePreview {
    id: i64,
    before: String,
    after: Option<String>,
}

/// Preview a template on your most recent documents
#[utoipa::path(get, path = "/api/settings/titles/preview", tag = "documents", params(TitlePreviewQuery), responses((status = 200, body = Vec<TitlePreview>)))]
async fn preview_titles(u: AuthUser, Query(q): Query<TitlePreviewQuery>) -> Res<Json<Vec<TitlePreview>>> {
    let t = store::Titles { template: q.template, auto: false, lang: q.lang.unwrap_or_else(|| "en".into()) };
    let docs = store::search(&SearchParams { sort: Some("added".into()), limit: Some(5), ..Default::default() }, &u)?;
    Ok(Json(docs.items.iter().map(|d| TitlePreview { id: d.id, before: d.title.clone(), after: store::render_title(d, &t) }).collect()))
}

// ───────────────────────── expedientes ─────────────────────────

#[derive(Serialize, ToSchema)]
struct BundleSummary {
    id: i64,
    name: String,
    /// Checklist items
    total: i64,
    /// Items that already have a document
    done: i64,
    updated: String,
}

#[derive(Serialize, ToSchema)]
struct BundleItem {
    id: i64,
    label: String,
    document: Option<Document>,
}

#[derive(Serialize, ToSchema)]
struct Bundle {
    id: i64,
    name: String,
    items: Vec<BundleItem>,
    shares: Vec<Share>,
}

#[derive(Deserialize, ToSchema)]
struct BundleInput {
    name: Option<String>,
    /// Checklist to start with (one label per item)
    #[serde(default)]
    items: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct BundleItemInput {
    label: Option<String>,
    /// Document that fulfils it; null leaves it pending
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<i64>)]
    doc_id: Option<Option<i64>>,
}

fn nullable<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}

fn own_bundle(u: &AuthUser, id: i64) -> Res<String> {
    db::with(|c| c.query_row("SELECT name FROM bundles WHERE id=? AND user_id=?", [id, u.id], |r| r.get(0)))
        .map_err(|_| store::Error::NotFound.into())
}
fn touch_bundle(id: i64) {
    let _ = db::with(|c| c.execute("UPDATE bundles SET updated=datetime('now') WHERE id=?", [id]));
}

/// The bundle's items with their documents (only those the user can see and that aren't in the trash).
pub(crate) fn bundle_items(u: &AuthUser, id: i64) -> store::Result<Vec<(i64, String, Option<Document>)>> {
    let rows: Vec<(i64, String, Option<i64>)> = db::with(|c| {
        c.prepare_cached("SELECT id, label, doc_id FROM bundle_items WHERE bundle_id=? ORDER BY position, id")?
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect()
    })?;
    Ok(rows
        .into_iter()
        .map(|(i, label, doc)| {
            let d = doc.filter(|d| store::authorize(u, *d).is_ok()).and_then(|d| store::get_doc_brief(d).ok()).filter(|d| d.deleted_at.is_none());
            (i, label, d)
        })
        .collect())
}

fn bundle(u: &AuthUser, id: i64) -> Res<Bundle> {
    let name = own_bundle(u, id)?;
    let items = bundle_items(u, id)?.into_iter().map(|(id, label, document)| BundleItem { id, label, document }).collect();
    let shares = db::with(|c| {
        c.prepare_cached("SELECT id, created, expires, views FROM bundle_shares WHERE bundle_id=? AND expires > datetime('now') ORDER BY id DESC")?
            .query_map([id], |r| Ok(Share { id: r.get(0)?, created: r.get(1)?, expires: r.get(2)?, views: r.get(3)?, url: None }))?
            .collect::<rusqlite::Result<Vec<_>>>()
    }).map_err(store::Error::from)?;
    Ok(Bundle { id, name, items, shares })
}

/// Your bundles
#[utoipa::path(get, path = "/api/bundles", tag = "bundles", responses((status = 200, body = Vec<BundleSummary>)))]
async fn list_bundles(u: AuthUser) -> Res<Json<Vec<BundleSummary>>> {
    Ok(Json(db::with(|c| {
        c.prepare_cached(
            "SELECT b.id, b.name, (SELECT count(*) FROM bundle_items i WHERE i.bundle_id=b.id),
               (SELECT count(*) FROM bundle_items i JOIN documents d ON d.id=i.doc_id WHERE i.bundle_id=b.id AND d.deleted_at IS NULL), b.updated
             FROM bundles b WHERE b.user_id=? ORDER BY b.updated DESC",
        )?
        .query_map([u.id], |r| Ok(BundleSummary { id: r.get(0)?, name: r.get(1)?, total: r.get(2)?, done: r.get(3)?, updated: r.get(4)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()
    }).map_err(store::Error::from)?))
}

/// Create a bundle
#[utoipa::path(post, path = "/api/bundles", tag = "bundles", request_body = BundleInput, responses((status = 201, body = Bundle)))]
async fn create_bundle(u: AuthUser, Json(b): Json<BundleInput>) -> Res<(StatusCode, Json<Bundle>)> {
    let name: String = b.name.as_deref().unwrap_or("").trim().chars().take(80).collect();
    if name.is_empty() {
        return Err(bad("the bundle needs a name"));
    }
    let id = db::write(|c| {
        c.execute("INSERT INTO bundles(user_id, name) VALUES(?,?)", params![u.id, name])?;
        let id = c.last_insert_rowid();
        for (i, l) in b.items.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).enumerate() {
            c.execute("INSERT INTO bundle_items(bundle_id, label, position) VALUES(?,?,?)", params![id, l.chars().take(120).collect::<String>(), i as i64])?;
        }
        Ok(id)
    }).map_err(store::Error::from)?;
    Ok((StatusCode::CREATED, Json(bundle(&u, id)?)))
}

/// A bundle with its checklist and active links
#[utoipa::path(get, path = "/api/bundles/{id}", tag = "bundles", params(("id" = i64, Path,)), responses((status = 200, body = Bundle), (status = 404, body = ErrorBody)))]
async fn get_bundle(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Bundle>> {
    Ok(Json(bundle(&u, id)?))
}

/// Rename a bundle
#[utoipa::path(patch, path = "/api/bundles/{id}", tag = "bundles", params(("id" = i64, Path,)), request_body = BundleInput, responses((status = 200, body = Bundle)))]
async fn update_bundle(u: AuthUser, Path(id): Path<i64>, Json(b): Json<BundleInput>) -> Res<Json<Bundle>> {
    own_bundle(&u, id)?;
    if let Some(n) = b.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        db::with(|c| c.execute("UPDATE bundles SET name=?, updated=datetime('now') WHERE id=?", params![n.chars().take(80).collect::<String>(), id])).map_err(store::Error::from)?;
    }
    Ok(Json(bundle(&u, id)?))
}

/// Delete a bundle (its documents are kept)
#[utoipa::path(delete, path = "/api/bundles/{id}", tag = "bundles", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_bundle(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    own_bundle(&u, id)?;
    db::with(|c| c.execute("DELETE FROM bundles WHERE id=?", [id])).map_err(store::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add an item to a bundle's checklist
#[utoipa::path(post, path = "/api/bundles/{id}/items", tag = "bundles", params(("id" = i64, Path,)), request_body = BundleItemInput, responses((status = 200, body = Bundle)))]
async fn add_bundle_item(u: AuthUser, Path(id): Path<i64>, Json(i): Json<BundleItemInput>) -> Res<Json<Bundle>> {
    own_bundle(&u, id)?;
    let doc = i.doc_id.flatten();
    if let Some(d) = doc {
        store::authorize(&u, d)?;
    }
    let label = match (i.label.as_deref().map(str::trim).filter(|l| !l.is_empty()), doc) {
        (Some(l), _) => l.chars().take(120).collect(),
        (None, Some(d)) => store::get_doc_brief(d)?.title,
        _ => return Err(bad("give a label or a document")),
    };
    db::with(|c| {
        c.execute(
            "INSERT INTO bundle_items(bundle_id, label, doc_id, position) VALUES(?1,?2,?3,(SELECT coalesce(max(position),0)+1 FROM bundle_items WHERE bundle_id=?1))",
            params![id, label, doc],
        )
    }).map_err(store::Error::from)?;
    touch_bundle(id);
    Ok(Json(bundle(&u, id)?))
}

fn item_bundle(u: &AuthUser, item: i64) -> Res<i64> {
    let b: i64 = db::with(|c| c.query_row("SELECT bundle_id FROM bundle_items WHERE id=?", [item], |r| r.get(0))).map_err(store::Error::from)?;
    own_bundle(u, b)?;
    Ok(b)
}

/// Rename an item or attach/detach its document
#[utoipa::path(patch, path = "/api/bundle-items/{id}", tag = "bundles", params(("id" = i64, Path,)), request_body = BundleItemInput, responses((status = 200, body = Bundle)))]
async fn update_bundle_item(u: AuthUser, Path(item): Path<i64>, Json(i): Json<BundleItemInput>) -> Res<Json<Bundle>> {
    let b = item_bundle(&u, item)?;
    if let Some(Some(d)) = i.doc_id {
        store::authorize(&u, d)?;
    }
    db::with(|c| {
        if let Some(l) = i.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            c.execute("UPDATE bundle_items SET label=? WHERE id=?", params![l.chars().take(120).collect::<String>(), item])?;
        }
        if let Some(d) = i.doc_id {
            c.execute("UPDATE bundle_items SET doc_id=? WHERE id=?", params![d, item])?;
        }
        Ok(())
    }).map_err(store::Error::from)?;
    touch_bundle(b);
    Ok(Json(bundle(&u, b)?))
}

/// Remove an item from a bundle
#[utoipa::path(delete, path = "/api/bundle-items/{id}", tag = "bundles", params(("id" = i64, Path,)), responses((status = 200, body = Bundle)))]
async fn delete_bundle_item(u: AuthUser, Path(item): Path<i64>) -> Res<Json<Bundle>> {
    let b = item_bundle(&u, item)?;
    db::with(|c| c.execute("DELETE FROM bundle_items WHERE id=?", [item])).map_err(store::Error::from)?;
    touch_bundle(b);
    Ok(Json(bundle(&u, b)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ZipQuery {
    /// Language of the checklist file inside: en | es
    lang: Option<String>,
}

/// Download a bundle as a .zip
///
/// One file per item that has a document (numbered in checklist order, named after the item), plus a
/// checklist text file that marks what is still missing.
#[utoipa::path(get, path = "/api/bundles/{id}/zip", tag = "bundles", params(("id" = i64, Path,), ZipQuery),
    responses((status = 200, description = "ZIP file", content_type = "application/zip", body = Vec<u8>)))]
async fn bundle_zip(u: AuthUser, Path(id): Path<i64>, Query(q): Query<ZipQuery>, req: Request) -> Res<Response> {
    let name = own_bundle(&u, id)?;
    let items = bundle_items(&u, id)?;
    zip_response(name, items, q.lang.unwrap_or_default().starts_with("es"), true, req).await
}

/// Arma el .zip en un archivo temporal y lo envía por partes (no ocupa RAM aunque pese cientos de MB).
pub(crate) async fn zip_response(name: String, items: Vec<(i64, String, Option<Document>)>, es: bool, with_missing: bool, req: Request) -> Res<Response> {
    let path = cfg().data.join("tmp").join(format!("zip-{}.zip", random_hex(8)));
    let (n, p) = (name.clone(), path.clone());
    tokio::task::spawn_blocking(move || crate::public::write_zip(&n, &items, es, with_missing, &p)).await.map_err(bad)?.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let ascii: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let res = serve_file(path.clone(), "application/zip", Some(format!("attachment; filename=\"{}.zip\"", ascii.trim())), req).await;
    // El archivo ya está abierto por la respuesta: se puede borrar del disco (se libera al terminar el envío).
    let _ = tokio::fs::remove_file(&path).await;
    res
}

/// Share a bundle with a link that expires
///
/// Whoever has the link sees the bundle's documents and can download them one by one or all as a .zip.
#[utoipa::path(post, path = "/api/bundles/{id}/shares", tag = "bundles", params(("id" = i64, Path,)), request_body = ShareInput, responses((status = 201, body = Share)))]
async fn create_bundle_share(Editor(u): Editor, headers: HeaderMap, Path(id): Path<i64>, Json(i): Json<ShareInput>) -> Res<(StatusCode, Json<Share>)> {
    if u.limited() {
        return Err(forbidden("a token limited to some folders or apps cannot do this"));
    }
    let name = own_bundle(&u, id)?;
    let days = i.days.unwrap_or(7).clamp(1, 90);
    let token = random_hex(24);
    let sid = db::with(|c| {
        c.execute("INSERT INTO bundle_shares(bundle_id, user_id, token_hash, expires) VALUES(?,?,?, datetime('now', '+' || ? || ' days'))", params![id, u.id, auth::sha(&token), days])?;
        Ok(c.last_insert_rowid())
    }).map_err(store::Error::from)?;
    store::audit(Some(&u), "share link created", &name, &format!("bundle, {days} days"));
    let mut s = bundle(&u, id)?.shares.into_iter().find(|s| s.id == sid).ok_or(store::Error::NotFound)?;
    s.url = Some(format!("{}/b/{token}", crate::oauth::origin(&headers)));
    Ok((StatusCode::CREATED, Json(s)))
}

/// Revoke a bundle link
#[utoipa::path(delete, path = "/api/bundle-shares/{id}", tag = "bundles", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_bundle_share(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    let b: i64 = db::with(|c| c.query_row("SELECT bundle_id FROM bundle_shares WHERE id=?", [id], |r| r.get(0))).map_err(store::Error::from)?;
    own_bundle(&u, b)?;
    db::with(|c| c.execute("DELETE FROM bundle_shares WHERE id=?", [id])).map_err(store::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}
