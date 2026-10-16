//! Thread débit : durée de téléchargement d'un fichier en HTTP.

use crate::config::Config;
use crate::state::App;
use crate::warn;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn run(app: &App, cfg: &Config) {
    let timeout = Duration::from_millis(cfg.bw_timeout);
    loop {
        let mbps = match measure(&cfg.bw_url, timeout) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!("mesure de débit ({}) : {e}", cfg.bw_url);
                None
            }
        };
        app.write().record_bandwidth(mbps);
        if app.control.sleep(Duration::from_secs(cfg.bw_interval)) {
            return;
        }
    }
}

/// Débit en Mb/s.
pub fn measure(url: &str, timeout: Duration) -> Result<f64, String> {
    let (host, path) = parse_url(url).ok_or("URL invalide (http:// uniquement)")?;
    let addr = host.to_socket_addrs().ok().and_then(|mut a| a.next()).ok_or("adresse introuvable")?;

    let start = Instant::now();
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connexion : {e}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;

    // Compte les octets au fil de l'eau : le fichier n'est jamais gardé en mémoire
    let mut buf = [0u8; 16 * 1024];
    let mut total = read(&mut stream, &mut buf)?;
    if !buf.starts_with(b"HTTP/1.") || buf.get(9..12) != Some(b"200") {
        return Err("réponse HTTP inattendue".into());
    }
    loop {
        match read(&mut stream, &mut buf)? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total as f64 * 8.0 / start.elapsed().as_secs_f64().max(1e-6) / 1e6)
}

fn read(stream: &mut TcpStream, buf: &mut [u8]) -> Result<usize, String> {
    stream.read(buf).map_err(|e| format!("lecture : {e}"))
}

/// "http://hote:port/chemin" -> ("hote:port", "/chemin")
pub fn parse_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if host.is_empty() {
        return None;
    }
    let host = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
    let path = if path.is_empty() { "/" } else { path };
    Some((host, path.to_string()))
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
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") && client.read(&mut byte).unwrap() > 0 {
                request.push(byte[0]);
            }
            client.write_all(b"HTTP/1.0 200 OK\r\n\r\n").unwrap();
            client.write_all(&vec![0u8; 1 << 20]).unwrap();
        });
        assert!(measure(&format!("http://{addr}/x"), Duration::from_secs(5)).unwrap() > 0.0);
    }
}
