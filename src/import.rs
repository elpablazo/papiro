//! Importador del export de Paperless-ngx (`document_exporter`): manifest.json + archivos.
//! Trae documentos (original, PDF de archivo, texto ya extraído, fechas, ASN), etiquetas, corresponsales,
//! tipos con sus reglas, notas y campos personalizados. No vuelve a hacer OCR. Es idempotente: se puede
//! repetir y lo ya importado (mismo SHA-256) se omite.
use crate::{db, ingest, store};
use rusqlite::{OptionalExtension, params};
use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, HashMap}, fmt, fs, io::BufReader, path::{Path, PathBuf}};

/// Recorre un arreglo JSON elemento por elemento sin cargar el manifiesto entero en memoria.
struct Each<'a>(&'a mut dyn FnMut(Value) -> Result<(), String>);
impl<'de> Visitor<'de> for Each<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(v) = seq.next_element::<Value>()? {
            (self.0)(v).map_err(serde::de::Error::custom)?;
        }
        Ok(())
    }
}
impl<'de> DeserializeSeed<'de> for Each<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

fn manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            manifests(&p, out);
        } else if name == "manifest.json" || name.ends_with("-manifest.json") {
            out.push(p);
        }
    }
}

fn each_record(files: &[PathBuf], f: &mut dyn FnMut(Value) -> Result<(), String>) -> Result<(), String> {
    for file in files {
        let mut de = serde_json::Deserializer::from_reader(BufReader::new(fs::File::open(file).map_err(|e| e.to_string())?));
        Each(f).deserialize(&mut de).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

const ALGOS: [&str; 7] = ["none", "any", "all", "literal", "regex", "fuzzy", "auto"];

#[derive(Default)]
struct Maps {
    labels: HashMap<(&'static str, i64), i64>,
    storage_paths: HashMap<i64, String>,
    /// pk del campo → (nombre, opciones de select id → etiqueta)
    fields: HashMap<i64, (String, HashMap<String, String>)>,
    docs: HashMap<i64, i64>,
}

pub fn run(dir: &Path) -> Result<String, String> {
    let mut files = vec![];
    manifests(dir, &mut files);
    // El manifiesto principal primero: trae etiquetas y demás antes que los documentos.
    files.sort_by_key(|p| (p.file_name().is_some_and(|n| n != "manifest.json"), p.clone()));
    if files.is_empty() {
        return Err(format!("no manifest.json in {}", dir.display()));
    }
    let (mut m, mut n, mut ignored) = (Maps::default(), BTreeMap::<&str, usize>::new(), BTreeMap::<String, usize>::new());
    let mut parents: Vec<(i64, i64)> = vec![];
    let e = |e: rusqlite::Error| e.to_string();

    // Pasada 1: catálogos y documentos.
    each_record(&files, &mut |r| {
        let (model, pk, f) = (r["model"].as_str().unwrap_or_default(), r["pk"].as_i64().unwrap_or(0), &r["fields"]);
        let kind = match model {
            "documents.tag" => Some("tag"),
            "documents.correspondent" => Some("correspondent"),
            "documents.documenttype" => Some("doctype"),
            _ => None,
        };
        if let Some(kind) = kind {
            let algo = ALGOS.get(f["matching_algorithm"].as_u64().unwrap_or(0) as usize).unwrap_or(&"none");
            let id: i64 = db::with(|c| {
                c.query_row(
                    "INSERT INTO labels(kind, name, color, is_inbox, match, algo, insensitive) VALUES(?,?,coalesce(?, '#64748b'),?,?,?,?)
                     ON CONFLICT(kind, name) DO UPDATE SET match=excluded.match, algo=excluded.algo, insensitive=excluded.insensitive,
                       is_inbox=excluded.is_inbox, color=excluded.color RETURNING id",
                    params![kind, f["name"].as_str(), f["color"].as_str(), f["is_inbox_tag"].as_bool().unwrap_or(false),
                            f["match"].as_str().unwrap_or(""), algo, f["is_insensitive"].as_bool().unwrap_or(true)],
                    |r| r.get(0),
                )
            })
            .map_err(e)?;
            m.labels.insert((kind, pk), id);
            if let Some(parent) = f["tn_parent"].as_i64().or(f["parent"].as_i64()) {
                parents.push((pk, parent));
            }
            *n.entry(match kind { "tag" => "tags", "correspondent" => "correspondents", _ => "document types" }).or_default() += 1;
            return Ok(());
        }
        match model {
            "documents.storagepath" => {
                m.storage_paths.insert(pk, f["name"].as_str().unwrap_or_default().to_string());
            }
            "documents.customfield" => {
                let opts = f["extra_data"]["select_options"].as_array().into_iter().flatten()
                    .filter_map(|o| Some((o["id"].as_str()?.to_string(), o["label"].as_str()?.to_string()))).collect();
                m.fields.insert(pk, (f["name"].as_str().unwrap_or_default().to_string(), opts));
            }
            "documents.document" => import_doc(dir, &r, &mut m, &mut n)?,
            "documents.note" | "documents.customfieldinstance" => {} // pasada 2
            other => *ignored.entry(other.to_string()).or_default() += 1,
        }
        Ok(())
    })?;
    for (child, parent) in parents {
        if let (Some(c), Some(p)) = (m.labels.get(&("tag", child)), m.labels.get(&("tag", parent))) {
            db::with(|db| db.execute("UPDATE labels SET parent_id=? WHERE id=?", [p, c])).map_err(e)?;
        }
    }

    // Pasada 2: lo que cuelga de los documentos (en el manifiesto aparece antes que ellos).
    each_record(&files, &mut |r| {
        let f = &r["fields"];
        let Some(&doc) = f["document"].as_i64().and_then(|pk| m.docs.get(&pk)) else { return Ok(()) };
        match r["model"].as_str().unwrap_or_default() {
            "documents.note" => {
                let text = f["note"].as_str().unwrap_or_default();
                let added = db::with(|c| {
                    c.execute(
                        "INSERT INTO notes(doc_id, text, created) SELECT ?1, ?2, coalesce(datetime(?3), datetime('now'))
                         WHERE NOT EXISTS (SELECT 1 FROM notes WHERE doc_id=?1 AND text=?2)",
                        params![doc, text, f["created"].as_str()],
                    )
                })
                .map_err(e)?;
                *n.entry("notes").or_default() += added;
            }
            "documents.customfieldinstance" => {
                let Some((name, opts)) = f["field"].as_i64().and_then(|pk| m.fields.get(&pk)) else { return Ok(()) };
                let value = ["value_text", "value_long_text", "value_url", "value_date", "value_monetary", "value_int", "value_float", "value_bool", "value_document_ids"]
                    .iter().map(|k| &f[*k]).find(|v| !v.is_null()).cloned()
                    .or_else(|| f["value_select"].as_str().map(|id| json!(opts.get(id).cloned().unwrap_or_else(|| id.to_string()))))
                    .unwrap_or(Value::Null);
                db::with(|c| c.execute("UPDATE documents SET custom=json_patch(custom, ?) WHERE id=?", params![json!({name: value}).to_string(), doc])).map_err(e)?;
                *n.entry("custom field values").or_default() += 1;
            }
            _ => {}
        }
        Ok(())
    })?;

    let mut out = String::from("Import finished\n");
    for (k, v) in &n {
        out.push_str(&format!("  {v:>6}  {k}\n"));
    }
    if !ignored.is_empty() {
        out.push_str("Not migrated (no equivalent in Papiro):\n");
        for (k, v) in &ignored {
            out.push_str(&format!("  {v:>6}  {k}\n"));
        }
    }
    Ok(out)
}

fn import_doc(dir: &Path, r: &Value, m: &mut Maps, n: &mut BTreeMap<&'static str, usize>) -> Result<(), String> {
    let (pk, f) = (r["pk"].as_i64().unwrap_or(0), &r["fields"]);
    let e = |e: rusqlite::Error| e.to_string();
    let Some(src) = r["__exported_file_name__"].as_str().map(|p| dir.join(p)).filter(|p| p.is_file()) else {
        *n.entry("documents without a file in the export (skipped)").or_default() += 1;
        return Ok(());
    };
    let sha = ingest::sha256_file(&src).map_err(|e| e.to_string())?;
    let archive = r["__exported_archive_name__"].as_str().map(|p| dir.join(p)).filter(|p| p.is_file());
    if let Some(id) = db::with(|c| c.query_row("SELECT id FROM documents WHERE sha256=?", [&sha], |r| r.get(0)).optional()).map_err(e)? {
        // Ya estaba importado. Si su archivo falta o está dañado, se repone desde el export.
        let mut repaired = false;
        if !ingest::original_ok(id, &sha) {
            ingest::restore_original(id, &src, &sha).map_err(|x| format!("could not restore document {id}: {x}"))?;
            repaired = true;
        }
        if let Some(a) = archive.as_ref().filter(|_| !store::doc_dir(id).join("archive.pdf").is_file()) {
            fs::copy(a, store::doc_dir(id).join("archive.pdf")).map_err(|x| x.to_string())?;
            repaired = true;
        }
        m.docs.insert(pk, id);
        *n.entry(if repaired { "documents repaired (a file was missing)" } else { "documents that already existed (skipped)" }).or_default() += 1;
        return Ok(());
    }
    // Paperless guarda SHA-256 del original: sirve para comprobar que el export no se corrompió.
    if f["checksum"].as_str().is_some_and(|c| c.len() == 64 && !c.eq_ignore_ascii_case(&sha)) {
        *n.entry("documents whose checksum differs from the manifest (imported anyway)").or_default() += 1;
    }
    let name = f["original_filename"].as_str().or(r["__exported_file_name__"].as_str()).unwrap_or("document");
    let name = name.rsplit('/').next().unwrap_or(name);
    let ext = Path::new(name).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    let label = |kind: &'static str, key: &str| f[key].as_i64().and_then(|pk| m.labels.get(&(kind, pk)).copied());
    // Paperless no guarda el número de páginas de algunas imágenes: se cuenta del PDF.
    let pdf = archive.clone().or_else(|| (ext == "pdf").then(|| src.clone()));
    let pages = f["page_count"].as_i64().filter(|p| *p > 0).or_else(|| pdf.as_ref().and_then(|p| ingest::pdf_pages(p).ok())).unwrap_or(0);
    let mut custom = json!({});
    // La ruta de almacenamiento de Paperless se convierte en la carpeta del documento.
    let folder = f["storage_path"].as_i64().and_then(|pk| m.storage_paths.get(&pk)).and_then(|sp| store::ensure_folder(&sp.replace('/', " - ")).ok().flatten());
    if let Some(root) = f["root_document"].as_i64() {
        custom["Versión del documento Paperless"] = json!(root);
    }
    // Los archivos se copian y verifican en una carpeta temporal; la fila y la carpeta aparecen juntas.
    let stage = crate::cfg().data.join("tmp").join(format!("stage-import-{pk}"));
    let io = |x: std::io::Error| format!("could not copy {}: {x}", src.display());
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(io)?;
    ingest::copy_verified(&src, &stage.join("original"), &sha).map_err(io)?;
    if let Some(a) = &archive {
        fs::copy(a, stage.join("archive.pdf")).map_err(io)?;
    }
    let inserted = db::write(|c| {
        let id: i64 = c.query_row(
            "INSERT INTO documents(id, title, content, created, added, modified, correspondent_id, doctype_id, asn, original_name, mime, ext,
                                   size, pages, sha256, has_archive, has_thumb, custom, deleted_at, folder_id)
             VALUES(?, ?, ?, coalesce(date(?), date('now')), coalesce(datetime(?), datetime('now')), coalesce(datetime(?), datetime('now')),
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime(?), ?) RETURNING id",
            params![store::next_doc_id(c)?, f["title"].as_str().unwrap_or(name), f["content"].as_str().unwrap_or(""), f["created"].as_str(), f["added"].as_str(), f["modified"].as_str(),
                    label("correspondent", "correspondent"), label("doctype", "document_type"), f["archive_serial_number"].as_i64(), name,
                    f["mime_type"].as_str().unwrap_or("application/octet-stream"), ext, fs::metadata(&src).map(|x| x.len() as i64).unwrap_or(0),
                    // La miniatura se genera la primera vez que se pide (importar 10 000 documentos no lanza 10 000 procesos).
                    pages, sha, archive.is_some(), pdf.is_some(), custom.to_string(), f["deleted_at"].as_str(), folder],
            |r| r.get(0),
        )?;
        for t in f["tags"].as_array().into_iter().flatten().filter_map(Value::as_i64) {
            if let Some(tag) = m.labels.get(&("tag", t)) {
                c.execute("INSERT OR IGNORE INTO document_tags(doc_id, tag_id) VALUES(?,?)", [id, *tag])?;
            }
        }
        ingest::place(&stage, id)?;
        Ok(id)
    });
    let id = inserted.map_err(|x| {
        let _ = fs::remove_dir_all(&stage);
        format!("document \"{name}\": {x}")
    })?;
    let _ = crate::suggest::analyze(id, false);
    m.docs.insert(pk, id);
    *n.entry("documents imported").or_default() += 1;
    Ok(())
}
