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

/// Cuerpo de error común a toda la API.
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Papiro API",
        description = "API REST de Papiro. Autenticación: cabecera `Authorization: Bearer <token>` con un token creado en Ajustes → Tokens de API (o `POST /api/tokens`). La interfaz web usa una cookie de sesión.\n\nRoles: `viewer` solo lee; `editor` sube y edita; `admin` además administra usuarios, webhooks y borra definitivamente. Un token puede ser de solo lectura.\n\nLos errores devuelven siempre `{\"error\": \"mensaje\"}` con el código HTTP correspondiente.\n\nPara agentes de IA existe además un servidor MCP (Streamable HTTP) en `/mcp` que acepta el mismo token."
    ),
    tags(
        (name = "documentos", description = "Subir, buscar, leer y editar documentos"),
        (name = "carpetas", description = "Carpetas anidadas para ordenar los documentos"),
        (name = "etiquetas", description = "Etiquetas, corresponsales y tipos de documento"),
        (name = "cola", description = "Estado de la ingesta y eventos en vivo"),
        (name = "webhooks", description = "Avisos HTTP firmados cuando cambia un documento"),
        (name = "cuenta", description = "Sesión, usuarios y tokens de API"),
        (name = "sistema", description = "Salud, integridad y auditoría (administradores)")
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
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).description(Some("Token de API (ppr_…)")).build()),
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
        .routes(routes!(list_users, create_user))
        .routes(routes!(update_user, delete_user))
        .split_for_parts()
}

/// Argon2 usa 19 MB por cálculo: se hacen como mucho dos a la vez y el resto espera su turno.
async fn hashing<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    let _permit = tokio::time::timeout(Duration::from_secs(8), auth::HASHING.acquire())
        .await
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "servidor ocupado; intenta de nuevo en unos segundos".into()))?
        .map_err(bad)?;
    tokio::task::spawn_blocking(f).await.map_err(bad)
}

// ───────────────────────── documentos ─────────────────────────

/// Buscar y listar documentos
///
/// Búsqueda full-text (SQLite FTS5, ordenada por relevancia BM25) combinada con filtros.
/// Sin `q` devuelve el listado ordenado por fecha. Solo incluye lo que el usuario puede ver.
#[utoipa::path(get, path = "/api/documents", tag = "documentos", params(SearchParams),
    responses((status = 200, body = DocList), (status = 401, body = ErrorBody)))]
async fn list_documents(u: AuthUser, Query(p): Query<SearchParams>) -> Res<Json<DocList>> {
    Ok(Json(store::search(&p, &u)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct UploadQuery {
    /// Espera a que termine el procesamiento y devuelve los documentos ya creados
    wait: Option<bool>,
}

/// Formulario multipart de subida
#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadForm {
    /// Archivo a subir (se puede repetir el campo para subir varios)
    #[schema(value_type = String, format = Binary)]
    file: String,
    /// Título (por defecto, el nombre del archivo)
    title: Option<String>,
    /// Fecha del documento YYYY-MM-DD (por defecto se detecta del contenido)
    created: Option<String>,
    correspondent_id: Option<i64>,
    doctype_id: Option<i64>,
    /// IDs de etiquetas separados por coma
    tags: Option<String>,
    /// `true` para que solo lo vean quien lo sube y los administradores
    private: Option<bool>,
    /// Carpeta en la que se guarda
    folder_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
struct UploadResult {
    /// Un trabajo de ingesta por archivo
    jobs: Vec<Job>,
    /// Con `wait=true`: los documentos resultantes (también los ya existentes si el archivo era duplicado)
    documents: Vec<Document>,
}

/// Subir documentos
///
/// Guarda los archivos y los encola para extracción/OCR. Responde `202` de inmediato con los trabajos;
/// con `?wait=true` espera el resultado y responde `201` con los documentos. Los duplicados (mismo
/// SHA-256) no se vuelven a guardar: el trabajo termina en `duplicate` apuntando al documento existente.
/// Un tipo de archivo no soportado se rechaza con `415` sin encolar nada. Si un trabajo falla, el
/// archivo se conserva y se puede reintentar con `POST /api/jobs/{id}/retry`.
#[utoipa::path(post, path = "/api/documents", tag = "documentos", params(UploadQuery),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 202, body = UploadResult, description = "Encolado"), (status = 201, body = UploadResult, description = "Procesado (wait=true)"),
              (status = 400, body = ErrorBody), (status = 415, body = ErrorBody), (status = 507, body = ErrorBody, description = "Sin espacio en disco")))]
