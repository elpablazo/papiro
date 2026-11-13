//! Análisis local de un documento. Nada sale del servidor: no hay modelos ni llamadas externas.
//!
//! - **Datos por reglas:** vencimiento, importe total, CURP y RFC.
//! - **Sugerencias por parecido:** se buscan, con el índice de búsqueda, los documentos ya archivados
//!   que comparten las palabras más características de este, y se propone lo que ellos tienen
//!   (carpeta, tipo, corresponsal, etiquetas). No hay nada que entrenar: cada documento que se
//!   archiva o se corrige cuenta para el siguiente.
use crate::{db, ingest, store};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

// ponytail: umbrales fijados midiendo con un archivo de prueba (ver docs/AUTOARCHIVADO.md); si con
// archivos reales sugiere de más o de menos, este es el sitio que ajustar.
/// Parecido mínimo para que un documento cuente como vecino (parte de las palabras características que comparte).
const MIN_SIM: f64 = 0.25;
/// Parecido a partir del cual un vecino pesa completo.
const STRONG: f64 = 0.6;
/// Confianza mínima para proponer una sugerencia.
const SUGGEST: f64 = 0.45;
/// Confianza (y al menos dos vecinos de acuerdo) para aplicar sin preguntar.
const AUTO: f64 = 0.7;
const FIELDS: [(&str, &str); 3] = [("folder_id", "folder_id"), ("doctype_id", "doctype_id"), ("correspondent_id", "correspondent_id")];

/// Minúsculas y sin acentos, como el índice de búsqueda (unicode61 remove_diacritics).
pub fn norm(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

fn head(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// Palabras (solo letras, de 3 a 30) normalizadas.
fn words(text: &str) -> impl Iterator<Item = String> {
    norm(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| (3..=30).contains(&w.chars().count()) && w.chars().all(char::is_alphabetic))
        .map(String::from)
        .collect::<Vec<_>>()
        .into_iter()
}

// ───────────────────────── datos por reglas ─────────────────────────

/// Fecha de vencimiento (YYYY-MM-DD): la más lejana que aparezca justo después de una palabra como
/// «vigencia», «vence» o «válido hasta». Si solo hay un año («VIGENCIA 2029»), el 31 de diciembre.
pub fn expiry(text: &str) -> Option<String> {
    static KEY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\b(vigencia|vigente hasta|vence|vencimiento|valid[oa] hasta|validez|caducidad|caduca|expira|expiracion|expiry|expires|expiration|valid until|valid thru|valid through)\b").unwrap()
    });
    static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(20\d{2})\b").unwrap());
    static SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s(?:al?|hasta|to|until)\s|\s[-–—]\s").unwrap());
    let t = norm(&head(text, 30_000));
    let mut best: Option<String> = None;
    for m in KEY.find_iter(&t) {
        let window = head(&t[m.end()..], 80);
        // «del 01/01/2026 al 31/12/2026»: se mira cada tramo y se toma la fecha más lejana.
        let date = SEP.split(&window).filter_map(ingest::detect_date).max();
        let found = date.or_else(|| YEAR.find_iter(&window).filter_map(|y| y.as_str().parse::<u32>().ok()).max().map(|y| format!("{y}-12-31")));
        if found > best {
            best = found;
        }
    }
    best
}

/// Importe total: el mayor número con formato de dinero (signo $ o decimales) que siga a «total».
pub fn amount(text: &str) -> Option<f64> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:^|[^a-z])total(?: a pagar| general| neto| mxn| m\.n\.)?[^0-9\n]{0,24}?(\$)?\s*(\d{1,3}(?:,\d{3})+(?:\.\d{1,2})?|\d+(?:\.\d{1,2})?)").unwrap()
    });
    RE.captures_iter(&norm(&head(text, 30_000)))
        .filter(|c| c.get(1).is_some() || c[2].contains('.'))
        .filter_map(|c| c[2].replace(',', "").parse::<f64>().ok())
        .filter(|v| *v > 0.0 && *v < 1e9)
        .fold(None, |best: Option<f64>, v| Some(best.map_or(v, |b| b.max(v))))
}

