//! Respaldo, restauración y exportación. Todo el estado de Papiro es `papiro.db` + `docs/`.
use crate::{cfg, db, store};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<(u64, u64)> {
    let (mut copied, mut bytes) = (0, 0);
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if e.file_type()?.is_dir() {
            let (c, b) = copy_tree(&from, &to)?;
            copied += c;
            bytes += b;
        } else {
            let len = e.metadata()?.len();
            // Los archivos de un documento no cambian una vez escritos: si ya está y mide lo mismo, se salta.
            if fs::metadata(&to).map(|m| m.len()).ok() != Some(len) {
                let tmp = to.with_extension("copiando");
                fs::copy(&from, &tmp)?;
                fs::File::open(&tmp)?.sync_all()?;
                fs::rename(&tmp, &to)?;
                copied += 1;
            }
            bytes += len;
        }
    }
    Ok((copied, bytes))
}

/// Copias diarias de la base guardadas en `historial/` (`papiro-AAAA-MM-DD.db`), de la más antigua a la más nueva.
fn history(dest: &Path) -> Vec<String> {
    let mut days: Vec<String> = fs::read_dir(dest.join("historial"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_prefix("papiro-")?.strip_suffix(".db").map(String::from))
        .collect();
    days.sort();
    days
}

/// Respaldo consistente en una carpeta, con el servidor en marcha. Es incremental: repetirlo solo
/// copia lo nuevo. Además de la copia más reciente de la base se conserva una por día
/// (`PAPIRO_BACKUP_KEEP`, 14 por defecto), para poder volver a un día anterior si un error se
/// descubre tarde. Los archivos de documentos nunca se quitan del respaldo, así que cualquier
/// copia del historial se puede restaurar completa.
// ponytail: los archivos de documentos borrados definitivamente se quedan en el respaldo para siempre;
// si el espacio importa, podar los que no aparezcan en ninguna copia del historial.
pub fn backup(dest: &Path) -> Result<String, String> {
    let e = |x: std::io::Error| format!("{}: {x}", dest.display());
    fs::create_dir_all(dest).map_err(e)?;
    let tmp = dest.join("papiro.db.nuevo");
    db::snapshot(&tmp).map_err(|x| format!("could not copy the database: {x}"))?;
    fs::File::open(&tmp).and_then(|f| f.sync_all()).map_err(e)?;
    fs::rename(&tmp, dest.join("papiro.db")).map_err(e)?;
    let keep = cfg().backup_keep as usize;
    if keep > 0 {
        let day: String = db::with(|c| c.query_row("SELECT date('now','localtime')", [], |r| r.get(0))).map_err(|x| x.to_string())?;
        let daily = dest.join("historial").join(format!("papiro-{day}.db"));
        fs::create_dir_all(dest.join("historial")).map_err(e)?;
        let _ = fs::remove_file(&daily);
        // Enlace duro: la copia de hoy no ocupa espacio extra y sobrevive cuando mañana se reemplace papiro.db.
        if fs::hard_link(dest.join("papiro.db"), &daily).is_err() {
            fs::copy(dest.join("papiro.db"), &daily).map_err(e)?;
        }
        let days = history(dest);
        for old in &days[..days.len().saturating_sub(keep)] {
            let _ = fs::remove_file(dest.join("historial").join(format!("papiro-{old}.db")));
        }
    }
    let days = history(dest);
    let (copied, bytes) = copy_tree(&cfg().data.join("docs"), &dest.join("docs")).map_err(e)?;
    let docs: i64 = db::with(|c| c.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))).unwrap_or(0);
    let at: String = db::with(|c| c.query_row("SELECT datetime('now')", [], |r| r.get(0))).unwrap_or_default();
    let info = json!({"created": at, "papiro": env!("CARGO_PKG_VERSION"), "schema": db::schema_version(), "documents": docs, "bytes": bytes, "history": days});
    fs::write(dest.join("RESPALDO.json"), serde_json::to_string_pretty(&info).unwrap_or_default()).map_err(e)?;
    store::meta_set("backup", &json!({"at": at, "dest": dest.to_string_lossy(), "documents": docs, "bytes": bytes, "ok": true, "history": days.len()}).to_string());
    Ok(format!("backup in {}: {docs} documents, {:.1} MB, {copied} new files, {} daily database copies", dest.display(), bytes as f64 / 1048576.0, days.len()))
}

