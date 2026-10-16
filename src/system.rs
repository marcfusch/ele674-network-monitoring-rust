//! Interactions avec le système : signaux, droits, fichier PID.

use crate::config::Config;
use std::ffi::CString;
use std::time::Duration;

const PIDFILE: &str = "/run/netmon.pid";

pub enum Signal {
    Stop,   // SIGTERM, SIGINT
    Report, // SIGHUP
}

/// Bloque les signaux dans tous les threads (à appeler avant de les créer) :
/// seul `wait_signal` les reçoit.
pub fn block_signals() -> libc::sigset_t {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut set, sig);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

pub fn wait_signal(set: &libc::sigset_t) -> Signal {
    loop {
        let mut sig = 0;
        if unsafe { libc::sigwait(set, &mut sig) } == 0 {
            return if sig == libc::SIGHUP { Signal::Report } else { Signal::Stop };
        }
    }
}

pub fn write_pidfile() {
    let _ = std::fs::write(PIDFILE, format!("{}\n", std::process::id()));
}

/// Donne le dossier des rapports et le journal à `user`, puis prend son uid/gid.
pub fn drop_privileges(user: &str, cfg: &Config) -> Result<u32, String> {
    let name = CString::new(user).map_err(|e| e.to_string())?;
    let pw = unsafe { libc::getpwnam(name.as_ptr()) };
    if pw.is_null() {
        return Err("utilisateur inconnu".into());
    }
    let (uid, gid) = unsafe { ((*pw).pw_uid, (*pw).pw_gid) };

    for path in std::iter::once(&cfg.report_dir).chain(&cfg.log_file) {
        if let Ok(p) = CString::new(path.as_str()) {
            unsafe { libc::chown(p.as_ptr(), uid, gid) };
        }
    }
    unsafe {
        // gid avant uid : une fois l'uid changé, on ne peut plus changer le gid
        if libc::setgroups(0, std::ptr::null()) != 0 || libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if libc::setuid(0) == 0 {
            return Err("root est encore récupérable".into());
        }
    }
    Ok(uid)
}

/// `netmon --stop` : envoie SIGTERM à l'instance en cours et attend sa fin.
pub fn stop_running_instance() -> i32 {
    let Some(pid) = std::fs::read_to_string(PIDFILE).ok().and_then(|s| s.trim().parse::<i32>().ok()) else {
        return 0;
    };
    // Fichier PID périmé : ce PID n'est peut-être plus netmon
    if read_proc(pid, "comm").trim() != "netmon" {
        return 0;
    }
    unsafe { libc::kill(pid, libc::SIGTERM) };

    for _ in 0..150 {
        if !is_running(pid) {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("netmon: l'instance {pid} ne s'est pas arrêtée en 15 s");
    1
}

fn read_proc(pid: i32, file: &str) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/{file}")).unwrap_or_default()
}

/// Faux si le processus a disparu ou n'est plus qu'un zombie.
fn is_running(pid: i32) -> bool {
    let stat = read_proc(pid, "stat");
    let state = stat.rsplit(") ").next().unwrap_or_default();
    !stat.is_empty() && !state.starts_with('Z')
}
