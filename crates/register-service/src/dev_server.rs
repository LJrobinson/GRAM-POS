//! Opt-in loopback-only preview adapter, never a production network server.
use gram_register_service::RegisterService;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, io::Read, net::IpAddr, path::PathBuf};
use tiny_http::{Header, Method, Response, Server};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    command: String,
    payload: Value,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut db: Option<PathBuf> = None;
    let mut port: u16 = 4174;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                db = Some(
                    args.next()
                        .ok_or("--db requires an explicit local file path")?
                        .into(),
                )
            }
            "--port" => port = args.next().ok_or("--port requires an integer")?.parse()?,
            _ => {
                return Err(
                    "Usage: register-dev-server --db /path/to/demo.sqlite [--port 4174]".into(),
                )
            }
        }
    }
    let db = db.ok_or(
        "An explicit --db path is required; no real register file is selected implicitly.",
    )?;
    let mut service = RegisterService::open(db)?;
    let server = Server::http(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    eprintln!("GRAM development adapter listening on http://127.0.0.1:{port}. DEMO ONLY; no production server or payment processing.");
    for mut request in server.incoming_requests() {
        let header = |name: &'static str| {
            request
                .headers()
                .iter()
                .find(|h| h.field.equiv(name))
                .map(|h| h.value.as_str().to_owned())
        };
        let host = header("Host").unwrap_or_default();
        let allowed_host =
            host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}");
        let origin = header("Origin");
        let allowed_origin = origin
            .as_deref()
            .map(|origin| {
                matches!(
                    origin,
                    "http://localhost:5173"
                        | "http://127.0.0.1:5173"
                        | "http://localhost:4173"
                        | "http://127.0.0.1:4173"
                ) || origin == format!("http://127.0.0.1:{port}")
                    || origin == format!("http://localhost:{port}")
            })
            .unwrap_or(true);
        let loopback = request
            .remote_addr()
            .map(|a| {
                matches!(a.ip(),IpAddr::V4(ip) if ip.is_loopback())
                    || matches!(a.ip(),IpAddr::V6(ip) if ip.is_loopback())
            })
            .unwrap_or(false);
        if !allowed_host || !allowed_origin || !loopback {
            respond(
                request,
                403,
                json!({"ok":false,"error":{"code":"FORBIDDEN_ORIGIN","message":"Only the loopback development frontend may call this adapter."}}),
            );
            continue;
        }
        if request.method() == &Method::Get && request.url() == "/health" {
            respond(
                request,
                200,
                json!({"ok":true,"demo":true,"adapter":"loopback-development-only"}),
            );
            continue;
        }
        if request.method() != &Method::Post || request.url() != "/api/command" {
            respond(
                request,
                405,
                json!({"ok":false,"error":{"code":"METHOD_NOT_ALLOWED","message":"Use POST /api/command with JSON."}}),
            );
            continue;
        }
        if !header("Content-Type")
            .map(|v| v.split(';').next().unwrap_or("").trim() == "application/json")
            .unwrap_or(false)
        {
            respond(
                request,
                415,
                json!({"ok":false,"error":{"code":"JSON_REQUIRED","message":"Content-Type must be application/json."}}),
            );
            continue;
        }
        let mut bytes = Vec::new();
        if request
            .as_reader()
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 1_048_576
        {
            respond(
                request,
                413,
                json!({"ok":false,"error":{"code":"INVALID_INPUT","message":"Command body is unreadable or exceeds 1 MiB."}}),
            );
            continue;
        }
        match serde_json::from_slice::<Command>(&bytes) {
            Ok(command) => match service.handle(&command.command, command.payload) {
                Ok(data) => respond(request, 200, json!({"ok":true,"data":data})),
                Err(error) => respond(request, 400, json!({"ok":false,"error":error})),
            },
            Err(_) => respond(
                request,
                400,
                json!({"ok":false,"error":{"code":"INVALID_INPUT","message":"Expected a JSON command and payload."}}),
            ),
        }
    }
    Ok(())
}
fn respond(request: tiny_http::Request, status: u16, value: Value) {
    let response = Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8")
                .expect("literal header"),
        )
        .with_header(Header::from_bytes("Cache-Control", "no-store").expect("literal header"))
        .with_header(
            Header::from_bytes("X-Content-Type-Options", "nosniff").expect("literal header"),
        );
    let _ = request.respond(response);
}
