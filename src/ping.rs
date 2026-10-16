//! Thread ping : temps de connexion TCP vers chaque cible (pas besoin de root).

use crate::config::Config;
use crate::state::App;
use crate::{info, warn};
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn run(app: &App, cfg: &Config) {
    let timeout = Duration::from_millis(cfg.ping_timeout);
    let mut was_up: HashMap<String, bool> = HashMap::new();
    loop {
        for target in app.targets() {
            let latency = probe(&target, timeout);
            app.write().record_ping(&target, latency);
            log_change(&mut was_up, &target, latency);
        }
        if app.control.sleep(Duration::from_secs(cfg.ping_interval)) {
            return;
        }
    }
}

/// Temps de connexion en ms, None si la cible ne répond pas.
pub fn probe(target: &str, timeout: Duration) -> Option<f64> {
    let addr = target.to_socket_addrs().ok()?.next()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, timeout).ok()?;
    Some(start.elapsed().as_secs_f64() * 1000.0)
}

/// Journalise seulement les changements d'état (joignable / injoignable).
fn log_change(was_up: &mut HashMap<String, bool>, target: &str, latency: Option<f64>) {
    let up = latency.is_some();
    if was_up.insert(target.to_string(), up) == Some(up) {
        return;
    }
    match latency {
        Some(ms) => info!("cible {target} joignable ({ms:.1} ms)"),
        None => warn!("cible {target} injoignable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_open_and_closed_ports() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        assert!(probe(&addr, Duration::from_secs(1)).is_some());
        drop(listener);
        assert!(probe(&addr, Duration::from_secs(1)).is_none());
        assert!(probe("sans-port", Duration::from_secs(1)).is_none());
    }
}
