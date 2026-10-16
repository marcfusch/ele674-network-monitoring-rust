//! Arrêt et rapport immédiat : Mutex + Condvar.
//! Les threads attendent sur la Condvar au lieu de dormir : un arrêt les réveille aussitôt.

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

    /// Attend `timeout` ; renvoie true si l'arrêt est demandé entre-temps.
    pub fn sleep(&self, timeout: Duration) -> bool {
        self.wait(timeout, false) == Wake::Shutdown
    }

    /// Attend `timeout`, un arrêt, ou une demande de rapport si `report`.
    pub fn wait(&self, timeout: Duration, report: bool) -> Wake {
        let deadline = Instant::now() + timeout;
        let mut flags = self.lock();
        loop {
            if flags.shutdown {
                return Wake::Shutdown;
            }
            if report && flags.report_now {
                flags.report_now = false;
                return Wake::Report;
            }
            let now = Instant::now();
            if now >= deadline {
                return Wake::Timeout;
            }
            flags = self.cv.wait_timeout(flags, deadline - now).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    // Un Mutex "empoisonné" (panique d'un autre thread) reste utilisable
    fn lock(&self) -> MutexGuard<'_, Flags> {
        self.flags.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn shutdown_wakes_sleeping_thread() {
        let control = Arc::new(Control::default());
        let c = control.clone();
        let sleeper = thread::spawn(move || {
            let start = Instant::now();
            assert!(c.sleep(Duration::from_secs(30)));
            start.elapsed()
        });
        thread::sleep(Duration::from_millis(50));
        control.shutdown();
        assert!(sleeper.join().unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn report_request_is_consumed_once() {
        let control = Control::default();
        control.request_report();
        assert_eq!(control.wait(Duration::from_millis(10), true), Wake::Report);
        assert_eq!(control.wait(Duration::from_millis(10), true), Wake::Timeout);
    }
}