/// Restaura un respaldo en una carpeta de datos vacía. Debe llamarse antes de abrir la base.
/// Con `day` (AAAA-MM-DD) restaura la copia de la base de ese día en lugar de la más reciente.
pub fn restore(src: &Path, data: &Path, day: Option<&str>) -> Result<String, String> {
    let base = match day {
        Some(d) => src.join("historial").join(format!("papiro-{d}.db")),
        None => src.join("papiro.db"),
    };
    if !base.is_file() {
        return Err(match day {
            Some(d) => format!("there is no copy from {d}. Available days: {}", history(src).join(", ")),
            None => format!("{} does not contain a Papiro backup (papiro.db is missing)", src.display()),
        });
    }
    if data.join("papiro.db").exists() {
        return Err(format!("{} already has a database; restore into an empty data directory so nothing is overwritten", data.display()));
    }
    let e = |x: std::io::Error| x.to_string();
    let q = |x: rusqlite::Error| format!("the database copy cannot be read: {x}");
    fs::create_dir_all(data.join("docs")).map_err(e)?;
    fs::create_dir_all(data.join("tmp")).map_err(e)?;
    let staged = data.join("papiro.db.restaurando");
    fs::copy(&base, &staged).map_err(e)?;
    // Solo se traen los archivos de los documentos que esa copia de la base contiene.
    let ids: Vec<i64> = {
        let c = rusqlite::Connection::open(&staged).map_err(q)?;
        let ids = c.prepare("SELECT id FROM documents").and_then(|mut s| s.query_map([], |r| r.get(0))?.collect()).map_err(q)?;
        // Al volver a un día anterior, los números que el respaldo ya usó después no se vuelven a dar.
        let used = fs::read_dir(src.join("docs")).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_string_lossy().parse::<i64>().ok()).max().unwrap_or(0);
        let _ = c.execute("INSERT OR REPLACE INTO meta(key, value) VALUES('doc_seq', ?)", [used.to_string()]);
        ids
    };
    let (mut files, mut bytes) = (0, 0);
    for id in ids {
        let from = src.join("docs").join(id.to_string());
        // Si falta la carpeta de un documento, la verificación que sigue lo señala.
        if from.is_dir() {
            let (f, b) = copy_tree(&from, &data.join("docs").join(id.to_string())).map_err(e)?;
            files += f;
            bytes += b;
        }
    }
    for side in ["papiro.db.restaurando-wal", "papiro.db.restaurando-shm"] {
        let _ = fs::remove_file(data.join(side));
    }
    fs::rename(&staged, data.join("papiro.db")).map_err(e)?;
    Ok(format!("restored {files} files ({:.1} MB) and the {} database", bytes as f64 / 1048576.0, day.map(String::from).unwrap_or_else(|| "latest".into())))
}

/// Exportación portable: un `manifest.json` legible y los archivos con nombres humanos, para no
/// depender de Papiro para recuperar los documentos.
pub fn export(dest: &Path) -> Result<String, String> {
    let e = |x: std::io::Error| x.to_string();
    fs::create_dir_all(dest.join("documentos")).map_err(e)?;
    let labels: std::collections::HashMap<i64, String> =
        store::KINDS.iter().flat_map(|k| store::list_labels(k).unwrap_or_default()).map(|l| (l.id, l.name)).collect();
    let ids: Vec<i64> = db::with(|c| c.prepare("SELECT id FROM documents ORDER BY id")?.query_map([], |r| r.get(0))?.collect()).map_err(|x| x.to_string())?;
    let mut out: Vec<Value> = vec![];
    for id in &ids {
        let d = store::get_doc(*id).map_err(|x| x.to_string())?;
        let clean: String = d.title.chars().map(|c| if c.is_alphanumeric() || " ._-".contains(c) { c } else { '_' }).take(80).collect();
        let ext = Path::new(&d.original_name).extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
        let (name, archive) = (format!("{id:06} {clean}{ext}"), format!("{id:06} {clean} (con texto).pdf"));
        fs::copy(store::doc_dir(*id).join("original"), dest.join("documentos").join(&name)).map_err(|x| format!("document {id}: {x}"))?;
        if d.has_archive {
            fs::copy(store::doc_dir(*id).join("archive.pdf"), dest.join("documentos").join(&archive)).map_err(|x| format!("document {id}: {x}"))?;
        }
        let name_of = |i: Option<i64>| i.and_then(|i| labels.get(&i).cloned());
        out.push(json!({
            "id": d.id, "title": d.title, "date": d.created, "added": d.added, "file": name, "archive_file": d.has_archive.then_some(archive),
            "original_name": d.original_name, "sha256": d.sha256, "pages": d.pages, "asn": d.asn,
            "correspondent": name_of(d.correspondent_id), "document_type": name_of(d.doctype_id),
            "tags": d.tags.iter().filter_map(|t| labels.get(t)).collect::<Vec<_>>(), "summary": d.summary, "custom_fields": d.custom,
            "notes": d.notes, "in_trash": d.deleted_at.is_some(), "private": d.is_private, "text": d.content,
        }));
    }
    fs::write(dest.join("manifest.json"), serde_json::to_string_pretty(&out).unwrap_or_default()).map_err(e)?;
    Ok(format!("exported {} documents to {}", ids.len(), dest.display()))
}
