//! Inicio de sesión OAuth 2.1 para clientes MCP (Claude, ChatGPT, Cursor…), según la especificación
//! de autorización de MCP: metadatos del recurso protegido (RFC 9728) y del servidor de autorización
//! (RFC 8414), registro dinámico de clientes (RFC 7591), código de autorización con PKCE S256 y
//! tokens de refresco que rotan en cada uso.
//!
//! La pantalla de consentimiento es parte de la interfaz (ruta `/oauth/authorize`): ahí el usuario
//! inicia sesión, elige si la app puede escribir y a qué carpetas llega. Lo concedido queda en
//! `oauth_grants`; cada token de acceso (1 h) es una fila de `api_tokens` con ese mismo alcance, así
//! que el resto del servidor no distingue un token OAuth de uno creado a mano.
use crate::{
    api::ApiError,
    auth::{self, AuthUser},
    db, random_hex, store,
};
use axum::{
    Form, Json, Router,
    extract::Path,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use base64::Engine;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ACCESS_SECS: i64 = 3600;
const REFRESH_DAYS: i64 = 90;
pub const SCOPES: [&str; 2] = ["papiro:read", "papiro:write"];

/// Origen público del servidor. `PAPIRO_PUBLIC_URL` manda; si no, se deduce de las cabeceras del proxy.
pub fn origin(h: &HeaderMap) -> String {
    if let Some(u) = crate::env("PUBLIC_URL") {
        return u.trim_end_matches('/').to_string();
    }
    let get = |n: &str| h.get(n).and_then(|v| v.to_str().ok()).map(str::trim);
    let host = get("x-forwarded-host").or_else(|| get("host")).filter(|h| {
        !h.is_empty() && h.len() <= 253 && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
    });
    let proto = match get("x-forwarded-proto") {
        Some("https") => "https",
        _ => "http",
    };
    host.map(|h| format!("{proto}://{h}")).unwrap_or_default()
}

/// Cabecera que, en un 401 de /mcp, le dice al cliente dónde empezar el inicio de sesión.
pub fn www_authenticate(h: &HeaderMap) -> String {
    format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"{}\"", origin(h), SCOPES.join(" "))
}

fn cors(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, OPTIONS"));
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("authorization, content-type, mcp-protocol-version, mcp-session-id, last-event-id"));
    h.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, HeaderValue::from_static("www-authenticate, mcp-session-id"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// Las rutas públicas de OAuth y de /mcp aceptan peticiones de otros orígenes (inspectores MCP en el
/// navegador). Es seguro porque solo funcionan con token Bearer, nunca con la cookie de sesión.
pub async fn cors_layer(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let p = req.uri().path();
    let public = p.starts_with("/.well-known/oauth") || p.starts_with("/.well-known/openid") || p == "/oauth/token" || p == "/oauth/register" || p == "/mcp";
    if !public {
        return next.run(req).await;
    }
    if req.method() == Method::OPTIONS {
        return cors(StatusCode::NO_CONTENT.into_response());
    }
    cors(next.run(req).await)
}

fn oauth_error(code: StatusCode, error: &str, description: &str) -> Response {
    (code, Json(json!({"error": error, "error_description": description}))).into_response()
}

async fn metadata_resource(h: HeaderMap) -> Json<Value> {
    let o = origin(&h);
    Json(json!({
        "resource": format!("{o}/mcp"),
        "authorization_servers": [o],
        "scopes_supported": SCOPES,
        "bearer_methods_supported": ["header"],
        "resource_name": "Papiro",
        "resource_documentation": "https://github.com/elpablazo/papiro#api",
    }))
}

async fn metadata_server(h: HeaderMap) -> Json<Value> {
    let o = origin(&h);
    Json(json!({
        "issuer": o,
        "authorization_endpoint": format!("{o}/oauth/authorize"),
        "token_endpoint": format!("{o}/oauth/token"),
        "registration_endpoint": format!("{o}/oauth/register"),
        "scopes_supported": SCOPES,
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true,
        "service_documentation": "https://github.com/elpablazo/papiro#api",
    }))
}

// ───────────────────────── registro de clientes ─────────────────────────

/// Una dirección de retorno válida: https, localhost (apps de escritorio y CLIs) o un esquema propio
/// de una app nativa (cursor://, vscode://…). Nunca javascript:, data: o file:.
fn redirect_ok(uri: &str) -> bool {
    let Some((scheme, rest)) = uri.split_once(':') else { return false };
    let scheme = scheme.to_ascii_lowercase();
    if uri.contains('#') || uri.len() > 2000 {
        return false;
    }
    match scheme.as_str() {
        "https" => rest.starts_with("//") && rest.len() > 2,
        "http" => {
            let host = rest.trim_start_matches("//").split(['/', '?']).next().unwrap_or("");
            let host = host.rsplit_once(':').map_or(host, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host });
            matches!(host, "localhost" | "127.0.0.1" | "[::1]")
        }
        "javascript" | "data" | "file" | "vbscript" | "about" | "blob" => false,
        s => s.contains('.') || s.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-'),
    }
}

