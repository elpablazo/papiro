//! Notificaciones push del navegador (Web Push): RFC 8030 para el envío, RFC 8291 para cifrar el
//! mensaje (aes128gcm) y RFC 8292 (VAPID) para firmarlo. Sin servicios de terceros: el servidor
//! habla directamente con el servicio push del navegador (Google, Mozilla, Apple…).
//!
//! La clave VAPID se genera la primera vez y se guarda en la tabla `meta`. El envío ocurre en un
//! hilo aparte con una cola en memoria: avisar nunca retrasa la ingesta de un documento.
use crate::{auth::AuthUser, db, random_hex, store};
use aes_gcm::{Aes128Gcm, KeyInit, aead::Aead};
use base64::Engine;
use p256::{PublicKey, SecretKey, ecdsa::{Signature, SigningKey, signature::Signer}};
use rusqlite::params;
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    sync::{OnceLock, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Los navegadores entregan las claves en base64url, a veces con relleno o en base64 normal.
pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s.trim().trim_end_matches('=').replace('+', "-").replace('/', "_")).map_err(|e| format!("invalid base64: {e}"))
}

fn random_secret() -> SecretKey {
    loop {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).expect("getrandom");
        if let Ok(k) = SecretKey::from_slice(&b) {
            return k;
        }
    }
}

fn point(k: &PublicKey) -> Vec<u8> {
    k.to_sec1_bytes().to_vec()
}

/// Clave VAPID del servidor (se crea una vez y se guarda en la base).
fn vapid() -> &'static SecretKey {
    static KEY: OnceLock<SecretKey> = OnceLock::new();
    KEY.get_or_init(|| {
        if let Some(k) = store::meta_get("vapid_key").and_then(|h| hex::decode(h).ok()).and_then(|b| SecretKey::from_slice(&b).ok()) {
            return k;
        }
        let k = random_secret();
        store::meta_set("vapid_key", &hex::encode(k.to_bytes()));
        k
    })
}

/// Clave pública VAPID (base64url, punto sin comprimir) que el navegador necesita para suscribirse.
pub fn public_key() -> String {
    B64.encode(point(&vapid().public_key()))
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    hkdf::Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut out).expect("hkdf length");
    out
}

/// Cifra un mensaje para una suscripción (RFC 8291, un solo registro aes128gcm).
pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8], local: &SecretKey, salt: &[u8; 16]) -> Result<Vec<u8>, String> {
    let ua = PublicKey::from_sec1_bytes(ua_public).map_err(|_| "invalid p256dh key".to_string())?;
    let as_public = point(&local.public_key());
    let shared = local.diffie_hellman(&ua);
    let key_info = [b"WebPush: info\0".as_slice(), ua_public, &as_public].concat();
    let ikm = hkdf(auth, shared.raw_secret_bytes().as_slice(), &key_info, 32);
    let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16);
    let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12);
    let mut record = plaintext.to_vec();
    record.push(2); // delimitador del último (y único) registro
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|e| e.to_string())?;
    let nonce = aes_gcm::Nonce::try_from(nonce.as_slice()).map_err(|_| "nonce".to_string())?;
    let sealed = cipher.encrypt(&nonce, record.as_slice()).map_err(|e| e.to_string())?;
    let mut body = Vec::with_capacity(86 + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(as_public.len() as u8);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// Cabecera `Authorization: vapid t=<JWT ES256>, k=<clave pública>` para el servicio push del endpoint.
fn vapid_header(endpoint: &str) -> String {
    let audience = endpoint.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let exp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) + 12 * 3600;
    let sub = crate::env("PUSH_CONTACT").unwrap_or_else(|| "https://github.com/elpablazo/papiro".into());
    let head = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = B64.encode(json!({"aud": audience, "exp": exp, "sub": sub}).to_string());
    let signing = format!("{head}.{claims}");
    let sig: Signature = SigningKey::from(vapid().clone()).sign(signing.as_bytes());
    format!("vapid t={signing}.{}, k={}", B64.encode(sig.to_bytes()), public_key())
}

// ───────────────────────── suscripciones ─────────────────────────

