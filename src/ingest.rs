//! Cola de ingesta acotada y pipeline de extracción / OCR.
//!
//! La RAM se mantiene plana por diseño: los archivos van a disco (nunca a memoria), solo corren
//! `PAPIRO_WORKERS` documentos a la vez y cada documento se procesa página por página con
//! `PAPIRO_OCR_THREADS` subprocesos de tesseract como máximo.
//!
//! Custodia: un documento solo existe cuando su fila y sus archivos están completos. Los archivos se
//! preparan en una carpeta temporal, se verifica el SHA-256 de la copia y la carpeta se mueve a su
//! sitio (rename atómico) dentro de la misma transacción que inserta la fila. La subida original no
//! se borra hasta entonces, y si el trabajo falla se conserva para reintentar.
use crate::{cfg, db, store};
use regex::Regex;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Condvar, Mutex, atomic::AtomicUsize, atomic::Ordering},
    time::{Duration, Instant},
};

pub enum JobErr {
    Duplicate(i64),
    /// Fallo que puede ser pasajero: se reintenta una vez solo.
    Failed(String),
    /// Fallo que no cambia al reintentar (tipo no soportado, archivo inexistente).
    Unsupported(String),
}
impl<E: std::fmt::Display> From<E> for JobErr {
    fn from(e: E) -> Self {
        JobErr::Failed(e.to_string())
    }
}
type R<T> = Result<T, JobErr>;

const MAX_ATTEMPTS: i64 = 2;
static WAKE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let (mut f, mut h, mut buf) = (fs::File::open(path)?, Sha256::new(), vec![0u8; 1 << 16]);
    loop {
        match f.read(&mut buf)? {
            0 => return Ok(hex::encode(h.finalize())),
            n => h.update(&buf[..n]),
        }
    }
}

/// Encola un archivo ya guardado en disco. `meta` admite title, created, correspondent_id, doctype_id, tags, is_private.
pub fn enqueue(path: &Path, filename: &str, source: &str, meta: &Value, user_id: Option<i64>) -> store::Result<i64> {
    let id = db::with(|c| {
        c.query_row(
            "INSERT INTO jobs(path, filename, source, meta, user_id) VALUES(?,?,?,?,?) RETURNING id",
            params![path.to_string_lossy(), filename, source, meta.to_string(), user_id],
            |r| r.get(0),
        )
    })?;
    notify_job(id);
    WAKE.1.notify_one();
    Ok(id)
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone)]
pub struct Job {
    pub id: i64,
    pub filename: String,
    /// upload | consume | mail | mcp
    pub source: String,
    /// queued | running | done | duplicate | failed
    pub status: String,
    pub error: Option<String>,
    /// Documento resultante (o el original, si fue duplicado)
    pub doc_id: Option<i64>,
    pub created: String,
    pub finished: Option<String>,
    pub attempts: i64,
    /// El archivo se conservó y el trabajo se puede reintentar con `POST /api/jobs/{id}/retry`
    pub can_retry: bool,
    #[serde(skip)]
    pub user_id: Option<i64>,
    #[serde(skip)]
    pub path: String,
}

pub fn jobs(where_sql: &str, arg: i64) -> store::Result<Vec<Job>> {
    Ok(db::with(|c| {
        c.prepare_cached(&format!(
            "SELECT id, filename, source, status, error, doc_id, created, finished, attempts, user_id, path FROM jobs WHERE {where_sql} ORDER BY id DESC LIMIT 100"
        ))?
        .query_map([arg], |r| {
            let (status, path): (String, String) = (r.get(3)?, r.get(10)?);
            Ok(Job {
                id: r.get(0)?,
                filename: r.get(1)?,
                source: r.get(2)?,
                can_retry: status == "failed" && Path::new(&path).is_file(),
                status,
                error: r.get(4)?,
                doc_id: r.get(5)?,
                created: r.get(6)?,
                finished: r.get(7)?,
                attempts: r.get(8)?,
                user_id: r.get(9)?,
                path,
            })
        })?
        .collect()
    })?)
}

pub fn job(id: i64) -> store::Result<Job> {
    jobs("id=?", id)?.pop().ok_or(store::Error::NotFound)
}

fn notify_job(id: i64) {
    if let Ok(j) = job(id) {
        store::notify(json!({"type": "job", "user_id": j.user_id, "job": j}));
    }
}

/// Vuelve a encolar un trabajo fallido cuyo archivo se conservó.
pub fn retry(id: i64) -> store::Result<Job> {
    let j = job(id)?;
    if !j.can_retry {
        return Err(store::Error::Bad("este trabajo no se puede reintentar (no falló o su archivo ya no existe)".into()));
    }
    db::with(|c| c.execute("UPDATE jobs SET status='queued', error=NULL, attempts=0, finished=NULL WHERE id=?", [id]))?;
    notify_job(id);
    WAKE.1.notify_one();
    job(id)
}

/// Descarta un trabajo terminado; si falló, borra también el archivo que se había conservado.
pub fn dismiss(id: i64) -> store::Result<()> {
    let j = job(id)?;
    if j.status == "queued" || j.status == "running" {
        return Err(store::Error::Bad("el trabajo sigue en curso".into()));
    }
    if j.status == "failed" && Path::new(&j.path).starts_with(cfg().data.join("failed")) {
        let _ = fs::remove_file(&j.path);
    }
    db::with(|c| c.execute("DELETE FROM jobs WHERE id=?", [id]))?;
    Ok(())
}

