use crate::{config::PingConfig, state::MonitorState};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

pub fn run(state: Arc<RwLock<MonitorState>>, cfg: PingConfig) {
    let timeout = Duration::from_millis(cfg.timeout_ms);
    loop {
        for target in &cfg.targets {
            let latency = probe(target, timeout);
            state.write().unwrap().record_ping(target, latency);
        }
        thread::sleep(Duration::from_secs(cfg.interval_secs));
    }
}

// Temps de connexion TCP en ms, None si échec
fn probe(target: &str, timeout: Duration) -> Option<f64> {
    let addr = target.to_socket_addrs().ok()?.next()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, timeout).ok()?;
    Some(start.elapsed().as_secs_f64() * 1000.0)
}
