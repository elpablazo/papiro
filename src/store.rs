//! Dominio: documentos, etiquetas, búsqueda y eventos. Lo usan por igual la API REST y el servidor MCP.
use crate::{auth::AuthUser, cfg, db};
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value as Sql};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::OnceLock};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug)]
pub enum Error {
    NotFound,
    Conflict(String),
    Bad(String),
    Other(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
                Error::Conflict(format!("conflict: {e}"))
            }
            _ => Error::Other(e.to_string()),
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(e.to_string())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound => write!(f, "not found"),
            Error::Conflict(m) | Error::Bad(m) | Error::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Distingue "campo ausente" de "campo = null" en los PATCH.
fn dbl<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Option<T>>, D::Error> {
    Deserialize::deserialize(d).map(Some)
}

// ───────────────────────── etiquetas / corresponsales / tipos ─────────────────────────

pub const KINDS: [&str; 3] = ["tag", "correspondent", "doctype"];

#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct Label {
    pub id: i64,
    pub name: String,
    /// Hex color (#rrggbb)
    pub color: String,
    /// Tags only: assigned to every new document
    pub is_inbox: bool,
    pub parent_id: Option<i64>,
    /// Text of the automatic assignment rule
    #[serde(rename = "match")]
    pub matching: String,
    /// none | any | all | literal | regex | fuzzy | auto
    pub algo: String,
    pub insensitive: bool,
    pub document_count: i64,
}

#[derive(Deserialize, ToSchema, Default, Debug)]
pub struct LabelInput {
    pub name: Option<String>,
    pub color: Option<String>,
    pub is_inbox: Option<bool>,
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub parent_id: Option<Option<i64>>,
    #[serde(rename = "match")]
    pub matching: Option<String>,
    pub algo: Option<String>,
    pub insensitive: Option<bool>,
}

fn label_row(r: &Row) -> rusqlite::Result<Label> {
    Ok(Label {
        id: r.get(0)?,
        name: r.get(1)?,
        color: r.get(2)?,
        is_inbox: r.get(3)?,
        parent_id: r.get(4)?,
        matching: r.get(5)?,
        algo: r.get(6)?,
        insensitive: r.get(7)?,
        document_count: r.get(8)?,
    })
}

fn label_sql(kind: &str, filter: &str) -> String {
    let count = match kind {
        "tag" => "SELECT count(*) FROM document_tags t JOIN documents d ON d.id=t.doc_id WHERE t.tag_id=l.id AND d.deleted_at IS NULL",
        "correspondent" => "SELECT count(*) FROM documents d WHERE d.correspondent_id=l.id AND d.deleted_at IS NULL",
        _ => "SELECT count(*) FROM documents d WHERE d.doctype_id=l.id AND d.deleted_at IS NULL",
    };
    format!(
        "SELECT l.id, l.name, l.color, l.is_inbox, l.parent_id, l.match, l.algo, l.insensitive, ({count})
         FROM labels l WHERE l.kind=?1 {filter} ORDER BY l.name COLLATE NOCASE"
    )
}

pub fn list_labels(kind: &str) -> Result<Vec<Label>> {
    Ok(db::with(|c| c.prepare_cached(&label_sql(kind, ""))?.query_map([kind], label_row)?.collect())?)
}

pub fn get_label(kind: &str, id: i64) -> Result<Label> {
    Ok(db::with(|c| c.query_row(&label_sql(kind, "AND l.id=?2"), params![kind, id], label_row))?)
}

const PALETTE: [&str; 10] =
    ["#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#06b6d4", "#3b82f6", "#8b5cf6", "#d946ef", "#ec4899"];

pub fn create_label(kind: &str, i: &LabelInput) -> Result<Label> {
    let name = i.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).ok_or(Error::Bad("name is required".into()))?;
    let color = i.color.clone().unwrap_or_else(|| PALETTE[name.bytes().map(|b| b as usize).sum::<usize>() % 10].into());
    let id = db::with(|c| {
        c.execute(
            "INSERT INTO labels(kind,name,color,is_inbox,parent_id,match,algo,insensitive) VALUES(?,?,?,?,?,?,?,?)",
            params![
                kind,
                name,
                color,
                i.is_inbox.unwrap_or(false),
                i.parent_id.flatten(),
                i.matching.clone().unwrap_or_default(),
                i.algo.clone().unwrap_or_else(|| "none".into()),
                i.insensitive.unwrap_or(true)
            ],
        )?;
        Ok(c.last_insert_rowid())
    })
    .map_err(|e| match Error::from(e) {
        Error::Conflict(_) => Error::Conflict(format!("\"{name}\" already exists")),
        e => e,
    })?;
    get_label(kind, id)
}

/// Devuelve el id de la etiqueta con ese nombre, creándola si no existe.
pub fn ensure_label(kind: &str, name: &str) -> Result<i64> {
    let found = db::with(|c| {
        c.query_row("SELECT id FROM labels WHERE kind=? AND name=?", params![kind, name.trim()], |r| r.get(0)).optional()
    })?;
    match found {
        Some(id) => Ok(id),
        None => Ok(create_label(kind, &LabelInput { name: Some(name.into()), ..Default::default() })?.id),
    }
}

