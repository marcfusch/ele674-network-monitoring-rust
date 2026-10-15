use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

const HISTORY: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub time: DateTime<Local>,
    pub value: Option<f64>, // latence (ms) ou débit (Mb/s), None si échec
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub count: usize,
    pub loss_percent: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct MonitorState {
    pub pings: HashMap<String, VecDeque<Sample>>,
    pub bandwidth: VecDeque<Sample>,
}

impl MonitorState {
    pub fn record_ping(&mut self, target: &str, value: Option<f64>) {
        push(self.pings.entry(target.to_string()).or_default(), value);
    }

    pub fn record_bandwidth(&mut self, value: Option<f64>) {
        push(&mut self.bandwidth, value);
    }

    pub fn summary(&self) -> serde_json::Value {
        let pings: HashMap<_, _> = self
            .pings
            .iter()
            .map(|(t, h)| (t, serde_json::json!({ "last": h.back(), "stats": stats(h) })))
            .collect();
        serde_json::json!({
            "time": Local::now(),
            "pings": pings,
            "bandwidth": { "last": self.bandwidth.back(), "stats": stats(&self.bandwidth) },
        })
    }
}

fn push(history: &mut VecDeque<Sample>, value: Option<f64>) {
    if history.len() == HISTORY {
        history.pop_front();
    }
    history.push_back(Sample { time: Local::now(), value });
}

pub fn stats(history: &VecDeque<Sample>) -> Stats {
    let ok: Vec<f64> = history.iter().filter_map(|s| s.value).collect();
    let count = history.len();
    Stats {
        count,
        loss_percent: if count == 0 { 0.0 } else { (count - ok.len()) as f64 * 100.0 / count as f64 },
        min: ok.iter().copied().reduce(f64::min),
        max: ok.iter().copied().reduce(f64::max),
        avg: (!ok.is_empty()).then(|| ok.iter().sum::<f64>() / ok.len() as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_with_losses() {
        let mut s = MonitorState::default();
        for v in [Some(10.0), None, Some(20.0), Some(30.0)] {
            s.record_ping("a", v);
        }
        let st = stats(&s.pings["a"]);
        assert_eq!((st.count, st.loss_percent), (4, 25.0));
        assert_eq!((st.min, st.max, st.avg), (Some(10.0), Some(30.0), Some(20.0)));
    }
}