async fn upload_documents(Editor(u): Editor, Query(q): Query<UploadQuery>, mut mp: Multipart) -> Res<(StatusCode, Json<UploadResult>)> {
    let mut staged: Vec<(std::path::PathBuf, String)> = vec![];
    let received = receive_upload(&mut mp, &mut staged).await;
    let meta = match received {
        Ok(meta) if !staged.is_empty() => meta,
        other => {
            // Nada queda a medias: si la subida falla, sus temporales se borran.
            for (p, _) in &staged {
                let _ = tokio::fs::remove_file(p).await;
            }
            let err = other.err().unwrap_or_else(|| bad("falta el campo «file»"));
            // El motivo queda en el log: una subida cortada por el navegador solo se ve desde aquí.
            println!("subida rechazada ({}): {}", err.0.as_u16(), err.1);
            return Err(err);
        }
    };
    let mut rx = store::bus().subscribe();
    let mut ids = vec![];
    for (path, filename) in &staged {
        ids.push(ingest::enqueue(path, filename, "upload", &meta, Some(u.id))?);
        store::audit(Some(&u), "documento subido", filename, "");
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
        std::io::ErrorKind::StorageFull => ApiError(StatusCode::INSUFFICIENT_STORAGE, "no queda espacio en disco".into()),
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut meta = json!({});
    while let Some(mut field) = mp.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or_default().rsplit(['/', '\\']).next().filter(|n| !n.trim().is_empty()).unwrap_or("documento").to_string();
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
                    _ => return Err(bad("esa carpeta no existe")),
                },
                "correspondent_id" | "doctype_id" => meta[&name] = v.trim().parse::<i64>().map(Value::from).unwrap_or(Value::Null),
                "private" => meta["is_private"] = Value::Bool(matches!(v.trim(), "true" | "1" | "on")),
                _ => meta[&name] = Value::from(v),
            }
        }
    }
    Ok(meta)
}

/// Solo el dueño de un documento o un administrador puede hacerlo privado o público.
fn check_privacy(u: &AuthUser, id: i64, p: &DocPatch) -> Res<()> {
    if p.is_private.is_some() && u.role != "admin" && store::get_doc_brief(id)?.owner_id != Some(u.id) {
        return Err(forbidden("solo quien subió el documento o un administrador puede cambiar su privacidad"));
    }
    Ok(())
}

#[derive(Serialize, ToSchema)]
struct Affected {
    affected: usize,
}

/// Edición masiva
///
/// Aplica los mismos campos que `PATCH /api/documents/{id}` a todos los `ids`, o una acción
/// (`trash`, `restore`, `purge`; esta última solo administradores). Todo o nada.
#[utoipa::path(post, path = "/api/documents/bulk", tag = "documentos", request_body = BulkEdit,
    responses((status = 200, body = Affected), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn bulk_edit(Editor(u): Editor, Json(b): Json<BulkEdit>) -> Res<Json<Affected>> {
    if b.action.as_deref() == Some("purge") && !u.admin() {
        return Err(forbidden("solo un administrador puede borrar definitivamente"));
    }
    for id in &b.ids {
        store::authorize(&u, *id)?;
        check_privacy(&u, *id, &b.patch)?;
    }
    let n = store::bulk(&b)?;
    store::audit(Some(&u), &format!("edición masiva{}", b.action.as_deref().map(|a| format!(" ({a})")).unwrap_or_default()), &format!("{n} documentos"), &format!("{:?}", b.ids));
    Ok(Json(Affected { affected: n }))
}

/// Leer un documento (metadatos, texto completo y notas)
#[utoipa::path(get, path = "/api/documents/{id}", tag = "documentos", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 404, body = ErrorBody)))]
async fn get_document(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    Ok(Json(store::get_doc(id)?))
}

/// Editar un documento
///
/// Solo cambia los campos enviados. `null` en `correspondent_id`, `doctype_id`, `asn` o `summary` los vacía.
#[utoipa::path(patch, path = "/api/documents/{id}", tag = "documentos", params(("id" = i64, Path,)), request_body = DocPatch,
    responses((status = 200, body = Document), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update_document(Editor(u): Editor, Path(id): Path<i64>, Json(p): Json<DocPatch>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    check_privacy(&u, id, &p)?;
    let d = store::update_doc(id, &p)?;
    store::audit(Some(&u), "documento editado", &format!("documento {id}"), &d.title);
    Ok(Json(d))
}

#[derive(Deserialize, ToSchema)]
struct SuggestAction {
    /// `accept` aplica las sugerencias pendientes, `dismiss` las descarta, `refresh` vuelve a analizar el documento
    action: String,
    /// Limita a ciertas sugerencias: `folder_id`, `doctype_id`, `correspondent_id`, `tag:<id>`. Sin él, todas
    fields: Option<Vec<String>>,
}

/// Sugerencias del análisis local
///
/// Papiro analiza cada documento sin salir del servidor: detecta por reglas el vencimiento, el importe,
/// la CURP y el RFC, y propone carpeta, tipo, corresponsal y etiquetas por parecido con lo ya archivado.
/// Aquí se aceptan o descartan las sugerencias pendientes, o se pide un análisis nuevo.
#[utoipa::path(post, path = "/api/documents/{id}/suggest", tag = "documentos", params(("id" = i64, Path,)), request_body = SuggestAction,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn document_suggest(Editor(u): Editor, Path(id): Path<i64>, Json(a): Json<SuggestAction>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    match a.action.as_str() {
        "accept" => {
            crate::suggest::resolve(id, true, a.fields.as_deref())?;
            store::audit(Some(&u), "sugerencias aceptadas", &format!("documento {id}"), "");
        }
        "dismiss" => crate::suggest::resolve(id, false, a.fields.as_deref())?,
        "refresh" => crate::suggest::analyze(id, true)?,
        _ => return Err(bad("action debe ser accept, dismiss o refresh")),
    }
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct DeleteQuery {
    /// Borra definitivamente (fila y archivos) en lugar de mandar a la papelera. Solo administradores.
    permanent: Option<bool>,
}

/// Enviar a la papelera (o borrar definitivamente)
#[utoipa::path(delete, path = "/api/documents/{id}", tag = "documentos", params(("id" = i64, Path,), DeleteQuery),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn delete_document(Editor(u): Editor, Path(id): Path<i64>, Query(q): Query<DeleteQuery>) -> Res<StatusCode> {
    store::authorize(&u, id)?;
    let title = store::get_doc_brief(id)?.title;
    if q.permanent == Some(true) {
        if !u.admin() {
            return Err(forbidden("solo un administrador puede borrar definitivamente"));
        }
        store::purge(id)?;
        store::audit(Some(&u), "documento borrado definitivamente", &format!("documento {id}"), &title);
    } else {
        store::trash(id, true)?;
        store::audit(Some(&u), "documento a la papelera", &format!("documento {id}"), &title);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Restaurar desde la papelera
#[utoipa::path(post, path = "/api/documents/{id}/restore", tag = "documentos", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 404, body = ErrorBody)))]
async fn restore_document(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Document>> {
    store::authorize(&u, id)?;
    store::trash(id, false)?;
    store::audit(Some(&u), "documento restaurado", &format!("documento {id}"), "");
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileQuery {
    /// `original` (lo que se subió) o `archive` (PDF con capa de texto). Por defecto: archive si existe.
    variant: Option<String>,
    /// Fuerza la descarga en lugar de mostrarlo en el navegador
    download: Option<bool>,
}

async fn serve_file(path: std::path::PathBuf, mime: &str, disposition: Option<String>, req: Request) -> Res<Response> {
    let mut res = tower_http::services::ServeFile::new(path).oneshot(req).await.map_err(bad)?.map(axum::body::Body::new);
    if res.status() == StatusCode::NOT_FOUND {
        return Err(ApiError(StatusCode::NOT_FOUND, "el archivo de este documento no está en disco; revisa Ajustes → Salud".into()));
    }
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).map_err(bad)?);
    if let Some(d) = disposition {
        h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&d).map_err(bad)?);
    }
    Ok(res)
}