pub fn update_label(kind: &str, id: i64, i: &LabelInput) -> Result<Label> {
    db::with(|c| {
        let n = c.execute(
            "UPDATE labels SET name=coalesce(?3,name), color=coalesce(?4,color), is_inbox=coalesce(?5,is_inbox),
               match=coalesce(?6,match), algo=coalesce(?7,algo), insensitive=coalesce(?8,insensitive)
             WHERE kind=?1 AND id=?2",
            params![kind, id, i.name, i.color, i.is_inbox, i.matching, i.algo, i.insensitive],
        )?;
        if n == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        if let Some(p) = i.parent_id {
            c.execute("UPDATE labels SET parent_id=? WHERE id=? AND ? IS NOT id", params![p, id, p])?;
        }
        Ok(())
    })?;
    get_label(kind, id)
}

pub fn delete_label(kind: &str, id: i64) -> Result<()> {
    match db::with(|c| c.execute("DELETE FROM labels WHERE kind=? AND id=?", params![kind, id]))? {
        0 => Err(Error::NotFound),
        _ => Ok(()),
    }
}

// ───────────────────────── documentos ─────────────────────────

// ───────────────────────── carpetas ─────────────────────────

#[derive(Serialize, ToSchema, Clone)]
pub struct Folder {
    pub id: i64,
    pub name: String,
    /// Containing folder (null = root)
    pub parent_id: Option<i64>,
    /// Full path, e.g. `Home/Receipts`
    pub path: String,
    /// Visible documents in the folder and its subfolders
    pub document_count: i64,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct FolderInput {
    pub name: Option<String>,
    /// Containing folder; null puts it at the root
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub parent_id: Option<Option<i64>>,
}

/// Ids de una carpeta y de todas sus subcarpetas (un parámetro: el id de la carpeta).
const SUBTREE: &str = "WITH RECURSIVE sub(id) AS (SELECT ? UNION ALL SELECT f.id FROM folders f JOIN sub ON f.parent_id=sub.id) SELECT id FROM sub";

/// Todas las carpetas, ordenadas por ruta, con el número de documentos que el usuario puede ver.
pub fn list_folders(u: &AuthUser) -> Result<Vec<Folder>> {
    let sql = format!(
        "SELECT f.id, f.name, f.parent_id, (SELECT count(*) FROM documents d WHERE d.folder_id=f.id AND d.deleted_at IS NULL AND {}) FROM folders f",
        visible(u)
    );
    let mut fs: Vec<Folder> = db::with(|c| {
        c.prepare(&sql)?
            .query_map([], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, path: String::new(), document_count: r.get(3)? }))?
            .collect()
    })?;
    let at: HashMap<i64, usize> = fs.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
    for i in 0..fs.len() {
        let (own, mut path, mut up) = (fs[i].document_count, fs[i].name.clone(), fs[i].parent_id);
        // El límite de profundidad evita un bucle si la base tuviera un ciclo.
        for _ in 0..64 {
            let Some(&j) = up.and_then(|id| at.get(&id)) else { break };
            fs[j].document_count += own;
            path = format!("{}/{path}", fs[j].name);
            up = fs[j].parent_id;
        }
        fs[i].path = path;
    }
    fs.sort_by_key(|f| f.path.to_lowercase());
    Ok(fs)
}

fn folder_name(name: Option<&str>) -> Result<&str> {
    match name.map(str::trim) {
        Some(n) if !n.is_empty() && n.chars().count() <= 80 && !n.contains('/') => Ok(n),
        _ => Err(Error::Bad("a folder name must be 1 to 80 characters long and cannot contain \"/\"".into())),
    }
}

fn folder_conflict(e: rusqlite::Error) -> Error {
    match Error::from(e) {
        Error::Conflict(_) => Error::Conflict("a folder with that name already exists here".into()),
        e => e,
    }
}

pub fn create_folder(i: &FolderInput) -> Result<i64> {
    let name = folder_name(i.name.as_deref())?;
    let parent = i.parent_id.flatten();
    if parent.is_some_and(|p| !folder_exists(p)) {
        return Err(Error::Bad("the parent folder does not exist".into()));
    }
    db::with(|c| {
        c.execute("INSERT INTO folders(name, parent_id) VALUES(?, ?)", params![name, parent])?;
        Ok(c.last_insert_rowid())
    })
    .map_err(folder_conflict)
}

pub fn folder_exists(id: i64) -> bool {
    db::with(|c| c.query_row("SELECT 1 FROM folders WHERE id=?", [id], |_| Ok(())).optional()).is_ok_and(|r| r.is_some())
}

/// Renombra o mueve una carpeta. No se puede mover dentro de sí misma ni de una de sus subcarpetas.
pub fn update_folder(id: i64, i: &FolderInput) -> Result<()> {
    if !folder_exists(id) {
        return Err(Error::NotFound);
    }
    let name = i.name.as_deref().map(|n| folder_name(Some(n))).transpose()?;
    db::write(|c| {
        if let Some(parent) = i.parent_id {
            if let Some(p) = parent {
                let inside: bool = c.query_row(&format!("SELECT ?2 IN ({})", SUBTREE.replace('?', "?1")), params![id, p], |r| r.get(0)).unwrap_or(true);
                let exists = c.query_row("SELECT 1 FROM folders WHERE id=?", [p], |_| Ok(())).optional()?.is_some();
                if inside || !exists {
                    return Err(rusqlite::Error::InvalidParameterName("target".into()));
                }
            }
            c.execute("UPDATE folders SET parent_id=? WHERE id=?", params![parent, id])?;
        }
        if let Some(n) = name {
            c.execute("UPDATE folders SET name=? WHERE id=?", params![n, id])?;
        }
        Ok(())
    })
    .map_err(|e| match e {
        rusqlite::Error::InvalidParameterName(_) => Error::Bad("a folder cannot be moved into itself or into a folder that does not exist".into()),
        e => folder_conflict(e),
    })
}

