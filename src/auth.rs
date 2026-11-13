//! Usuarios, roles, sesiones (cookie) y tokens de API (Bearer). Nada se guarda en claro: solo hashes.
//!
//! Roles: `admin` (todo), `editor` (subir y editar; no borra definitivamente ni administra) y
//! `viewer` (solo lectura). Un token puede ser de solo lectura aunque su usuario pueda escribir.
use crate::{api::ApiError, db, random_hex, store};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{Method, StatusCode, header, request::Parts},
};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, net::SocketAddr, sync::Mutex, time::Instant};

pub const COOKIE: &str = "papiro_session";
pub const ROLES: [&str; 3] = ["admin", "editor", "viewer"];

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Debug)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    /// admin | editor | viewer
    pub role: String,
    pub is_admin: bool,
    /// The request came with a read-only token
    #[serde(skip)]
    pub read_only: bool,
    #[serde(skip)]
    pub ip: String,
}

impl AuthUser {
    pub fn new(id: i64, username: String, role: String) -> Self {
        AuthUser { id, username, is_admin: role == "admin", role, read_only: false, ip: String::new() }
    }
    pub fn can_write(&self) -> bool {
        self.role != "viewer" && !self.read_only
    }
    pub fn admin(&self) -> bool {
        self.role == "admin" && !self.read_only
    }
}