/// Descargar el archivo
///
/// Admite peticiones por rangos (`Range`).
#[utoipa::path(get, path = "/api/documents/{id}/file", tag = "documentos", params(("id" = i64, Path,), FileQuery),
    responses((status = 200, description = "Contenido del archivo", content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = ErrorBody)))]
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
static RENDERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Miniatura de la primera página (JPEG)
///
/// Si aún no existe (documentos importados), se genera en la primera petición y se guarda.
#[utoipa::path(get, path = "/api/documents/{id}/thumb", tag = "documentos", params(("id" = i64, Path,)),
    responses((status = 200, description = "Imagen JPEG", content_type = "image/jpeg", body = Vec<u8>), (status = 404, body = ErrorBody)))]
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

/// Página renderizada como imagen (JPEG, 1400 px de ancho)
///
/// Es lo que usa la vista previa: funciona igual en cualquier navegador, también en móviles.
/// Disponible para PDF e imágenes (cualquier documento con PDF original o de archivo).
#[utoipa::path(get, path = "/api/documents/{id}/pages/{page}", tag = "documentos", params(("id" = i64, Path,), ("page" = u32, Path, description = "Número de página, desde 1")),
    responses((status = 200, description = "Imagen JPEG", content_type = "image/jpeg", body = Vec<u8>), (status = 404, body = ErrorBody)))]
async fn document_page(u: AuthUser, Path((id, page)): Path<(i64, u32)>) -> Res<Response> {
    store::authorize(&u, id)?;
    let d = store::get_doc_brief(id)?;
    let file = store::doc_dir(id).join(if d.has_archive { "archive.pdf" } else { "original" });
    if (!d.has_archive && d.mime != "application/pdf") || page < 1 || i64::from(page) > d.pages.max(1) {
        return Err(store::Error::NotFound.into());
    }
    if !file.is_file() {
        return Err(ApiError(StatusCode::NOT_FOUND, "el archivo de este documento no está en disco; revisa Ajustes → Salud".into()));
    }
    let _permit = RENDERS.acquire().await.map_err(bad)?;
    let out = tokio::task::spawn_blocking(move || {
        let n = page.to_string();
        std::process::Command::new("pdftoppm")
            .args(["-f", &n, "-l", &n, "-scale-to-x", "1400", "-scale-to-y", "-1", "-jpeg", "-jpegopt", "quality=80"])
            .arg(file)
            .output()
    })
    .await
    .map_err(bad)?
    .map_err(|e| store::Error::Other(e.to_string()))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(store::Error::Other("no se pudo renderizar la página".into()).into());
    }
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=604800")], out.stdout).into_response())
}

