//! HTTP/1.0 minimal : lecture d'une requête, écriture d'une réponse.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const MAX_REQUEST: u64 = 16 * 1024;
const MAX_BODY: usize = 4096;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

pub struct Response {
    pub code: u16,
    pub mime: &'static str,
    pub body: String,
}

impl Response {
    pub fn json(code: u16, body: impl Into<String>) -> Self {
        Self { code, mime: "application/json", body: body.into() }
    }

    pub fn html(body: &str) -> Self {
        Self { code: 200, mime: "text/html; charset=utf-8", body: body.into() }
    }
}

pub fn read_request(stream: &TcpStream) -> io::Result<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.take(MAX_REQUEST));

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default();
    let path = path.split('?').next().unwrap_or_default().to_string();

    let mut body_len = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            body_len = v.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0; body_len.min(MAX_BODY)];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, body: String::from_utf8_lossy(&body).into_owned() })
}

pub fn write_response(mut stream: &TcpStream, r: &Response) -> io::Result<()> {
    let reason = match r.code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.0 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        r.code,
        r.mime,
        r.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(r.body.as_bytes())
}