#[derive(Deserialize)]
struct Registration {
    redirect_uris: Vec<String>,
    client_name: Option<String>,
}

async fn register(Json(r): Json<Registration>) -> Response {
    if r.redirect_uris.is_empty() || r.redirect_uris.len() > 10 || !r.redirect_uris.iter().all(|u| redirect_ok(u)) {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_redirect_uri", "redirect_uris must be https, http://localhost or an app's own scheme");
    }
    let name: String = r.client_name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("MCP client").chars().take(80).collect();
    let id = format!("ppc_{}", random_hex(16));
    let uris = serde_json::to_string(&r.redirect_uris).unwrap_or_default();
    if let Err(e) = db::with(|c| c.execute("INSERT INTO oauth_clients(id, name, redirect_uris) VALUES(?,?,?)", params![id, name, uris])) {
        return oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", &e.to_string());
    }
    store::audit_raw(None, "", "app registered (OAuth)", &name, &r.redirect_uris.join(" "), "");
    (
        StatusCode::CREATED,
        Json(json!({
            "client_id": id, "client_name": name, "redirect_uris": r.redirect_uris,
            "token_endpoint_auth_method": "none", "grant_types": ["authorization_code", "refresh_token"], "response_types": ["code"],
            "client_id_issued_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        })),
    )
        .into_response()
}

fn client(id: &str) -> Option<(String, Vec<String>)> {
    db::with(|c| {
        c.query_row("SELECT name, redirect_uris FROM oauth_clients WHERE id=?", [id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).optional()
    })
    .ok()
    .flatten()
    .map(|(n, u)| (n, serde_json::from_str(&u).unwrap_or_default()))
}

// ───────────────────────── consentimiento (lo llama la interfaz) ─────────────────────────

#[derive(Deserialize)]
struct AuthRequest {
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    response_type: String,
    #[serde(default)]
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

fn check_request(r: &AuthRequest) -> Result<String, ApiError> {
    let bad = |m: &str| ApiError(StatusCode::BAD_REQUEST, m.into());
    let (name, uris) = client(&r.client_id).ok_or_else(|| bad("this app is not registered with Papiro; connect it again from the app"))?;
    if !uris.iter().any(|u| u == &r.redirect_uri) {
        return Err(bad("the return address does not match the one the app registered"));
    }
    if r.response_type != "code" {
        return Err(bad("unsupported response_type (only code)"));
    }
    if r.code_challenge_method != "S256" || r.code_challenge.len() < 43 || r.code_challenge.len() > 128 {
        return Err(bad("the app must use PKCE with S256"));
    }
    Ok(name)
}

/// Lo que la pantalla de consentimiento muestra: qué app, adónde vuelve y qué pide.
async fn describe(_u: AuthUser, axum::extract::Query(r): axum::extract::Query<AuthRequest>) -> Result<Json<Value>, ApiError> {
    let name = check_request(&r)?;
    let host = r.redirect_uri.split("://").nth(1).unwrap_or(&r.redirect_uri).split(['/', '?']).next().unwrap_or("").to_string();
    let wants_write = r.scope.as_deref().is_none_or(|s| s.split_whitespace().any(|x| x == "papiro:write"));
    Ok(Json(json!({"client_name": name, "redirect_host": host, "write": wants_write})))
}

#[derive(Deserialize)]
struct Decision {
    #[serde(flatten)]
    request: AuthRequest,
    approve: bool,
    #[serde(default)]
    read_only: bool,
    /// null = todo el archivo
    folders: Option<Vec<i64>>,
}

fn with_params(uri: &str, params: &[(&str, &str)]) -> String {
    let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    let q = params.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&");
    format!("{uri}{}{q}", if uri.contains('?') { '&' } else { '?' })
}

async fn decide(u: AuthUser, headers: HeaderMap, Json(d): Json<Decision>) -> Result<Json<Value>, ApiError> {
    if u.token {
        return Err(ApiError(StatusCode::FORBIDDEN, "origin not allowed".into()));
    }
    let name = check_request(&d.request)?;
    let r = &d.request;
    let iss = origin(&headers);
    let mut out: Vec<(&str, &str)> = vec![("iss", &iss)];
    if let Some(s) = &r.state {
        out.push(("state", s));
    }
    if !d.approve {
        out.insert(0, ("error", "access_denied"));
        store::audit(Some(&u), "app connection denied (OAuth)", &name, "");
        return Ok(Json(json!({"redirect": with_params(&r.redirect_uri, &out)})));
    }
    let read_only = d.read_only || !u.can_write();
    let folders = d.folders.map(|f| f.into_iter().filter(|id| store::folder_exists(*id)).collect::<Vec<_>>());
    if folders.as_ref().is_some_and(|f| f.is_empty()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "choose at least one folder, or give the token access to the whole archive".into()));
    }
    let code = random_hex(32);
    let (fjson, cid, redirect, challenge) = (folders.as_ref().map(|f| serde_json::to_string(f).unwrap_or_default()), r.client_id.clone(), r.redirect_uri.clone(), r.code_challenge.clone());
    db::write(|c| {
        c.execute("INSERT INTO oauth_grants(client_id, user_id, read_only, folders) VALUES(?,?,?,?)", params![cid, u.id, read_only, fjson])?;
        let grant = c.last_insert_rowid();
        c.execute(
            "INSERT INTO oauth_codes(code_hash, grant_id, redirect_uri, challenge, expires) VALUES(?,?,?,?, datetime('now','+5 minutes'))",
            params![auth::sha(&code), grant, redirect, challenge],
        )
    })
    .map_err(store::Error::from)?;
    let scope = match (&folders, read_only) {
        (None, false) => "full access".to_string(),
        (None, true) => "read-only".to_string(),
        (Some(f), ro) => format!("{} · {} folder(s)", if ro { "read-only" } else { "read and write" }, f.len()),
    };
    store::audit(Some(&u), "app connected (OAuth)", &name, &scope);
    out.insert(0, ("code", &code));
    Ok(Json(json!({"redirect": with_params(&r.redirect_uri, &out)})))
}

// ───────────────────────── tokens ─────────────────────────

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

fn pkce_ok(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len()) && base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}

/// Emite un token de acceso (1 h) y un token de refresco nuevo para el permiso concedido.
fn issue(grant: i64) -> rusqlite::Result<Value> {
    let (access, refresh) = (format!("ppo_{}", random_hex(24)), format!("ppf_{}", random_hex(32)));
    db::write(|c| {
        let (user, name, read_only, folders): (i64, String, bool, Option<String>) = c.query_row(
            "SELECT g.user_id, k.name, g.read_only, g.folders FROM oauth_grants g JOIN oauth_clients k ON k.id=g.client_id WHERE g.id=?",
            [grant],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        c.execute("DELETE FROM api_tokens WHERE grant_id=? AND expires < datetime('now')", [grant])?;
        c.execute(
            "INSERT INTO api_tokens(user_id, name, token_hash, read_only, expires, folders, grant_id) VALUES(?,?,?,?, datetime('now', '+' || ? || ' seconds'),?,?)",
            params![user, name, auth::sha(&access), read_only, ACCESS_SECS, folders, grant],
        )?;
        c.execute(
            "UPDATE oauth_grants SET refresh_hash=?, refresh_expires=datetime('now', '+' || ? || ' days'), last_used=datetime('now') WHERE id=?",
            params![auth::sha(&refresh), REFRESH_DAYS, grant],
        )?;
        let scope = if read_only { "papiro:read" } else { "papiro:read papiro:write" };
        Ok(json!({"access_token": access, "token_type": "Bearer", "expires_in": ACCESS_SECS, "refresh_token": refresh, "scope": scope}))
    })
}

async fn token(Form(t): Form<TokenRequest>) -> Response {
    let invalid = |d: &str| oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", d);
    let grant = match t.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier)) = (&t.code, &t.code_verifier) else {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "code and code_verifier are required");
            };
            // De un solo uso: el código se borra al canjearlo, salga bien o mal.
            let row: Option<(i64, String, String, String, bool)> = db::write(|c| {
                c.query_row(
                    "DELETE FROM oauth_codes WHERE code_hash=? RETURNING grant_id, redirect_uri, challenge,
                       (SELECT client_id FROM oauth_grants WHERE id=grant_id), expires > datetime('now')",
                    [auth::sha(code)],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
            })
            .unwrap_or(None);
            let Some((grant, redirect, challenge, client_id, fresh)) = row else { return invalid("unknown or already used code") };
            if !fresh || t.redirect_uri.as_deref().is_some_and(|r| r != redirect) || t.client_id.as_deref().is_some_and(|c| c != client_id) || !pkce_ok(verifier, &challenge) {
                let _ = db::with(|c| c.execute("DELETE FROM oauth_grants WHERE id=? AND refresh_hash IS NULL", [grant]));
                return invalid("the code expired or does not match (redirect_uri, client_id or code_verifier)");
            }
            grant
        }
        "refresh_token" => {
            let Some(rt) = &t.refresh_token else { return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "refresh_token is required") };
            let row: Option<(i64, String)> = db::with(|c| {
                c.query_row("SELECT id, client_id FROM oauth_grants WHERE refresh_hash=? AND refresh_expires > datetime('now')", [auth::sha(rt)], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
            })
            .unwrap_or(None);
            match row {
                Some((g, c)) if t.client_id.as_deref().is_none_or(|x| x == c) => g,
                _ => return invalid("the refresh token is not valid (expired or revoked)"),
            }
        }
        _ => return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type", "use authorization_code or refresh_token"),
    };
    match issue(grant) {
        Ok(v) => Json(v).into_response(),
        Err(e) => oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", &e.to_string()),
    }
}

