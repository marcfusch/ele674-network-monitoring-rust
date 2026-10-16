//! netmon : moniteur de connectivité réseau multi-thread (ELE674, phase 3).

mod bandwidth;
mod config;
mod control;
mod log;
mod ping;
mod report;
mod state;
mod system;
mod time;
mod web;

use config::Config;
use state::App;
use system::Signal;
use std::thread;

const DEFAULT_CONFIG: &str = "/etc/netmon/config.toml";

type Task = fn(&App, &Config);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        return println!("netmon {}", env!("CARGO_PKG_VERSION"));
    }
    if args.iter().any(|a| a == "--stop") {
        std::process::exit(system::stop_running_instance());
    }
    let path = match args.iter().position(|a| a == "-c") {
        Some(i) => args.get(i + 1).map_or(DEFAULT_CONFIG, |s| s.as_str()),
        None => DEFAULT_CONFIG,
    };
    let cfg = config::load(path).unwrap_or_else(|e| {
        eprintln!("netmon: configuration {path} invalide : {e}");
        std::process::exit(1);
    });

    let signals = system::block_signals();
    setup(&cfg);
    info!("démarrage netmon {} (config {path})", env!("CARGO_PKG_VERSION"));

    let app = App::new(cfg.targets.clone());
    run_threads(&app, &cfg, &signals);
    info!("arrêt propre terminé");
}

/// Dossier des rapports, journal, fichier PID, puis abandon des droits root.
fn setup(cfg: &Config) {
    if let Err(e) = std::fs::create_dir_all(&cfg.report_dir) {
        eprintln!("netmon: {} : {e}", cfg.report_dir);
    }
    if let Some(file) = &cfg.log_file {
        if let Err(e) = log::init(file, cfg.log_max_kb) {
            eprintln!("netmon: journal {file} : {e}");
        }
    }
    system::write_pidfile();

    let is_root = unsafe { libc::getuid() } == 0;
    if let (Some(user), true) = (&cfg.user, is_root) {
        match system::drop_privileges(user, cfg) {
            Ok(uid) => info!("droits root abandonnés : {user} (uid {uid})"),
            Err(e) => error!("abandon des droits vers {user} : {e}"),
        }
    }
}

/// Lance les 4 threads et attend un signal d'arrêt.
fn run_threads(app: &App, cfg: &Config, signals: &libc::sigset_t) {
    let tasks: [(&str, Task); 4] = [
        ("ping", ping::run),
        ("bandwidth", bandwidth::run),
        ("report", report::run),
        ("web", web::run),
    ];

    // Fin du scope = attente de la fin de tous les threads
    thread::scope(|s| {
        for (name, task) in tasks {
            thread::Builder::new()
                .name(name.into())
                .spawn_scoped(s, move || task(app, cfg))
                .expect("création de thread");
        }
        loop {
            match system::wait_signal(signals) {
                Signal::Report => {
                    info!("SIGHUP : rapport immédiat");
                    app.control.request_report();
                }
                Signal::Stop => {
                    info!("signal d'arrêt reçu");
                    app.control.shutdown();
                    break;
                }
            }
        }
    });
}
