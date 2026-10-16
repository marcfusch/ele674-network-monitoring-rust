//! Synchronisation entre threads : un Mutex protège les drapeaux de contrôle,
//! un Condvar réveille les threads endormis (arrêt demandé ou rapport immédiat).
//! Les threads n'utilisent jamais thread::sleep : leur attente est interruptible,
//! donc un SIGTERM arrête l'application en quelques millisecondes.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Flags {
    shutdown: bool,
    report_now: bool,
}

#[derive(Default)]
pub struct Control {
    flags: Mutex<Flags>,
    cv: Condvar,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Wake {
    Timeout,
    Report,
    Shutdown,
}

impl Control {
    // Un thread qui panique en tenant le verrou "empoisonne" le Mutex :
    // les drapeaux restent cohérents, on récupère donc la valeur au lieu de planter.
    fn lock(&self) -> MutexGuard<'_, Flags> {
        self.flags.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn shutdown(&self) {
        self.lock().shutdown = true;
        self.cv.notify_all();
    }

    pub fn is_shutdown(&self) -> bool {
        self.lock().shutdown
    }

    pub fn request_report(&self) {
        self.lock().report_now = true;
        self.cv.notify_all();
    }

    /// Attend `timeout` ou un arrêt. Renvoie true si l'arrêt est demandé.
    pub fn sleep(&self, timeout: Duration) -> bool {
        self.wait(timeout, false) == Wake::Shutdown
    }

    /// Attend `timeout`, un arrêt, ou (si `report` vaut true) une demande de rapport.
    pub fn wait(&self, timeout: Duration, report: bool) -> Wake {
        let deadline = Instant::now() + timeout;
        let mut f = self.lock();
        loop {
            if f.shutdown {
                return Wake::Shutdown;
            }
            if report && f.report_now {
                f.report_now = false;
                return Wake::Report;
            }
            let now = Instant::now();
            if now >= deadline {
                return Wake::Timeout;
            }
            // wait_timeout relâche le Mutex pendant l'attente (pas d'attente active)
            f = self.cv.wait_timeout(f, deadline - now).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn shutdown_wakes_sleeping_thread() {
        let c = Arc::new(Control::default());
        let c2 = c.clone();
        let t = thread::spawn(move || {
            let start = Instant::now();
            assert!(c2.sleep(Duration::from_secs(30)));
            start.elapsed()
        });
        thread::sleep(Duration::from_millis(50));
        c.shutdown();
        assert!(t.join().unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn report_request_is_consumed_once() {
        let c = Control::default();
        c.request_report();
        assert_eq!(c.wait(Duration::from_millis(10), true), Wake::Report);
        assert_eq!(c.wait(Duration::from_millis(10), true), Wake::Timeout);
    }
}
