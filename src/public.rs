//! Lo que se abre sin iniciar sesión, siempre con un token largo y aleatorio en la URL:
//! - `/s/{token}`: un documento compartido con un enlace que caduca (solo lectura, sin cuenta).
//! - `/cal/{token}.ics`: el calendario de vencimientos de un usuario, para suscribirse desde el teléfono.
//! Los tokens se guardan solo como hash; revocarlos (o que caduquen) deja el enlace muerto al instante.
use crate::{api, auth, db, ingest, oauth, store};
use axum::{
    Router,
    extract::{Path, Query, Request},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use rusqlite::{OptionalExtension, params};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// (id del enlace, documento, caduca) de un enlace vigente cuyo documento no está en la papelera.
fn share(token: &str) -> Option<(i64, i64, String)> {
    db::with(|c| {
        c.query_row(
            "SELECT s.id, s.doc_id, s.expires FROM shares s JOIN documents d ON d.id=s.doc_id
             WHERE s.token_hash=? AND s.expires > datetime('now') AND d.deleted_at IS NULL",
            [auth::sha(token)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
    })
    .ok()
    .flatten()
}

fn private_headers(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    h.insert("X-Robots-Tag", HeaderValue::from_static("noindex, nofollow"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    r
}

const STYLE: &str = "body{margin:0;font:15px/1.5 ui-sans-serif,system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;background:#f6f3ee;color:#1d1a17}
@media (prefers-color-scheme:dark){body{background:#12100e;color:#f3efe9}.card,header{background:#1b1917!important;border-color:#2d2925!important}.muted{color:#aaa39a!important}}
header{position:sticky;top:0;display:flex;gap:12px;align-items:center;padding:12px 16px;background:#fffdfa;border-bottom:1px solid #e8e2d8}
h1{font-size:17px;margin:0;line-height:1.25;overflow-wrap:anywhere}.muted{color:#6b645b;font-size:13px}.grow{flex:1;min-width:0}
a.btn{display:inline-flex;align-items:center;gap:6px;height:38px;padding:0 14px;border-radius:11px;background:#c2410c;color:#fff;font-weight:600;text-decoration:none;white-space:nowrap}
main{display:grid;gap:14px;justify-items:center;padding:16px}img{width:min(100%,900px);height:auto;background:#fff;border-radius:6px;box-shadow:0 1px 3px rgb(0 0 0/.18),0 10px 30px rgb(0 0 0/.12)}
.card{background:#fffdfa;border:1px solid #e8e2d8;border-radius:18px;padding:28px 24px;max-width:420px;text-align:center}
footer{text-align:center;padding:10px 16px 28px}";

fn shell(title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><meta name=robots content=noindex>\
         <meta name=color-scheme content='light dark'><title>{}</title><link rel=icon href=/assets/icon.svg><style>{STYLE}</style></head><body>{body}</body></html>",
        esc(title)
    );
    private_headers(Html(html).into_response())
}

fn gone() -> Response {
    let mut r = shell(
        "Papiro",
        "<main><div class=card><h1>This link has expired or was revoked</h1><p class=muted>Este enlace caducó o fue revocado. Pide uno nuevo a quien te lo compartió.</p></div></main>",
    );
    *r.status_mut() = StatusCode::NOT_FOUND;
    r
}

async fn page(Path(token): Path<String>) -> Response {
    let Some((sid, id, expires)) = share(&token) else { return gone() };
    let Ok(d) = store::get_doc_brief(id) else { return gone() };
    let _ = db::with(|c| c.execute("UPDATE shares SET views=views+1 WHERE id=?", [sid]));
    let pdf = (d.has_archive || d.mime == "application/pdf") && d.pages > 0;
    let content = if pdf {
        (1..=d.pages.clamp(1, 60)).map(|n| format!("<img loading={} src='/s/{token}/pages/{n}' alt='{n}'>", if n == 1 { "eager" } else { "lazy" })).collect::<String>()
    } else if d.mime.starts_with("image/") {
        format!("<img src='/s/{token}/file' alt=''>")
    } else {
        "<div class=card><p>No preview for this file type; download it to open it.</p></div>".into()
    };
    let size = if d.size > 1 << 20 { format!("{:.1} MB", d.size as f64 / 1048576.0) } else { format!("{} KB", (d.size / 1024).max(1)) };
    shell(
        &d.title,
        &format!(
            "<header><img src=/assets/icon.svg alt='' style='width:30px;height:30px;box-shadow:none;border-radius:8px;background:none'>\
             <div class=grow><h1>{}</h1><div class=muted>{} · {}</div></div><a class=btn href='/s/{token}/file?download=1'>⬇</a></header><main>{content}</main>\
             <footer class=muted>Shared with Papiro · expires {} UTC</footer>",
            esc(&d.title),
            esc(&d.original_name),
            size,
            esc(&expires[..16.min(expires.len())])
        ),
    )
}

#[derive(serde::Deserialize)]
struct FileQuery {
    download: Option<String>,
}

async fn file(Path(token): Path<String>, Query(q): Query<FileQuery>, req: Request) -> Response {
    let Some((_, id, _)) = share(&token) else { return gone() };
    let Ok(d) = store::get_doc_brief(id) else { return gone() };
    let ascii: String = d.original_name.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let inline = q.download.is_none() && (d.mime == "application/pdf" || d.mime.starts_with("image/"));
    let disposition = format!("{}; filename=\"{ascii}\"", if inline { "inline" } else { "attachment" });
    match api::serve_file(store::doc_dir(id).join("original"), &d.mime, Some(disposition), req).await {
        Ok(r) => private_headers(r),
        Err(e) => e.into_response(),
    }
}

async fn page_img(Path((token, n)): Path<(String, u32)>) -> Response {
    let Some((_, id, _)) = share(&token) else { return gone() };
    let Ok(d) = store::get_doc_brief(id) else { return gone() };
    if n < 1 || i64::from(n) > d.pages.max(1) || (!d.has_archive && d.mime != "application/pdf") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = store::doc_dir(id).join(if d.has_archive { "archive.pdf" } else { "original" });
    let Ok(_permit) = api::RENDERS.acquire().await else { return StatusCode::SERVICE_UNAVAILABLE.into_response() };
    match tokio::task::spawn_blocking(move || ingest::render_page(&path, n, 1400)).await {
        Ok(Ok(jpeg)) => private_headers(([(header::CONTENT_TYPE, "image/jpeg")], jpeg).into_response()),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ───────────────────────── calendario de vencimientos ─────────────────────────

/// Texto de iCalendar: escapa y parte las líneas largas a 75 bytes (RFC 5545).
fn ical_line(out: &mut String, line: &str) {
    let mut width = 0;
    for ch in line.chars() {
        if width + ch.len_utf8() > 74 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += ch.len_utf8();
    }
    out.push_str("\r\n");
}
fn ical_text(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace(['\r', '\n'], " ")
}

async fn calendar(Path(token): Path<String>, headers: HeaderMap) -> Response {
    let token = token.trim_end_matches(".ics");
    let user = db::with(|c| {
        c.query_row("SELECT id, username, role FROM users WHERE calendar_hash=?", [auth::sha(token)], |r| Ok(auth::AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
    })
    .ok()
    .flatten();
    let Some(u) = user else { return StatusCode::NOT_FOUND.into_response() };
    let origin = oauth::origin(&headers);
    let stamp: String = db::with(|c| c.query_row("SELECT strftime('%Y%m%dT%H%M%SZ','now')", [], |r| r.get(0))).unwrap_or_default();
    let mut out = String::new();
    for l in ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Papiro//Due dates//EN", "CALSCALE:GREGORIAN", "METHOD:PUBLISH", "X-WR-CALNAME:Papiro", "REFRESH-INTERVAL;VALUE=DURATION:PT6H", "X-PUBLISHED-TTL:PT6H"] {
        ical_line(&mut out, l);
    }
    for (id, title, expires, _) in store::due(&u, None).unwrap_or_default() {
        let date = expires.replace('-', "");
        for l in [
            "BEGIN:VEVENT".to_string(),
            format!("UID:papiro-doc-{id}@{}", origin.split("://").nth(1).unwrap_or("papiro")),
            format!("DTSTAMP:{stamp}"),
            format!("DTSTART;VALUE=DATE:{date}"),
            format!("SUMMARY:{}", ical_text(&format!("⏰ {title}"))),
            format!("URL:{origin}/#/?doc={id}"),
            format!("DESCRIPTION:{}", ical_text(&format!("{origin}/#/?doc={id}"))),
            "TRANSP:TRANSPARENT".into(),
            "BEGIN:VALARM".into(),
            "ACTION:DISPLAY".into(),
            format!("DESCRIPTION:{}", ical_text(&title)),
            "TRIGGER:-P7D".into(),
            "END:VALARM".into(),
            "END:VEVENT".into(),
        ] {
            ical_line(&mut out, &l);
        }
    }
    ical_line(&mut out, "END:VCALENDAR");
    private_headers(([(header::CONTENT_TYPE, "text/calendar; charset=utf-8")], out).into_response())
}

/// Crea (o reemplaza) el enlace de calendario del usuario y devuelve el token nuevo.
pub fn new_calendar(user_id: i64) -> Result<String, store::Error> {
    let token = crate::random_hex(24);
    db::with(|c| c.execute("UPDATE users SET calendar_hash=? WHERE id=?", params![auth::sha(&token), user_id]))?;
    Ok(token)
}

pub fn router() -> Router {
    Router::new()
        .route("/s/{token}", get(page))
        .route("/s/{token}/file", get(file))
        .route("/s/{token}/pages/{n}", get(page_img))
        .route("/cal/{token}", get(calendar))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ical_folding_and_escaping() {
        let mut out = String::new();
        ical_line(&mut out, &format!("SUMMARY:{}", ical_text(&"Póliza; seguro, coche ".repeat(5))));
        assert!(out.lines().all(|l| l.len() <= 75), "{out}");
        assert!(out.contains("\\;") && out.contains("\\,"));
        assert!(out.ends_with("\r\n"));
    }

    #[test]
    fn html_escaping() {
        assert_eq!(esc("<a href=\"x\">'&'</a>"), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }
}
