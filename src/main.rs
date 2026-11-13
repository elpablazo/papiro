//! Papiro: archivo de documentos con OCR en un solo binario.
mod api;
mod auth;
mod backup;
mod db;
mod import;
mod ingest;
mod llm;
mod mail;
mod mcp;
mod store;
mod suggest;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

pub struct Llm {
    /// anthropic | openai (cualquier API compatible con /chat/completions: OpenAI, Ollama, OpenRouter…)
    pub provider: String,
    pub key: String,
    pub base_url: String,
    pub model: String,
    /// Clasificar y resumir automáticamente cada documento nuevo
    pub auto: bool,
}

pub struct Imap {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub folder: String,
    pub interval: u64,
}

pub struct Config {
    pub data: PathBuf,
    pub consume: Option<PathBuf>,
    pub consume_interval: u64,
    pub listen: String,
    pub workers: usize,
    pub ocr_threads: usize,
    pub ocr_langs: String,
    pub ocr_dpi: u32,
    pub ocr_args: Vec<String>,
    /// Detectar y corregir páginas giradas (necesita los datos `osd` de Tesseract)
    pub ocr_rotate: bool,
    pub ocr_rotate_conf: f32,
    /// Confianza media de OCR por debajo de la cual se comprueba si la página está girada
    pub ocr_rotate_below: f32,
    pub cmd_timeout: u64,
    pub max_upload: usize,
    pub gotenberg: Option<String>,
    pub llm: Option<Llm>,
    pub imap: Option<Imap>,
    pub webhook_private: bool,
    /// Respaldo automático a esta carpeta (otro volumen o disco)
    pub backup_dir: Option<PathBuf>,
    pub backup_hours: u64,
    /// Copias diarias de la base que conserva el respaldo (0 = solo la más reciente)
    pub backup_keep: u64,
    /// Cada cuántas horas se verifica la integridad de todos los documentos (0 = nunca)
    pub verify_hours: u64,
    /// Días tras los que la papelera se vacía sola (0 = nunca)
    pub trash_days: i64,
}

pub fn env(key: &str) -> Option<String> {
    std::env::var(format!("PAPIRO_{key}")).ok().filter(|v| !v.trim().is_empty())
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

static CFG: OnceLock<Config> = OnceLock::new();
pub fn cfg() -> &'static Config {
    CFG.get_or_init(|| {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        Config {
            data: env("DATA").unwrap_or_else(|| "./data".into()).into(),
            consume: env("CONSUME").map(PathBuf::from),
            consume_interval: env_or("CONSUME_INTERVAL", 5),
            listen: env("LISTEN").unwrap_or_else(|| "[::]:8000".into()),
            workers: env_or("WORKERS", 1).max(1),
            ocr_threads: env_or("OCR_THREADS", cpus.min(2)).max(1),
            ocr_langs: env("OCR_LANGS").unwrap_or_else(|| "spa+eng".into()),
            ocr_dpi: env_or("OCR_DPI", 300),
            ocr_args: env("OCR_ARGS").map(|a| a.split_whitespace().map(String::from).collect()).unwrap_or_default(),
            ocr_rotate: env_or("OCR_ROTATE", true),
            ocr_rotate_conf: env_or("OCR_ROTATE_CONFIDENCE", 10.0),
            ocr_rotate_below: env_or("OCR_ROTATE_BELOW", 75.0),
            cmd_timeout: env_or("CMD_TIMEOUT", 600),
            max_upload: env_or("MAX_UPLOAD_MB", 512) << 20,
            gotenberg: env("GOTENBERG_URL"),
            llm: env("LLM_API_KEY").map(|key| {
                let provider = env("LLM_PROVIDER").unwrap_or_else(|| "anthropic".into());
                let anthropic = provider == "anthropic";
                Llm {
                    key,
                    base_url: env("LLM_BASE_URL")
                        .unwrap_or_else(|| if anthropic { "https://api.anthropic.com" } else { "https://api.openai.com/v1" }.into()),
                    model: env("LLM_MODEL").unwrap_or_else(|| if anthropic { "claude-opus-5-5" } else { "gpt-4o-mini" }.into()),
                    auto: env_or("LLM_AUTO", false),
                    provider,
                }
            }),
            imap: env("IMAP_HOST").map(|host| Imap {
                host,
                port: env_or("IMAP_PORT", 993),
                user: env("IMAP_USER").unwrap_or_default(),
                password: env("IMAP_PASSWORD").unwrap_or_default(),
                folder: env("IMAP_FOLDER").unwrap_or_else(|| "INBOX".into()),
                interval: env_or("IMAP_INTERVAL", 300),
            }),
            webhook_private: env_or("WEBHOOK_ALLOW_PRIVATE", false),
            backup_dir: env("BACKUP_DIR").map(PathBuf::from),
            backup_hours: env_or("BACKUP_HOURS", 24).max(1),
            backup_keep: env_or("BACKUP_KEEP", 14),
            verify_hours: env_or("VERIFY_HOURS", 24),
            trash_days: env_or("TRASH_DAYS", 0),
        }
    })
}