fn tmp() -> PathBuf {
    cfg().data.join("tmp")
}

/// Limpieza de arranque: restos de trabajos interrumpidos y carpetas de documentos sin fila.
/// Nada que pueda ser un documento se borra: lo huérfano se aparta en `orphans/`.
pub fn cleanup() {
    let live: HashSet<String> =
        db::with(|c| c.prepare("SELECT path FROM jobs WHERE status IN ('queued','running','failed')")?.query_map([], |r| r.get(0))?.collect()).unwrap_or_default();
    for e in fs::read_dir(tmp()).into_iter().flatten().flatten() {
        let (path, name) = (e.path(), e.file_name().to_string_lossy().to_string());
        if name.starts_with("work-") || name.starts_with("stage-") {
            let _ = fs::remove_dir_all(&path);
        } else if name.starts_with("up-") && !live.contains(path.to_string_lossy().as_ref()) {
            let _ = fs::remove_file(&path);
        }
    }
    let ids: HashSet<String> = db::with(|c| c.prepare("SELECT id FROM documents")?.query_map([], |r| r.get::<_, i64>(0).map(|i| i.to_string()))?.collect()).unwrap_or_default();
    for e in fs::read_dir(cfg().data.join("docs")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !ids.contains(&name) {
            set_aside(&e.path(), &name);
        }
    }
}

fn set_aside(dir: &Path, name: &str) {
    let dest = cfg().data.join("orphans");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if fs::create_dir_all(&dest).and_then(|_| fs::rename(dir, dest.join(format!("{name}-{stamp}")))).is_ok() {
        println!("integridad: la carpeta docs/{name} no corresponde a ningún documento; se apartó en orphans/");
    }
}

pub fn start_workers() {
    // Lo que quedó a medias en un reinicio vuelve a la cola.
    let _ = db::with(|c| c.execute("UPDATE jobs SET status='queued' WHERE status='running'", []));
    cleanup();
    for _ in 0..cfg().workers {
        std::thread::spawn(worker);
    }
    if let Some(dir) = cfg().consume.clone() {
        std::thread::spawn(move || watch(dir));
    }
}

struct Claimed {
    id: i64,
    path: String,
    filename: String,
    source: String,
    attempts: i64,
    doc_id: Option<i64>,
    user_id: Option<i64>,
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect()
}