/// Clasificar y resumir con IA
///
/// Envía el texto al LLM configurado (`PAPIRO_LLM_*`) y aplica título, resumen, corresponsal, tipo,
/// etiquetas y fecha sugeridos. Responde `501` si no hay LLM configurado.
#[utoipa::path(post, path = "/api/documents/{id}/ai", tag = "documentos", params(("id" = i64, Path,)),
    responses((status = 200, body = Document), (status = 501, body = ErrorBody), (status = 502, body = ErrorBody)))]
async fn document_ai(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Document>> {
    if cfg().llm.is_none() {
        return Err(ApiError(StatusCode::NOT_IMPLEMENTED, "no hay LLM configurado (PAPIRO_LLM_API_KEY)".into()));
    }
    store::authorize(&u, id)?;
    tokio::task::spawn_blocking(move || crate::llm::enrich(id))
        .await
        .map_err(bad)?
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    store::audit(Some(&u), "documento clasificado con IA", &format!("documento {id}"), "");
    Ok(Json(store::get_doc(id)?))
}

#[derive(Deserialize, ToSchema)]
struct NoteInput {
    text: String,
}

/// Añadir una nota
#[utoipa::path(post, path = "/api/documents/{id}/notes", tag = "documentos", params(("id" = i64, Path,)), request_body = NoteInput,
    responses((status = 201, body = Note), (status = 404, body = ErrorBody)))]
async fn add_note(Editor(u): Editor, Path(id): Path<i64>, Json(n): Json<NoteInput>) -> Res<(StatusCode, Json<Note>)> {
    store::authorize(&u, id)?;
    Ok((StatusCode::CREATED, Json(store::add_note(id, n.text.trim())?)))
}

/// Borrar una nota
#[utoipa::path(delete, path = "/api/notes/{id}", tag = "documentos", params(("id" = i64, Path,)), responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_note(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    store::authorize(&u, store::note_doc(id)?)?;
    store::delete_note(id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── etiquetas, corresponsales, tipos ─────────────────────────

macro_rules! label_routes {
    ($kind:literal, $path:literal, $path_id:literal, $what:literal, $list:ident, $create:ident, $update:ident, $delete:ident) => {
        #[doc = concat!("Listar ", $what, " (con número de documentos)")]
        #[utoipa::path(get, path = $path, tag = "etiquetas", responses((status = 200, body = Vec<Label>)))]
        async fn $list(_u: AuthUser) -> Res<Json<Vec<Label>>> {
            Ok(Json(store::list_labels($kind)?))
        }

        #[doc = concat!("Crear en ", $what)]
        ///
        /// `match` + `algo` definen la asignación automática al ingerir: `any` (alguna palabra),
        /// `all` (todas), `literal` (frase exacta), `regex`; `none` la desactiva.
        #[utoipa::path(post, path = $path, tag = "etiquetas", request_body = LabelInput,
            responses((status = 201, body = Label), (status = 409, body = ErrorBody)))]
        async fn $create(_u: Editor, Json(i): Json<LabelInput>) -> Res<(StatusCode, Json<Label>)> {
            Ok((StatusCode::CREATED, Json(store::create_label($kind, &i)?)))
        }

        #[doc = concat!("Editar en ", $what)]
        #[utoipa::path(patch, path = $path_id, tag = "etiquetas", params(("id" = i64, Path,)), request_body = LabelInput,
            responses((status = 200, body = Label), (status = 404, body = ErrorBody)))]
        async fn $update(_u: Editor, Path(id): Path<i64>, Json(i): Json<LabelInput>) -> Res<Json<Label>> {
            Ok(Json(store::update_label($kind, id, &i)?))
        }

        #[doc = concat!("Borrar de ", $what, " (los documentos se conservan)")]
        #[utoipa::path(delete, path = $path_id, tag = "etiquetas", params(("id" = i64, Path,)),
            responses((status = 204), (status = 404, body = ErrorBody)))]
        async fn $delete(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
            let name = store::get_label($kind, id)?.name;
            store::delete_label($kind, id)?;
            store::audit(Some(&u), concat!("borrado en ", $what), &name, "");
            Ok(StatusCode::NO_CONTENT)
        }
    };
}
label_routes!("tag", "/api/tags", "/api/tags/{id}", "etiquetas", list_tags, create_tag, update_tag, delete_tag);
label_routes!("correspondent", "/api/correspondents", "/api/correspondents/{id}", "corresponsales", list_correspondents, create_correspondent, update_correspondent, delete_correspondent);
label_routes!("doctype", "/api/doctypes", "/api/doctypes/{id}", "tipos de documento", list_doctypes, create_doctype, update_doctype, delete_doctype);

// ───────────────────────── carpetas ─────────────────────────

fn folder(u: &AuthUser, id: i64) -> Res<Json<Folder>> {
    Ok(Json(store::list_folders(u)?.into_iter().find(|f| f.id == id).ok_or(store::Error::NotFound)?))
}

