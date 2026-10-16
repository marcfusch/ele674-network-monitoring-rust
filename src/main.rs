//! netmon - moniteur de connectivité réseau multi-thread (ELE674, phase 3)
//!
//! Threads : ping (latence TCP), bandwidth (débit HTTP), report (rapports
//! JSON), web (interface + API). Le thread principal attend les signaux :
//! SIGTERM/SIGINT = arrêt propre, SIGHUP = rapport immédiat.

mod bandwidth;
mod config;
mod control;
mod log;
mod ping;
mod report;
mod state;
mod web;

use config::Config;
use state::App;
use std::ffi::CString;
use std::thread;

const DEFAULT_CONFIG: &str = "/etc/netmon/config.toml";
const PIDFILE: &str = "/run/netmon.pid";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        return println!("netmon {}", env!("CARGO_PKG_VERSION"));
    }
    if args.iter().any(|a| a == "--stop") {
        std::process::exit(stop_running_instance());
    }
    let path = match args.iter().position(|a| a == "-c") {
        Some(i) => args.get(i + 1).map_or(DEFAULT_CONFIG, |s| s.as_str()),
        None => DEFAULT_CONFIG,
    };
    let cfg = config::load(path).unwrap_or_else(|e| {
        eprintln!("netmon: configuration {path} invalide : {e}");
        std::process::exit(1);
    });

    // Signaux bloqués AVANT de créer les threads (le masque est hérité) :
    // seul le thread principal les reçoit, de façon synchrone avec sigwait.
    let signals = block_signals();

    if let Err(e) = std::fs::create_dir_all(&cfg.report_dir) {
        eprintln!("netmon: création de {} : {e}", cfg.report_dir);
    }
    if let Some(f) = &cfg.log_file {
        if let Err(e) = log::init(f, cfg.log_max_kb) {
            eprintln!("netmon: journal {f} : {e}");
        }
    }
    // Fichier PID écrit en root (avant l'abandon des droits), utilisé par --stop
    let _ = std::fs::write(PIDFILE, format!("{}\n", std::process::id()));
    if let (Some(user), 0) = (&cfg.user, unsafe { libc::getuid() }) {
        match drop_privileges(user, &cfg) {
            Ok(uid) => info!("droits root abandonnés : utilisateur {user} (uid {uid})"),
            Err(e) => error!("abandon des droits vers {user} : {e} (on continue en root)"),
        }
    }
    info!("démarrage netmon {} (config {path})", env!("CARGO_PKG_VERSION"));

    let app = App::new(cfg.targets.clone());
    let (app, cfg) = (&app, &cfg);
    // thread::scope : les threads empruntent `app` et `cfg` (pas besoin d'Arc),
    // et la fin du bloc attend automatiquement que tous les threads soient finis.
    thread::scope(|s| {
        let threads: [(&str, fn(&App, &Config)); 4] =
            [("ping", ping::run), ("bandwidth", bandwidth::run), ("report", report::run), ("web", web::run)];
        for (name, run) in threads {
            thread::Builder::new()
                .name(name.into())
                .spawn_scoped(s, move || run(app, cfg))
                .expect("création de thread");
        }
        wait_for_signals(app, &signals);
    });
    info!("arrêt propre terminé");
}

fn wait_for_signals(app: &App, signals: &libc::sigset_t) {
    loop {
        let mut sig = 0;
        if unsafe { libc::sigwait(signals, &mut sig) } != 0 {
            continue;
        }
        if sig == libc::SIGHUP {
            info!("SIGHUP reçu : rapport immédiat");
            app.control.request_report();
        } else {
            info!("signal {sig} reçu : arrêt en cours");
            app.control.shutdown(); // réveille tous les threads
            return;
        }
    }
}

fn block_signals() -> libc::sigset_t {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut set, s);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

/// Donne le dossier des rapports et le journal à `user`, puis bascule le
/// processus sur son uid/gid. Le service n'a pas besoin de root
/// (connexions TCP sortantes, port 8080 > 1024).
fn drop_privileges(user: &str, cfg: &Config) -> Result<u32, String> {
    let name = CString::new(user).map_err(|e| e.to_string())?;
    let pw = unsafe { libc::getpwnam(name.as_ptr()) };
    if pw.is_null() {
        return Err("utilisateur inconnu".into());
    }
    let (uid, gid) = unsafe { ((*pw).pw_uid, (*pw).pw_gid) };
    for p in std::iter::once(&cfg.report_dir).chain(cfg.log_file.iter()) {
        if let Ok(c) = CString::new(p.as_str()) {
            unsafe { libc::chown(c.as_ptr(), uid, gid) };
        }
    }
    unsafe {
        // Groupes et gid d'abord, uid en dernier (après, on n'a plus le droit)
        if libc::setgroups(0, std::ptr::null()) != 0 || libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if libc::setuid(0) == 0 {
            return Err("vérification échouée : root récupérable".into());
        }
    }
    Ok(uid)
}

/// `netmon --stop` : SIGTERM à l'instance en cours, puis attente de sa fin.
/// Appelé par le script d'arrêt (BusyBox n'a pas `kill` ici) AVANT qu'init ne
/// remonte le rootfs en lecture seule : le dernier rapport peut être écrit.
fn stop_running_instance() -> i32 {
    let pid: i32 = match std::fs::read_to_string(PIDFILE).ok().and_then(|s| s.trim().parse().ok()) {
        Some(p) => p,
        None => return 0, // aucune instance
    };
    // Fichier PID périmé (après un crash) : ce PID n'est peut-être plus netmon
    if std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim() != "netmon" {
        return 0;
    }
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..150 {
        // Fini quand le processus a disparu ou n'est plus qu'un zombie (Z)
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        if stat.is_empty() || stat.rsplit(") ").next().map_or(false, |s| s.starts_with('Z')) {
            return 0;
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("netmon: l'instance {pid} ne s'est pas arrêtée en 15 s");
    1
}