pub struct Subscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub folders: Option<Vec<i64>>,
    pub lang: String,
    pub reminders: bool,
}

pub fn subscribe(user_id: i64, s: &Subscription) -> Result<(), store::Error> {
    if !s.endpoint.starts_with("https://") {
        return Err(store::Error::Bad("the push endpoint must use https".into()));
    }
    store::webhook_target_ok(&s.endpoint).map_err(store::Error::Bad)?;
    let (key, auth) = (b64_decode(&s.p256dh).map_err(store::Error::Bad)?, b64_decode(&s.auth).map_err(store::Error::Bad)?);
    if PublicKey::from_sec1_bytes(&key).is_err() || auth.len() < 16 {
        return Err(store::Error::Bad("invalid push subscription keys".into()));
    }
    let folders = s.folders.as_ref().map(|f| serde_json::to_string(f).unwrap_or_default());
    db::with(|c| {
        c.execute(
            "INSERT INTO push_subs(user_id, endpoint, p256dh, auth, folders, lang, reminders) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(endpoint) DO UPDATE SET user_id=?1, p256dh=?3, auth=?4, folders=?5, lang=?6, reminders=?7",
            params![user_id, s.endpoint, s.p256dh, s.auth, folders, s.lang, s.reminders],
        )
    })?;
    Ok(())
}

pub fn unsubscribe(user_id: i64, endpoint: &str) -> Result<(), store::Error> {
    db::with(|c| c.execute("DELETE FROM push_subs WHERE user_id=? AND endpoint=?", params![user_id, endpoint]))?;
    Ok(())
}

/// Suscripción de este dispositivo (por endpoint), si existe.
pub fn find(user_id: i64, endpoint: &str) -> Option<(Option<Vec<i64>>, String, bool)> {
    db::with(|c| {
        c.query_row("SELECT folders, last_status, reminders FROM push_subs WHERE user_id=? AND endpoint=?", params![user_id, endpoint], |r| {
            Ok((crate::auth::parse_folders(r.get(0)?), r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get(2)?))
        })
    })
    .ok()
}

pub fn count(user_id: i64) -> i64 {
    db::with(|c| c.query_row("SELECT count(*) FROM push_subs WHERE user_id=?", [user_id], |r| r.get(0))).unwrap_or(0)
}

// ───────────────────────── envío ─────────────────────────

struct Message {
    sub_id: i64,
    endpoint: String,
    p256dh: String,
    auth: String,
    payload: String,
    /// Mensajes con la misma etiqueta se reemplazan en el servicio push si aún no se entregaron
    topic: Option<String>,
}

fn queue() -> &'static mpsc::Sender<Message> {
    static Q: OnceLock<mpsc::Sender<Message>> = OnceLock::new();
    Q.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Message>();
        std::thread::spawn(move || {
            for m in rx {
                let mut status = send(&m);
                // Un servicio saturado (429/5xx) o caído: un reintento a los 30 s. Las notificaciones no se acumulan.
                if status.starts_with('5') || status == "429" || status.starts_with("error") {
                    std::thread::sleep(Duration::from_secs(30));
                    status = send(&m);
                }
                let _ = db::with(|c| match status.as_str() {
                    // La suscripción ya no existe (el usuario desinstaló la app o revocó el permiso).
                    "404" | "410" => c.execute("DELETE FROM push_subs WHERE id=?", [m.sub_id]),
                    _ => c.execute("UPDATE push_subs SET last_status=? WHERE id=?", params![status, m.sub_id]),
                });
                if !status.starts_with('2') && status != "404" && status != "410" {
                    eprintln!("push: {} → {status}", m.endpoint.split('/').take(3).collect::<Vec<_>>().join("/"));
                }
            }
        });
        tx
    })
}

