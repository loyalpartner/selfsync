use std::io::Cursor;

use tiny_http::{Header, Response, Server};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::get_mapping;

const EMAIL_HEADER: &str = "X-Sync-User-Email";

pub fn start(_upstream_base: &str) -> Result<(Server, u16), Box<dyn std::error::Error>> {
    let server = Server::http("127.0.0.1:0").map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or("failed to get server address")?
        .port();
    info!(port, "proxy listening");
    Ok((server, port))
}

pub fn run(server: Server, upstream_base: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    for mut request in server.incoming_requests() {
        let result = handle_request(&client, &mut request, upstream_base);
        match result {
            Ok(response) => {
                if let Err(e) = request.respond(response) {
                    error!("respond error: {e}");
                }
            }
            Err(e) => {
                error!("request error: {e}");
                let resp = Response::from_string(format!("proxy error: {e}")).with_status_code(502);
                let _ = request.respond(resp);
            }
        }
    }

    Ok(())
}

fn handle_request(
    client: &reqwest::blocking::Client,
    request: &mut tiny_http::Request,
    upstream_base: &str,
) -> Result<Response<Cursor<Vec<u8>>>, Box<dyn std::error::Error>> {
    let request_url = format!("http://localhost{}", request.url());
    let parsed = Url::parse(&request_url)?;
    let client_id = parsed
        .query_pairs()
        .find(|(k, _)| k == "client_id")
        .map(|(_, v)| v.to_string());

    let email = client_id
        .as_deref()
        .and_then(|id| get_mapping().and_then(|m| m.lookup(id)));

    match &email {
        Some(email) => info!(
            email,
            client_id = client_id.as_deref().unwrap_or("?"),
            "sync request"
        ),
        None => warn!(client_id = ?client_id, "sync request from unknown user"),
    }

    let upstream_url = build_upstream_url(upstream_base, request.url())?;
    debug!(upstream_url, "forwarding request");

    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body)?;

    let mut upstream_req = client.post(&upstream_url);

    for header in request.headers() {
        let name = header.field.as_str().as_str();
        let value = header.value.as_str();
        if matches!(
            name.to_lowercase().as_str(),
            "host" | "connection" | "transfer-encoding" | "content-length"
        ) {
            continue;
        }
        upstream_req = upstream_req.header(name, value);
    }

    if let Some(email) = email {
        upstream_req = upstream_req.header(EMAIL_HEADER, email);
    }

    let upstream_resp = upstream_req.body(body).send()?;

    let status = upstream_resp.status().as_u16();
    debug!(status, "upstream response");

    let resp_headers: Vec<Header> = upstream_resp
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            let name_str = name.as_str();
            if matches!(
                name_str.to_lowercase().as_str(),
                "transfer-encoding" | "connection"
            ) {
                return None;
            }
            let value_str = value.to_str().ok()?;
            Header::from_bytes(name_str.as_bytes(), value_str.as_bytes()).ok()
        })
        .collect();

    let resp_body = upstream_resp.bytes()?.to_vec();
    let mut response = Response::from_data(resp_body).with_status_code(status);
    for header in resp_headers {
        response.add_header(header);
    }

    Ok(response)
}

fn build_upstream_url(
    upstream_base: &str,
    local_path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let base = Url::parse(upstream_base)?;
    let stripped = local_path
        .strip_prefix("/chrome-sync")
        .unwrap_or(local_path);
    let base_str = base.as_str().trim_end_matches('/');
    Ok(format!("{base_str}{stripped}"))
}