/// Borra la carpeta y sus subcarpetas. Los documentos nunca se borran: pasan a la carpeta superior.
pub fn delete_folder(id: i64) -> Result<usize> {
    if !folder_exists(id) {
        return Err(Error::NotFound);
    }
    Ok(db::write(|c| {
        let parent: Option<i64> = c.query_row("SELECT parent_id FROM folders WHERE id=?", [id], |r| r.get(0))?;
        let moved = c.execute(&format!("UPDATE documents SET folder_id=?1 WHERE folder_id IN ({})", SUBTREE.replace('?', "?2")), params![parent, id])?;
        c.execute("DELETE FROM folders WHERE id=?", [id])?;
        Ok(moved)
    })?)
}

/// Id de la carpeta con esa ruta (`Casa/Recibos`), creando los niveles que falten.
pub fn ensure_folder(path: &str) -> Result<Option<i64>> {
    let mut parent: Option<i64> = None;
    for part in path.split('/').map(str::trim).filter(|p| !p.is_empty()) {
        let name = folder_name(Some(part))?;
        let found = db::with(|c| c.query_row("SELECT id FROM folders WHERE name=? AND coalesce(parent_id,0)=?", params![name, parent.unwrap_or(0)], |r| r.get(0)).optional())?;
        parent = Some(match found {
            Some(id) => id,
            None => create_folder(&FolderInput { name: Some(name.into()), parent_id: Some(parent) })?,
        });
    }
    Ok(parent)
}

#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub created: String,
}

#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct Document {
    pub id: i64,
    pub title: String,
    /// Document date (YYYY-MM-DD)
    pub created: String,
    pub added: String,
    pub modified: String,
    pub correspondent_id: Option<i64>,
    pub doctype_id: Option<i64>,
    pub tags: Vec<i64>,
    /// Archive serial number of the physical copy (ASN)
    pub asn: Option<i64>,
    pub original_name: String,
    pub mime: String,
    pub size: i64,
    pub pages: i64,
    pub sha256: String,
    /// An OCR-generated PDF with a text layer exists
    pub has_archive: bool,
    pub has_thumb: bool,
    pub summary: Option<String>,
    /// Custom fields (free-form object, name → value)
    pub custom: Value,
    pub deleted_at: Option<String>,
    /// User who uploaded it (null for imported documents and those that came in by folder or mail)
    pub owner_id: Option<i64>,
    /// Private: only its owner and administrators can see it
    pub is_private: bool,
    /// Folder it is in (null = no folder)
    pub folder_id: Option<i64>,
    /// Expiry date (YYYY-MM-DD), detected from the text or set by hand
    pub expires: Option<String>,
    /// Total amount of the document, detected from the text or set by hand
    pub amount: Option<f64>,
    /// Local analysis suggestions: `folder_id`, `doctype_id`, `correspondent_id` (each with
    /// `value`, `confidence`, `similar` and `applied`) and `tags` (a list). Those with `applied: true` were already applied automatically.
    #[schema(value_type = Object)]
    pub suggest: Value,
    /// Snippet with the matches; \u0001 and \u0002 delimit the highlighted parts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Full text (only in the single-document response)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<Note>>,
}

const DOC_COLS: &str = "d.id, d.title, d.created, d.added, d.modified, d.correspondent_id, d.doctype_id, d.asn,
  d.original_name, d.mime, d.size, d.pages, d.sha256, d.has_archive, d.has_thumb, d.summary, d.custom, d.deleted_at,
  (SELECT group_concat(tag_id) FROM document_tags WHERE doc_id=d.id), d.owner_id, d.is_private, d.folder_id, d.expires, d.amount, d.suggest";