/// CURP y RFC, solo cuando el documento contiene exactamente uno (una factura trae dos RFC y no se adivina cuál).
pub fn identifiers(text: &str) -> Vec<(&'static str, String)> {
    static CURP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z][AEIOUX][A-Z]{2}(\d{2})(\d{2})(\d{2})[HM][A-Z]{5}[A-Z0-9]\d\b").unwrap());
    static RFC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-ZÑ&]{3,4}(\d{2})(\d{2})(\d{2})[A-Z0-9]{3}\b").unwrap());
    let t = head(text, 30_000).to_uppercase();
    let only = |re: &Regex| {
        let found: HashSet<&str> = re
            .captures_iter(&t)
            .filter(|c| (1..=12).contains(&c[2].parse::<u32>().unwrap_or(0)) && (1..=31).contains(&c[3].parse::<u32>().unwrap_or(0)))
            .map(|c| c.get(0).unwrap().as_str())
            .collect();
        (found.len() == 1).then(|| found.into_iter().next().unwrap().to_string())
    };
    [("CURP", only(&CURP)), ("RFC", only(&RFC))].into_iter().filter_map(|(k, v)| Some((k, v?))).collect()
}

// ───────────────────────── sugerencias por parecido ─────────────────────────

struct Neighbor {
    sim: f64,
    fields: [Option<i64>; 3],
    tags: Vec<i64>,
}