/// Listar carpetas
///
/// Todas las carpetas, ordenadas por ruta. `document_count` cuenta lo que hay en la carpeta y en sus subcarpetas.
#[utoipa::path(get, path = "/api/folders", tag = "carpetas", responses((status = 200, body = Vec<Folder>)))]
async fn list_folders(u: AuthUser) -> Res<Json<Vec<Folder>>> {
    Ok(Json(store::list_folders(&u)?))
}

/// Crear una carpeta
#[utoipa::path(post, path = "/api/folders", tag = "carpetas", request_body = FolderInput,
    responses((status = 201, body = Folder), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn create_folder(Editor(u): Editor, Json(i): Json<FolderInput>) -> Res<(StatusCode, Json<Folder>)> {
    let f = folder(&u, store::create_folder(&i)?)?;
    store::audit(Some(&u), "carpeta creada", &f.path, "");
    store::notify(json!({"type": "folders"}));
    Ok((StatusCode::CREATED, f))
}

/// Renombrar o mover una carpeta
#[utoipa::path(patch, path = "/api/folders/{id}", tag = "carpetas", params(("id" = i64, Path,)), request_body = FolderInput,
    responses((status = 200, body = Folder), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn update_folder(Editor(u): Editor, Path(id): Path<i64>, Json(i): Json<FolderInput>) -> Res<Json<Folder>> {
    store::update_folder(id, &i)?;
    store::notify(json!({"type": "folders"}));
    folder(&u, id)
}

/// Borrar una carpeta
///
/// Borra la carpeta y sus subcarpetas. Los documentos no se borran: pasan a la carpeta superior
/// (o quedan sin carpeta si estaba en la raíz).
#[utoipa::path(delete, path = "/api/folders/{id}", tag = "carpetas", params(("id" = i64, Path,)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_folder(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    let path = folder(&u, id)?.0.path;
    let moved = store::delete_folder(id)?;
    store::audit(Some(&u), "carpeta borrada", &path, &format!("{moved} documento(s) pasaron a la carpeta superior"));
    store::notify(json!({"type": "folders"}));
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────── cola, eventos, estadísticas ─────────────────────────

/// Cada quien ve sus subidas y lo que entró por carpeta o correo; los administradores, todo.
fn job_visible(u: &AuthUser, j: &Job) -> bool {
    u.role == "admin" || j.user_id.is_none() || j.user_id == Some(u.id)
}

/// Últimos 100 trabajos de ingesta
#[utoipa::path(get, path = "/api/jobs", tag = "cola", responses((status = 200, body = Vec<Job>)))]
async fn list_jobs(u: AuthUser) -> Res<Json<Vec<Job>>> {
    Ok(Json(ingest::jobs("id > ?", 0)?.into_iter().filter(|j| job_visible(&u, j)).collect()))
}

fn own_job(u: &AuthUser, id: i64) -> Res<Job> {
    Some(ingest::job(id)?).filter(|j| job_visible(u, j)).ok_or_else(|| store::Error::NotFound.into())
}

/// Estado de un trabajo de ingesta
#[utoipa::path(get, path = "/api/jobs/{id}", tag = "cola", params(("id" = i64, Path,)),
    responses((status = 200, body = Job), (status = 404, body = ErrorBody)))]
async fn get_job(u: AuthUser, Path(id): Path<i64>) -> Res<Json<Job>> {
    Ok(Json(own_job(&u, id)?))
}

/// Reintentar un trabajo fallido
///
/// El archivo de un trabajo fallido se conserva (campo `can_retry`); esto lo vuelve a encolar.
#[utoipa::path(post, path = "/api/jobs/{id}/retry", tag = "cola", params(("id" = i64, Path,)),
    responses((status = 200, body = Job), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn retry_job(Editor(u): Editor, Path(id): Path<i64>) -> Res<Json<Job>> {
    own_job(&u, id)?;
    Ok(Json(ingest::retry(id)?))
}

/// Descartar un trabajo terminado
///
/// Si había fallado, borra también el archivo que se había conservado.
#[utoipa::path(delete, path = "/api/jobs/{id}", tag = "cola", params(("id" = i64, Path,)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn dismiss_job(Editor(u): Editor, Path(id): Path<i64>) -> Res<StatusCode> {
    let j = own_job(&u, id)?;
    ingest::dismiss(id)?;
    if j.status == "failed" {
        store::audit(Some(&u), "trabajo fallido descartado", &j.filename, j.error.as_deref().unwrap_or(""));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Eventos en vivo (Server-Sent Events)
///
/// Cada mensaje es JSON: `{"type":"job","job":{…}}` o `{"type":"document.created|updated|deleted|purged","id":N}`.
#[utoipa::path(get, path = "/api/events", tag = "cola", responses((status = 200, description = "Flujo SSE", content_type = "text/event-stream", body = String)))]
async fn events(u: AuthUser) -> impl IntoResponse {
    let stream = tokio_stream::wrappers::BroadcastStream::new(store::bus().subscribe()).filter_map(move |m| {
        let data = m.ok()?;
        // Los trabajos de otros usuarios (y sus nombres de archivo) no se anuncian a los demás.
        let owner = serde_json::from_str::<Value>(&data).ok().and_then(|v| v["user_id"].as_i64());
        (u.role == "admin" || owner.is_none() || owner == Some(u.id)).then(|| Ok::<_, std::convert::Infallible>(sse::Event::default().data(data)))
    });
    sse::Sse::new(stream).keep_alive(sse::KeepAlive::default())
}

/// Totales del archivo y de la cola
#[utoipa::path(get, path = "/api/stats", tag = "cola", responses((status = 200, body = Stats)))]
async fn get_stats(u: AuthUser) -> Res<Json<Stats>> {
    Ok(Json(store::stats(&u)?))
}

// ───────────────────────── sistema ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Status {
    version: &'static str,
    /// Versión del esquema de la base de datos
    schema: i64,
    /// La corrección de páginas giradas está activa
    ocr_rotate: bool,
    ocr_langs: String,
    queued: i64,
    failed_jobs: i64,
    /// Avisos de webhook pendientes de entregar
    pending_webhooks: i64,
    /// Última verificación de integridad (null si aún no ha corrido ninguna)
    last_verify: Option<VerifyReport>,
    /// Último respaldo automático o manual (null si nunca se ha hecho)
    last_backup: Option<Value>,
    /// Cada cuántas horas se respalda solo (null = desactivado: falta PAPIRO_BACKUP_DIR)
    backup_every_hours: Option<u64>,
}

/// Estado del sistema (admin)
#[utoipa::path(get, path = "/api/status", tag = "sistema", responses((status = 200, body = Status)))]
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
    /// `false` para comprobar solo existencia y tamaño (rápido); por defecto recalcula cada SHA-256
    deep: Option<bool>,
}