fn doc_row(r: &Row) -> rusqlite::Result<Document> {
    let custom: String = r.get(16)?;
    let tags: Option<String> = r.get(18)?;
    Ok(Document {
        id: r.get(0)?,
        title: r.get(1)?,
        created: r.get(2)?,
        added: r.get(3)?,
        modified: r.get(4)?,
        correspondent_id: r.get(5)?,
        doctype_id: r.get(6)?,
        asn: r.get(7)?,
        original_name: r.get(8)?,
        mime: r.get(9)?,
        size: r.get(10)?,
        pages: r.get(11)?,
        sha256: r.get(12)?,
        has_archive: r.get(13)?,
        has_thumb: r.get(14)?,
        summary: r.get(15)?,
        custom: serde_json::from_str(&custom).unwrap_or(json!({})),
        deleted_at: r.get(17)?,
        tags: tags.map(|t| t.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default(),
        owner_id: r.get(19)?,
        is_private: r.get(20)?,
        folder_id: r.get(21)?,
        expires: r.get(22)?,
        amount: r.get(23)?,
        suggest: serde_json::from_str(&r.get::<_, String>(24)?).unwrap_or(json!({})),
        snippet: None,
        content: None,
        notes: None,
    })
}

/// Número para un documento nuevo. Nunca se repite, aunque el anterior se haya borrado del todo:
/// así ni el respaldo ni la caché del navegador confunden un documento con otro que tuvo su número.
/// Debe llamarse dentro de la transacción que inserta el documento.
pub fn next_doc_id(c: &Connection) -> rusqlite::Result<i64> {
    let id: i64 = c.query_row(
        "SELECT max(coalesce((SELECT max(id) FROM documents), 0), coalesce((SELECT CAST(value AS INTEGER) FROM meta WHERE key='doc_seq'), 0)) + 1",
        [],
        |r| r.get(0),
    )?;
    c.execute("INSERT OR REPLACE INTO meta(key, value) VALUES('doc_seq', ?)", [id.to_string()])?;
    Ok(id)
}

pub fn doc_dir(id: i64) -> PathBuf {
    cfg().data.join("docs").join(id.to_string())
}

pub fn exists(id: i64) -> bool {
    db::with(|c| c.query_row("SELECT 1 FROM documents WHERE id=?", [id], |_| Ok(())).optional()).is_ok_and(|r| r.is_some())
}

/// Condición SQL de visibilidad: los documentos privados solo los ven su dueño y los administradores.
fn visible(u: &AuthUser) -> String {
    if u.role == "admin" { "1".into() } else { format!("(d.is_private=0 OR d.owner_id={})", u.id) }
}

/// Comprueba que el usuario puede ver el documento. Si no, responde igual que si no existiera.
pub fn authorize(u: &AuthUser, id: i64) -> Result<()> {
    Ok(db::with(|c| c.query_row(&format!("SELECT 1 FROM documents d WHERE d.id=? AND {}", visible(u)), [id], |_| Ok(())))?)
}

/// Documento sin el texto completo.
pub fn get_doc_brief(id: i64) -> Result<Document> {
    Ok(db::with(|c| c.query_row(&format!("SELECT {DOC_COLS} FROM documents d WHERE d.id=?"), [id], doc_row))?)
}

/// Documento con texto completo y notas.
pub fn get_doc(id: i64) -> Result<Document> {
    let mut d = get_doc_brief(id)?;
    db::with(|c| {
        d.content = Some(c.query_row("SELECT content FROM documents WHERE id=?", [id], |r| r.get(0))?);
        let notes = c
            .prepare_cached("SELECT id, text, created FROM notes WHERE doc_id=? ORDER BY id")?
            .query_map([id], |r| Ok(Note { id: r.get(0)?, text: r.get(1)?, created: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
        d.notes = Some(notes);
        Ok(())
    })?;
    Ok(d)
}

#[derive(Deserialize, IntoParams, Default, Debug)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// Text to search for (full-text). Supports "quoted phrases"; the last word matches as a prefix.
    pub q: Option<String>,
    /// Comma-separated tag IDs; the document must have all of them
    pub tags: Option<String>,
    pub correspondent: Option<i64>,
    pub doctype: Option<i64>,
    /// Folder, including its subfolders. `0` = documents with no folder
    pub folder: Option<i64>,
    /// Earliest document date (YYYY-MM-DD)
    pub from: Option<String>,
    /// Latest document date (YYYY-MM-DD)
    pub to: Option<String>,
    /// Only documents that have expired or expire within the next 60 days (sorted by expiry date)
    pub expiring: Option<bool>,
    /// Only documents with an inbox tag
    pub inbox: Option<bool>,
    /// Show the trash instead of the active documents
    pub trash: Option<bool>,
    /// relevance | created | added | modified | title | asn | expires
    pub sort: Option<String>,
    /// asc | desc
    pub order: Option<String>,
    /// At most 200 (default 50)
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct DocList {
    pub total: i64,
    /// Sum of the amounts of all matching documents (null if none has an amount)
    pub amount_total: Option<f64>,
    pub items: Vec<Document>,
}

/// Convierte lo que teclea el usuario en una consulta FTS5 segura.
pub fn fts_query(q: &str) -> Option<String> {
    let (mut tokens, mut cur, mut in_q) = (Vec::<(String, bool)>::new(), String::new(), false);
    for ch in q.chars() {
        if ch == '"' || (ch.is_whitespace() && !in_q) {
            if !cur.trim().is_empty() {
                tokens.push((cur.trim().to_string(), in_q));
            }
            cur.clear();
            in_q ^= ch == '"';
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        tokens.push((cur.trim().to_string(), in_q));
    }
    tokens.retain(|(t, _)| t.chars().any(char::is_alphanumeric));
    let n = tokens.len();
    let parts: Vec<String> = tokens
        .iter()
        .enumerate()
        .map(|(i, (t, phrase))| format!("\"{t}\"{}", if i + 1 == n && !phrase { "*" } else { "" }))
        .collect();
    (n > 0).then(|| parts.join(" "))
}

pub fn search(p: &SearchParams, u: &AuthUser) -> Result<DocList> {
    let fts = p.q.as_deref().and_then(fts_query);
    let mut wh: Vec<String> = vec![format!("d.deleted_at IS {} NULL", if p.trash == Some(true) { "NOT" } else { "" }), visible(u)];
    let mut args: Vec<Sql> = vec![];
    if let Some(m) = &fts {
        wh.push("docs_fts MATCH ?".into());
        args.push(m.clone().into());
    }
    let tags: Vec<i64> = p.tags.as_deref().unwrap_or("").split(',').filter_map(|t| t.trim().parse().ok()).collect();
    if !tags.is_empty() {
        let marks = vec!["?"; tags.len()].join(",");
        wh.push(format!(
            "d.id IN (SELECT doc_id FROM document_tags WHERE tag_id IN ({marks}) GROUP BY doc_id HAVING count(*)={})",
            tags.len()
        ));
        args.extend(tags.iter().map(|t| Sql::from(*t)));
    }
    for (col, v) in [("d.correspondent_id", p.correspondent), ("d.doctype_id", p.doctype)] {
        if let Some(v) = v {
            wh.push(format!("{col}=?"));
            args.push(v.into());
        }
    }
    match p.folder {
        Some(0) => wh.push("d.folder_id IS NULL".into()),
        Some(f) => {
            wh.push(format!("d.folder_id IN ({SUBTREE})"));
            args.push(f.into());
        }
        None => {}
    }
    for (op, v) in [(">=", &p.from), ("<=", &p.to)] {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            wh.push(format!("d.created {op} ?"));
            args.push(v.to_string().into());
        }
    }
    if p.expiring == Some(true) {
        wh.push("d.expires <= date('now','localtime','+60 days')".into());
    }
    if p.inbox == Some(true) {
        wh.push("d.id IN (SELECT doc_id FROM document_tags t JOIN labels l ON l.id=t.tag_id WHERE l.is_inbox)".into());
    }
    let from = if fts.is_some() { "docs_fts JOIN documents d ON d.id=docs_fts.rowid" } else { "documents d" };
    let (snip, rank) = if fts.is_some() {
        ("snippet(docs_fts, 1, char(1), char(2), '…', 24)", "bm25(docs_fts, 8.0, 1.0)")
    } else {
        ("NULL", "0")
    };
    let dir = if p.order.as_deref() == Some("asc") { "ASC" } else { "DESC" };
    let order = match p.sort.as_deref().unwrap_or(if fts.is_some() { "relevance" } else if p.expiring == Some(true) { "expires" } else { "created" }) {
        "expires" => format!("d.expires IS NULL, d.expires {}", if p.order.as_deref() == Some("desc") { "DESC" } else { "ASC" }),
        "relevance" if fts.is_some() => format!("{rank} ASC, d.created DESC"),
        "added" => format!("d.added {dir}, d.id {dir}"),
        "modified" => format!("d.modified {dir}"),
        "title" => format!("d.title COLLATE NOCASE {dir}"),
        "asn" => format!("d.asn IS NULL, d.asn {dir}"),
        _ => format!("d.created {dir}, d.id {dir}"),
    };
    let wh = wh.join(" AND ");
    Ok(db::with(|c| {
        let (total, amount_total) = c.query_row(&format!("SELECT count(*), sum(d.amount) FROM {from} WHERE {wh}"), params_from_iter(&args), |r| Ok((r.get(0)?, r.get(1)?)))?;
        args.push(p.limit.unwrap_or(50).clamp(1, 200).into());
        args.push(p.offset.unwrap_or(0).max(0).into());
        let items = c
            .prepare_cached(&format!("SELECT {DOC_COLS}, {snip} FROM {from} WHERE {wh} ORDER BY {order} LIMIT ? OFFSET ?"))?
            .query_map(params_from_iter(&args), |r| {
                let mut d = doc_row(r)?;
                d.snippet = r.get(25)?;
                Ok(d)
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(DocList { total, amount_total, items })
    })?)
}

#[derive(Deserialize, ToSchema, Default, Debug)]
pub struct DocPatch {
    pub title: Option<String>,
    /// Document date (YYYY-MM-DD)
    pub created: Option<String>,
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub correspondent_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub doctype_id: Option<Option<i64>>,
    /// Replaces all tags
    pub tags: Option<Vec<i64>>,
    pub add_tags: Option<Vec<i64>>,
    pub remove_tags: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub asn: Option<Option<i64>>,
    /// Replaces the custom fields
    pub custom: Option<Value>,
    pub content: Option<String>,
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<String>)]
    pub summary: Option<Option<String>>,
    /// Private (owner and administrators only). Only the owner or an administrator can change it.
    pub is_private: Option<bool>,
    /// Destination folder; null takes it out of its folder
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<i64>)]
    pub folder_id: Option<Option<i64>>,
    /// Expiry date (YYYY-MM-DD); null clears it
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<String>)]
    pub expires: Option<Option<String>>,
    /// Total amount; null clears it
    #[serde(default, deserialize_with = "dbl")]
    #[schema(value_type = Option<f64>)]
    pub amount: Option<Option<f64>>,
}