/// Cliente HTTP saliente compartido (webhooks, LLM, Gotenberg).
pub fn http() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(120))).build().into())
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("getrandom");
    hex::encode(buf)
}

// ───────────────────────── interfaz web embebida ─────────────────────────

const ASSETS: [(&str, &str, &[u8]); 12] = [
    ("index.html", "text/html; charset=utf-8", include_bytes!("../web/index.html")),
    ("app.js", "text/javascript; charset=utf-8", include_bytes!("../web/app.js")),
    ("app.css", "text/css; charset=utf-8", include_bytes!("../web/app.css")),
    ("preact.js", "text/javascript; charset=utf-8", include_bytes!("../web/preact.js")),
    ("sw.js", "text/javascript; charset=utf-8", include_bytes!("../web/sw.js")),
    ("manifest.webmanifest", "application/manifest+json", include_bytes!("../web/manifest.webmanifest")),
    ("icon.svg", "image/svg+xml", include_bytes!("../web/icon.svg")),
    ("icon-192.png", "image/png", include_bytes!("../web/icon-192.png")),
    ("icon-512.png", "image/png", include_bytes!("../web/icon-512.png")),
    ("icon-maskable.png", "image/png", include_bytes!("../web/icon-maskable.png")),
    ("apple-touch-icon.png", "image/png", include_bytes!("../web/apple-touch-icon.png")),
    ("og.png", "image/png", include_bytes!("../web/og.png")),
];

/// Link previews need absolute URLs: `%ORIGIN%` in index.html becomes this server's public origin.
/// Only a well-formed host is used, so a crafted `Host` header can't inject markup.
fn with_origin(page: &[u8], headers: &axum::http::HeaderMap) -> Vec<u8> {
    let get = |h: &str| headers.get(h).and_then(|v| v.to_str().ok()).map(str::trim);
    let host = get("x-forwarded-host").or_else(|| get("host")).filter(|h| {
        !h.is_empty() && h.len() <= 253 && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
    });
    let proto = match get("x-forwarded-proto") {
        Some("https") => "https",
        _ => "http",
    };
    let origin = host.map(|h| format!("{proto}://{h}")).unwrap_or_default();
    String::from_utf8_lossy(page).replace("%ORIGIN%", &origin).into_bytes()
}

async fn asset(uri: Uri, headers: axum::http::HeaderMap) -> Response {
    // /assets/app.js y /sw.js (el service worker tiene que vivir en la raíz para cubrir toda la app).
    let name = uri.path().trim_start_matches("/assets/").trim_start_matches('/');
    // Cualquier otra ruta es de la SPA: devuelve index.html
    let i = ASSETS.iter().position(|a| a.0 == name).unwrap_or(0);
    let (name, mime, bytes) = ASSETS[i];
    // PAPIRO_WEB_DIR sirve la interfaz desde disco (para desarrollarla sin recompilar)
    if let Some(disk) = env("WEB_DIR").and_then(|d| std::fs::read(PathBuf::from(d).join(name)).ok()) {
        let body = if i == 0 { with_origin(&disk, &headers) } else { disk };
        return ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")], body).into_response();
    }
    // The page depends on the host it's served from, so it gets no ETag (it's tiny anyway).
    if i == 0 {
        return ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")], with_origin(bytes, &headers)).into_response();
    }
    // El navegador revalida siempre, pero solo vuelve a descargar lo que cambió.
    static ETAGS: OnceLock<Vec<String>> = OnceLock::new();
    let etag = &ETAGS.get_or_init(|| ASSETS.iter().map(|a| format!("\"{}\"", &hex::encode(<sha2::Sha256 as sha2::Digest>::digest(a.2))[..16])).collect())[i];
    if headers.get(header::IF_NONE_MATCH).is_some_and(|v| v.as_bytes() == etag.as_bytes()) {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())]).into_response();
    }
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache"), (header::ETAG, etag.as_str())], bytes.to_vec()).into_response()
}

