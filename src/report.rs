//! Thread rapports : un JSON par intervalle, sur demande et à l'arrêt.

use crate::config::Config;
use crate::control::Wake;
use crate::state::App;
use crate::{error, info, time};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub fn run(app: &App, cfg: &Config) {
    loop {
        let wake = app.control.wait(Duration::from_secs(cfg.report_interval), true);
        match write(app, cfg) {
            Ok(path) => info!("rapport écrit : {path}"),
            Err(e) => error!("écriture du rapport : {e}"),
        }
        if wake == Wake::Shutdown {
            return;
        }
    }
}

fn write(app: &App, cfg: &Config) -> std::io::Result<String> {
    std::fs::create_dir_all(&cfg.report_dir)?;
    let n = app.reports_written.fetch_add(1, Ordering::Relaxed);
    let date = time::format(time::now(), "%Y%m%d_%H%M%S");
    let path = format!("{}/report_{date}_{n:04}.json", cfg.report_dir);

    let mut summary = app.read().summary();
    summary["uptime_s"] = app.started.elapsed().as_secs().into();

    // Fichier temporaire puis rename : jamais de rapport à moitié écrit
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, summary.to_string())?;
    std::fs::rename(&tmp, &path)?;
    rotate(Path::new(&cfg.report_dir), cfg.report_keep)?;
    Ok(path)
}

/// Garde les `keep` rapports les plus récents (les noms se trient par date).
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
