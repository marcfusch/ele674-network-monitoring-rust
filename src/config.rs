//! Lecture de config.toml (sections, nombres, "chaînes", ["listes"]).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Config {
    pub ping_interval: u64, // s
    pub ping_timeout: u64,  // ms
    pub targets: Vec<String>,
    pub bw_interval: u64,
    pub bw_timeout: u64,
    pub bw_url: String,
    pub report_interval: u64,
    pub report_dir: String,
    pub report_keep: usize,
    pub web_bind: String,
    pub user: Option<String>,
    pub log_file: Option<String>,
    pub log_max_kb: u64,
}

pub fn load(path: &str) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Config, String> {
    let raw = read_keys(text)?;
    let cfg = from_keys(&raw)?;
    validate(&cfg)?;
    Ok(cfg)
}

/// "section.cle" -> valeur brute
fn read_keys(text: &str) -> Result<HashMap<String, String>, String> {
    let mut keys = HashMap::new();
    let mut section = String::new();
    for (n, line) in text.lines().enumerate() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            keys.insert(format!("{section}.{}", key.trim()), value.trim().to_string());
        } else {
            return Err(format!("ligne {} : '=' attendu", n + 1));
        }
    }
    Ok(keys)
}

fn from_keys(keys: &HashMap<String, String>) -> Result<Config, String> {
    let num = |k: &str, default: u64| match keys.get(k) {
        Some(v) => v.parse::<u64>().map_err(|_| format!("{k} : nombre attendu")),
        None => Ok(default),
    };
    let text = |k: &str| keys.get(k).map(|v| v.trim_matches('"').to_string());

    Ok(Config {
        ping_interval: num("ping.interval_secs", 5)?,
        ping_timeout: num("ping.timeout_ms", 1000)?,
        targets: keys.get("ping.targets").map(|v| parse_list(v)).unwrap_or_default(),
        bw_interval: num("bandwidth.interval_secs", 30)?,
        bw_timeout: num("bandwidth.timeout_ms", 10_000)?,
        bw_url: text("bandwidth.url").unwrap_or_default(),
        report_interval: num("report.interval_secs", 60)?,
        report_dir: text("report.output_dir").unwrap_or_else(|| "reports".into()),
        report_keep: num("report.keep", 20)? as usize,
        web_bind: text("web.bind").unwrap_or_else(|| "127.0.0.1:8080".into()),
        user: text("system.user"),
        log_file: text("system.log_file"),
        log_max_kb: num("system.log_max_kb", 64)?,
    })
}

fn validate(cfg: &Config) -> Result<(), String> {
    if cfg.targets.is_empty() || !cfg.targets.iter().all(|t| valid_target(t)) {
        return Err("ping.targets : liste de \"hote:port\" attendue".into());
    }
    if !cfg.bw_url.starts_with("http://") {
        return Err("bandwidth.url : http:// requis".into());
    }
    if cfg.ping_interval == 0 || cfg.bw_interval == 0 || cfg.report_interval == 0 {
        return Err("les intervalles doivent être >= 1".into());
    }
    Ok(())
}

/// "hote:port" avec un hôte non vide et un port entre 1 et 65535.
pub fn valid_target(target: &str) -> bool {
    match target.rsplit_once(':') {
        Some((host, port)) => !host.is_empty() && port.parse::<u16>().is_ok_and(|p| p > 0),
        None => false,
    }
}

/// Retire un commentaire `#`, sauf à l'intérieur d'une chaîne.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// `["a", "b"]` -> ["a", "b"]
fn parse_list(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config() {
        let c = parse(
            "[ping] # commentaire\ninterval_secs = 2\ntargets = [\"a:1\", \"b:2\"]\n\
             [bandwidth]\nurl = \"http://h:8000/x\"\n[system]\nuser = \"netmon\"\n",
        )
        .unwrap();
        assert_eq!((c.ping_interval, c.ping_timeout), (2, 1000));
        assert_eq!(c.targets, ["a:1", "b:2"]);
        assert_eq!(c.user.as_deref(), Some("netmon"));
        assert_eq!(c.report_keep, 20);
    }

    #[test]
    fn rejects_bad_config() {
        assert!(parse("[ping]\ntargets = [\"sans-port\"]\n[bandwidth]\nurl = \"http://h/\"").is_err());
        assert!(parse("[ping]\ntargets = [\"a:1\"]\n[bandwidth]\nurl = \"https://h/\"").is_err());
        assert!(parse("[ping]\ninterval_secs = abc").is_err());
    }

    #[test]
    fn repo_config_is_valid() {
        load(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")).unwrap();
    }

    #[test]
    fn target_validation() {
        assert!(valid_target("10.0.2.2:8000"));
        assert!(!valid_target("10.0.2.2"));
        assert!(!valid_target(":80"));
        assert!(!valid_target("h:0"));
        assert!(!valid_target("h:99999"));
    }
}
