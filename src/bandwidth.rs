//! Thread débit : temps de téléchargement d'un fichier en HTTP/1.0.
//! Client HTTP écrit à la main (quelques lignes) : pas de dépendance, et la
//! cible n'a de toute façon pas de TLS.

use crate::{config::Config, state::App, warn};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn run(app: &App, cfg: &Config) {
    let timeout = Duration::from_millis(cfg.bw_timeout);
    loop {
        let mbps = measure(&cfg.bw_url, timeout)
            .map_err(|e| warn!("mesure de débit ({}) : {e}", cfg.bw_url))
            .ok();
        app.write().record_bandwidth(mbps);
        if app.control.sleep(Duration::from_secs(cfg.bw_interval)) {
            return;
        }
    }
}

/// "http://hote:port/chemin" -> ("hote:port", "/chemin")
pub fn parse_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if host.is_empty() {
        return None;
    }
    let host = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
    Some((host, if path.is_empty() { "/".into() } else { path.into() }))
}

/// Débit de téléchargement en Mb/s
pub fn measure(url: &str, timeout: Duration) -> Result<f64, String> {
    let (host, path) = parse_url(url).ok_or("URL invalide (http:// uniquement)")?;
    let addr = host.to_socket_addrs().ok().and_then(|mut a| a.next()).ok_or("adresse introuvable")?;
    let start = Instant::now();
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connexion : {e}"))?;
    s.set_read_timeout(Some(timeout)).ok();
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").map_err(|e| e.to_string())?;

    let mut data = Vec::new();
    s.read_to_end(&mut data).map_err(|e| format!("lecture : {e}"))?;
    if !data.starts_with(b"HTTP/1.") || data.get(9..12) != Some(&b"200"[..]) {
        return Err("réponse HTTP inattendue".into());
    }
    Ok(data.len() as f64 * 8.0 / start.elapsed().as_secs_f64().max(1e-6) / 1e6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parsing() {
        assert_eq!(parse_url("http://10.0.2.2:8000/blob.bin"), Some(("10.0.2.2:8000".into(), "/blob.bin".into())));
        assert_eq!(parse_url("http://example.com"), Some(("example.com:80".into(), "/".into())));
        assert_eq!(parse_url("https://example.com/"), None);
    }

    #[test]
    fn measure_against_local_server() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let _ = c.read(&mut [0u8; 512]);
            c.write_all(b"HTTP/1.0 200 OK\r\n\r\n").unwrap();
            c.write_all(&vec![0u8; 1 << 20]).unwrap();
        });
        assert!(measure(&format!("http://{addr}/x"), Duration::from_secs(5)).unwrap() > 0.0);
    }
}