async fn security_headers(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    h.insert("X-Frame-Options", HeaderValue::from_static("SAMEORIGIN"));
    h.insert("Referrer-Policy", HeaderValue::from_static("same-origin"));
    h.entry(header::CONTENT_SECURITY_POLICY).or_insert(HeaderValue::from_static(
        "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; frame-src 'self' blob:; object-src 'self'; frame-ancestors 'self'",
    ));
    res
}

/// Registro de peticiones: todo lo que modifica, falla o tarda. Las lecturas normales no se registran.
async fn log_requests(req: Request, next: Next) -> Response {
    let (method, path, t0) = (req.method().clone(), req.uri().path().to_string(), Instant::now());
    let res = next.run(req).await;
    let (status, ms) = (res.status(), t0.elapsed().as_millis());
    let read = method == axum::http::Method::GET || method == axum::http::Method::HEAD;
    if path.starts_with("/api") && path != "/api/health" && (!read || status.is_server_error() || status == StatusCode::FORBIDDEN || ms > 2000) {
        println!("{method} {path} → {} in {ms} ms", status.as_u16());
    }
    res
}

/// Salud real: responde 503 si la base de datos no contesta.
async fn health() -> Response {
    match db::with(|c| c.query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))) {
        Ok(_) => "ok".into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, format!("database: {e}")).into_response(),
    }
}

fn app() -> Router {
    use tower_http::compression::{CompressionLayer, predicate::Predicate};
    let (api, mut openapi) = api::router();
    openapi.info.version = env!("CARGO_PKG_VERSION").into();
    let spec = serde_json::to_string(&openapi).expect("openapi");
    let textual = |_: StatusCode, _: axum::http::Version, h: &axum::http::HeaderMap, _: &axum::http::Extensions| {
        let ct = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        ["text/html", "text/css", "text/javascript", "json"].iter().any(|t| ct.contains(t))
    };
    Router::new()
        .merge(api)
        .route("/api/openapi.json", get(move |_u: auth::AuthUser| async move { ([(header::CONTENT_TYPE, "application/json")], spec) }))
        .route("/api/health", get(health))
        .route("/mcp", post(mcp::handle).get(|| async { StatusCode::METHOD_NOT_ALLOWED }))
        .fallback(asset)
        .layer(DefaultBodyLimit::max(cfg().max_upload))
        .layer(CompressionLayer::new().compress_when(tower_http::compression::predicate::SizeAbove::new(512).and(textual)))
        .layer(axum::middleware::map_response(security_headers))
        .layer(axum::middleware::from_fn(log_requests))
}

