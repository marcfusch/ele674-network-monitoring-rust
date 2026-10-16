//! Thread rapports : un fichier JSON par intervalle, sur demande (API, SIGHUP)
//! et à l'arrêt. Seuls les `keep` derniers sont conservés.

use crate::control::Wake;
use crate::log::{format_time, now};
use crate::{config::Config, error, info, state::App};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub fn run(app: &App, cfg: &Config) {
    loop {
        let wake = app.control.wait(Duration::from_secs(cfg.report_interval), true);
        match write_report(app, cfg) {
            Ok(path) => info!("rapport écrit : {path}"),
            Err(e) => error!("écriture du rapport : {e}"),
        }
        if wake == Wake::Shutdown {
            return; // dernier rapport écrit avant l'arrêt
        }
    }
}

fn write_report(app: &App, cfg: &Config) -> std::io::Result<String> {
    std::fs::create_dir_all(&cfg.report_dir)?;
    let n = app.reports_written.fetch_add(1, Ordering::Relaxed);
    let path = format!("{}/report_{}_{n:04}.json", cfg.report_dir, format_time(now(), "%Y%m%d_%H%M%S"));
    let mut summary = app.read().summary(); // verrou relâché dès la copie faite
    summary["uptime_s"] = app.started.elapsed().as_secs().into();
    // Écriture atomique : fichier temporaire puis rename. Un arrêt brutal ne
    // laisse jamais un rapport à moitié écrit.
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, summary.to_string())?;
    std::fs::rename(&tmp, &path)?;
    rotate(Path::new(&cfg.report_dir), cfg.report_keep)?;
    Ok(path)
}

/// Ne garde que les `keep` rapports les plus récents (les noms se trient par date)
pub fn rotate(dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut reports = list(dir)?;
    reports.sort();
    for name in &reports[..reports.len().saturating_sub(keep)] {
        std::fs::remove_file(dir.join(name))?;
    }
    Ok(())
}

pub fn list(dir: &Path) -> std::io::Result<Vec<String>> {
    Ok(std::fs::read_dir(dir)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("report_") && n.ends_with(".json"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_keeps_newest() {
        let dir = std::env::temp_dir().join(format!("netmon-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("report_2026_{i:04}.json")), "{}").unwrap();
        }
        std::fs::write(dir.join("autre.txt"), "").unwrap();
        rotate(&dir, 2).unwrap();
        let mut left = list(&dir).unwrap();
        left.sort();
        assert_eq!(left, ["report_2026_0003.json", "report_2026_0004.json"]);
        assert!(dir.join("autre.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
