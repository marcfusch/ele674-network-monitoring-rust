//! Dates via la libc (pas de bibliothèque de dates).

use std::ffi::CString;

/// Secondes depuis le 1er janvier 1970.
pub fn now() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

/// Date locale au format strftime, ex. "%Y-%m-%d %H:%M:%S".
pub fn format(t: i64, fmt: &str) -> String {
    let fmt = CString::new(fmt).unwrap_or_default();
    let mut buf = [0u8; 64];
    unsafe {
        let t = t as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

/// Date ISO 8601, ex. "2026-10-16T15:40:46".
pub fn iso(t: i64) -> String {
    format(t, "%Y-%m-%dT%H:%M:%S")
}