fn send(m: &Message) -> String {
    if let Err(e) = store::webhook_target_ok(&m.endpoint) {
        return format!("blocked: {e}");
    }
    let (Ok(key), Ok(auth)) = (b64_decode(&m.p256dh), b64_decode(&m.auth)) else { return "error: keys".into() };
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("getrandom");
    let body = match encrypt(&key, &auth, m.payload.as_bytes(), &random_secret(), &salt) {
        Ok(b) => b,
        Err(e) => return format!("error: {e}"),
    };
    let mut req = crate::http()
        .post(&m.endpoint)
        .config()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .header("Authorization", &vapid_header(&m.endpoint))
        .header("TTL", "86400")
        .header("Urgency", "normal")
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream");
    if let Some(t) = &m.topic {
        req = req.header("Topic", t);
    }
    match req.send(&body) {
        Ok(r) => r.status().as_u16().to_string(),
        Err(e) => format!("error: {e}"),
    }
}

fn enqueue(sub_id: i64, endpoint: String, p256dh: String, auth: String, payload: Value, topic: Option<String>) {
    let _ = queue().send(Message { sub_id, endpoint, p256dh, auth, payload: payload.to_string(), topic });
}

type Row = (i64, i64, String, String, String, String, Option<String>, String, String);

fn subs(where_sql: &str, arg: i64) -> Vec<Row> {
    db::with(|c| {
        c.prepare_cached(&format!(
            "SELECT s.id, s.user_id, u.username, u.role, s.endpoint, s.p256dh, s.folders, s.auth, s.lang FROM push_subs s JOIN users u ON u.id=s.user_id WHERE {where_sql}"
        ))?
        .query_map([arg], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))?
        .collect()
    })
    .unwrap_or_default()
}

/// Un documento nuevo: avisa a cada dispositivo suscrito cuyo usuario puede verlo y cuyas carpetas
/// lo incluyen. No se avisa a quien lo acaba de subir desde la propia interfaz: ya lo sabe.
pub fn document_created(doc_id: i64, uploader: Option<i64>, source: &str) {
    let all = subs("1", 0);
    if all.is_empty() {
        return;
    }
    let Ok(d) = store::get_doc_brief(doc_id) else { return };
    let folders: Vec<store::Folder> = store::list_folders(&AuthUser::new(0, String::new(), "admin".into())).unwrap_or_default();
    let path = d.folder_id.and_then(|f| folders.iter().find(|x| x.id == f)).map(|f| f.path.replace('/', " › "));
    for (id, user_id, username, role, endpoint, p256dh, scope, auth, lang) in all {
        if source == "upload" && uploader == Some(user_id) {
            continue;
        }
        let mut u = AuthUser::new(user_id, username, role);
        u.folders = crate::auth::parse_folders(scope);
        if store::authorize(&u, doc_id).is_err() {
            continue;
        }
        let es = lang.starts_with("es");
        let via = match (source, es) {
            ("mail", true) => Some("llegó por correo"),
            ("mail", false) => Some("arrived by email"),
            ("consume", true) => Some("desde la carpeta vigilada"),
            ("consume", false) => Some("from the watched folder"),
            ("mcp", true) => Some("lo subió un agente (MCP)"),
            ("mcp", false) => Some("added by an AI agent (MCP)"),
            _ => None,
        };
        let body = [Some(d.title.clone()), path.clone(), via.map(String::from)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        let payload = json!({
            "title": if es { "Nuevo documento en Papiro" } else { "New document in Papiro" },
            "body": body, "url": format!("/#/?doc={doc_id}"), "tag": format!("doc-{doc_id}"), "doc": doc_id,
            "badge": store::stats(&u).map(|s| s.inbox).unwrap_or(0),
        });
        enqueue(id, endpoint, p256dh, auth, payload, Some(format!("doc{doc_id}")));
    }
}

/// Notificación de prueba a todos los dispositivos del usuario. Devuelve cuántos hay.
pub fn test(user_id: i64) -> usize {
    let list = subs("s.user_id=?", user_id);
    for (id, _, _, _, endpoint, p256dh, _, auth, lang) in &list {
        let es = lang.starts_with("es");
        let payload = json!({
            "title": "Papiro",
            "body": if es { "Las notificaciones funcionan en este dispositivo ✓" } else { "Notifications work on this device ✓" },
            "url": "/", "tag": format!("test-{}", random_hex(3)),
        });
        enqueue(*id, endpoint.clone(), p256dh.clone(), auth.clone(), payload, None);
    }
    list.len()
}

/// Recordatorio diario (una vez al día, desde las 9:00 hora local): lo que vence hoy, mañana, en 7 y en
/// 30 días, en una sola notificación por usuario. Lo llama el hilo de mantenimiento cada minuto.
pub fn reminders_tick() {
    let Ok((today, hour)) = db::with(|c| c.query_row("SELECT date('now','localtime'), CAST(strftime('%H','now','localtime') AS INTEGER)", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))) else { return };
    if hour < 9 || store::meta_get("reminders_day").as_deref() == Some(today.as_str()) {
        return;
    }
    store::meta_set("reminders_day", &today);
    send_reminders(&today, None);
}