fn apply_patch(c: &Connection, id: i64, p: &DocPatch) -> rusqlite::Result<()> {
    let n = c.execute(
        "UPDATE documents SET title=coalesce(?2,title), created=coalesce(?3,created), content=coalesce(?4,content),
           custom=coalesce(?5,custom), is_private=coalesce(?6,is_private), modified=datetime('now') WHERE id=?1",
        params![id, p.title, p.created, p.content, p.custom.as_ref().map(Value::to_string), p.is_private],
    )?;
    if n == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    for (col, v) in [("correspondent_id", p.correspondent_id), ("doctype_id", p.doctype_id), ("asn", p.asn), ("folder_id", p.folder_id)] {
        if let Some(v) = v {
            // Lo que el usuario decide a mano deja sin efecto la sugerencia para ese campo.
            c.execute(&format!("UPDATE documents SET {col}=?, suggest=json_remove(suggest, '$.{col}') WHERE id=?"), params![v, id])?;
        }
    }
    if let Some(v) = &p.expires {
        c.execute("UPDATE documents SET expires=? WHERE id=?", params![v, id])?;
    }
    if let Some(v) = p.amount {
        c.execute("UPDATE documents SET amount=? WHERE id=?", params![v, id])?;
    }
    if let Some(s) = &p.summary {
        c.execute("UPDATE documents SET summary=? WHERE id=?", params![s, id])?;
    }
    if p.tags.is_some() {
        c.execute("DELETE FROM document_tags WHERE doc_id=?", [id])?;
    }
    for t in p.tags.iter().chain(&p.add_tags).flatten() {
        c.execute("INSERT OR IGNORE INTO document_tags(doc_id, tag_id) SELECT ?, id FROM labels WHERE id=? AND kind='tag'", [id, *t])?;
    }
    for t in p.remove_tags.iter().flatten() {
        c.execute("DELETE FROM document_tags WHERE doc_id=? AND tag_id=?", [id, *t])?;
    }
    Ok(())
}

