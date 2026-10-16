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
        ("POST", "/api/targets/add") => add_target(app, req.body.trim()),
        ("POST", "/api/targets/remove") => remove_target(app, req.body.trim()),
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
    let content = valid.then(|| std::fs::read_to_string(Path::new(&cfg.report_dir).join(name)));
    match content {
        Some(Ok(content)) => Response::json(200, content),
        _ => Response::json(404, r#"{"error":"rapport introuvable"}"#),
    }
}

fn add_target(app: &App, target: &str) -> Response {
    if !valid_target(target) {
        return Response::json(400, r#"{"error":"format attendu : adresse:port"}"#);
    }
    if !app.add_target(target) {
        return Response::json(400, r#"{"error":"cible déjà surveillée"}"#);
    }
    info!("cible ajoutée : {target}");
    Response::json(200, json!({ "targets": app.targets() }).to_string())
}

fn remove_target(app: &App, target: &str) -> Response {
    if !app.remove_target(target) {
        return Response::json(404, r#"{"error":"cible inconnue"}"#);
    }
    info!("cible supprimée : {target}");
    Response::json(200, json!({ "targets": app.targets() }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn setup(dir: &str) -> (App, Config) {
        let text = format!(
            "[ping]\ntargets = [\"a:1\"]\n[bandwidth]\nurl = \"http://h/x\"\n[report]\noutput_dir = \"{dir}\"\n"
        );
        (App::new(vec!["a:1".into()]), config::parse(&text).unwrap())
    }

    fn call(app: &App, cfg: &Config, method: &str, path: &str, body: &str) -> Response {
        let req = Request { method: method.into(), path: path.into(), body: body.into() };
        route(app, cfg, &req)
    }

    #[test]
    fn add_and_remove_targets() {
        let (app, cfg) = setup("/tmp");
        assert_eq!(call(&app, &cfg, "POST", "/api/targets/add", "b:2").code, 200);
        assert_eq!(call(&app, &cfg, "POST", "/api/targets/add", "b:2").code, 400);
        assert_eq!(call(&app, &cfg, "POST", "/api/targets/add", "sans-port").code, 400);
        assert_eq!(app.targets(), ["b:2", "a:1"]);
        assert_eq!(call(&app, &cfg, "POST", "/api/targets/remove", "a:1").code, 200);
        assert_eq!(call(&app, &cfg, "POST", "/api/targets/remove", "a:1").code, 404);
    }

    #[test]
    fn report_files_stay_in_their_directory() {
        let dir = std::env::temp_dir().join(format!("netmon-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("report_1.json"), "{\"ok\":1}").unwrap();
        let (app, cfg) = setup(dir.to_str().unwrap());

        assert_eq!(call(&app, &cfg, "GET", "/api/reports/report_1.json", "").body, "{\"ok\":1}");
        assert_eq!(call(&app, &cfg, "GET", "/api/reports/report_../../etc/passwd.json", "").code, 404);
        assert_eq!(call(&app, &cfg, "GET", "/api/reports/autre.txt", "").code, 404);
        assert_eq!(call(&app, &cfg, "GET", "/api/reports", "").body, "[\"report_1.json\"]");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn report_request_and_unknown_routes() {
        let (app, cfg) = setup("/tmp");
        assert_eq!(call(&app, &cfg, "POST", "/api/report", "").code, 202);
        assert_eq!(app.control.wait(std::time::Duration::ZERO, true), crate::control::Wake::Report);
        assert_eq!(call(&app, &cfg, "GET", "/inexistant", "").code, 404);
        assert_eq!(call(&app, &cfg, "DELETE", "/api/status", "").code, 404);
    }

    #[test]
    fn status_and_health_are_json() {
        let (app, cfg) = setup("/tmp");
        let status: serde_json::Value = serde_json::from_str(&call(&app, &cfg, "GET", "/api/status", "").body).unwrap();
        assert_eq!(status["targets"], json!(["a:1"]));
        let health: serde_json::Value = serde_json::from_str(&call(&app, &cfg, "GET", "/api/health", "").body).unwrap();
        assert_eq!(health["version"], env!("CARGO_PKG_VERSION"));
    }
}
