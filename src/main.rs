mod bandwidth;
mod config;
mod ping;
mod report;
mod state;
mod web;

use std::sync::{Arc, RwLock};
use std::thread;

fn main() {
    let cfg = config::load("config.toml").expect("config.toml invalide");
    let state = Arc::new(RwLock::new(state::MonitorState::default()));

    let handles = [
        thread::spawn({ let (s, c) = (state.clone(), cfg.ping); move || ping::run(s, c) }),
        thread::spawn({ let (s, c) = (state.clone(), cfg.bandwidth); move || bandwidth::run(s, c) }),
        thread::spawn({ let (s, c) = (state.clone(), cfg.report); move || report::run(s, c) }),
        thread::spawn({ let (s, c) = (state.clone(), cfg.web); move || web::run(s, c) }),
    ];
    for h in handles {
        h.join().unwrap();
    }
}
