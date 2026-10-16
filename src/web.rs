//! Interface web + API de contrôle à distance. Serveur HTTP/1.0 minimal écrit
//! avec la bibliothèque standard (une requête à la fois, suffisant ici) :
//!   GET  /              page de statut (rafraîchie toutes les 2 s)
//!   GET  /api/status    état complet en JSON
//!   GET  /api/health    santé du service (uid, uptime, rapports)
//!   GET  /api/reports   liste des rapports sur disque
//!   POST /api/report    demande un rapport immédiat
//!   POST /api/targets   remplace les cibles surveillées (tableau JSON)

use crate::{config::valid_target, config::Config, error, info, report, state::App};
use serde_json::json;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Moniteur réseau</title>
<h1>Moniteur réseau</h1><pre id="s">chargement...</pre>
<p><button onclick="fetch('/api/report',{method:'POST'})">Rapport maintenant</button>
<input id="t" size="60" placeholder='["10.0.2.2:8000","1.1.1.1:443"]'>
<button onclick="fetch('/api/targets',{method:'POST',body:document.getElementById('t').value})">Changer les cibles</button></p>
<script>
async function f(){const d=await (await fetch('/api/status')).json();let t='';
for(const[k,v]of Object.entries(d.pings)){const l=v.last&&v.last.value;
t+=(l!=null?'🟢 ':'🔴 ')+k+' : '+(l!=null?l.toFixed(1)+' ms':'injoignable')+' (perte '+v.stats.loss_percent.toFixed(0)+' %)\n';}
const b=d.bandwidth.last&&d.bandwidth.last.value;t+='\nDébit : '+(b!=null?b.toFixed(1)+' Mb/s':'n/a');
t+='\nUptime : '+d.uptime_s+' s, rapports écrits : '+d.reports_written;
document.getElementById('s').textContent=t;}
f();setInterval(f,2000);
</script>"#;

pub fn run(app: &App, cfg: &Config) {
    // Port occupé : on réessaie au lieu de faire planter toute l'application
    let listener = loop {
        match TcpListener::bind(&cfg.web_bind) {
            Ok(l) => break l,
            Err(e) => error!("serveur web sur {} : {e}, nouvel essai dans 5 s", cfg.web_bind),
        }
        if app.control.sleep(Duration::from_secs(5)) {
            return;
        }
    };
    // Non bloquant : on revérifie régulièrement si l'arrêt est demandé
    listener.set_nonblocking(true).ok();
    info!("interface web : http://{}", cfg.web_bind);
    while !app.control.is_shutdown() {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = handle(app, cfg, stream); // un client défaillant n'arrête pas le serveur
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                app.control.sleep(Duration::from_millis(100));
            }
            Err(e) => error!("serveur web : {e}"),
        }
    }
}

fn handle(app: &App, cfg: &Config, stream: TcpStream) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?; // un client lent ne bloque pas
    let mut reader = BufReader::new((&stream).take(16 * 1024)); // taille de requête bornée

    // Ligne de requête : "GET /chemin HTTP/1.1"
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("").to_string();

    // En-têtes : seule la taille du corps nous intéresse
    let mut len = 0;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; len.min(4096)];
    reader.read_exact(&mut body)?;

    let (code, mime, out) = route(app, cfg, &method, &path, &String::from_utf8_lossy(&body));
    let reason = match code { 200 => "OK", 202 => "Accepted", 400 => "Bad Request", _ => "Not Found" };
    write!(&stream, "HTTP/1.0 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\r\n", out.len())?;
    (&stream).write_all(out.as_bytes())
}

fn route(app: &App, cfg: &Config, method: &str, path: &str, body: &str) -> (u16, &'static str, String) {
    const JSON: &str = "application/json";
    match (method, path) {
        ("GET", "/") => (200, "text/html; charset=utf-8", PAGE.to_string()),
        ("GET", "/api/status") => {
            let mut s = app.read().summary();
            s["targets"] = json!(app.targets());
            s["uptime_s"] = app.started.elapsed().as_secs().into();
            s["reports_written"] = app.reports_written.load(Ordering::Relaxed).into();
            (200, JSON, s.to_string())
        }
        ("GET", "/api/health") => (200, JSON, json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "uid": unsafe { libc::getuid() },
            "uptime_s": app.started.elapsed().as_secs(),
            "reports_written": app.reports_written.load(Ordering::Relaxed),
        }).to_string()),
        ("GET", "/api/reports") => {
            let mut r = report::list(Path::new(&cfg.report_dir)).unwrap_or_default();
            r.sort();
            (200, JSON, json!(r).to_string())
        }
        ("POST", "/api/report") => {
            app.control.request_report();
            (202, JSON, r#"{"ok":true}"#.into())
        }
        ("POST", "/api/targets") => match serde_json::from_str::<Vec<String>>(body) {
            Ok(t) if !t.is_empty() && t.iter().all(|x| valid_target(x)) => {
                info!("cibles modifiées à distance : {t:?}");
                app.set_targets(t.clone());
                (200, JSON, json!({ "targets": t }).to_string())
            }
            _ => (400, JSON, r#"{"error":"tableau JSON de cibles \"hote:port\" attendu"}"#.into()),
        },
        _ => (404, JSON, r#"{"error":"introuvable"}"#.into()),
    }
}
