//! SQLite: una conexión por hilo en modo WAL (muchos lectores, un escritor) y migraciones versionadas.
use rusqlite::Connection;
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
/// Importaciones masivas: se relaja la sincronización por transacción (se sincroniza al terminar).
static BULK: AtomicBool = AtomicBool::new(false);
thread_local! { static CONN: RefCell<Option<Connection>> = const { RefCell::new(None) }; }

pub fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    CONN.with(|cell| {
        if cell.borrow().is_none() {
            let c = Connection::open(DB_PATH.get().expect("db::init"))?;
            // synchronous=FULL: una transacción confirmada sobrevive a un corte de energía.
            c.execute_batch(&format!(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous={}; PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=15000; PRAGMA cache_size=-2000;",
                if BULK.load(Ordering::Relaxed) { "NORMAL" } else { "FULL" }
            ))?;
            *cell.borrow_mut() = Some(c);
        }
        f(cell.borrow().as_ref().unwrap())
    })
}

fn busy(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if matches!(f.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
}

/// Transacción de escritura. BEGIN IMMEDIATE toma el candado de escritura desde el principio:
/// así dos escritores hacen cola (busy_timeout) en vez de fallar con «database is locked» al
/// intentar promover una transacción de lectura.
pub fn write<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    with(|c| {
        let mut attempt = 0;
        loop {
            match c.execute_batch("BEGIN IMMEDIATE") {
                Ok(()) => break,
                Err(e) if busy(&e) && attempt < 8 => {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(25 * attempt));
                }
                Err(e) => return Err(e),
            }
        }
        match f(c) {
            Ok(v) => c.execute_batch("COMMIT").map(|_| v).inspect_err(|_| drop(c.execute_batch("ROLLBACK"))),
            Err(e) => {
                let _ = c.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    })
}

pub fn bulk_mode() {
    BULK.store(true, Ordering::Relaxed);
}

/// Vacía el WAL a la base y lo sincroniza a disco.
pub fn checkpoint() {
    let _ = with(|c| c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)"));
}

pub fn schema_version() -> i64 {
    with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0))).unwrap_or(0)
}

/// Copia consistente de la base aunque haya escrituras en curso.
pub fn snapshot(dest: &std::path::Path) -> rusqlite::Result<()> {
    let _ = std::fs::remove_file(dest);
    with(|c| c.execute("VACUUM INTO ?", [dest.to_string_lossy()]).map(|_| ()))
}

/// Abre la base y aplica las migraciones pendientes. Devuelve las versiones aplicadas en este arranque.
/// Antes de migrar una base existente guarda una copia `papiro.db.antes-de-vN`.
pub fn init(path: PathBuf) -> Result<Vec<i64>, String> {
    DB_PATH.set(path.clone()).ok();
    let e = |e: rusqlite::Error| e.to_string();
    let latest = MIGRATIONS.last().map(|m| m.0).unwrap_or(0);
    let mut version: i64 = with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0))).map_err(e)?;
    // Las bases creadas antes de que existieran las migraciones ya tienen el esquema 1.
    let legacy: bool = with(|c| c.query_row("SELECT count(*) FROM sqlite_master WHERE name='documents'", [], |r| r.get(0))).map_err(e)?;
    if version == 0 && legacy {
        version = 1;
        with(|c| c.execute_batch("PRAGMA user_version=1")).map_err(e)?;
    }
    if version > latest {
        return Err(format!("la base de datos es de una versión más nueva de Papiro (esquema {version}; este binario conoce hasta {latest}). Actualiza Papiro; no se tocó nada."));
    }
    if version > 0 && version < latest {
        let backup = path.with_file_name(format!("papiro.db.antes-de-v{latest}"));
        snapshot(&backup).map_err(|x| format!("no se pudo respaldar la base antes de migrar: {x}"))?;
        println!("migración: copia de seguridad de la base en {}", backup.display());
    }
    let mut applied = vec![];
    for (n, sql) in MIGRATIONS.iter().filter(|m| m.0 > version) {
        write(|c| c.execute_batch(&format!("{sql}\nPRAGMA user_version={n};"))).map_err(|x| format!("migración {n} fallida (no se aplicó nada de ella): {x}"))?;
        println!("migración: esquema actualizado a la versión {n}");
        applied.push(*n);
    }
    Ok(applied)
}

/// Cada entrada se aplica una sola vez, en orden y dentro de una transacción. Nunca se edita una
/// migración ya publicada: los cambios de esquema se añaden como una entrada nueva.
const MIGRATIONS: &[(i64, &str)] = &[(1, V1), (2, V2), (3, V3), (4, V4)];