const USAGE: &str = "usage: papiro [serve]
       papiro import <Paperless export dir>       import a Paperless-ngx `document_exporter` export
       papiro backup <dir>                        consistent, incremental backup while running
       papiro restore <dir> [YYYY-MM-DD]          restore into an empty data dir (optionally that day's database)
       papiro verify [--quick]                    check every document against its files and SHA-256
       papiro export <dir>                        human-readable files + manifest.json, no Papiro needed
       papiro purge --all                         delete ALL documents (keeps tags, folders, users, settings)
       PAPIRO_NEW_PASSWORD=… papiro passwd <user>";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let c = cfg();
    let (cmd, arg) = (args.first().map(String::as_str).unwrap_or("serve"), args.get(1).map(Path::new));
    if cmd == "restore" {
        // Antes de abrir (y crear) la base: se restaura solo sobre una carpeta de datos vacía.
        match arg.ok_or(USAGE.to_string()).and_then(|src| backup::restore(src, &c.data, args.get(2).map(String::as_str))) {
            Ok(msg) => println!("{msg}"),
            Err(e) => fail(&e),
        }
    }
    for d in ["docs", "tmp"] {
        std::fs::create_dir_all(c.data.join(d)).unwrap_or_else(|e| fail(&format!("cannot create {}: {e}", c.data.join(d).display())));
    }
    let applied = db::init(c.data.join("papiro.db")).unwrap_or_else(|e| fail(&format!("cannot open the database: {e}")));
    if applied.contains(&2) {
        let fixed = store::repair_layout();
        if fixed > 0 {
            println!("migration: renamed {fixed} originals so they can be opened and downloaded");
        }
    }
    if applied.contains(&4) {
        println!("migration: analyzed {} documents (expiry dates, amounts and identifiers)", suggest::backfill());
    }
    // Las miniaturas hechas por versiones sin fuentes instaladas podían salir en blanco: se descartan
    // una vez y se regeneran solas la próxima vez que se piden.
    if store::meta_get("thumbs").as_deref() != Some("2") {
        let n = std::fs::read_dir(c.data.join("docs")).into_iter().flatten().flatten().filter(|e| std::fs::remove_file(e.path().join("thumb.jpg")).is_ok()).count();
        store::meta_set("thumbs", "2");
        if n > 0 {
            println!("migration: {n} thumbnails will be regenerated with the correct fonts");
        }
    }
    let report = |r: Result<String, String>| match r {
        Ok(msg) => println!("{msg}"),
        Err(e) => fail(&e),
    };
    match (cmd, arg) {
        ("serve", _) => serve(),
        ("import", Some(dir)) => {
            db::bulk_mode();
            report(import::run(dir));
            db::checkpoint();
        }
        ("backup", Some(dir)) => report(backup::backup(dir)),
        ("export", Some(dir)) => report(backup::export(dir)),
        ("purge", _) if matches!(args.get(1).map(String::as_str), Some("--all" | "--todo")) => {
            // Para empezar de cero antes de una migración. El respaldo conserva los archivos y las copias diarias de la base.
            let docs: Vec<(i64, String)> =
                db::with(|c| c.prepare("SELECT id, title FROM documents ORDER BY id")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).unwrap_or_else(|e| fail(&e.to_string()));
            for (id, title) in &docs {
                store::purge(*id).unwrap_or_else(|e| fail(&format!("document {id}: {e}")));
                store::audit_raw(None, "console", "permanently deleted from the console", &format!("document {id}"), title, "");
                println!("deleted #{id} \"{title}\"");
            }
            println!("{} documents deleted; tags, folders, users and settings were kept", docs.len());
        }
        ("restore", _) | ("verify", _) => {
            let r = store::verify(!matches!(args.get(1).map(String::as_str), Some("--quick" | "--rapido")));
            println!("{} documents checked in {} s · database: {} · search index: {}", r.documents, r.seconds, r.database, if r.search_index_ok { "ok" } else { "DAMAGED" });
            for p in &r.problems {
                println!("  PROBLEM document {} \"{}\": {}", p.document_id, p.title, p.problem);
            }
            if !r.problems.is_empty() || r.database != "ok" || !r.search_index_ok {
                fail(&format!("{} problems found", r.problems.len()));
            }
            println!("no problems");
        }
        ("passwd", Some(user)) => match env("NEW_PASSWORD") {
            Some(pw) => report(auth::set_password(&user.to_string_lossy(), &pw).map(|_| "password updated".into()).map_err(|e| e.to_string())),
            None => fail(USAGE),
        },
        _ => fail(USAGE),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// Tareas periódicas: limpieza, respaldo automático y verificación de integridad.
fn maintenance() {
    let c = cfg();
    let hours = |h: u64| Duration::from_secs(h * 3600);
    // La primera pasada ocurre a los 10 minutos de arrancar; después, cada intervalo.
    let soon = |every: Duration| Instant::now().checked_sub(every).map(|t| t + Duration::from_secs(600)).unwrap_or_else(Instant::now);
    let (mut last_backup, mut last_verify, mut last_clean) = (soon(hours(c.backup_hours)), soon(hours(c.verify_hours.max(1))), Instant::now());
    loop {
        std::thread::sleep(Duration::from_secs(60));
        if last_clean.elapsed() > hours(1) {
            last_clean = Instant::now();
            let _ = db::with(|c| {
                c.execute_batch(
                    "DELETE FROM sessions WHERE expires < datetime('now');
                     DELETE FROM jobs WHERE status IN ('done','duplicate') AND finished < datetime('now','-30 days');",
                )
            });
            if c.trash_days > 0 {
                let old: Vec<i64> = db::with(|db| {
                    db.prepare("SELECT id FROM documents WHERE deleted_at < datetime('now', '-' || ? || ' days')")?.query_map([c.trash_days], |r| r.get(0))?.collect()
                })
                .unwrap_or_default();
                for id in old {
                    if store::purge(id).is_ok() {
                        store::audit(None, "trash emptied", &format!("document {id}"), &format!("more than {} days in the trash", c.trash_days));
                    }
                }
            }
        }
        if let Some(dir) = c.backup_dir.as_ref().filter(|_| last_backup.elapsed() > hours(c.backup_hours)) {
            last_backup = Instant::now();
            match backup::backup(dir) {
                Ok(msg) => println!("automatic backup: {msg}"),
                Err(e) => {
                    eprintln!("AUTOMATIC BACKUP FAILED: {e}");
                    store::meta_set("backup", &serde_json::json!({"ok": false, "error": e, "dest": dir.to_string_lossy()}).to_string());
                }
            }
        }
        if c.verify_hours > 0 && last_verify.elapsed() > hours(c.verify_hours) {
            last_verify = Instant::now();
            let r = store::verify(true);
            if r.problems.is_empty() && r.database == "ok" && r.search_index_ok {
                println!("integrity: {} documents verified in {} s, no problems", r.documents, r.seconds);
            } else {
                eprintln!("INTEGRITY: {} problems (database: {}, search index ok: {})", r.problems.len(), r.database, r.search_index_ok);
                for p in r.problems.iter().take(20) {
                    eprintln!("  document {} \"{}\": {}", p.document_id, p.title, p.problem);
                }
            }
        }
    }
}

fn serve() {
    let c = cfg();
    auth::bootstrap_admin();
    let _ = db::with(|c| c.execute("DELETE FROM sessions WHERE expires < datetime('now')", []));
    ingest::start_workers();
    mail::start();
    std::thread::spawn(store::outbox_worker);
    std::thread::spawn(maintenance);
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).max_blocking_threads(16).enable_all().build().expect("tokio");
    rt.block_on(async {
        // [::] atiende IPv6 e IPv4 a la vez; si el host no tiene IPv6 se cae a 0.0.0.0.
        let listener = match tokio::net::TcpListener::bind(&c.listen).await {
            Ok(l) => l,
            Err(_) if c.listen.starts_with("[::]") => tokio::net::TcpListener::bind(c.listen.replace("[::]", "0.0.0.0")).await.unwrap_or_else(|e| fail(&format!("could not listen: {e}"))),
            Err(e) => fail(&format!("could not listen on {}: {e}", c.listen)),
        };
        println!(
            "papiro {} listening on http://{} · schema {} · OCR {} ({} threads, orientation correction {}) · automatic backup {}",
            env!("CARGO_PKG_VERSION"),
            c.listen,
            db::schema_version(),
            c.ocr_langs,
            c.ocr_threads,
            if ingest::rotate_on() { "on" } else { "off" },
            c.backup_dir.as_ref().map(|d| format!("every {} h to {}", c.backup_hours, d.display())).unwrap_or_else(|| "off".into()),
        );
        let shutdown = async {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            // Las conexiones SSE no se cierran solas: tras 2 s de gracia se sale igual.
            std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(2)); std::process::exit(0) });
        };
        axum::serve(listener, app().into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(shutdown).await.expect("server");
    });
}
