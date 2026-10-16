//! Thread ping : temps de connexion TCP vers chaque cible ("ping" sans ICMP,
//! donc sans droits root). Seuls les changements d'état sont journalisés.

use crate::{config::Config, info, state::App, warn};
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn run(app: &App, cfg: &Config) {
    let timeout = Duration::from_millis(cfg.ping_timeout);
    let mut was_up: HashMap<String, bool> = HashMap::new();
    loop {
        // Copie de la liste : le verrou n'est pas gardé pendant les mesures (lentes)
        for target in app.targets() {
            let latency = probe(&target, timeout);
            app.write().record_ping(&target, latency);
            if was_up.insert(target.clone(), latency.is_some()) != Some(latency.is_some()) {
                match latency {
                    Some(ms) => info!("cible {target} joignable ({ms:.1} ms)"),
                    None => warn!("cible {target} injoignable"),
                }
            }
        }
        if app.control.sleep(Duration::from_secs(cfg.ping_interval)) {
            return;
        }
    }
}

/// Temps de connexion TCP en ms, None si échec
pub fn probe(target: &str, timeout: Duration) -> Option<f64> {
    let addr = target.to_socket_addrs().ok()?.next()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, timeout).ok()?;
    Some(start.elapsed().as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_open_and_closed_ports() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        assert!(probe(&addr, Duration::from_secs(1)).is_some());
        drop(l);
        assert!(probe(&addr, Duration::from_secs(1)).is_none());
        assert!(probe("sans-port", Duration::from_secs(1)).is_none());
    }
}