// ───────────────────────── apps conectadas ─────────────────────────

async fn list_grants(u: AuthUser) -> Result<Json<Value>, ApiError> {
    let rows: Vec<Value> = db::with(|c| {
        c.prepare(
            "SELECT g.id, k.name, g.read_only, g.folders, g.created, g.last_used,
               (SELECT max(last_used) FROM api_tokens WHERE grant_id=g.id)
             FROM oauth_grants g JOIN oauth_clients k ON k.id=g.client_id WHERE g.user_id=? AND g.refresh_hash IS NOT NULL ORDER BY g.id DESC",
        )?
        .query_map([u.id], |r| {
            let (used, token_used): (Option<String>, Option<String>) = (r.get(5)?, r.get(6)?);
            Ok(json!({"id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "read_only": r.get::<_, bool>(2)?,
                "folders": auth::parse_folders(r.get(3)?), "created": r.get::<_, String>(4)?, "last_used": token_used.max(used)}))
        })?
        .collect()
    })
    .map_err(store::Error::from)?;
    Ok(Json(Value::Array(rows)))
}

async fn revoke_grant(u: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    if u.token {
        return Err(ApiError(StatusCode::FORBIDDEN, "a read-only token cannot revoke tokens".into()));
    }
    let name: Option<String> = db::with(|c| {
        c.query_row("DELETE FROM oauth_grants WHERE id=? AND user_id=? RETURNING (SELECT name FROM oauth_clients WHERE id=client_id)", [id, u.id], |r| r.get(0)).optional()
    })
    .map_err(store::Error::from)?;
    let name = name.ok_or(store::Error::NotFound)?;
    store::audit(Some(&u), "app disconnected (OAuth)", &name, "");
    Ok(StatusCode::NO_CONTENT)
}

