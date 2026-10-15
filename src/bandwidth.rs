use crate::{config::BandwidthConfig, state::MonitorState};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

pub fn run(state: Arc<RwLock<MonitorState>>, cfg: BandwidthConfig) {
    loop {
        let mbps = measure(&cfg.url);
        state.write().unwrap().record_bandwidth(mbps);
        thread::sleep(Duration::from_secs(cfg.interval_secs));
    }
}

// Débit de téléchargement en Mb/s, None si échec
fn measure(url: &str) -> Option<f64> {
    let start = Instant::now();
    let mut body = ureq::get(url).call().ok()?.into_body();
    let bytes = std::io::copy(&mut body.as_reader(), &mut std::io::sink()).ok()?;
    Some(bytes as f64 * 8.0 / start.elapsed().as_secs_f64() / 1e6)
}