/// Envía el resumen de vencimientos (a todos, o solo a un usuario para probarlo). Devuelve cuántos avisos salieron.
pub fn send_reminders(today: &str, only_user: Option<i64>) -> usize {
    let users: Vec<(i64, String, String)> = db::with(|c| {
        c.prepare("SELECT DISTINCT u.id, u.username, u.role FROM push_subs s JOIN users u ON u.id=s.user_id WHERE s.reminders")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect()
    })
    .unwrap_or_default();
    let mut sent = 0;
    for (id, name, role) in users.into_iter().filter(|u| only_user.is_none_or(|o| o == u.0)) {
        let u = AuthUser::new(id, name, role);
        let due = store::due(&u, Some(&[0, 1, 7, 30])).unwrap_or_default();
        if due.is_empty() {
            continue;
        }
        let devices: Vec<(i64, String, String, String, String)> = db::with(|c| {
            c.prepare("SELECT id, endpoint, p256dh, auth, lang FROM push_subs WHERE user_id=? AND reminders")?
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .collect()
        })
        .unwrap_or_default();
        for (sid, endpoint, p256dh, auth, lang) in devices {
            let es = lang.starts_with("es");
            let when = |d: i64| match (d, es) {
                (0, true) => "hoy".to_string(),
                (0, false) => "today".to_string(),
                (1, true) => "mañana".to_string(),
                (1, false) => "tomorrow".to_string(),
                (n, true) => format!("en {n} días"),
                (n, false) => format!("in {n} days"),
            };
            let mut lines: Vec<String> = due.iter().take(3).map(|(_, t, _, d)| format!("{t} ({})", when(*d))).collect();
            if due.len() > 3 {
                lines.push(if es { format!("y {} más", due.len() - 3) } else { format!("and {} more", due.len() - 3) });
            }
            let title = match (due.len(), es) {
                (1, true) => "1 documento vence pronto".to_string(),
                (n, true) => format!("{n} documentos vencen pronto"),
                (1, false) => "1 document is due soon".to_string(),
                (n, false) => format!("{n} documents are due soon"),
            };
            let url = if due.len() == 1 { format!("/#/?doc={}", due[0].0) } else { "/#/?view=expiring".into() };
            enqueue(sid, endpoint, p256dh, auth, json!({"title": title, "body": lines.join(" · "), "url": url, "tag": format!("due-{today}")}), Some(format!("due{}", today.replace('-', ""))));
            sent += 1;
        }
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ejemplo de la sección 5 del RFC 8291: mismas claves y sal → mismo mensaje cifrado, byte a byte.
    #[test]
    fn rfc8291_example() {
        let d = |s: &str| b64_decode(s).unwrap();
        let local = SecretKey::from_slice(&d("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
        assert_eq!(B64.encode(point(&local.public_key())), "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8");
        let salt: [u8; 16] = d("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let body = encrypt(
            &d("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
            &d("BTBZMqHH6r4Tts7J_aSIgg"),
            b"When I grow up, I want to be a watermelon",
            &local,
            &salt,
        )
        .unwrap();
        assert_eq!(
            B64.encode(body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn vapid_jwt_verifies() {
        use p256::ecdsa::{VerifyingKey, signature::Verifier};
        let key = random_secret();
        let msg = b"header.claims";
        let sig: Signature = SigningKey::from(key.clone()).sign(msg);
        assert!(VerifyingKey::from(key.public_key()).verify(msg, &sig).is_ok());
        assert_eq!(sig.to_bytes().len(), 64);
    }
}
