use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub ping: PingConfig,
    pub bandwidth: BandwidthConfig,
    pub report: ReportConfig,
    pub web: WebConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PingConfig {
    pub interval_secs: u64,
    pub timeout_ms: u64,
    pub targets: Vec<String>, // format "hote:port"
}

#[derive(Debug, Clone, Deserialize)]
pub struct BandwidthConfig {
    pub interval_secs: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportConfig {
    pub interval_secs: u64,
    pub output_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebConfig {
    pub bind: String,
}

pub fn load(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}
