//! Journal : stderr + fichier avec rotation par taille (fichier.log -> fichier.log.1).

use crate::time;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

struct LogFile {
    path: String,
    max_bytes: u64,
    file: File,
}

// Le Mutex évite que les lignes de deux threads se mélangent
static LOG: Mutex<Option<LogFile>> = Mutex::new(None);

pub fn init(path: &str, max_kb: u64) -> std::io::Result<()> {
    let file = open(path)?;
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(LogFile { path: path.to_string(), max_bytes: max_kb * 1024, file });
    Ok(())
}

pub fn write(level: &str, msg: std::fmt::Arguments) {
    let line = format!("{} [{level}] {msg}\n", time::format(time::now(), "%Y-%m-%d %H:%M:%S"));
    eprint!("{line}");

    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(log) = guard.as_mut() else { return };
    let _ = log.file.write_all(line.as_bytes());
    if log.file.metadata().is_ok_and(|m| m.len() > log.max_bytes) {
        let _ = std::fs::rename(&log.path, format!("{}.1", log.path));
        if let Ok(f) = open(&log.path) {
            log.file = f;
        }
    }
}

fn open(path: &str) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[macro_export]
macro_rules! info { ($($a:tt)*) => { $crate::log::write("INFO", format_args!($($a)*)) } }
#[macro_export]
macro_rules! warn { ($($a:tt)*) => { $crate::log::write("WARN", format_args!($($a)*)) } }
#[macro_export]
macro_rules! error { ($($a:tt)*) => { $crate::log::write("ERROR", format_args!($($a)*)) } }

#[cfg(test)]
mod tests {
    #[test]
    fn rotates_when_too_big() {
        let path = std::env::temp_dir().join(format!("netmon-log-{}", std::process::id()));
        let path = path.to_str().unwrap();
        super::init(path, 1).unwrap(); // 1 Ko maximum
        for i in 0..40 {
            crate::info!("ligne de test {i} pour remplir le journal");
        }
        let rotated = format!("{path}.1");
        assert!(std::fs::metadata(&rotated).is_ok());
        assert!(std::fs::metadata(path).unwrap().len() <= 1024);
        let _ = (std::fs::remove_file(path), std::fs::remove_file(rotated));
    }
}
