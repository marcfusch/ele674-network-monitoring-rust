use crate::{config::WebConfig, state::MonitorState};
use std::sync::{Arc, RwLock};
use tiny_http::{Header, Response, Server};

const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Moniteur réseau</title>
<h1>Moniteur réseau</h1><pre id="s">chargement...</pre>
<script>
async function f(){const d=await (await fetch('/api/status')).json();let t='';
for(const[k,v]of Object.entries(d.pings)){const l=v.last&&v.last.value;
t+=(l!=null?'🟢 ':'🔴 ')+k+' : '+(l!=null?l.toFixed(1)+' ms':'injoignable')+' (perte '+v.stats.loss_percent.toFixed(0)+' %)\n';}
const b=d.bandwidth.last&&d.bandwidth.last.value;t+='\nDébit : '+(b!=null?b.toFixed(1)+' Mb/s':'n/a');
document.getElementById('s').textContent=t;}
f();setInterval(f,2000);
</script>"#;

pub fn run(state: Arc<RwLock<MonitorState>>, cfg: WebConfig) {
    let server = Server::http(&cfg.bind).expect("démarrage du serveur web");
    println!("Interface web : http://{}", cfg.bind);
    for req in server.incoming_requests() {
        let (body, mime) = match req.url() {
            "/api/status" => (state.read().unwrap().summary().to_string(), "application/json"),
            _ => (PAGE.to_string(), "text/html; charset=utf-8"),
        };
        let header = Header::from_bytes("Content-Type", mime).unwrap();
        let _ = req.respond(Response::from_string(body).with_header(header));
    }
}
