//! Journal : niveau + date, sur stderr et dans un fichier avec rotation par
//! taille (fichier.log -> fichier.log.1). Un Mutex garantit qu'une ligne
//! n'est jamais mélangée avec celle d'un autre thread.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

struct LogFile {
    path: String,
    max_bytes: u64,
    file: File,
}

static LOG: Mutex<Option<LogFile>> = Mutex::new(None);

pub fn init(path: &str, max_kb: u64) -> std::io::Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(LogFile { path: path.to_string(), max_bytes: max_kb * 1024, file });
    Ok(())
}

pub fn write(level: &str, msg: std::fmt::Arguments) {
    let line = format!("{} [{level}] {msg}\n", format_time(now(), "%Y-%m-%d %H:%M:%S"));
    eprint!("{line}");
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(log) = guard.as_mut() {
        let _ = log.file.write_all(line.as_bytes());
        if log.file.metadata().map_or(false, |m| m.len() > log.max_bytes) {
            let _ = std::fs::rename(&log.path, format!("{}.1", log.path));
            if let Ok(f) = OpenOptions::new().create(true).append(true).open(&log.path) {
                log.file = f;
            }
        }
    }
}

/// Secondes depuis le 1er janvier 1970
pub fn now() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

/// Date locale formatée par strftime (libc) : pas besoin d'une bibliothèque de dates
pub fn format_time(t: i64, fmt: &str) -> String {
    let mut buf = [0u8; 64];
    let fmt = std::ffi::CString::new(fmt).unwrap_or_default();
    unsafe {
        let t = t as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

#[macro_export]
macro_rules! info { ($($a:tt)*) => { $crate::log::write("INFO", format_args!($($a)*)) } }
#[macro_export]
macro_rules! warn { ($($a:tt)*) => { $crate::log::write("WARN", format_args!($($a)*)) } }
#[macro_export]
macro_rules! error { ($($a:tt)*) => { $crate::log::write("ERROR", format_args!($($a)*)) } }