fn check_patch(p: &DocPatch) -> Result<()> {
    if p.folder_id.flatten().is_some_and(|f| !folder_exists(f)) {
        return Err(Error::Bad("that folder does not exist".into()));
    }
    if p.expires.as_ref().and_then(|e| e.as_deref()).is_some_and(|e| !valid_date(e)) {
        return Err(Error::Bad("expires must be YYYY-MM-DD".into()));
    }
    match &p.created {
        Some(d) if !valid_date(d) => Err(Error::Bad("created must be YYYY-MM-DD".into())),
        _ => Ok(()),
    }
}

pub fn update_doc(id: i64, p: &DocPatch) -> Result<Document> {
    check_patch(p)?;
    db::write(|c| apply_patch(c, id, p))?;
    emit("document.updated", id);
    get_doc(id)
}

pub fn valid_date(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && d.split('-').all(|x| x.parse::<u32>().is_ok())
}

#[derive(Deserialize, ToSchema, Debug)]
pub struct BulkEdit {
    pub ids: Vec<i64>,
    /// Optional: trash | restore | purge (purge is for administrators only)
    pub action: Option<String>,
    #[serde(flatten)]
    pub patch: DocPatch,
}

pub fn bulk(b: &BulkEdit) -> Result<usize> {
    match b.action.as_deref() {
        Some("trash") => b.ids.iter().try_for_each(|id| trash(*id, true))?,
        Some("restore") => b.ids.iter().try_for_each(|id| trash(*id, false))?,
        Some("purge") => b.ids.iter().try_for_each(|id| purge(*id))?,
        Some(a) => return Err(Error::Bad(format!("unknown action: {a}"))),
        None => {
            check_patch(&b.patch)?;
            db::write(|c| b.ids.iter().try_for_each(|id| apply_patch(c, *id, &b.patch)))?;
            b.ids.iter().for_each(|id| emit("document.updated", *id));
        }
    }
    Ok(b.ids.len())
}

pub fn trash(id: i64, on: bool) -> Result<()> {
    let sql = if on { "UPDATE documents SET deleted_at=datetime('now') WHERE id=?" } else { "UPDATE documents SET deleted_at=NULL WHERE id=?" };
    if db::with(|c| c.execute(sql, [id]))? == 0 {
        return Err(Error::NotFound);
    }
    emit(if on { "document.deleted" } else { "document.updated" }, id);
    Ok(())
}

/// Borrado definitivo: fila y archivos.
pub fn purge(id: i64) -> Result<()> {
    if db::with(|c| c.execute("DELETE FROM documents WHERE id=?", [id]))? == 0 {
        return Err(Error::NotFound);
    }
    let _ = std::fs::remove_dir_all(doc_dir(id));
    emit("document.purged", id);
    Ok(())
}

pub fn add_note(doc_id: i64, text: &str) -> Result<Note> {
    Ok(db::with(|c| {
        c.query_row("INSERT INTO notes(doc_id, text) VALUES(?,?) RETURNING id, text, created", params![doc_id, text], |r| {
            Ok(Note { id: r.get(0)?, text: r.get(1)?, created: r.get(2)? })
        })
    })?)
}

/// Documento al que pertenece una nota.
pub fn note_doc(id: i64) -> Result<i64> {
    Ok(db::with(|c| c.query_row("SELECT doc_id FROM notes WHERE id=?", [id], |r| r.get(0)))?)
}

pub fn delete_note(id: i64) -> Result<()> {
    db::with(|c| c.execute("DELETE FROM notes WHERE id=?", [id]))?;
    Ok(())
}

// ───────────────────────── eventos: SSE + webhooks ─────────────────────────

static BUS: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();

pub fn bus() -> &'static tokio::sync::broadcast::Sender<String> {
    BUS.get_or_init(|| tokio::sync::broadcast::channel(256).0)
}

/// Avisa a la UI (SSE) sin disparar webhooks.
pub fn notify(v: Value) {
    let _ = bus().send(v.to_string());
}

/// Evento de documento: UI + webhooks suscritos. Las entregas se guardan en una cola en la base
/// (tabla `outbox`), así que sobreviven a un reinicio y se reintentan con espera creciente.
pub fn emit(event: &'static str, doc_id: i64) {
    notify(json!({"type": event, "id": doc_id}));
    let body = json!({"event": event, "document_id": doc_id, "document": get_doc_brief(doc_id).ok()}).to_string();
    let _ = db::with(|c| {
        c.execute(
            "INSERT INTO outbox(hook_id, body) SELECT id, ?1 FROM webhooks WHERE events='*' OR instr(','||events||',', ','||?2||',')",
            params![body, event],
        )
    });
}

