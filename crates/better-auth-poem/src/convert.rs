use std::collections::HashMap;

use better_auth_core::{AuthError, AuthRequest, AuthResponse, ErrorMessageResponse, HttpMethod};
use poem::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use poem::{Body, IntoResponse, Request, RequestBody, Response};

/// Convert a Poem [`Request`] head + body into a framework-agnostic [`AuthRequest`].
///
/// Mirrors the axum equivalent (`convert_axum_request`): bounded body read,
/// `Content-Length` short-circuit at 413, lossy header conversion (non-UTF8
/// header values are dropped — matches axum behaviour).
pub async fn poem_request_to_auth_request(
    req: &Request,
    body: &mut RequestBody,
    max_body_bytes: usize,
) -> Result<AuthRequest, AuthError> {
    let method = match *req.method() {
        Method::GET => HttpMethod::Get,
        Method::POST => HttpMethod::Post,
        Method::PUT => HttpMethod::Put,
        Method::DELETE => HttpMethod::Delete,
        Method::PATCH => HttpMethod::Patch,
        Method::OPTIONS => HttpMethod::Options,
        Method::HEAD => HttpMethod::Head,
        _ => {
            return Err(AuthError::InvalidRequest(
                "Unsupported HTTP method".to_string(),
            ));
        }
    };

    let mut headers = HashMap::new();
    for (name, value) in req.headers().iter() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(name.to_string(), value_str.to_string());
        }
    }

    let path = req.uri().path().to_string();

    let mut query = HashMap::new();
    if let Some(query_str) = req.uri().query() {
        for pair in query_str.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut split = pair.splitn(2, '=');
            let key = decode_query_component(split.next().unwrap_or(""));
            let value = decode_query_component(split.next().unwrap_or(""));
            query.insert(key, value);
        }
    }

    if let Some(len) = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        && len > max_body_bytes
    {
        return Err(AuthError::payload_too_large(format!(
            "Request body exceeds the {}-byte limit",
            max_body_bytes
        )));
    }

    let taken = body.take().map_err(|err| {
        tracing::warn!(error = %err, "Failed to take request body");
        AuthError::bad_request("Failed to read request body")
    })?;
    let body_bytes = match taken.into_bytes_limit(max_body_bytes).await {
        Ok(bytes) => {
            if bytes.is_empty() {
                None
            } else {
                Some(bytes.to_vec())
            }
        }
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            if msg.contains("limit") || msg.contains("too large") {
                return Err(AuthError::payload_too_large(format!(
                    "Request body exceeds the {}-byte limit",
                    max_body_bytes
                )));
            }
            tracing::warn!(error = %err, "Failed to read request body");
            return Err(AuthError::bad_request("Failed to read request body"));
        }
    };

    Ok(AuthRequest::from_parts(
        method, path, headers, body_bytes, query,
    ))
}

/// Convert an [`AuthResponse`] into a Poem [`Response`].
///
/// Headers with names or values that fail Poem's validation are silently
/// dropped (matches axum behaviour).
pub fn auth_response_to_poem(resp: AuthResponse) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = Response::builder().status(status);
    for (name, value) in resp.headers {
        if let (Ok(hname), Ok(hvalue)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            builder = builder.header(hname, hvalue);
        }
    }
    builder.body(Body::from_bytes(resp.body.into()))
}

/// Convert an [`AuthError`] into a Poem [`Response`].
///
/// 500-class errors are masked to `"Internal server error"`; other errors
/// surface their `Display` impl in a `{ "message": ... }` body — matches the
/// axum integration's behaviour.
pub fn auth_error_to_poem(err: AuthError) -> Response {
    let status_code =
        StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let message = if err.status_code() == 500 {
        "Internal server error".to_string()
    } else {
        err.to_string()
    };
    let body = ErrorMessageResponse { message };
    let json = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status_code)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from_bytes(json.into()))
        .into_response()
}

fn decode_query_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push(((h << 4) | l) as u8 as char);
                    i += 3;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}
