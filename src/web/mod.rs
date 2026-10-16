//! Interface web et API de contrôle (port 8080).

mod api;
mod http;

use crate::config::Config;
use crate::state::App;
use crate::{error, info};
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

pub fn run(app: &App, cfg: &Config) {
    let Some(listener) = bind(app, &cfg.web_bind) else { return };
    info!("interface web : http://{}", cfg.web_bind);

    // Un thread par connexion : une connexion inactive ne bloque pas les autres
    thread::scope(|s| {
        while !app.control.is_shutdown() {
            match listener.accept() {
                Ok((stream, _)) => {
                    s.spawn(move || serve(app, cfg, stream));
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    app.control.sleep(Duration::from_millis(100));
                }
                Err(e) => error!("serveur web : {e}"),
            }
        }
    });
}

/// Réessaie toutes les 5 s si le port est occupé ; None si arrêt demandé.
fn bind(app: &App, addr: &str) -> Option<TcpListener> {
    loop {
        match TcpListener::bind(addr) {
            Ok(listener) => {
                // Non bloquant pour pouvoir vérifier l'arrêt régulièrement
                listener.set_nonblocking(true).ok()?;
                return Some(listener);
            }
            Err(e) => error!("serveur web sur {addr} : {e}, nouvel essai dans 5 s"),
        }
        if app.control.sleep(Duration::from_secs(5)) {
            return None;
        }
    }
}

fn serve(app: &App, cfg: &Config, stream: TcpStream) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    if let Ok(request) = http::read_request(&stream) {
        let _ = http::write_response(&stream, &api::route(app, cfg, &request));
    }
}
