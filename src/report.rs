use crate::{config::ReportConfig, state::MonitorState};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

pub fn run(state: Arc<RwLock<MonitorState>>, cfg: ReportConfig) {
    std::fs::create_dir_all(&cfg.output_dir).expect("création du dossier de rapports");
    loop {
        thread::sleep(Duration::from_secs(cfg.interval_secs));
        let summary = state.read().unwrap().summary();
        let path = format!("{}/report_{}.json", cfg.output_dir, chrono::Local::now().format("%Y%m%d_%H%M%S"));
        match std::fs::write(&path, serde_json::to_string_pretty(&summary).unwrap()) {
            Ok(()) => println!("Rapport écrit : {path}"),
            Err(e) => eprintln!("Erreur rapport {path} : {e}"),
        }
    }
}
