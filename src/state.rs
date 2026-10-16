//! État partagé entre les threads.

use crate::control::Control;
use crate::time;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Instant;

const HISTORY: usize = 100;

/// Latence (ms) ou débit (Mb/s) ; None si la mesure a échoué.
#[derive(Clone, Copy)]
pub struct Sample {
    pub time: i64,
    pub value: Option<f64>,
}

#[derive(Default)]
pub struct MonitorState {
    pub pings: HashMap<String, VecDeque<Sample>>,
    pub bandwidth: VecDeque<Sample>,
}

pub struct App {
    // RwLock : lectures fréquentes (web), écritures rares (mesures)
    pub state: RwLock<MonitorState>,
    pub targets: RwLock<Vec<String>>,
    pub control: Control,
    pub started: Instant,
    pub reports_written: AtomicU64,
}

impl App {
    pub fn new(targets: Vec<String>) -> Self {
        Self {
            state: RwLock::default(),
            targets: RwLock::new(targets),
            control: Control::default(),
            started: Instant::now(),
            reports_written: AtomicU64::new(0),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, MonitorState> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, MonitorState> {
        self.state.write().unwrap_or_else(|e| e.into_inner())
    }

    pub fn targets(&self) -> Vec<String> {
        self.targets.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_targets(&self, targets: Vec<String>) {
        self.write().pings.retain(|t, _| targets.contains(t));
        *self.targets.write().unwrap_or_else(|e| e.into_inner()) = targets;
    }

    /// Ajoute en tête de liste ; false si la cible est déjà surveillée.
    pub fn add_target(&self, target: &str) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        if targets.iter().any(|t| t == target) {
            return false;
        }
        targets.insert(0, target.to_string());
        true
    }

    /// Retire la cible et son historique ; false si elle n'existait pas.
    pub fn remove_target(&self, target: &str) -> bool {
        // Un seul verrou à la fois : pas d'interblocage avec set_targets
        let removed = {
            let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
            let before = targets.len();
            targets.retain(|t| t != target);
            targets.len() < before
        };
        self.write().pings.remove(target);
        removed
    }
}

impl MonitorState {
    pub fn record_ping(&mut self, target: &str, value: Option<f64>) {
        push(self.pings.entry(target.to_string()).or_default(), value);
    }

    pub fn record_bandwidth(&mut self, value: Option<f64>) {
        push(&mut self.bandwidth, value);
    }

    pub fn summary(&self) -> Value {
        let pings: serde_json::Map<String, Value> = self
            .pings
            .iter()
            .map(|(t, h)| (t.clone(), json!({ "last": last(h), "stats": stats(h) })))
            .collect();
        json!({
            "time": time::iso(time::now()),
            "pings": pings,
            "bandwidth": { "last": last(&self.bandwidth), "stats": stats(&self.bandwidth) },
        })
    }
}

fn push(history: &mut VecDeque<Sample>, value: Option<f64>) {
    if history.len() == HISTORY {
        history.pop_front();
    }
    history.push_back(Sample { time: time::now(), value });
}

fn last(history: &VecDeque<Sample>) -> Value {
    match history.back() {
        Some(s) => json!({ "time": time::iso(s.time), "value": s.value }),
        None => Value::Null,
    }
}

fn stats(history: &VecDeque<Sample>) -> Value {
    let ok: Vec<f64> = history.iter().filter_map(|s| s.value).collect();
    let count = history.len();
    let loss = if count == 0 { 0.0 } else { (count - ok.len()) as f64 * 100.0 / count as f64 };
    json!({
        "count": count,
        "loss_percent": loss,
        "min": ok.iter().copied().reduce(f64::min),
        "max": ok.iter().copied().reduce(f64::max),
        "avg": (!ok.is_empty()).then(|| ok.iter().sum::<f64>() / ok.len() as f64),
    })
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
        assert_eq!(st["count"], 4);
        assert_eq!(st["loss_percent"], 25.0);
        assert_eq!((st["min"].as_f64(), st["max"].as_f64(), st["avg"].as_f64()), (Some(10.0), Some(30.0), Some(20.0)));
    }

    #[test]
    fn history_is_bounded() {
        let mut s = MonitorState::default();
        for _ in 0..HISTORY + 10 {
            s.record_bandwidth(Some(1.0));
        }
        assert_eq!(s.bandwidth.len(), HISTORY);
    }

    #[test]
    fn added_target_goes_first_without_duplicates() {
        let app = App::new(vec!["a:1".into()]);
        assert!(app.add_target("b:2"));
        assert!(!app.add_target("a:1"));
        assert_eq!(app.targets(), ["b:2", "a:1"]);
    }

    #[test]
    fn remove_target_forgets_its_history() {
        let app = App::new(vec!["a:1".into(), "b:2".into()]);
        app.write().record_ping("a:1", Some(1.0));
        assert!(app.remove_target("a:1"));
        assert!(!app.remove_target("a:1"));
        assert_eq!(app.targets(), ["b:2"]);
        assert!(!app.read().pings.contains_key("a:1"));
    }

    #[test]
    fn removed_targets_are_forgotten() {
        let app = App::new(vec!["a:1".into(), "b:2".into()]);
        app.write().record_ping("a:1", Some(1.0));
        app.write().record_ping("b:2", Some(1.0));
        app.set_targets(vec!["b:2".into()]);
        assert_eq!(app.targets(), ["b:2"]);
        assert!(!app.read().pings.contains_key("a:1"));
    }
}
