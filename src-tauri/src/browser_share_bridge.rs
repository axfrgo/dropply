use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::app_state::AppState;
use crate::models::ImportConversationBundlePayload;
use crate::storage::sandbox::ShareBundleOrigin;

const BROWSER_SHARE_HOST: &str = "127.0.0.1";
const BROWSER_SHARE_PORT: u16 = 45123;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;

pub fn start(state: Arc<AppState>) {
    thread::spawn(move || {
        if let Err(error) = run(state) {
            eprintln!("Dropply browser share bridge failed: {error}");
        }
    });
}

fn run(state: Arc<AppState>) -> std::io::Result<()> {
    let listener = TcpListener::bind((BROWSER_SHARE_HOST, BROWSER_SHARE_PORT))?;

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let state = state.clone();
        thread::spawn(move || {
            let _ = handle_connection(stream, state);
        });
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            return write_json_response(
                &mut stream,
                400,
                None,
                &BridgeResponse::<serde_json::Value> {
                    ok: false,
                    error: Some(error),
                    data: None,
                },
            );
        }
    };

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/browser-share/health") => write_json_response(
            &mut stream,
            200,
            request.origin.as_deref(),
            &BridgeResponse {
                ok: true,
                error: None,
                data: Some(serde_json::json!({
                    "bridge": "dropply-browser-share",
                    "port": BROWSER_SHARE_PORT,
                    "status": "ready"
                })),
            },
        ),
        ("OPTIONS", "/v1/browser-share/bundle") => {
            if !request.origin.as_deref().map(is_allowed_browser_origin).unwrap_or(false) {
                return write_json_response(
                    &mut stream,
                    403,
                    request.origin.as_deref(),
                    &BridgeResponse::<serde_json::Value> {
                        ok: false,
                        error: Some("Only trusted browser-extension origins may use the Dropply browser bridge.".into()),
                        data: None,
                    },
                );
            }

            write_preflight_response(&mut stream, request.origin.as_deref())
        }
        ("POST", "/v1/browser-share/bundle") => {
            if !request.origin.as_deref().map(is_allowed_browser_origin).unwrap_or(false) {
                return write_json_response(
                    &mut stream,
                    403,
                    request.origin.as_deref(),
                    &BridgeResponse::<serde_json::Value> {
                        ok: false,
                        error: Some("Only trusted browser-extension origins may use the Dropply browser bridge.".into()),
                        data: None,
                    },
                );
            }

            let payload: ImportConversationBundlePayload = match serde_json::from_slice(&request.body) {
                Ok(payload) => payload,
                Err(error) => {
                    return write_json_response(
                        &mut stream,
                        400,
                        request.origin.as_deref(),
                        &BridgeResponse::<serde_json::Value> {
                            ok: false,
                            error: Some(format!("Dropply could not parse the browser bundle payload: {error}")),
                            data: None,
                        },
                    );
                }
            };

            let result = tauri::async_runtime::block_on(async {
                let item = state
                    .storage
                    .import_shared_conversation_bundle(ShareBundleOrigin::BrowserShare, payload)
                    .await?;
                state
                    .sync
                    .note_local_change(state.storage.clone())
                    .await?;
                Ok::<_, crate::AppError>(item)
            });

            match result {
                Ok(item) => write_json_response(
                    &mut stream,
                    200,
                    request.origin.as_deref(),
                    &BridgeResponse {
                        ok: true,
                        error: None,
                        data: Some(serde_json::json!({
                            "item_id": item.id,
                            "item_name": item.name,
                        })),
                    },
                ),
                Err(error) => write_json_response(
                    &mut stream,
                    400,
                    request.origin.as_deref(),
                    &BridgeResponse::<serde_json::Value> {
                        ok: false,
                        error: Some(error.to_string()),
                        data: None,
                    },
                ),
            }
        }
        _ => write_json_response(
            &mut stream,
            404,
            request.origin.as_deref(),
            &BridgeResponse::<serde_json::Value> {
                ok: false,
                error: Some("Dropply browser bridge endpoint was not found.".into()),
                data: None,
            },
        ),
    }
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let headers_end;

    loop {
        let mut chunk = [0_u8; 4096];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("Dropply could not read the browser request: {error}"))?;
        if read == 0 {
            return Err("Dropply received an empty browser-share request.".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            return Err("Dropply browser-share request headers were too large.".into());
        }

        if let Some(position) = find_header_terminator(&buffer) {
            headers_end = position + 4;
            break;
        }
    }

    let header_bytes = &buffer[..headers_end];
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|_| "Dropply browser-share request headers were not valid UTF-8.".to_string())?;
    let mut lines = header_text.split("\r\n").filter(|line| !line.is_empty());
    let request_line = lines
        .next()
        .ok_or_else(|| "Dropply browser-share request line was missing.".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| "Dropply browser-share request method was missing.".to_string())?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| "Dropply browser-share request path was missing.".to_string())?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .map(|value| {
            value.parse::<usize>().map_err(|_| {
                "Dropply browser-share request had an invalid Content-Length header.".to_string()
            })
        })
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BODY_BYTES {
        return Err(format!(
            "Dropply browser-share request exceeds the {} MB local bridge limit.",
            MAX_REQUEST_BODY_BYTES / (1024 * 1024)
        ));
    }

    while buffer.len() < headers_end + content_length {
        let mut chunk = [0_u8; 8192];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("Dropply could not finish reading the browser request body: {error}"))?;
        if read == 0 {
            return Err("Dropply browser-share request ended before the body finished.".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > headers_end + MAX_REQUEST_BODY_BYTES {
            return Err(format!(
                "Dropply browser-share request exceeds the {} MB local bridge limit.",
                MAX_REQUEST_BODY_BYTES / (1024 * 1024)
            ));
        }
    }

    Ok(HttpRequest {
        method,
        path,
        origin: headers.get("origin").cloned(),
        body: buffer[headers_end..headers_end + content_length].to_vec(),
    })
}

fn write_preflight_response(stream: &mut TcpStream, origin: Option<&str>) -> std::io::Result<()> {
    let origin = origin.unwrap_or("null");
    let response = format!(
        "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Max-Age: 600\r\nVary: Origin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(response.as_bytes())
}

fn write_json_response<T: Serialize>(
    stream: &mut TcpStream,
    status_code: u16,
    origin: Option<&str>,
    payload: &BridgeResponse<T>,
) -> std::io::Result<()> {
    let status_text = match status_code {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"serialization failed\"}".to_vec());
    let mut response = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(origin) = origin.filter(|value| is_allowed_browser_origin(value)) {
        response.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\n"));
    }
    response.push_str("\r\n");

    stream.write_all(response.as_bytes())?;
    stream.write_all(&body)
}

fn is_allowed_browser_origin(origin: &str) -> bool {
    origin.starts_with("chrome-extension://")
        || origin.starts_with("moz-extension://")
        || origin.starts_with("edge-extension://")
        || origin.starts_with("safari-web-extension://")
}

fn find_header_terminator(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    origin: Option<String>,
    body: Vec<u8>,
}

#[derive(Serialize)]
struct BridgeResponse<T> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}
