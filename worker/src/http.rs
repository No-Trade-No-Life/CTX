//! HTTP 层：与 axum 版逐条对齐的公开面（第一阶段）+ 媒体 + 接口性能指标。
use serde_json::json;
use worker::*;

use crate::db::{jstr, Db};

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Forbidden(String),
    NotFound,
    Unauthorized(String),
    Unavailable(String),
    Internal(String),
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            Self::BadRequest(m) => format!("bad request: {m}"),
            Self::Forbidden(m) => format!("forbidden: {m}"),
            Self::NotFound => "not found".to_owned(),
            Self::Unauthorized(m) => format!("unauthorized: {m}"),
            Self::Unavailable(m) => format!("unavailable: {m}"),
            Self::Internal(m) => format!("database error: {m}"),
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::Forbidden(_) => 403,
            Self::NotFound => 404,
            Self::Unauthorized(_) => 401,
            Self::Unavailable(_) => 503,
            Self::Internal(_) => 502,
        }
    }
}

impl From<worker::Error> for ApiError {
    fn from(error: worker::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

type ApiResult = std::result::Result<Response, ApiError>;

fn error_response(error: ApiError) -> Response {
    let body = json!({ "error": error.message() }).to_string();
    Response::from_json(&json!({ "error": error.message() }))
        .or_else(|_| Response::ok(body))
        .unwrap_or_else(|_| Response::empty().unwrap())
        .with_status(error.status())
}

pub fn json<T: serde::Serialize>(value: &T) -> ApiResult {
    Ok(Response::from_json(value)?)
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.to_string())
}

pub fn normalize_route(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut out = String::new();
    for (index, segment) in segments.iter().enumerate() {
        out.push('/');
        let dynamic = segment.len() >= 16
            && segment.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if dynamic {
            out.push(':');
            out.push_str(segments[index.saturating_sub(1)].trim_end_matches('s'));
            out.push_str("_id");
        } else {
            out.push_str(segment);
        }
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

fn is_media_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn image_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub async fn route(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path().to_owned();
    let method = req.method().to_string();

    let result = dispatch(&req, &env, &path, &method, &url).await;
    let response = match result {
        Ok(response) => response,
        Err(error) => error_response(error),
    };
    Ok(response)
}

async fn dispatch(req: &Request, env: &Env, path: &str, method: &str, url: &Url) -> ApiResult {
    let db = Db::new(env)?;
    if path == "/api/health" {
        return json(&json!({ "status": "ok", "service": "ctx" }));
    }
    if let Some(media_id) = path.strip_prefix("/media/") {
        return media(env, media_id).await;
    }
    if path == "/api/public/documents" && method == "GET" {
        let language = query_param(url, "language");
        return json(&db.public_documents(language.as_deref()).await?);
    }
    if let Some(rest) = path.strip_prefix("/api/public/documents/") {
        if let Some(document_id) = rest.strip_suffix("/comments") {
            let language = query_param(url, "language")
                .ok_or_else(|| ApiError::BadRequest("language is required".to_owned()))?;
            return json(&db.public_comments(document_id, &language).await?);
        }
        if !rest.contains('/') {
            let language = query_param(url, "language");
            let document = match db.read_document(rest, language.as_deref(), None).await? {
                None => return Err(ApiError::NotFound),
                Some(Err(())) => return Err(ApiError::Forbidden("document is private".to_owned())),
                Some(Ok(document)) => document,
            };
            db.record_view(rest).await?;
            return json(&document);
        }
    }
    if let Some(rest) = path.strip_prefix("/api/public/users/") {
        if let Some(owner_id) = rest.strip_suffix("/profile") {
            let language = query_param(url, "language");
            return json(&db.public_user_profile(owner_id, language.as_deref()).await?);
        }
    }
    if path == "/api/v1/admin/performance" {
        let provided = req
            .headers()
            .get("authorization")
            .ok()
            .flatten()
            .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned));
        let expected = env.secret("ADMIN_TOKEN").map(|s| s.to_string()).unwrap_or_default();
        if expected.is_empty() || provided.as_deref() != Some(expected.as_str()) {
            return Err(ApiError::Unauthorized("invalid or expired bearer token".to_owned()));
        }
        return json(&db.route_performance().await?);
    }
    Err(ApiError::NotFound)
}

async fn media(env: &Env, media_id: &str) -> ApiResult {
    if !is_media_id(media_id) {
        return Err(ApiError::NotFound);
    }
    let bucket = env.bucket("MEDIA")?;
    let Some(object) = bucket.get(media_id).execute().await? else {
        return Err(ApiError::NotFound);
    };
    let Some(body) = object.body() else {
        return Err(ApiError::NotFound);
    };
    let bytes = body.bytes().await?;
    let content_type = image_content_type(&bytes).ok_or(ApiError::NotFound)?;
    let mut headers = Headers::new();
    headers.set("content-type", content_type)?;
    headers.set("cache-control", "public, max-age=31536000, immutable")?;
    headers.set("x-content-type-options", "nosniff")?;
    let mut response = Response::from_bytes(bytes)?.with_headers(headers);
    response = response.with_status(200);
    Ok(response)
}

pub async fn assets(req: Request, env: &Env) -> Result<Response> {
    match env.assets("ASSETS") {
        Ok(assets) => assets.fetch_request(req).await,
        Err(_) => Response::error("not found", 404),
    }
}

pub fn media_prefix() -> &'static str {
    "media"
}

pub fn empty_ok() -> ApiResult {
    Ok(Response::empty()?.with_status(204))
}

pub fn no_content() -> Response {
    Response::empty().unwrap_or_else(|_| Response::ok("").unwrap())
}

pub fn _unused(value: &str) -> String {
    jstr(value).as_string().unwrap_or_default()
}