/// Limpieza periódica: códigos vencidos, tokens de acceso caducados y permisos sin canjear.
pub fn cleanup() {
    let _ = db::with(|c| {
        c.execute_batch(
            "DELETE FROM oauth_codes WHERE expires < datetime('now');
             DELETE FROM api_tokens WHERE grant_id IS NOT NULL AND expires < datetime('now');
             DELETE FROM oauth_grants WHERE (refresh_hash IS NULL AND created < datetime('now','-1 hour')) OR refresh_expires < datetime('now');",
        )
    });
}

pub fn router() -> Router {
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(metadata_resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(metadata_resource))
        .route("/.well-known/oauth-authorization-server", get(metadata_server))
        .route("/.well-known/oauth-authorization-server/mcp", get(metadata_server))
        .route("/.well-known/openid-configuration", get(metadata_server))
        .route("/oauth/register", post(register))
        .route("/oauth/token", post(token))
        .route("/api/oauth/request", get(describe))
        .route("/api/oauth/decide", post(decide))
        .route("/api/oauth/grants", get(list_grants))
        .route("/api/oauth/grants/{id}", delete(revoke_grant))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256() {
        // Ejemplo del apéndice B del RFC 7636.
        assert!(pkce_ok("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
        assert!(!pkce_ok("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXx", "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    }

    #[test]
    fn redirects() {
        assert!(redirect_ok("https://claude.ai/api/mcp/auth_callback"));
        assert!(redirect_ok("http://localhost:33418/callback"));
        assert!(redirect_ok("http://127.0.0.1/cb"));
        assert!(redirect_ok("cursor://anysphere.cursor-retrieval/oauth/callback"));
        assert!(!redirect_ok("http://evil.example/cb"));
        assert!(!redirect_ok("http://localhost.evil.example/cb"));
        assert!(!redirect_ok("javascript:alert(1)"));
        assert!(!redirect_ok("https://ok.example/cb#frag"));
    }

    #[test]
    fn query_building() {
        assert_eq!(with_params("https://a.b/cb?x=1", &[("code", "c d"), ("state", "s&t")]), "https://a.b/cb?x=1&code=c%20d&state=s%26t");
    }
}