/// Verificar la integridad ahora (admin)
///
/// Comprueba que cada documento tiene sus archivos y que el original conserva su SHA-256, además
/// de la integridad de la base de datos y del índice de búsqueda.
#[utoipa::path(post, path = "/api/verify", tag = "sistema", params(VerifyQuery), responses((status = 200, body = VerifyReport)))]
async fn run_verify(Admin(u): Admin, Query(q): Query<VerifyQuery>) -> Res<Json<VerifyReport>> {
    let report = tokio::task::spawn_blocking(move || store::verify(q.deep != Some(false))).await.map_err(bad)?;
    store::audit(Some(&u), "verificación de integridad", &format!("{} documentos", report.documents), &format!("{} problemas", report.problems.len()));
    Ok(Json(report))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Registro de auditoría (admin)
///
/// Quién hizo qué y desde qué IP: inicios de sesión, subidas, ediciones, borrados y cambios de administración.
#[utoipa::path(get, path = "/api/audit", tag = "sistema", params(PageQuery), responses((status = 200, body = Vec<AuditEntry>)))]
async fn list_audit(_a: Admin, Query(q): Query<PageQuery>) -> Res<Json<Vec<AuditEntry>>> {
    Ok(Json(store::list_audit(q.limit.unwrap_or(100), q.offset.unwrap_or(0))?))
}

// ───────────────────────── webhooks ─────────────────────────

#[derive(Serialize, ToSchema)]
struct Webhook {
    id: i64,
    url: String,
    /// `*` o lista separada por comas: document.created, document.updated, document.deleted, document.purged
    events: String,
    /// Indica si tiene secreto de firma (el secreto no se devuelve)
    signed: bool,
    /// Código HTTP (o error) de la última entrega
    last_status: Option<String>,
    last_at: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct WebhookInput {
    url: String,
    events: Option<String>,
    /// Si se indica, cada entrega lleva `X-Papiro-Signature: sha256=<HMAC-SHA256 del cuerpo>`
    secret: Option<String>,
}

fn webhooks() -> store::Result<Vec<Webhook>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, url, events, secret != '', last_status, last_at FROM webhooks ORDER BY id")?
            .query_map([], |r| Ok(Webhook { id: r.get(0)?, url: r.get(1)?, events: r.get(2)?, signed: r.get(3)?, last_status: r.get(4)?, last_at: r.get(5)? }))?
            .collect()
    })?)
}

/// Listar webhooks (admin)
#[utoipa::path(get, path = "/api/webhooks", tag = "webhooks", responses((status = 200, body = Vec<Webhook>)))]
async fn list_webhooks(_a: Admin) -> Res<Json<Vec<Webhook>>> {
    Ok(Json(webhooks()?))
}

/// Crear un webhook (admin)
///
/// En cada evento se hace `POST` a la URL con `{"event", "document_id", "document"}`. Las entregas se
/// guardan en una cola persistente y se reintentan hasta 6 veces. No se admiten direcciones internas.
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
    store::audit(Some(&u), "webhook creado", &hook.url, &hook.events);
    Ok((StatusCode::CREATED, Json(hook)))
}