fn worker() {
    loop {
        let claimed = db::with(|c| {
            c.query_row(
                "UPDATE jobs SET status='running', started=datetime('now'), attempts=attempts+1
                 WHERE id=(SELECT id FROM jobs WHERE status='queued' ORDER BY id LIMIT 1)
                 RETURNING id, path, filename, source, meta, attempts, doc_id, user_id",
                [],
                |r| {
                    Ok((
                        Claimed { id: r.get(0)?, path: r.get(1)?, filename: r.get(2)?, source: r.get(3)?, attempts: r.get(5)?, doc_id: r.get(6)?, user_id: r.get(7)? },
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
        });
        let Ok(Some((j, meta))) = claimed else {
            let guard = WAKE.0.lock().unwrap();
            let _ = WAKE.1.wait_timeout(guard, Duration::from_secs(5));
            continue;
        };
        notify_job(j.id);
        let t0 = Instant::now();
        let path = PathBuf::from(&j.path);
        let work = tmp().join(format!("work-{}", j.id));
        let meta: Value = serde_json::from_str(&meta).unwrap_or(json!({}));
        // Si el proceso murió después de guardar el documento pero antes de cerrar el trabajo, ya está hecho.
        let finished_before = j.doc_id.filter(|d| store::exists(*d));
        let res = match finished_before {
            Some(doc) => Ok(doc),
            None => fs::create_dir_all(&work).map_err(JobErr::from).and_then(|_| process(&j, &path, &meta, &work)),
        };
        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(tmp().join(format!("stage-{}", j.id)));
        let (status, err, doc) = match &res {
            Ok(doc) => ("done", None, Some(*doc)),
            Err(JobErr::Duplicate(doc)) => ("duplicate", Some(format!("duplicado del documento #{doc}")), Some(*doc)),
            Err(JobErr::Failed(e)) if j.attempts < MAX_ATTEMPTS && path.exists() => ("queued", Some(e.clone()), None),
            Err(JobErr::Failed(e)) | Err(JobErr::Unsupported(e)) => ("failed", Some(e.clone()), None),
        };
        let mut kept = j.path.clone();
        match status {
            "done" | "duplicate" => drop(fs::remove_file(&path)),
            "failed" if path.exists() => {
                // El archivo de un trabajo fallido se conserva: se puede reintentar o recuperar a mano.
                let keep = if j.source == "consume" {
                    path.parent().unwrap_or(Path::new(".")).join(".failed").join(path.file_name().unwrap_or_default())
                } else {
                    cfg().data.join("failed").join(format!("{}-{}", j.id, safe_name(&j.filename)))
                };
                let moved = keep.parent().map(fs::create_dir_all).transpose().is_ok()
                    && fs::rename(&path, &keep).or_else(|_| fs::copy(&path, &keep).and_then(|_| fs::remove_file(&path))).is_ok();
                if moved {
                    kept = keep.to_string_lossy().into_owned();
                }
            }
            "queued" => std::thread::sleep(Duration::from_secs(2)),
            _ => {}
        }
        if let (Ok(doc), true, None) = (&res, cfg().llm.as_ref().is_some_and(|l| l.auto), finished_before) {
            if let Err(e) = crate::llm::enrich(*doc) {
                eprintln!("llm: documento {doc}: {e}");
            }
        }
        let _ = db::with(|c| {
            c.execute(
                "UPDATE jobs SET status=?1, error=?2, doc_id=coalesce(?3, doc_id), path=?4,
                   finished=CASE WHEN ?1='queued' THEN NULL ELSE datetime('now') END WHERE id=?5",
                params![status, err, doc, kept, j.id],
            )
        });
        println!(
            "trabajo {} «{}» ({}): {} en {} ms{}",
            j.id,
            j.filename,
            j.source,
            match status {
                "queued" => "se reintentará",
                s => s,
            },
            t0.elapsed().as_millis(),
            err.as_deref().map(|e| format!(" — {}", e.lines().next().unwrap_or(""))).unwrap_or_default()
        );
        if let (Ok(doc), None) = (res, finished_before) {
            store::emit("document.created", doc);
        }
        notify_job(j.id);
        if status == "queued" {
            WAKE.1.notify_one();
        }
    }
}

/// Carpeta vigilada por sondeo: un archivo entra cuando su tamaño y fecha dejan de cambiar.
// ponytail: sondeo cada 5 s en vez de inotify (funciona igual en NFS/SMB y bind mounts); cambiar si la latencia importa.
fn watch(dir: PathBuf) {
    let mut seen: HashMap<PathBuf, (u64, std::time::SystemTime, bool)> = HashMap::new();
    loop {
        let mut files = vec![];
        walk(&dir, &mut files);
        seen.retain(|p, _| files.iter().any(|(f, _, _)| f == p));
        for (path, size, mtime) in files {
            let entry = seen.entry(path.clone()).or_insert((u64::MAX, mtime, false));
            if (entry.0, entry.1) != (size, mtime) {
                *entry = (size, mtime, false);
            } else if !entry.2 {
                entry.2 = true;
                let active = db::with(|c| {
                    c.query_row("SELECT 1 FROM jobs WHERE path=? AND status IN ('queued','running')", [path.to_string_lossy()], |_| Ok(()))
                        .optional()
                });
                if matches!(active, Ok(None)) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    // consume/Casa/Recibos/luz.pdf se guarda en la carpeta «Casa/Recibos».
                    let sub = path.parent().and_then(|p| p.strip_prefix(&dir).ok()).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                    let meta = if sub.is_empty() { json!({}) } else { json!({"folder": sub}) };
                    let _ = enqueue(&path, &name, "consume", &meta, None);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(cfg().consume_interval));
    }
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, std::time::SystemTime)>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let (path, Ok(md)) = (e.path(), e.metadata()) else { continue };
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if md.is_dir() {
            walk(&path, out);
        } else if md.len() > 0 {
            out.push((path, md.len(), md.modified().unwrap_or(std::time::UNIX_EPOCH)));
        }
    }
}

// ───────────────────────── pipeline ─────────────────────────

struct Extracted {
    content: String,
    pages: i64,
    archive: Option<PathBuf>,
    /// PDF del que sale la miniatura
    thumb_src: Option<PathBuf>,
}

/// Comprueba al recibir una subida si el tipo de archivo se puede procesar.
pub fn supported(path: &Path, filename: &str) -> Result<(), String> {
    match detect(path, filename) {
        Ok(_) => Ok(()),
        Err(JobErr::Unsupported(e) | JobErr::Failed(e)) => Err(e),
        Err(JobErr::Duplicate(_)) => Ok(()),
    }
}

#[derive(PartialEq)]
enum Kind {
    Pdf,
    Image,
    Office,
    Text,
}

fn detect(path: &Path, filename: &str) -> R<(Kind, String, &'static str)> {
    let mut head = [0u8; 12];
    let n = fs::File::open(path)?.read(&mut head)?;
    let head = &head[..n];
    let ext = Path::new(filename).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let img = |e: &str, m| Ok((Kind::Image, e.to_string(), m));
    if head.starts_with(b"%PDF") {
        return Ok((Kind::Pdf, "pdf".into(), "application/pdf"));
    } else if head.starts_with(b"\x89PNG") {
        return img("png", "image/png");
    } else if head.starts_with(b"\xff\xd8") {
        return img("jpg", "image/jpeg");
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        return img("tiff", "image/tiff");
    } else if head.starts_with(b"RIFF") && head.ends_with(b"WEBP") {
        return img("webp", "image/webp");
    } else if head.starts_with(b"GIF8") {
        return img("gif", "image/gif");
    }
    let office = [
        ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        ("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        ("odt", "application/vnd.oasis.opendocument.text"),
        ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
        ("odp", "application/vnd.oasis.opendocument.presentation"),
        ("doc", "application/msword"),
        ("xls", "application/vnd.ms-excel"),
        ("ppt", "application/vnd.ms-powerpoint"),
        ("rtf", "application/rtf"),
    ];
    if let Some((e, m)) = office.iter().find(|(e, _)| *e == ext) {
        return Ok((Kind::Office, e.to_string(), m));
    }
    let text = [("txt", "text/plain"), ("md", "text/markdown"), ("csv", "text/csv")];
    if let Some((e, m)) = text.iter().find(|(e, _)| *e == ext) {
        return Ok((Kind::Text, e.to_string(), m));
    }
    Err(JobErr::Unsupported(format!("tipo de archivo no soportado: {filename}")))
}

/// Ejecuta un programa externo con tope de tiempo; devuelve stdout o el final de stderr como error.
/// El proceso hijo se espera siempre (también si se agota el tiempo), así que no deja zombis.
fn run(prog: &str, args: &[&str]) -> R<String> {
    let mut child = Command::new(prog)
        .args(args)
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar {prog}: {e}"))?;
    let drain = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    };
    let (out, err) = (drain(Box::new(child.stdout.take().expect("stdout"))), drain(Box::new(child.stderr.take().expect("stderr"))));
    let deadline = Instant::now() + Duration::from_secs(cfg().cmd_timeout);
    let mut nap = 1;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(nap));
                nap = (nap * 2).min(20);
            }
            other => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = (out.join(), err.join());
                return Err(JobErr::Failed(match other {
                    Err(e) => format!("{prog}: {e}"),
                    _ => format!("{prog}: tiempo agotado ({} s)", cfg().cmd_timeout),
                }));
            }
        }
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if status.success() {
        return Ok(String::from_utf8_lossy(&out).into_owned());
    }
    let err = String::from_utf8_lossy(&err);
    let tail: String = err.trim().chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
    Err(JobErr::Failed(format!("{prog}: {tail}")))
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