/// ¿La URL apunta a una dirección pública? Evita que un webhook se use para alcanzar servicios
/// internos (SSRF). `PAPIRO_WEBHOOK_ALLOW_PRIVATE=true` lo permite para redes de confianza.
pub fn webhook_target_ok(url: &str) -> std::result::Result<(), String> {
    use std::net::{IpAddr, ToSocketAddrs};
    let (https, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(r), _) => (true, r),
        (_, Some(r)) => (false, r),
        _ => return Err("the URL must start with http:// or https://".into()),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("");
    let (host, port) = match authority.strip_prefix('[').and_then(|a| a.split_once(']')) {
        Some((h, p)) => (h, p.strip_prefix(':')),
        None => authority.rsplit_once(':').map_or((authority, None), |(h, p)| (h, Some(p))),
    };
    let port = port.and_then(|p| p.parse::<u16>().ok()).unwrap_or(if https { 443 } else { 80 });
    if host.is_empty() {
        return Err("the URL has no host".into());
    }
    if cfg().webhook_private {
        return Ok(());
    }
    let addrs: Vec<_> = (host, port).to_socket_addrs().map_err(|e| format!("could not resolve {host}: {e}"))?.collect();
    let internal = |ip: IpAddr| match ip.to_canonical() {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || v.octets()[0] == 100 && (v.octets()[1] & 0xc0) == 64,
        IpAddr::V6(v) => v.is_loopback() || v.is_unspecified() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
    };
    if addrs.is_empty() || addrs.iter().any(|a| internal(a.ip())) {
        return Err(format!("{host} points to an internal address, which is not allowed (set PAPIRO_WEBHOOK_ALLOW_PRIVATE=true to allow it)"));
    }
    Ok(())
}

/// Un intento de entrega con firma HMAC-SHA256. Devuelve el código HTTP o el error.
pub fn deliver(id: i64, url: &str, secret: &str, body: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let status = match webhook_target_ok(url) {
        Err(e) => format!("blocked: {e}"),
        Ok(()) => {
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).expect("hmac");
            mac.update(body.as_bytes());
            let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
            let res = crate::http()
                .post(url)
                .config()
                .timeout_global(Some(std::time::Duration::from_secs(15)))
                .max_redirects(0)
                .build()
                .header("Content-Type", "application/json")
                .header("User-Agent", "papiro-webhook")
                .header("X-Papiro-Signature", &sig)
                .send(body);
            match res {
                Ok(r) => r.status().as_u16().to_string(),
                Err(e) => format!("error: {e}"),
            }
        }
    };
    let _ = db::with(|c| c.execute("UPDATE webhooks SET last_status=?, last_at=datetime('now') WHERE id=?", params![status, id]));
    status
}