/// Borrar un webhook (admin)
#[utoipa::path(delete, path = "/api/webhooks/{id}", tag = "webhooks", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_webhook(Admin(u): Admin, Path(id): Path<i64>) -> Res<StatusCode> {
    db::with(|c| c.execute("DELETE FROM webhooks WHERE id=?", [id])).map_err(store::Error::from)?;
    store::audit(Some(&u), "webhook borrado", &format!("webhook {id}"), "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
struct WebhookTest {
    /// Código HTTP recibido o descripción del error
    status: String,
}

/// Enviar un evento de prueba (`ping`) (admin)
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
    /// Aún no hay usuarios: hay que crear el administrador con `POST /api/auth/setup`
    setup_required: bool,
    user: Option<AuthUser>,
    /// Hay un LLM configurado (habilita «Clasificar con IA»)
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

/// Estado de la sesión
#[utoipa::path(get, path = "/api/auth/status", tag = "cuenta", security(()), responses((status = 200, body = AuthStatus)))]
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
        Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "demasiados intentos desde tu conexión; espera unos minutos".into()))
    }
}

/// Crear la cuenta de administrador inicial
///
/// Solo funciona mientras no exista ningún usuario.
#[utoipa::path(post, path = "/api/auth/setup", tag = "cuenta", security(()), request_body = Credentials,
    responses((status = 200, body = AuthUser), (status = 409, body = ErrorBody)))]
async fn auth_setup(parts: Parts, Json(c): Json<Credentials>) -> Res<Response> {
    attempt(&parts)?;
    if auth::user_count() > 0 {
        return Err(ApiError(StatusCode::CONFLICT, "la instalación ya tiene usuarios".into()));
    }
    let user = hashing(move || auth::create_user(&c.username, &c.password, "admin")).await??;
    store::audit(Some(&user), "cuenta de administrador creada", &user.username, "");
    session_response(user, &parts.headers)
}

/// Iniciar sesión (cookie)
///
/// Límite: 10 intentos cada 5 minutos por dirección IP (`429`). El límite es por conexión, no por
/// usuario: nadie puede dejar fuera a otra persona fallando su contraseña.
#[utoipa::path(post, path = "/api/auth/login", tag = "cuenta", security(()), request_body = Credentials,
    responses((status = 200, body = AuthUser), (status = 401, body = ErrorBody), (status = 429, body = ErrorBody), (status = 503, body = ErrorBody)))]
async fn auth_login(parts: Parts, Json(c): Json<Credentials>) -> Res<Response> {
    let ip = attempt(&parts)?;
    let user = hashing(move || auth::login(&c.username, &c.password, &ip)).await??;
    session_response(user, &parts.headers)
}

/// Cerrar sesión
#[utoipa::path(post, path = "/api/auth/logout", tag = "cuenta", responses((status = 204)))]
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

/// Cambiar la contraseña propia (cierra las demás sesiones)
#[utoipa::path(post, path = "/api/auth/password", tag = "cuenta", request_body = PasswordChange,
    responses((status = 204), (status = 401, body = ErrorBody)))]
async fn auth_password(u: AuthUser, parts: Parts, Json(p): Json<PasswordChange>) -> Res<StatusCode> {
    if u.read_only {
        return Err(forbidden("un token de solo lectura no puede cambiar la contraseña"));
    }
    let ip = attempt(&parts)?;
    let who = u.clone();
    hashing(move || {
        auth::login(&who.username, &p.current, &ip)?;
        Ok::<_, ApiError>(auth::set_password(&who.username, &p.new)?)
    })
    .await??;
    store::audit(Some(&u), "contraseña cambiada", &u.username, "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
struct TokenInfo {
    id: i64,
    name: String,
    created: String,
    last_used: Option<String>,
    read_only: bool,
    /// Fecha de caducidad (null = no caduca)
    expires: Option<String>,
    /// Solo se devuelve una vez, al crearlo
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct TokenInput {
    /// Para qué se usa (p. ej. «n8n», «Claude MCP»)
    name: String,
    /// El token solo puede leer (buscar, ver, descargar)
    #[serde(default)]
    read_only: bool,
    /// Días hasta que caduca (omitir = no caduca)
    expires_days: Option<i64>,
}

fn tokens(user_id: i64) -> store::Result<Vec<TokenInfo>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, name, created, last_used, read_only, expires FROM api_tokens WHERE user_id=? ORDER BY id")?
            .query_map([user_id], |r| {
                Ok(TokenInfo { id: r.get(0)?, name: r.get(1)?, created: r.get(2)?, last_used: r.get(3)?, read_only: r.get(4)?, expires: r.get(5)?, token: None })
            })?
            .collect()
    })?)
}

/// Listar los tokens de API propios
#[utoipa::path(get, path = "/api/tokens", tag = "cuenta", responses((status = 200, body = Vec<TokenInfo>)))]
async fn list_tokens(u: AuthUser) -> Res<Json<Vec<TokenInfo>>> {
    Ok(Json(tokens(u.id)?))
}

