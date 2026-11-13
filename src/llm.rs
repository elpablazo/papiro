//! Clasificación y resumen opcionales con un LLM. La clave llega solo por PAPIRO_LLM_API_KEY.
use crate::{cfg, store};
use serde_json::{Value, json};

const MAX_CHARS: usize = 60_000;

const SYSTEM: &str = "You file personal and business documents into an archive. Given the text of one document \
(often noisy OCR), return its metadata. Write `title` and `summary` in the document's own language. `title` is short \
and specific (issuer + what it is + period or number when present), never a file name. `summary` is two or three \
sentences with the facts someone would search for later: who issued it, what it is, amounts, dates, identifiers. \
For `correspondent`, `document_type` and `tags`, reuse an existing value from the lists provided whenever one fits, \
and only propose a new one when none does; use null when the document does not say. At most five tags. `date` is the \
document's own date (issue date), as YYYY-MM-DD, or null.";

fn schema() -> Value {
    let nullable = json!({"type": ["string", "null"]});
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "summary", "correspondent", "document_type", "tags", "date"],
        "properties": {
            "title": {"type": "string"}, "summary": {"type": "string"},
            "correspondent": nullable, "document_type": nullable, "date": nullable,
            "tags": {"type": "array", "items": {"type": "string"}}
        }
    })
}

fn call(system: &str, user: &str) -> Result<Value, String> {
    let l = cfg().llm.as_ref().ok_or("LLM no configurado")?;
    let base = l.base_url.trim_end_matches('/');
    let text = if l.provider == "anthropic" {
        let mut body = json!({
            "model": l.model, "max_tokens": 16000, "system": system,
            "messages": [{"role": "user", "content": user}],
            "output_config": {"format": {"type": "json_schema", "schema": schema()}}
        });
        let mut req = crate::http()
            .post(&format!("{base}/v1/messages"))
            .header("x-api-key", &l.key)
            .header("anthropic-version", "2026-10-21");
        // En los modelos que pueden declinar por clasificadores de seguridad, que la API reintente sola con otro modelo.
        let fallback = base == "https://api.anthropic.com"
            && ["claude-opus-5", "claude-fable-5-1", "claude-fable-5-2", "claude-sonnet-5-5"].iter().any(|m| l.model.starts_with(m));
        if fallback {
            body["fallbacks"] = json!("default");
            req = req.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }
        let v = send(req, &body)?;
        if v["stop_reason"] == "refusal" {
            return Err("el modelo declinó procesar este documento".into());
        }
        if v["stop_reason"] == "max_tokens" {
            return Err("la respuesta del modelo quedó truncada (max_tokens)".into());
        }
        v["content"].as_array().and_then(|c| c.iter().find(|b| b["type"] == "text")).and_then(|b| b["text"].as_str()).map(String::from)
    } else {
        let body = json!({
            "model": l.model,
            "messages": [{"role": "system", "content": format!("{system}\nReply with one JSON object matching this JSON Schema: {}", schema())},
                         {"role": "user", "content": user}],
            "response_format": {"type": "json_object"}
        });
        let req = crate::http().post(&format!("{base}/chat/completions")).header("Authorization", &format!("Bearer {}", l.key));
        send(req, &body)?["choices"][0]["message"]["content"].as_str().map(String::from)
    }
    .ok_or("respuesta del LLM sin texto")?;
    serde_json::from_str(text.trim().trim_start_matches("```json").trim_matches('`').trim()).map_err(|e| format!("el LLM no devolvió JSON válido: {e}"))
}

fn send(req: ureq::RequestBuilder<ureq::typestate::WithBody>, body: &Value) -> Result<Value, String> {
    // Los 4xx/5xx se leen como respuesta para poder mostrar el mensaje de error del proveedor.
    let mut res = req.config().http_status_as_error(false).build().send_json(body).map_err(|e| format!("LLM inalcanzable: {e}"))?;
    let status = res.status();
    let v: Value = res.body_mut().read_json().map_err(|e| format!("respuesta ilegible del LLM ({status}): {e}"))?;
    if !status.is_success() {
        return Err(format!("LLM respondió {status}: {}", v["error"]["message"].as_str().unwrap_or("sin detalle")));
    }
    Ok(v)
}

/// Pide al LLM título, resumen, corresponsal, tipo, etiquetas y fecha, y los aplica al documento.
pub fn enrich(doc_id: i64) -> Result<(), String> {
    let d = store::get_doc(doc_id).map_err(|e| e.to_string())?;
    let content = d.content.clone().unwrap_or_default();
    let total = content.chars().count();
    let mut text: String = content.chars().take(MAX_CHARS).collect();
    if total > MAX_CHARS {
        // Para clasificar basta el principio; se le dice al modelo que no está viendo todo.
        text.push_str(&format!("\n\n[Text truncated: showing the first {MAX_CHARS} of {total} characters]"));
    }
    let names = |kind| store::list_labels(kind).map(|l| l.into_iter().map(|x| x.name).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    let user = format!(
        "Existing correspondents: {}\nExisting document types: {}\nExisting tags: {}\n\nFile name: {}\n\n<document>\n{text}\n</document>",
        names("correspondent"), names("doctype"), names("tag"), d.original_name
    );
    let r = call(SYSTEM, &user)?;
    let str_of = |k: &str| r[k].as_str().map(str::trim).filter(|s| !s.is_empty());
    let label = |kind: &str, k: &str| str_of(k).and_then(|n| store::ensure_label(kind, n).ok());
    let stem = std::path::Path::new(&d.original_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let patch = store::DocPatch {
        // El título solo se cambia si sigue siendo el nombre del archivo (no pisa lo que escribió una persona).
        title: str_of("title").filter(|_| d.title == stem).map(String::from),
        summary: str_of("summary").map(|s| Some(s.to_string())),
        created: str_of("date").filter(|x| store::valid_date(x)).map(String::from),
        correspondent_id: d.correspondent_id.is_none().then(|| label("correspondent", "correspondent")).flatten().map(Some),
        doctype_id: d.doctype_id.is_none().then(|| label("doctype", "document_type")).flatten().map(Some),
        add_tags: r["tags"].as_array().map(|t| t.iter().filter_map(Value::as_str).take(5).filter_map(|n| store::ensure_label("tag", n).ok()).collect()),
        ..Default::default()
    };
    store::update_doc(doc_id, &patch).map_err(|e| e.to_string())?;
    Ok(())
}
