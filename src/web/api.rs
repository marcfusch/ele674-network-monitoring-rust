//! Routes de l'API.

use super::http::{Request, Response};
use crate::config::{valid_target, Config};
use crate::state::App;
use crate::{info, report};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::Ordering;

const PAGE: &str = include_str!("page.html");

pub fn route(app: &App, cfg: &Config, req: &Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => Response::html(PAGE),
        ("GET", "/api/status") => status(app),
        ("GET", "/api/health") => health(app),
        ("GET", "/api/reports") => report_list(cfg),
        ("GET", path) if path.starts_with("/api/reports/") => {
            report_file(cfg, &path["/api/reports/".len()..])
        }
        ("POST", "/api/report") => {
            app.control.request_report();
            Response::json(202, r#"{"ok":true}"#)
        }
        ("POST", "/api/targets") => set_targets(app, &req.body),
        _ => Response::json(404, r#"{"error":"introuvable"}"#),
    }
}

fn status(app: &App) -> Response {
    let mut s = app.read().summary();
    s["targets"] = json!(app.targets());
    s["uptime_s"] = app.started.elapsed().as_secs().into();
    s["reports_written"] = app.reports_written.load(Ordering::Relaxed).into();
    Response::json(200, s.to_string())
}

fn health(app: &App) -> Response {
    let h = json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uid": unsafe { libc::getuid() },
        "uptime_s": app.started.elapsed().as_secs(),
        "reports_written": app.reports_written.load(Ordering::Relaxed),
    });
    Response::json(200, h.to_string())
}

fn report_list(cfg: &Config) -> Response {
    let mut names = report::list(Path::new(&cfg.report_dir)).unwrap_or_default();
    names.sort();
    Response::json(200, json!(names).to_string())
}

fn report_file(cfg: &Config, name: &str) -> Response {
    // Pas de "/" : impossible de sortir du dossier des rapports
    let valid = name.starts_with("report_") && name.ends_with(".json") && !name.contains(['/', '\\']);
    match std::fs::read_to_string(Path::new(&cfg.report_dir).join(name)) {
        Ok(content) if valid => Response::json(200, content),
        _ => Response::json(404, r#"{"error":"rapport introuvable"}"#),
    }
}

fn set_targets(app: &App, body: &str) -> Response {
    match serde_json::from_str::<Vec<String>>(body) {
        Ok(targets) if !targets.is_empty() && targets.iter().all(|t| valid_target(t)) => {
            info!("cibles modifiées à distance : {targets:?}");
            app.set_targets(targets.clone());
            Response::json(200, json!({ "targets": targets }).to_string())
        }
        _ => Response::json(400, r#"{"error":"tableau JSON de cibles \"hote:port\" attendu"}"#),
    }
}