pub fn pdf_pages(pdf: &Path) -> Result<i64, JobErr> {
    let info = run("pdfinfo", &[s(pdf)])?;
    Ok(info.lines().find_map(|l| l.strip_prefix("Pages:")).and_then(|v| v.trim().parse().ok()).unwrap_or(0))
}

/// Resolución nativa de las imágenes de cada página (para no rasterizar de más).
fn native_dpi(pdf: &Path) -> HashMap<usize, u32> {
    let mut map = HashMap::new();
    for l in run("pdfimages", &["-list", s(pdf)]).unwrap_or_default().lines().skip(2) {
        let c: Vec<&str> = l.split_whitespace().collect();
        if let (Some(page), Some(ppi)) = (c.first().and_then(|p| p.parse().ok()), c.get(12).and_then(|p| p.parse::<u32>().ok())) {
            let e = map.entry(page).or_insert(0);
            *e = (*e).max(ppi);
        }
    }
    map
}

/// Grados que hay que girar la página (sentido horario) para que el texto quede derecho, según la
/// detección de orientación de Tesseract. 0 si no está activada, no hay confianza o hay poco texto.
fn rotation(img: &Path, dpi: Option<&str>) -> u32 {
    if !rotate_on() {
        return 0;
    }
    let mut args = vec![s(img), "stdout", "--psm", "0"];
    if let Some(d) = dpi {
        args.extend(["--dpi", d]);
    }
    let Ok(out) = run("tesseract", &args) else { return 0 };
    let field = |k: &str| out.lines().find_map(|l| l.strip_prefix(k)).and_then(|v| v.trim().parse::<f32>().ok());
    match (field("Rotate:"), field("Orientation confidence:")) {
        (Some(r), Some(c)) if c >= cfg().ocr_rotate_conf && [90.0, 180.0, 270.0].contains(&r) => r as u32,
        _ => 0,
    }
}

/// Confianza media (0–100) y número de palabras reconocidas, leídos del TSV de Tesseract.
fn confidence(tsv: &Path) -> (usize, f32) {
    let text = fs::read_to_string(tsv).unwrap_or_default();
    let confs: Vec<f32> = text.lines().skip(1).filter_map(|l| {
        let c: Vec<&str> = l.split('\t').collect();
        (c.first() == Some(&"5") && c.get(11).is_some_and(|w| w.trim().chars().count() > 1)).then(|| c.get(10)?.parse().ok()).flatten()
    }).collect();
    (confs.len(), if confs.is_empty() { 0.0 } else { confs.iter().sum::<f32>() / confs.len() as f32 })
}

/// OCR de una imagen. Devuelve los grados que la página estaba girada (0 casi siempre).
///
/// Detectar la orientación cuesta ~1 s por página, así que solo se hace cuando el primer OCR sale
/// con poca confianza o casi sin palabras (lo que pasa con una página de lado o de cabeza). En ese
/// caso se repite el reconocimiento dejando que Tesseract segmente con su detector de orientación.
fn ocr(img: &Path, base: &Path, fixed: &[&str], dpi: Option<&str>) -> R<u32> {
    let pass = |extra: &[&str]| -> R<()> {
        let mut args = vec![s(img), s(base), "-l", cfg().ocr_langs.as_str()];
        args.extend(fixed);
        args.extend(extra);
        args.extend(cfg().ocr_args.iter().map(String::as_str));
        args.extend(["txt", "pdf", "tsv"]);
        run("tesseract", &args).map(|_| ())
    };
    pass(&[])?;
    let (words, conf) = confidence(&base.with_extension("tsv"));
    if !rotate_on() || (words >= 8 && conf >= cfg().ocr_rotate_below) {
        return Ok(0);
    }
    let rot = rotation(img, dpi);
    if rot != 0 {
        pass(&["--psm", "1"])?;
    }
    Ok(rot)
}