/// Crear un token de API
///
/// El valor del token (`ppr_…`) solo aparece en esta respuesta; en el servidor se guarda su hash.
/// Un usuario lector solo puede crear tokens de solo lectura.
#[utoipa::path(post, path = "/api/tokens", tag = "cuenta", request_body = TokenInput, responses((status = 201, body = TokenInfo), (status = 403, body = ErrorBody)))]
async fn create_token(u: AuthUser, Json(t): Json<TokenInput>) -> Res<(StatusCode, Json<TokenInfo>)> {
    if u.read_only {
        return Err(forbidden("un token de solo lectura no puede crear otros tokens"));
    }
    let (id, token) = auth::new_api_token(u.id, t.name.trim(), t.read_only || u.role == "viewer", t.expires_days)?;
    store::audit(Some(&u), "token creado", t.name.trim(), if t.read_only { "solo lectura" } else { "" });
    let mut info = tokens(u.id)?.into_iter().find(|x| x.id == id).ok_or(store::Error::NotFound)?;
    info.token = Some(token);
    Ok((StatusCode::CREATED, Json(info)))
}

/// Revocar un token de API
#[utoipa::path(delete, path = "/api/tokens/{id}", tag = "cuenta", params(("id" = i64, Path,)), responses((status = 204)))]
async fn delete_token(u: AuthUser, Path(id): Path<i64>) -> Res<StatusCode> {
    if u.read_only {
        return Err(forbidden("un token de solo lectura no puede revocar tokens"));
    }
    db::with(|c| c.execute("DELETE FROM api_tokens WHERE id=? AND user_id=?", [id, u.id])).map_err(store::Error::from)?;
    store::audit(Some(&u), "token revocado", &format!("token {id}"), "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
struct UserInput {
    username: String,
    password: String,
    /// admin | editor | viewer (por defecto editor)
    role: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct UserPatch {
    /// admin | editor | viewer
    role: Option<String>,
    /// Nueva contraseña (cierra las sesiones de ese usuario)
    password: Option<String>,
}

fn users() -> store::Result<Vec<AuthUser>> {
    Ok(db::with(|c| c.prepare_cached("SELECT id, username, role FROM users ORDER BY id")?.query_map([], |r| Ok(AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())?)
}

/// Listar usuarios (admin)
#[utoipa::path(get, path = "/api/users", tag = "cuenta", responses((status = 200, body = Vec<AuthUser>)))]
async fn list_users(_a: Admin) -> Res<Json<Vec<AuthUser>>> {
    Ok(Json(users()?))
}

/// Crear un usuario (admin)
#[utoipa::path(post, path = "/api/users", tag = "cuenta", request_body = UserInput,
    responses((status = 201, body = AuthUser), (status = 409, body = ErrorBody)))]
async fn create_user(Admin(a): Admin, Json(i): Json<UserInput>) -> Res<(StatusCode, Json<AuthUser>)> {
    let user = hashing(move || auth::create_user(&i.username, &i.password, i.role.as_deref().unwrap_or("editor"))).await??;
    store::audit(Some(&a), "usuario creado", &user.username, &user.role);
    Ok((StatusCode::CREATED, Json(user)))
}

/// Siempre debe quedar al menos un administrador.
fn keep_an_admin(target: &AuthUser) -> Res<()> {
    if target.role == "admin" && users()?.iter().filter(|u| u.role == "admin").count() <= 1 {
        return Err(bad("es el único administrador; nombra otro antes"));
    }
    Ok(())
}

/// Cambiar el rol o la contraseña de un usuario (admin)
#[utoipa::path(patch, path = "/api/users/{id}", tag = "cuenta", params(("id" = i64, Path,)), request_body = UserPatch,
    responses((status = 200, body = AuthUser), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn update_user(Admin(a): Admin, Path(id): Path<i64>, Json(p): Json<UserPatch>) -> Res<Json<AuthUser>> {
    let target = users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?;
    if let Some(role) = p.role.as_deref().filter(|r| *r != target.role) {
        if !auth::ROLES.contains(&role) {
            return Err(bad("rol inválido (admin, editor o viewer)"));
        }
        keep_an_admin(&target)?;
        db::with(|c| c.execute("UPDATE users SET role=?, is_admin=? WHERE id=?", params![role, role == "admin", id])).map_err(store::Error::from)?;
        store::audit(Some(&a), "rol cambiado", &target.username, &format!("{} → {role}", target.role));
    }
    if let Some(pw) = p.password {
        let name = target.username.clone();
        hashing(move || auth::set_password(&name, &pw)).await??;
        store::audit(Some(&a), "contraseña restablecida", &target.username, "");
    }
    Ok(Json(users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?))
}

/// Borrar un usuario (admin)
///
/// Sus documentos se conservan (quedan sin dueño; los privados pasan a verlos solo los administradores).
#[utoipa::path(delete, path = "/api/users/{id}", tag = "cuenta", params(("id" = i64, Path,)),
    responses((status = 204), (status = 400, body = ErrorBody)))]
async fn delete_user(Admin(a): Admin, Path(id): Path<i64>) -> Res<StatusCode> {
    if a.id == id {
        return Err(bad("no puedes borrar tu propio usuario"));
    }
    let target = users()?.into_iter().find(|u| u.id == id).ok_or(store::Error::NotFound)?;
    keep_an_admin(&target)?;
    db::with(|c| c.execute("DELETE FROM users WHERE id=?", [id])).map_err(store::Error::from)?;
    store::audit(Some(&a), "usuario borrado", &target.username, "");
    Ok(StatusCode::NO_CONTENT)
}