/// Hilo que vacía la cola de webhooks: hasta 6 intentos por aviso, con espera creciente.
pub fn outbox_worker() {
    loop {
        let due: Vec<(i64, i64, String, String, String, i64)> = db::with(|c| {
            c.prepare_cached(
                "SELECT o.id, w.id, w.url, w.secret, o.body, o.attempts FROM outbox o JOIN webhooks w ON w.id=o.hook_id
                 WHERE o.next_at <= datetime('now') ORDER BY o.id LIMIT 20",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect()
        })
        .unwrap_or_default();
        for (id, hook, url, secret, body, attempts) in due {
            let status = deliver(hook, &url, &secret, &body);
            let _ = db::with(|c| {
                if status.starts_with('2') || attempts >= 5 {
                    c.execute("DELETE FROM outbox WHERE id=?", [id])
                } else {
                    // 1, 4, 9, 16, 25 minutos
                    c.execute("UPDATE outbox SET attempts=attempts+1, next_at=datetime('now', '+' || ((attempts+1)*(attempts+1)) || ' minutes') WHERE id=?", [id])
                }
            });
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

// ───────────────────────── auditoría ─────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub username: String,
    pub action: String,
    /// What it affected (e.g. "document 12", or a username)
    pub target: String,
    pub detail: String,
    pub ip: String,
}

pub fn audit_raw(user_id: Option<i64>, username: &str, action: &str, target: &str, detail: &str, ip: &str) {
    let _ = db::with(|c| {
        c.execute("INSERT INTO audit(user_id, username, action, target, detail, ip) VALUES(?,?,?,?,?,?)", params![user_id, username, action, target, detail, ip])
    });
}

/// Registra quién hizo qué. `user` es None para acciones del propio sistema.
pub fn audit(user: Option<&AuthUser>, action: &str, target: &str, detail: &str) {
    match user {
        Some(u) => audit_raw(Some(u.id), &u.username, action, target, detail, &u.ip),
        None => audit_raw(None, "system", action, target, detail, ""),
    }
}

pub fn list_audit(limit: i64, offset: i64) -> Result<Vec<AuditEntry>> {
    Ok(db::with(|c| {
        c.prepare_cached("SELECT id, at, username, action, target, detail, ip FROM audit ORDER BY id DESC LIMIT ? OFFSET ?")?
            .query_map([limit.clamp(1, 500), offset.max(0)], |r| {
                Ok(AuditEntry { id: r.get(0)?, at: r.get(1)?, username: r.get(2)?, action: r.get(3)?, target: r.get(4)?, detail: r.get(5)?, ip: r.get(6)? })
            })?
            .collect()
    })?)
}

// ───────────────────────── estado e integridad ─────────────────────────

pub fn meta_set(key: &str, value: &str) {
    let _ = db::with(|c| c.execute("INSERT INTO meta(key, value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key, value]));
}

pub fn meta_get(key: &str) -> Option<String> {
    db::with(|c| c.query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0)).optional()).ok().flatten()
}

#[derive(Serialize, ToSchema)]
pub struct Stats {
    pub documents: i64,
    pub inbox: i64,
    pub trash: i64,
    pub pages: i64,
    pub bytes: i64,
    pub queued: i64,
    /// Failed ingest jobs still waiting to be reviewed
    pub failed: i64,
    /// Documents that have expired or expire within the next 60 days
    pub expiring: i64,
    pub version: &'static str,
}

pub fn stats(u: &AuthUser) -> Result<Stats> {
    let v = visible(u);
    Ok(db::with(|c| {
        c.query_row(
            &format!(
                "SELECT (SELECT count(*) FROM documents d WHERE d.deleted_at IS NULL AND {v}),
                    (SELECT count(DISTINCT t.doc_id) FROM document_tags t JOIN labels l ON l.id=t.tag_id JOIN documents d ON d.id=t.doc_id WHERE l.is_inbox AND d.deleted_at IS NULL AND {v}),
                    (SELECT count(*) FROM documents d WHERE d.deleted_at IS NOT NULL AND {v}),
                    (SELECT coalesce(sum(pages),0) FROM documents d WHERE {v}), (SELECT coalesce(sum(size),0) FROM documents d WHERE {v}),
                    (SELECT count(*) FROM jobs WHERE status IN ('queued','running')), (SELECT count(*) FROM jobs WHERE status='failed'),
                    (SELECT count(*) FROM documents d WHERE d.deleted_at IS NULL AND d.expires <= date('now','localtime','+60 days') AND {v})"
            ),
            [],
            |r| {
                Ok(Stats {
                    documents: r.get(0)?,
                    inbox: r.get(1)?,
                    trash: r.get(2)?,
                    pages: r.get(3)?,
                    bytes: r.get(4)?,
                    queued: r.get(5)?,
                    failed: r.get(6)?,
                    expiring: r.get(7)?,
                    version: env!("CARGO_PKG_VERSION"),
                })
            },
        )
    })?)
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct Problem {
    pub document_id: i64,
    pub title: String,
    pub problem: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct VerifyReport {
    pub at: String,
    /// The SHA-256 of every original was recomputed (false = existence and size only)
    pub deep: bool,
    pub documents: i64,
    /// Result of SQLite's `PRAGMA integrity_check`
    pub database: String,
    pub search_index_ok: bool,
    pub problems: Vec<Problem>,
    pub seconds: f64,
}

/// Comprueba que cada documento tiene sus archivos y que el original conserva su huella.
pub fn verify(deep: bool) -> VerifyReport {
    let t0 = std::time::Instant::now();
    let rows: Vec<(i64, String, String, i64, bool)> =
        db::with(|c| c.prepare("SELECT id, title, sha256, size, has_archive FROM documents")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect())
            .unwrap_or_default();
    let mut problems = vec![];
    for (id, title, sha, size, has_archive) in &rows {
        let dir = doc_dir(*id);
        let original = dir.join("original");
        let issue = match std::fs::metadata(&original) {
            Err(_) => Some("the original file is missing".to_string()),
            Ok(m) if m.len() as i64 != *size => Some(format!("the original is {} bytes but should be {size}", m.len())),
            Ok(_) if deep && !crate::ingest::original_ok(*id, sha) => Some("the original does not match its SHA-256".to_string()),
            Ok(_) if *has_archive && !dir.join("archive.pdf").is_file() => Some("the archive PDF is missing".to_string()),
            Ok(_) => None,
        };
        if let Some(problem) = issue {
            problems.push(Problem { document_id: *id, title: title.clone(), problem });
        }
    }
    let database = db::with(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))).unwrap_or_else(|e| e.to_string());
    let search_index_ok = db::with(|c| c.execute("INSERT INTO docs_fts(docs_fts) VALUES('integrity-check')", [])).is_ok();
    let at = db::with(|c| c.query_row("SELECT datetime('now')", [], |r| r.get(0))).unwrap_or_default();
    let report = VerifyReport { at, deep, documents: rows.len() as i64, database, search_index_ok, problems, seconds: (t0.elapsed().as_secs_f64() * 100.0).round() / 100.0 };
    meta_set("verify", &serde_json::to_string(&report).unwrap_or_default());
    report
}

/// Correcciones de datos tras migrar a la versión 2 del esquema: las versiones anteriores guardaban
/// el original como `original.<ext>`, pero la aplicación lo lee como `original`.
pub fn repair_layout() -> usize {
    let rows: Vec<(i64, String)> = db::with(|c| c.prepare("SELECT id, ext FROM documents")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).unwrap_or_default();
    let mut fixed = 0;
    for (id, ext) in rows {
        let dir = doc_dir(id);
        let (old, new) = (dir.join(format!("original.{ext}")), dir.join("original"));
        if !ext.is_empty() && old.is_file() && !new.exists() && std::fs::rename(&old, &new).is_ok() {
            fixed += 1;
        }
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_is_safe_and_prefixes_last_word() {
        assert_eq!(fts_query("factura cfe").unwrap(), r#""factura" "cfe"*"#);
        assert_eq!(fts_query(r#""estado de cuenta" bbva"#).unwrap(), r#""estado de cuenta" "bbva"*"#);
        assert_eq!(fts_query(r#"REF-512379"#).unwrap(), r#""REF-512379"*"#);
        assert_eq!(fts_query(r#"a" OR NOT ( * "#).unwrap(), r#""a" "OR NOT ( *""#);
        assert_eq!(fts_query(" - \"\" * "), None);
        assert!(valid_date("2024-03-12") && !valid_date("12/03/2024"));
    }

    #[test]
    fn webhook_targets() {
        assert!(webhook_target_ok("ftp://x").is_err());
        assert!(webhook_target_ok("http://127.0.0.1:8080/hook").is_err());
        assert!(webhook_target_ok("http://localhost/hook").is_err());
        assert!(webhook_target_ok("http://10.0.0.5/hook").is_err());
        assert!(webhook_target_ok("http://[::1]:9000/hook").is_err());
        assert!(webhook_target_ok("http://169.254.169.254/latest/meta-data").is_err());
        assert!(webhook_target_ok("http://usuario:clave@192.168.1.1/x").is_err());
    }
}