fn process_pdf(src: &Path, work: &Path) -> R<Extracted> {
    let pages = pdf_pages(src)?;
    let txt = work.join("text.txt");
    run("pdftotext", &["-layout", "-enc", "UTF-8", s(src), s(&txt)])?;
    let mut texts: Vec<String> = fs::read_to_string(&txt).unwrap_or_default().split('\x0c').map(String::from).collect();
    texts.resize(pages as usize, String::new());
    // Solo se hace OCR a las páginas que no traen texto propio (página por página: un PDF mixto no pierde nada).
    let need: Vec<usize> =
        (1..=pages as usize).filter(|p| texts[p - 1].chars().filter(|c| c.is_alphanumeric()).count() < 20).collect();
    let mut archive = None;
    if !need.is_empty() {
        let dpis = native_dpi(src);
        let (next, results, error) = (AtomicUsize::new(0), Mutex::new(Vec::new()), Mutex::new(None::<JobErr>));
        std::thread::scope(|sc| {
            for _ in 0..cfg().ocr_threads.min(need.len()) {
                sc.spawn(|| {
                    while let Some(&p) = need.get(next.fetch_add(1, Ordering::Relaxed)) {
                        if error.lock().unwrap().is_some() {
                            return;
                        }
                        let dpi = dpis.get(&p).copied().unwrap_or(cfg().ocr_dpi).clamp(150, cfg().ocr_dpi).to_string();
                        let (base, n) = (work.join(format!("p{p}")), p.to_string());
                        let one = || -> R<(String, u32)> {
                            // PGM sin comprimir: codificar PNG de un escaneo con ruido costaba ~6 s por página.
                            run("pdftoppm", &["-r", &dpi, "-gray", "-f", &n, "-l", &n, "-singlefile", s(src), s(&base)])?;
                            let img = base.with_extension("pgm");
                            let rot = ocr(&img, &base, &["--dpi", dpi.as_str(), "-c", "textonly_pdf=1"], Some(&dpi))?;
                            let _ = fs::remove_file(&img);
                            Ok((fs::read_to_string(base.with_extension("txt"))?, rot))
                        };
                        match one() {
                            Ok((t, rot)) => results.lock().unwrap().push((p, t, rot)),
                            Err(e) => *error.lock().unwrap() = Some(e),
                        }
                    }
                });
            }
        });
        if let Some(e) = error.into_inner().unwrap() {
            return Err(e);
        }
        let mut turns = vec![];
        for (p, t, rot) in results.into_inner().unwrap() {
            texts[p - 1] = t;
            if rot != 0 {
                turns.push(format!("--rotate=+{rot}:{p}"));
            }
        }
        // PDF de archivo = original intacto + capa de texto invisible (sin recomprimir imágenes).
        let layers: Vec<String> = need.iter().map(|p| s(&work.join(format!("p{p}.pdf"))).to_string()).collect();
        let (layer, mut out) = (work.join("layer.pdf"), work.join("archive.pdf"));
        let mut args = vec!["--empty", "--pages"];
        args.extend(layers.iter().map(String::as_str));
        args.extend(["--", s(&layer)]);
        run("qpdf", &args)?;
        let to = format!("--to={}", need.iter().map(usize::to_string).collect::<Vec<_>>().join(","));
        run("qpdf", &[s(src), "--overlay", s(&layer), &to, "--", s(&out)])?;
        if !turns.is_empty() {
            // Las páginas escaneadas de lado o de cabeza se enderezan en el PDF de archivo.
            let upright = work.join("archive-upright.pdf");
            let mut args = vec![s(&out)];
            args.extend(turns.iter().map(String::as_str));
            args.extend(["--", s(&upright)]);
            run("qpdf", &args)?;
            out = upright;
        }
        archive = Some(out);
    }
    let content = texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    Ok(Extracted { content, pages, thumb_src: Some(archive.clone().unwrap_or_else(|| src.to_path_buf())), archive })
}

fn process_image(src: &Path, work: &Path) -> R<Extracted> {
    let base = work.join("ocr");
    let rot = ocr(src, &base, &[], None)?;
    let mut pdf = base.with_extension("pdf");
    if rot != 0 {
        let upright = work.join("ocr-upright.pdf");
        run("qpdf", &[s(&pdf), &format!("--rotate=+{rot}"), "--", s(&upright)])?;
        pdf = upright;
    }
    Ok(Extracted {
        content: fs::read_to_string(base.with_extension("txt"))?.trim().to_string(),
        pages: pdf_pages(&pdf)?,
        archive: Some(pdf.clone()),
        thumb_src: Some(pdf),
    })
}