const V1: &str = r#"
CREATE TABLE users(
  id INTEGER PRIMARY KEY, username TEXT UNIQUE NOT NULL COLLATE NOCASE, password_hash TEXT NOT NULL,
  is_admin INTEGER NOT NULL DEFAULT 0, created TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE sessions(
  token_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, expires TEXT NOT NULL);
CREATE TABLE api_tokens(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, name TEXT NOT NULL,
  token_hash TEXT UNIQUE NOT NULL, created TEXT NOT NULL DEFAULT (datetime('now')), last_used TEXT);
-- Etiquetas, corresponsales y tipos comparten forma: kind = tag | correspondent | doctype
CREATE TABLE labels(
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL COLLATE NOCASE, color TEXT NOT NULL DEFAULT '#64748b',
  is_inbox INTEGER NOT NULL DEFAULT 0, parent_id INTEGER REFERENCES labels(id) ON DELETE SET NULL,
  match TEXT NOT NULL DEFAULT '', algo TEXT NOT NULL DEFAULT 'none', insensitive INTEGER NOT NULL DEFAULT 1,
  UNIQUE(kind, name));
CREATE TABLE documents(
  id INTEGER PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL DEFAULT '',
  created TEXT NOT NULL, added TEXT NOT NULL DEFAULT (datetime('now')), modified TEXT NOT NULL DEFAULT (datetime('now')),
  correspondent_id INTEGER REFERENCES labels(id) ON DELETE SET NULL,
  doctype_id INTEGER REFERENCES labels(id) ON DELETE SET NULL,
  asn INTEGER UNIQUE, original_name TEXT NOT NULL, mime TEXT NOT NULL, ext TEXT NOT NULL,
  size INTEGER NOT NULL, pages INTEGER NOT NULL DEFAULT 0, sha256 TEXT UNIQUE NOT NULL,
  has_archive INTEGER NOT NULL DEFAULT 0, has_thumb INTEGER NOT NULL DEFAULT 0,
  summary TEXT, custom TEXT NOT NULL DEFAULT '{}', deleted_at TEXT);
CREATE INDEX documents_created ON documents(created);
CREATE INDEX documents_corr ON documents(correspondent_id);
CREATE INDEX documents_type ON documents(doctype_id);
CREATE TABLE document_tags(
  doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY(doc_id, tag_id)) WITHOUT ROWID;
CREATE INDEX document_tags_tag ON document_tags(tag_id);
CREATE TABLE notes(
  id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  text TEXT NOT NULL, created TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE jobs(
  id INTEGER PRIMARY KEY, path TEXT NOT NULL, filename TEXT NOT NULL, source TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued', error TEXT, doc_id INTEGER, meta TEXT NOT NULL DEFAULT '{}',
  created TEXT NOT NULL DEFAULT (datetime('now')), started TEXT, finished TEXT);
CREATE INDEX jobs_status ON jobs(status);
CREATE TABLE webhooks(
  id INTEGER PRIMARY KEY, url TEXT NOT NULL, events TEXT NOT NULL DEFAULT '*', secret TEXT NOT NULL DEFAULT '',
  last_status TEXT, last_at TEXT);
CREATE VIRTUAL TABLE docs_fts USING fts5(
  title, content, content='documents', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER docs_ai AFTER INSERT ON documents BEGIN
  INSERT INTO docs_fts(rowid, title, content) VALUES (new.id, new.title, new.content); END;
CREATE TRIGGER docs_ad AFTER DELETE ON documents BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, title, content) VALUES ('delete', old.id, old.title, old.content); END;
CREATE TRIGGER docs_au AFTER UPDATE OF title, content ON documents BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, title, content) VALUES ('delete', old.id, old.title, old.content);
  INSERT INTO docs_fts(rowid, title, content) VALUES (new.id, new.title, new.content); END;
"#;

// v2: roles, documentos privados, tokens de solo lectura y con caducidad, auditoría,
// cola persistente de webhooks, reintentos de trabajos.
const V2: &str = r#"
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'editor';   -- admin | editor | viewer
UPDATE users SET role='admin' WHERE is_admin=1;
ALTER TABLE api_tokens ADD COLUMN read_only INTEGER NOT NULL DEFAULT 0;
ALTER TABLE api_tokens ADD COLUMN expires TEXT;
ALTER TABLE documents ADD COLUMN owner_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE documents ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN user_id INTEGER;
CREATE TABLE audit(
  id INTEGER PRIMARY KEY, at TEXT NOT NULL DEFAULT (datetime('now')), user_id INTEGER, username TEXT NOT NULL DEFAULT '',
  action TEXT NOT NULL, target TEXT NOT NULL DEFAULT '', detail TEXT NOT NULL DEFAULT '', ip TEXT NOT NULL DEFAULT '');
CREATE INDEX audit_at ON audit(at);
CREATE TABLE outbox(
  id INTEGER PRIMARY KEY, hook_id INTEGER NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE, body TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0, next_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
"#;

// v3: carpetas anidadas. Un documento está en una carpeta o en ninguna.
const V3: &str = r#"
CREATE TABLE folders(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE,
  parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE, created TEXT NOT NULL DEFAULT (datetime('now')));
CREATE UNIQUE INDEX folders_name ON folders(coalesce(parent_id, 0), name);
ALTER TABLE documents ADD COLUMN folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL;
CREATE INDEX documents_folder ON documents(folder_id);
"#;

// v4: análisis local. Vencimiento, importe y sugerencias por documento; vocabulario del índice de
// búsqueda (cuántos documentos contienen cada palabra) para encontrar documentos parecidos.
const V4: &str = r#"
ALTER TABLE documents ADD COLUMN expires TEXT;
ALTER TABLE documents ADD COLUMN amount REAL;
ALTER TABLE documents ADD COLUMN suggest TEXT NOT NULL DEFAULT '{}';
CREATE INDEX documents_expires ON documents(expires) WHERE expires IS NOT NULL;
CREATE VIRTUAL TABLE docs_vocab USING fts5vocab(docs_fts, 'row');
"#;