pub fn sha(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn hash_password(pw: &str) -> Result<String, store::Error> {
    if pw.chars().count() < 8 {
        return Err(store::Error::Bad("the password must be at least 8 characters long".into()));
    }
    Argon2::default().hash_password(pw.as_bytes()).map(|h| h.to_string()).map_err(|e| store::Error::Other(e.to_string()))
}

fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

pub fn create_user(username: &str, password: &str, role: &str) -> Result<AuthUser, store::Error> {
    let username = username.trim();
    if username.is_empty() {
        return Err(store::Error::Bad("the username is required".into()));
    }
    if !ROLES.contains(&role) {
        return Err(store::Error::Bad("invalid role (admin, editor or viewer)".into()));
    }
    let hash = hash_password(password)?;
    let id = db::with(|c| {
        c.execute("INSERT INTO users(username, password_hash, is_admin, role) VALUES(?,?,?,?)", params![username, hash, role == "admin", role])?;
        Ok(c.last_insert_rowid())
    })?;
    Ok(AuthUser::new(id, username.into(), role.into()))
}

pub fn set_password(username: &str, password: &str) -> Result<(), store::Error> {
    let hash = hash_password(password)?;
    match db::write(|c| {
        // Cambiar la contraseña cierra las sesiones abiertas.
        c.execute("DELETE FROM sessions WHERE user_id=(SELECT id FROM users WHERE username=?)", [username])?;
        c.execute("UPDATE users SET password_hash=? WHERE username=?", params![hash, username])
    })? {
        0 => Err(store::Error::NotFound),
        _ => Ok(()),
    }
}

pub fn user_count() -> i64 {
    db::with(|c| c.query_row("SELECT count(*) FROM users", [], |r| r.get(0))).unwrap_or(0)
}

/// Primer arranque: crea el admin desde PAPIRO_ADMIN_USER / PAPIRO_ADMIN_PASSWORD si no hay usuarios.
/// Sin esas variables, la interfaz muestra un asistente para crear la cuenta inicial.
pub fn bootstrap_admin() {
    if user_count() == 0 {
        if let (Some(u), Some(p)) = (crate::env("ADMIN_USER"), crate::env("ADMIN_PASSWORD")) {
            match create_user(&u, &p, "admin") {
                Ok(_) => println!("administrator user \"{u}\" created"),
                Err(e) => eprintln!("could not create the administrator: {e}"),
            }
        }
    }
}

// ───────────────────────── freno a fuerza bruta ─────────────────────────
//
// El límite es por dirección IP, no por usuario: quien adivina contraseñas se frena a sí mismo y no
// puede dejar fuera al dueño de la cuenta. Además solo se calculan dos Argon2 a la vez (cada uno usa
// 19 MB), de modo que una avalancha de intentos no puede agotar la memoria.

const WINDOW_SECS: u64 = 300;
const MAX_ATTEMPTS: usize = 10;
static FAILS: Mutex<Option<HashMap<String, Vec<Instant>>>> = Mutex::new(None);
pub static HASHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn fails<T>(f: impl FnOnce(&mut HashMap<String, Vec<Instant>>) -> T) -> T {
    let mut g = FAILS.lock().unwrap_or_else(|p| p.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if map.len() > 20_000 {
        map.retain(|_, v| v.last().is_some_and(|t| t.elapsed().as_secs() < WINDOW_SECS));
    }
    f(map)
}

/// Cuenta un intento de esta IP y dice si puede continuar. Se llama antes de gastar CPU en
/// verificar nada, así que una ráfaga en paralelo desde una misma IP se corta en el intento 10.
pub fn allow_attempt(ip: &str) -> bool {
    fails(|m| {
        let v = m.entry(ip.to_string()).or_default();
        v.retain(|t| t.elapsed().as_secs() < WINDOW_SECS);
        if v.len() >= MAX_ATTEMPTS {
            return false;
        }
        v.push(Instant::now());
        true
    })
}

pub fn login(username: &str, password: &str, ip: &str) -> Result<AuthUser, ApiError> {
    let row = db::with(|c| {
        c.query_row("SELECT id, username, role, password_hash FROM users WHERE username=?", [username.trim()], |r| {
            Ok((AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?), r.get::<_, String>(3)?))
        })
        .optional()
    })
    .map_err(store::Error::from)?;
    // Se verifica siempre un hash para no revelar por tiempo si el usuario existe.
    const DUMMY: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Zm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm8";
    let ok = verify_password(password, row.as_ref().map(|r| r.1.as_str()).unwrap_or(DUMMY));
    match row {
        Some((mut user, _)) if ok => {
            fails(|m| m.remove(ip));
            user.ip = ip.to_string();
            store::audit(Some(&user), "logged in", "", "");
            Ok(user)
        }
        _ => {
            store::audit_raw(None, username.trim(), "login failed", "", "", ip);
            Err(ApiError(StatusCode::UNAUTHORIZED, "wrong username or password".into()))
        }
    }
}

/// Crea una sesión de 30 días y devuelve la cabecera Set-Cookie.
pub fn new_session(user_id: i64, https: bool) -> Result<String, store::Error> {
    let token = random_hex(32);
    db::with(|c| c.execute("INSERT INTO sessions(token_hash, user_id, expires) VALUES(?,?,datetime('now','+30 days'))", params![sha(&token), user_id]))?;
    Ok(format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000{}", if https { "; Secure" } else { "" }))
}

pub fn new_api_token(user_id: i64, name: &str, read_only: bool, days: Option<i64>) -> Result<(i64, String), store::Error> {
    let token = format!("ppr_{}", random_hex(24));
    let id = db::with(|c| {
        c.execute(
            "INSERT INTO api_tokens(user_id, name, token_hash, read_only, expires) VALUES(?,?,?,?, CASE WHEN ?5 IS NULL THEN NULL ELSE datetime('now', '+' || ?5 || ' days') END)",
            params![user_id, name, sha(&token), read_only, days.filter(|d| *d > 0)],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    Ok((id, token))
}

pub fn cookie_token(parts: &Parts) -> Option<&str> {
    parts.headers.get(header::COOKIE)?.to_str().ok()?.split(';').find_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('='))
}

/// IP del cliente. Si la conexión viene de un proxy en red privada (Traefik, Caddy…), se toma la
/// última dirección de X-Forwarded-For, que es la que ese proxy vio; si es directa, la del socket.
pub fn client_ip(parts: &Parts) -> String {
    let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip());
    let behind_proxy = peer.is_none_or(|ip| match ip.to_canonical() {
        std::net::IpAddr::V4(v) => v.is_private() || v.is_loopback(),
        std::net::IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00,
    });
    let forwarded = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.rsplit(',').next()).map(|v| v.trim().to_string());
    match (behind_proxy, forwarded, peer) {
        (true, Some(f), _) if !f.is_empty() => f,
        (_, _, Some(p)) => p.to_canonical().to_string(),
        _ => "unknown".into(),
    }
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "not authenticated".into())
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, ApiError> {
        let ip = client_ip(parts);
        let bearer = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
        if let Some(token) = bearer {
            let hash = sha(token.trim());
            return db::with(|c| {
                let u = c
                    .query_row(
                        "SELECT u.id, u.username, u.role, t.read_only FROM api_tokens t JOIN users u ON u.id=t.user_id
                         WHERE t.token_hash=? AND (t.expires IS NULL OR t.expires > datetime('now'))",
                        [&hash],
                        |r| Ok((AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?), r.get::<_, bool>(3)?)),
                    )
                    .optional()?;
                if u.is_some() {
                    c.execute(
                        "UPDATE api_tokens SET last_used=datetime('now') WHERE token_hash=? AND (last_used IS NULL OR last_used < datetime('now','-1 minute'))",
                        [&hash],
                    )?;
                }
                Ok(u)
            })
            .map_err(store::Error::from)?
            .map(|(mut u, read_only)| {
                u.read_only = read_only;
                u.ip = ip;
                u
            })
            .ok_or_else(unauthorized);
        }
        let token = cookie_token(parts).ok_or_else(unauthorized)?;
        // CSRF: con cookie, las escrituras deben venir de nuestra propia interfaz (cabecera que un
        // formulario de otro sitio no puede enviar) y, si el navegador dice el origen, del nuestro.
        if !matches!(parts.method, Method::GET | Method::HEAD) {
            let h = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok());
            let same_origin = match (h("origin"), h("host")) {
                (Some(origin), Some(host)) => origin.split("://").nth(1) == Some(host),
                _ => true,
            };
            if h("x-requested-with") != Some("papiro") || !same_origin {
                return Err(ApiError(StatusCode::FORBIDDEN, "origin not allowed".into()));
            }
        }
        db::with(|c| {
            c.query_row(
                "SELECT u.id, u.username, u.role FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires > datetime('now')",
                [sha(token)],
                |r| Ok(AuthUser::new(r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
        })
        .map_err(store::Error::from)?
        .map(|mut u| {
            u.ip = ip;
            u
        })
        .ok_or_else(unauthorized)
    }
}

/// Extractor para rutas que modifican datos: rechaza a lectores y a tokens de solo lectura.
pub struct Editor(pub AuthUser);

impl<S: Send + Sync> FromRequestParts<S> for Editor {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, s: &S) -> Result<Self, ApiError> {
        let u = AuthUser::from_request_parts(parts, s).await?;
        if u.can_write() { Ok(Editor(u)) } else { Err(ApiError(StatusCode::FORBIDDEN, "your user or token is read-only".into())) }
    }
}

/// Extractor para rutas solo de administradores.
pub struct Admin(pub AuthUser);

impl<S: Send + Sync> FromRequestParts<S> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, s: &S) -> Result<Self, ApiError> {
        let u = AuthUser::from_request_parts(parts, s).await?;
        if u.admin() { Ok(Admin(u)) } else { Err(ApiError(StatusCode::FORBIDDEN, "administrator required".into())) }
    }
}