/// Texto de un XML de Office: quita etiquetas y convierte fin de párrafo/celda en salto de línea.
fn xml_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 3);
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else { break };
        let tag = &rest[lt + 1..lt + gt];
        if ["/w:p", "/a:p", "/text:p", "/text:h", "/si", "/table:table-row", "w:br/", "w:br /"].contains(&tag) {
            out.push('\n');
        } else if ["w:tab/", "/table:table-cell", "text:tab/"].contains(&tag) {
            out.push(' ');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

// ponytail: en XLSX solo se indexan las cadenas compartidas (texto), no los números de las celdas.
fn office_text(path: &Path, ext: &str) -> R<String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.to_string()))
        .filter(|n| match ext {
            "docx" => n == "word/document.xml",
            "xlsx" => n == "xl/sharedStrings.xml",
            "pptx" => n.starts_with("ppt/slides/slide") && n.ends_with(".xml"),
            _ => n == "content.xml",
        })
        .collect();
    names.sort_by_key(|n| (n.len(), n.clone()));
    let mut out = String::new();
    for n in names {
        let mut xml = String::new();
        zip.by_name(&n)?.take(64 << 20).read_to_string(&mut xml)?;
        out.push_str(&xml_text(&xml));
        out.push('\n');
    }
    Ok(out.trim().to_string())
}

fn process_office(src: &Path, filename: &str, ext: &str, work: &Path) -> R<Extracted> {
    if let Some(url) = &cfg().gotenberg {
        // Con Gotenberg hay PDF real para vista previa (y soporte de .doc/.xls/.ppt/.rtf).
        use ureq::unversioned::multipart::{Form, Part};
        let form = Form::new().part("files", Part::file(src)?.file_name(filename));
        let mut resp = crate::http().post(&format!("{}/forms/libreoffice/convert", url.trim_end_matches('/'))).send(form)?;
        let pdf = work.join("office.pdf");
        std::io::copy(&mut resp.body_mut().as_reader(), &mut fs::File::create(&pdf)?)?;
        let mut ex = process_pdf(&pdf, work)?;
        ex.archive = ex.archive.or(Some(pdf));
        return Ok(ex);
    }
    if !["docx", "xlsx", "pptx", "odt", "ods", "odp"].contains(&ext) {
        return Err(JobErr::Unsupported(format!(".{ext} requiere PAPIRO_GOTENBERG_URL para convertirse")));
    }
    Ok(Extracted { content: office_text(src, ext)?, pages: 1, archive: None, thumb_src: None })
}

// ───────────────────────── guardado ─────────────────────────

fn io_err(e: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

/// Copia un archivo, lo sincroniza a disco y comprueba que la copia tiene el SHA-256 esperado.
pub fn copy_verified(src: &Path, dst: &Path, sha: &str) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    fs::File::open(dst)?.sync_all()?;
    if sha256_file(dst)? != sha {
        let _ = fs::remove_file(dst);
        return Err(std::io::Error::other("la copia no coincide con el original (SHA-256 distinto)"));
    }
    Ok(())
}

fn sync_path(p: &Path) {
    let _ = fs::File::open(p).and_then(|f| f.sync_all());
}

/// Mueve una carpeta ya completa a su sitio definitivo. Se llama dentro de la transacción que inserta
/// la fila: si el movimiento falla, la fila no se confirma.
pub fn place(stage: &Path, id: i64) -> rusqlite::Result<()> {
    let dst = store::doc_dir(id);
    if dst.exists() {
        set_aside(&dst, &id.to_string());
    }
    fs::rename(stage, &dst).map_err(io_err)
}

/// ¿El original guardado de un documento existe y conserva su SHA-256?
pub fn original_ok(id: i64, sha: &str) -> bool {
    sha256_file(&store::doc_dir(id).join("original")).is_ok_and(|s| s == sha)
}

/// Repone el original de un documento a partir de un archivo idéntico (misma huella).
pub fn restore_original(id: i64, src: &Path, sha: &str) -> std::io::Result<()> {
    let dir = store::doc_dir(id);
    fs::create_dir_all(&dir)?;
    let tmp = dir.join("original.tmp");
    copy_verified(src, &tmp, sha)?;
    fs::rename(&tmp, dir.join("original"))?;
    sync_path(&dir);
    Ok(())
}

fn existing(sha: &str) -> R<Option<i64>> {
    Ok(db::with(|c| c.query_row("SELECT id FROM documents WHERE sha256=?", [sha], |r| r.get(0)).optional())?)
}