/// Documentos ya archivados que más se parecen a este, con qué parte de sus palabras características comparten.
fn neighbors(c: &Connection, id: i64, title: &str, content: &str, owner: Option<i64>) -> rusqlite::Result<Vec<Neighbor>> {
    let n: f64 = c.query_row("SELECT count(*) FROM documents WHERE deleted_at IS NULL", [], |r| r.get(0))?;
    let mut tf: HashMap<String, f64> = HashMap::new();
    for w in words(&format!("{title} {}", head(content, 30_000))) {
        *tf.entry(w).or_default() += 1.0;
    }
    let mut frequent: Vec<(String, f64)> = tf.into_iter().collect();
    frequent.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // Palabras características: frecuentes aquí y raras en el archivo (y presentes en algún otro documento).
    let mut terms: Vec<(String, f64)> = vec![];
    {
        let mut df = c.prepare_cached("SELECT doc FROM docs_vocab WHERE term=?")?;
        for (w, f) in frequent.into_iter().take(400) {
            let docs: f64 = df.query_row([&w], |r| r.get(0)).optional()?.unwrap_or(0.0);
            if docs >= 2.0 && docs < n {
                terms.push((w, (1.0 + f.ln()) * (n / docs).ln()));
            }
        }
    }
    terms.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    terms.truncate(24);
    let total: f64 = terms.iter().map(|t| t.1).sum();
    if terms.len() < 3 || total <= 0.0 {
        return Ok(vec![]);
    }
    let query = terms.iter().map(|(w, _)| format!("\"{w}\"")).collect::<Vec<_>>().join(" OR ");
    // Los documentos privados de otra persona no cuentan: sus datos no deben asomar en una sugerencia.
    let found = c
        .prepare_cached(
            "SELECT d.folder_id, d.doctype_id, d.correspondent_id, (SELECT group_concat(tag_id) FROM document_tags WHERE doc_id=d.id),
                    d.title || ' ' || substr(d.content, 1, 30000)
             FROM docs_fts JOIN documents d ON d.id=docs_fts.rowid
             WHERE docs_fts MATCH ?1 AND d.id != ?2 AND d.deleted_at IS NULL AND (d.is_private=0 OR d.owner_id IS ?3)
             ORDER BY bm25(docs_fts) LIMIT 12",
        )?
        .query_map(params![query, id, owner], |r| {
            let tags: Option<String> = r.get(3)?;
            let text: String = r.get(4)?;
            let has: HashSet<String> = words(&text).collect();
            Ok(Neighbor {
                sim: terms.iter().filter(|t| has.contains(&t.0)).map(|t| t.1).sum::<f64>() / total,
                fields: [r.get(0)?, r.get(1)?, r.get(2)?],
                tags: tags.map(|t| t.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(found.into_iter().filter(|x| x.sim >= MIN_SIM).collect())
}

/// Para cada valor que tienen los vecinos: (valor, confianza, cuántos vecinos lo tienen), de mayor a menor confianza.
/// La confianza es la parte del parecido que respalda ese valor, rebajada si ni el mejor vecino se parece mucho.
/// Un vecino que aún no tiene ese dato (p. ej. sin carpeta) no cuenta: no es un voto en contra.
fn vote(ns: &[Neighbor], values: impl Fn(&Neighbor) -> Vec<i64>) -> Vec<(i64, f64, usize)> {
    let total: f64 = ns.iter().filter(|n| !values(n).is_empty()).map(|n| n.sim).sum();
    let mut by: HashMap<i64, (f64, f64, usize)> = HashMap::new();
    for n in ns {
        for v in values(n) {
            let e = by.entry(v).or_default();
            *e = (e.0 + n.sim, e.1.max(n.sim), e.2 + 1);
        }
    }
    let mut out: Vec<(i64, f64, usize)> = by.into_iter().map(|(v, (w, top, count))| (v, w / total * (top / STRONG).min(1.0), count)).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Analiza un documento. Las reglas solo rellenan lo que está vacío. Con `similar`, además busca
/// parecidos: lo muy claro se aplica (y queda anotado como aplicado) y lo dudoso queda como sugerencia.
pub fn analyze(id: i64, similar: bool) -> store::Result<()> {
    db::write(|c| {
        let (title, content, fields, expires, amt, custom, owner): (String, String, [Option<i64>; 3], Option<String>, Option<f64>, String, Option<i64>) = c.query_row(
            "SELECT title, content, folder_id, doctype_id, correspondent_id, expires, amount, custom, owner_id FROM documents WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, [r.get(2)?, r.get(3)?, r.get(4)?], r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )?;
        if let Some(d) = expiry(&content).filter(|_| expires.is_none()) {
            c.execute("UPDATE documents SET expires=? WHERE id=?", params![d, id])?;
        }
        if let Some(a) = amount(&content).filter(|_| amt.is_none()) {
            c.execute("UPDATE documents SET amount=? WHERE id=?", params![a, id])?;
        }
        let mut fields_json: Value = serde_json::from_str(&custom).unwrap_or(json!({}));
        let before = fields_json.to_string();
        for (k, v) in identifiers(&content) {
            if fields_json.get(k).is_none() {
                fields_json[k] = v.into();
            }
        }
        if fields_json.to_string() != before {
            c.execute("UPDATE documents SET custom=? WHERE id=?", params![fields_json.to_string(), id])?;
        }
        if !similar {
            return Ok(());
        }
        let ns = neighbors(c, id, &title, &content, owner)?;
        let mut sug = json!({});
        for (i, (key, col)) in FIELDS.iter().enumerate() {
            let Some(&(v, conf, count)) = vote(&ns, |n| n.fields[i].into_iter().collect()).first().filter(|_| fields[i].is_none()) else { continue };
            let auto = conf >= AUTO && count >= 2;
            if auto {
                c.execute(&format!("UPDATE documents SET {col}=? WHERE id=?"), params![v, id])?;
            }
            if auto || conf >= SUGGEST {
                sug[*key] = json!({"value": v, "confidence": round2(conf), "similar": count, "applied": auto});
            }
        }
        let skip: HashSet<i64> = c
            .prepare_cached("SELECT tag_id FROM document_tags WHERE doc_id=?1 UNION SELECT id FROM labels WHERE is_inbox")?
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let tags: Vec<Value> = vote(&ns, |n| n.tags.clone())
            .into_iter()
            .filter(|(v, conf, _)| *conf >= SUGGEST && !skip.contains(v))
            .map(|(v, conf, count)| json!({"value": v, "confidence": round2(conf), "similar": count}))
            .collect();
        if !tags.is_empty() {
            sug["tags"] = tags.into();
        }
        c.execute("UPDATE documents SET suggest=? WHERE id=?", params![sug.to_string(), id])?;
        Ok(())
    })?;
    Ok(())
}

/// Acepta o descarta sugerencias pendientes. `only` limita a ciertas claves (`folder_id`, `doctype_id`,
/// `correspondent_id`, `tag:<id>`); sin él, todas.
pub fn resolve(id: i64, accept: bool, only: Option<&[String]>) -> store::Result<()> {
    let wanted = |k: &str| only.is_none_or(|o| o.iter().any(|x| x == k));
    db::write(|c| {
        let raw: String = c.query_row("SELECT suggest FROM documents WHERE id=?", [id], |r| r.get(0))?;
        let mut sug: Value = serde_json::from_str(&raw).unwrap_or(json!({}));
        for (key, col) in FIELDS {
            let pending = sug.get(key).filter(|s| s["applied"] != true && wanted(key)).and_then(|s| s["value"].as_i64());
            if let Some(v) = pending {
                if accept {
                    c.execute(&format!("UPDATE documents SET {col}=?, modified=datetime('now') WHERE id=?"), params![v, id])?;
                }
                sug.as_object_mut().map(|o| o.remove(key));
            }
        }
        if let Some(tags) = sug.get_mut("tags").and_then(Value::as_array_mut) {
            let mut keep = vec![];
            for t in tags.drain(..) {
                match t["value"].as_i64().filter(|v| wanted(&format!("tag:{v}"))) {
                    Some(v) if accept => {
                        c.execute("INSERT OR IGNORE INTO document_tags(doc_id, tag_id) SELECT ?, id FROM labels WHERE id=? AND kind='tag'", [id, v])?;
                    }
                    Some(_) => {}
                    None => keep.push(t),
                }
            }
            *tags = keep;
        }
        if sug["tags"].as_array().is_some_and(Vec::is_empty) {
            sug.as_object_mut().map(|o| o.remove("tags"));
        }
        c.execute("UPDATE documents SET suggest=? WHERE id=?", params![sug.to_string(), id])?;
        Ok(())
    })?;
    store::emit("document.updated", id);
    Ok(())
}

/// Aplica las reglas a todos los documentos (al actualizar desde una versión sin análisis, o tras importar).
pub fn backfill() -> usize {
    let ids: Vec<i64> = db::with(|c| c.prepare("SELECT id FROM documents")?.query_map([], |r| r.get(0))?.collect()).unwrap_or_default();
    ids.iter().filter(|id| analyze(**id, false).is_ok()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vencimientos() {
        assert_eq!(expiry("CREDENCIAL PARA VOTAR\nVIGENCIA 2019 - 2029").as_deref(), Some("2029-12-31"));
        assert_eq!(expiry("Póliza 123. Vigencia: del 01/02/2026 al 31/01/2027. Prima total").as_deref(), Some("2027-01-31"));
        assert_eq!(expiry("Válido hasta el 5 de marzo de 2030").as_deref(), Some("2030-03-05"));
        assert_eq!(expiry("Fecha de vencimiento 2028-07-15\nOtra cosa. Vence 01/01/2027").as_deref(), Some("2028-07-15"));
        assert_eq!(expiry("Manual de usuario. Fabricado en 2024. Convence a cualquiera"), None);
    }

    #[test]
    fn importes() {
        assert_eq!(amount("Subtotal $1,000.00\nIVA $160.00\nTOTAL $1,160.00"), Some(1160.0));
        assert_eq!(amount("Total a pagar: 845.50 MXN"), Some(845.5));
        assert_eq!(amount("TOTAL MXN $ 2,345"), Some(2345.0));
        assert_eq!(amount("Subtotal 900.00"), None);
        assert_eq!(amount("Total de páginas 12. Total 5 piezas"), None);
    }

    #[test]
    fn identificadores() {
        let one = identifiers("CURP: GOMC900514HDFRRL09 y nada más");
        assert_eq!(one, vec![("CURP", "GOMC900514HDFRRL09".to_string())]);
        assert_eq!(identifiers("RFC emisor ABC010203XY1, RFC receptor GOMC900514AB2"), vec![]);
        assert_eq!(identifiers("rfc GOMC900514AB2"), vec![("RFC", "GOMC900514AB2".to_string())]);
        assert_eq!(identifiers("código ABCD991399XYZ"), vec![]); // mes 13: no es una fecha
    }

    #[test]
    fn votos() {
        let n = |sim, folder| Neighbor { sim, fields: [folder, None, None], tags: vec![] };
        // Dos vecinos muy parecidos en la misma carpeta y uno flojo en otra: gana con confianza alta.
        let v = vote(&[n(0.8, Some(1)), n(0.7, Some(1)), n(0.3, Some(2))], |x| x.fields[0].into_iter().collect());
        assert_eq!((v[0].0, v[0].2), (1, 2));
        assert!(v[0].1 >= AUTO && v[1].1 < SUGGEST);
        // Un solo vecino poco parecido se queda lejos de aplicarse solo.
        assert!(vote(&[n(0.26, Some(1))], |x| x.fields[0].into_iter().collect())[0].1 < AUTO - 0.2);
        // Los vecinos sin carpeta no restan: dos archivados juntos bastan aunque haya otros sin archivar.
        let v = vote(&[n(0.8, Some(1)), n(0.7, Some(1)), n(0.9, None), n(0.9, None)], |x| x.fields[0].into_iter().collect());
        assert!(v[0].1 >= AUTO);
        assert_eq!(norm("Póliza ÑANDÚ"), "poliza nandu");
    }
}
