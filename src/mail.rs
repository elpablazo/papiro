//! Ingesta por correo (opcional): revisa un buzón IMAP y encola los adjuntos de los mensajes no leídos.
use crate::{Imap, cfg, ingest, random_hex};
use std::{net::TcpStream, sync::Arc, time::Duration};

const EXTS: [&str; 17] = ["pdf", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "docx", "xlsx", "pptx", "odt", "ods", "odp", "doc", "xls", "txt"];

pub fn start() {
    if let Some(c) = &cfg().imap {
        std::thread::spawn(move || {
            loop {
                match fetch(c) {
                    Ok(0) => {}
                    Ok(n) => println!("correo: {n} adjunto(s) encolado(s)"),
                    Err(e) => eprintln!("correo: {e}"),
                }
                std::thread::sleep(Duration::from_secs(c.interval.max(30)));
            }
        });
    }
}

fn fetch(c: &Imap) -> Result<usize, Box<dyn std::error::Error>> {
    let tcp = TcpStream::connect((c.host.as_str(), c.port))?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    // El puerto 143 solo se acepta sin TLS para servidores locales de prueba (PAPIRO_IMAP_PORT=143).
    if c.port == 143 {
        let mut client = imap::Client::new(tcp);
        client.read_greeting()?;
        return drain(client.login(&c.user, &c.password).map_err(|e| e.0)?, c);
    }
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let tls_cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let conn = rustls::ClientConnection::new(Arc::new(tls_cfg), c.host.clone().try_into()?)?;
    let mut client = imap::Client::new(rustls::StreamOwned::new(conn, tcp));
    client.read_greeting()?;
    drain(client.login(&c.user, &c.password).map_err(|e| e.0)?, c)
}

fn drain<T: std::io::Read + std::io::Write>(mut s: imap::Session<T>, c: &Imap) -> Result<usize, Box<dyn std::error::Error>> {
    s.select(&c.folder)?;
    let mut queued = 0;
    for uid in s.uid_search("UNSEEN")? {
        // BODY[] marca el mensaje como leído: no se vuelve a procesar.
        for msg in s.uid_fetch(uid.to_string(), "BODY[]")?.iter() {
            let Some(raw) = msg.body() else { continue };
            let mail = mailparse::parse_mail(raw)?;
            for part in mail.parts() {
                let name = part.get_content_disposition().params.get("filename").cloned().or_else(|| part.ctype.params.get("name").cloned());
                let Some(name) = name.map(|n| n.rsplit(['/', '\\']).next().unwrap_or_default().to_string()) else { continue };
                let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
                if !EXTS.contains(&ext.as_str()) {
                    continue;
                }
                let path = cfg().data.join("tmp").join(format!("up-{}", random_hex(8)));
                std::fs::write(&path, part.get_body_raw()?)?;
                ingest::enqueue(&path, &name, "mail", &serde_json::json!({}), None).map_err(|e| e.to_string())?;
                queued += 1;
            }
        }
    }
    let _ = s.logout();
    Ok(queued)
}