fn process(j: &Claimed, path: &Path, meta: &Value, work: &Path) -> R<i64> {
    if !path.is_file() {
        return Err(JobErr::Unsupported("el archivo ya no existe".into()));
    }
    let filename = j.filename.as_str();
    let sha = sha256_file(path)?;
    if let Some(id) = existing(&sha)? {
        if Some(id) == j.doc_id {
            return Ok(id); // este mismo trabajo ya lo había guardado antes de un reinicio
        }
        // Ya está archivado. Si la copia guardada falta o está dañada, esta subida idéntica la repara.
        if !original_ok(id, &sha) {
            restore_original(id, path, &sha)?;
            println!("integridad: el original del documento {id} faltaba o estaba dañado; se repuso con una subida idéntica");
        }
        return Err(JobErr::Duplicate(id));
    }
    let (kind, ext, mime) = detect(path, filename)?;
    let ex = match kind {
        Kind::Pdf => process_pdf(path, work)?,
        Kind::Image => process_image(path, work)?,
        Kind::Office => process_office(path, filename, &ext, work)?,
        Kind::Text => {
            let mut buf = Vec::new();
            fs::File::open(path)?.take(8 << 20).read_to_end(&mut buf)?;
            Extracted { content: String::from_utf8_lossy(&buf).into_owned(), pages: 1, archive: None, thumb_src: None }
        }
    };
    let thumb = ex.thumb_src.as_ref().and_then(|pdf| make_thumb(pdf, &work.join("thumb")));

    let stem = Path::new(filename).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| filename.into());
    let title = meta["title"].as_str().filter(|t| !t.trim().is_empty()).map(String::from).unwrap_or(stem);
    let created = meta["created"].as_str().filter(|d| store::valid_date(d)).map(String::from).or_else(|| detect_date(&ex.content));
    let haystack = format!("{title}\n{}", ex.content);
    let pick = |kind: &str, given: &Value| -> Option<i64> {
        given.as_i64().or_else(|| store::list_labels(kind).ok()?.iter().find(|l| matches(l, &haystack)).map(|l| l.id))
    };
    let (corr, dtype) = (pick("correspondent", &meta["correspondent_id"]), pick("doctype", &meta["doctype_id"]));
    let mut tags: Vec<i64> = meta["tags"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    tags.extend(store::list_labels("tag")?.iter().filter(|l| l.is_inbox || matches(l, &haystack)).map(|l| l.id));
    let size = fs::metadata(path)?.len() as i64;
    // Carpeta: la elegida al subir, o la subcarpeta de la carpeta vigilada en la que se dejó el archivo.
    let folder = match (meta["folder_id"].as_i64().filter(|f| store::folder_exists(*f)), meta["folder"].as_str()) {
        (Some(f), _) => Some(f),
        (None, Some(path)) => store::ensure_folder(path).ok().flatten(),
        _ => None,
    };

    // 1. Todo se prepara y se verifica en una carpeta temporal del mismo volumen.
    let stage = tmp().join(format!("stage-{}", j.id));
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)?;
    copy_verified(path, &stage.join("original"), &sha)?;
    if let Some(a) = &ex.archive {
        fs::rename(a, stage.join("archive.pdf"))?;
        sync_path(&stage.join("archive.pdf"));
    }
    if let Some(t) = &thumb {
        fs::rename(t, stage.join("thumb.jpg"))?;
    }
    sync_path(&stage);
    // 2. Fila y archivos aparecen juntos: la carpeta se mueve a su sitio dentro de la transacción.
    let inserted = db::write(|c| {
        let id: i64 = c.query_row(
            "INSERT INTO documents(id, title, content, created, correspondent_id, doctype_id, original_name, mime, ext, size, pages, sha256,
                                   has_archive, has_thumb, owner_id, is_private, folder_id)
             VALUES(?,?,?,coalesce(?, date('now','localtime')),?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id",
            params![store::next_doc_id(c)?, title, ex.content, created, corr, dtype, filename, mime, ext, size, ex.pages, sha, ex.archive.is_some(),
                    thumb.is_some() || ex.thumb_src.is_some(), j.user_id, meta["is_private"].as_bool().unwrap_or(false) && j.user_id.is_some(), folder],
            |r| r.get(0),
        )?;
        for t in &tags {
            c.execute("INSERT OR IGNORE INTO document_tags(doc_id, tag_id) SELECT ?, id FROM labels WHERE id=? AND kind='tag'", [id, *t])?;
        }
        c.execute("UPDATE jobs SET doc_id=? WHERE id=?", [id, j.id])?;
        place(&stage, id)?;
        Ok(id)
    });
    match inserted.map_err(store::Error::from) {
        Ok(id) => {
            sync_path(&cfg().data.join("docs"));
            // Análisis local (reglas y parecidos). Si falla, el documento ya está guardado igual.
            if let Err(e) = crate::suggest::analyze(id, true) {
                eprintln!("análisis del documento {id}: {e}");
            }
            Ok(id)
        }
        // Otro worker guardó el mismo archivo mientras este lo procesaba.
        Err(store::Error::Conflict(_)) => Err(JobErr::Duplicate(existing(&sha)?.unwrap_or(0))),
        Err(e) => Err(e.into()),
    }
}

/// Miniatura JPEG de la primera página de un PDF; `base` va sin extensión.
pub fn make_thumb(pdf: &Path, base: &Path) -> Option<PathBuf> {
    run("pdftoppm", &["-f", "1", "-l", "1", "-scale-to", "640", "-jpeg", "-singlefile", s(pdf), s(base)]).ok()?;
    Some(base.with_extension("jpg")).filter(|p| p.is_file())
}

/// Devuelve la miniatura de un documento, generándola si aún no existe (p. ej. tras una importación).
pub fn ensure_thumb(id: i64, has_archive: bool, mime: &str) -> Option<PathBuf> {
    let dir = store::doc_dir(id);
    let thumb = dir.join("thumb.jpg");
    if thumb.is_file() {
        return Some(thumb);
    }
    let pdf = if has_archive { dir.join("archive.pdf") } else { dir.join("original") };
    if !pdf.is_file() || (!has_archive && mime != "application/pdf") {
        return None;
    }
    let made = make_thumb(&pdf, &tmp().join(format!("thumb-{id}-{}", crate::random_hex(4))))?;
    fs::rename(&made, &thumb).ok()?;
    Some(thumb)
}

/// La corrección de orientación se usa si no se desactivó y Tesseract tiene instalados los datos `osd`.
pub fn rotate_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| cfg().ocr_rotate && run("tesseract", &["--list-langs"]).is_ok_and(|out| out.lines().any(|l| l.trim() == "osd")))
}

// ───────────────────────── reglas de asignación y fechas ─────────────────────────

/// Mismos algoritmos que Paperless para que las reglas importadas sigan funcionando.
// ponytail: "fuzzy" se evalúa como "all" y "auto" (clasificador ML) no existe; para eso está el LLM opcional.
pub fn matches(l: &store::Label, text: &str) -> bool {
    let m = l.matching.trim();
    if m.is_empty() || l.algo == "none" || l.algo == "auto" {
        return false;
    }
    let re = |pat: String| Regex::new(&format!("{}{pat}", if l.insensitive { "(?i)" } else { "" })).map(|r| r.is_match(text)).unwrap_or(false);
    let word = |w: &str| re(format!(r"\b{}\b", regex::escape(w)));
    match l.algo.as_str() {
        "any" => m.split_whitespace().any(word),
        "all" | "fuzzy" => m.split_whitespace().all(word),
        "literal" => word(m),
        "regex" => re(m.to_string()),
        _ => false,
    }
}

/// Primera fecha plausible del texto, como YYYY-MM-DD. Formatos numéricos se leen día/mes/año.
pub fn detect_date(text: &str) -> Option<String> {
    use std::sync::LazyLock;
    const MONTHS: &str = "enero|febrero|marzo|abril|mayo|junio|julio|agosto|septiembre|setiembre|octubre|noviembre|diciembre|january|february|march|april|may|june|july|august|september|october|november|december";
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(&format!(
            r"(?i)\b(?:(?P<y1>\d{{4}})-(?P<m1>\d{{2}})-(?P<d1>\d{{2}})|(?P<d2>\d{{1,2}})[/.-](?P<m2>\d{{1,2}})[/.-](?P<y2>\d{{4}})|(?P<d3>\d{{1,2}})\s+(?:de\s+)?(?P<m3>{MONTHS})\s+(?:de\s+|del\s+)?(?P<y3>\d{{4}})|(?P<m4>{MONTHS})\s+(?P<d4>\d{{1,2}}),?\s+(?P<y4>\d{{4}}))\b"
        ))
        .unwrap()
    });
    let head: String = text.chars().take(6000).collect();
    for c in RE.captures_iter(&head) {
        let g = |n: &str| (1..=4).find_map(|i| c.name(&format!("{n}{i}"))).map(|m| m.as_str().to_lowercase());
        let (Some(y), Some(m), Some(d)) = (g("y"), g("m"), g("d")) else { continue };
        let (y, mut d): (u32, u32) = (y.parse().ok()?, d.parse().ok()?);
        let mut m: u32 = m.parse().ok().or_else(|| {
            let i = MONTHS.split('|').position(|x| x == m)? as u32;
            Some(match i {
                0..=8 => i + 1,
                9..=12 => i,      // setiembre comparte el 9
                _ => i - 12,      // meses en inglés
            })
        })?;
        if m > 12 && d <= 12 {
            (m, d) = (d, m);
        }
        if (1950..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
            return Some(format!("{y:04}-{m:02}-{d:02}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(algo: &str, m: &str) -> store::Label {
        store::Label { id: 1, name: "x".into(), color: "".into(), is_inbox: false, parent_id: None, matching: m.into(), algo: algo.into(), insensitive: true, document_count: 0 }
    }

    #[test]
    fn dates() {
        assert_eq!(detect_date("Folio: X  Fecha: 17/02/2021 Página").unwrap(), "2021-02-17");
        assert_eq!(detect_date("emitido el 3 de marzo de 2024 en CDMX").unwrap(), "2024-03-03");
        assert_eq!(detect_date("Invoice date: March 12, 2023").unwrap(), "2023-03-12");
        assert_eq!(detect_date("Dated 2022-11-05.").unwrap(), "2022-11-05");
        assert_eq!(detect_date("12/25/2020 (formato gringo)").unwrap(), "2020-12-25");
        assert_eq!(detect_date("5 de setiembre de 2019").unwrap(), "2019-09-05");
        assert_eq!(detect_date("1 de diciembre de 2019").unwrap(), "2019-12-01");
        assert_eq!(detect_date("December 1, 2019").unwrap(), "2019-12-01");
        assert_eq!(detect_date("sin fecha 99/99/9999"), None);
    }

    #[test]
    fn matching() {
        let t = "Recibo de la Comisión Federal de Electricidad, folio 88";
        assert!(matches(&label("any", "telmex electricidad"), t));
        assert!(!matches(&label("all", "telmex electricidad"), t));
        assert!(matches(&label("all", "comisión federal"), t));
        assert!(matches(&label("literal", "federal de electricidad"), t));
        assert!(!matches(&label("literal", "electric"), t));
        assert!(matches(&label("regex", r"folio\s+\d+"), t));
        assert!(!matches(&label("regex", r"((("), t));
        assert!(!matches(&label("none", "recibo"), t));
    }

    #[test]
    fn office_xml() {
        let x = "<w:p><w:r><w:t>Hola &amp; adiós</w:t></w:r></w:p><w:p><w:r><w:t>Línea 2</w:t></w:r></w:p>";
        assert_eq!(xml_text(x), "Hola & adiós\nLínea 2\n");
    }
}
