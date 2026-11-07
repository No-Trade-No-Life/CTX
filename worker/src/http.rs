//! HTTP 层：与 axum 版逐条对齐的公开面（第一阶段）+ 媒体 + 接口性能指标。
use serde_json::json;
use worker::*;

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::{self, hash_secret, Principal};
use crate::db::{document_from_row, jstr, Db, DocRow};
use crate::models::{DocumentMove, DocumentSave, NewDocument, NewDocumentComment};

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

pub async fn route(mut req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path().to_owned();
    let method = req.method().to_string();

    let result = dispatch(&mut req, &env, &path, &method, &url).await;
    let response = match result {
        Ok(response) => response,
        Err(error) => error_response(error),
    };
    Ok(response)
}

async fn dispatch(req: &mut Request, env: &Env, path: &str, method: &str, url: &Url) -> ApiResult {
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
    if let Some(rest) = path.strip_prefix("/api/v1/") {
        return private(req, env, &db, rest, method, url).await;
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

#[derive(Deserialize)]
struct DocumentInput {
    title: String,
    source_language: Option<String>,
    content: Option<String>,
    parent_id: Option<String>,
}

#[derive(Deserialize)]
struct DocumentUpdateInput {
    title: String,
    source_language: Option<String>,
    content: String,
    message: Option<String>,
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct MoveInput {
    parent_id: Option<String>,
    after_id: Option<String>,
}

#[derive(Deserialize)]
struct PublishInput {
    visibility: String,
}

#[derive(Deserialize)]
struct PublicationTimeInput {
    published_at: i64,
}

#[derive(Deserialize)]
struct CommentAnchorInput {
    quote: String,
    prefix: String,
    suffix: String,
}

#[derive(Deserialize)]
struct CommentInput {
    language: String,
    content: String,
    anchor: Option<CommentAnchorInput>,
}

#[derive(Deserialize)]
struct ApiKeyInput {
    label: String,
}

fn language_or_und(value: Option<&str>) -> String {
    value.unwrap_or("und").to_owned()
}

fn publication_visibility(value: &str) -> std::result::Result<&'static str, ApiError> {
    match value {
        "public" => Ok("public"),
        "private" => Ok("private"),
        _ => Err(ApiError::BadRequest("visibility must be public or private".to_owned())),
    }
}

async fn owned(db: &Db, principal: &Principal, document_id: &str) -> std::result::Result<(), ApiError> {
    match db.get_document(document_id).await? {
        Some(detail) if detail.document.owner_id == principal.user_id => Ok(()),
        _ => Err(ApiError::NotFound),
    }
}

async fn private(req: &mut Request, env: &Env, db: &Db, rest: &str, method: &str, url: &Url) -> ApiResult {
    let issuer = env.var("AUTH_ISSUER").map(|v| v.to_string()).unwrap_or_else(|_| "https://auth.ntnl.io".to_owned());
    let audience = env.var("AUTH_AUDIENCE").map(|v| v.to_string()).unwrap_or_else(|_| "ctx.ntnl.io".to_owned());
    let principal = auth::principal(req, db, &issuer, &audience).await?;

    if rest == "me" && method == "GET" {
        let root: Option<String> = db.root_user_id().await?;
        return json(&json!({
            "user_id": principal.user_id,
            "is_root": root.as_deref() == Some(principal.user_id.as_str()),
            "setup_required": root.is_none(),
        }));
    }
    if rest == "setup" && method == "POST" {
        let root: Option<String> = db.root_user_id().await?;
        if let Some(root) = root {
            if root != principal.user_id {
                return Err(ApiError::Forbidden("root user has already been initialized".to_owned()));
            }
        } else {
            db.initialize_root_user(&principal.user_id).await?;
        }
        return json(&json!({ "user_id": principal.user_id, "is_root": true, "setup_required": false }));
    }
    if rest == "api-keys" {
        if method == "GET" {
            return json(&db.list_api_keys(&principal.user_id).await?);
        }
        if method == "POST" {
            let input: ApiKeyInput = req.json().await?;
            let secret = uuid::Uuid::new_v4().simple().to_string();
            let prefix: String = secret.chars().take(10).collect();
            let key = db.create_api_key(&principal.user_id, &input.label, &prefix, &hash_secret(&secret)).await?;
            let mut value = serde_json::to_value(&key).unwrap_or_else(|_| json!({}));
            if let Value::Object(map) = &mut value {
                map.insert("secret".to_owned(), Value::String(format!("ctx_{}_{}", principal.user_id, secret)));
            }
            let mut response = Response::from_json(&value)?;
            return Ok(response.with_status(201));
        }
    }
    if let Some(id) = rest.strip_prefix("api-keys/") {
        if method == "DELETE" {
            if db.revoke_api_key(&principal.user_id, id).await? {
                return empty_ok();
            }
            return Err(ApiError::NotFound);
        }
    }
    if rest == "documents" {
        if method == "GET" {
            return json(&db.list_documents(&principal.user_id).await?);
        }
        if method == "POST" {
            let input: DocumentInput = req.json().await?;
            let created = db
                .create_document(&NewDocument {
                    title: &input.title,
                    content: input.content.as_deref().unwrap_or_default(),
                    message: "Created document",
                    author_id: &principal.user_id,
                    source_language: &language_or_und(input.source_language.as_deref()),
                    parent_id: input.parent_id.as_deref(),
                })
                .await?;
            return match created {
                Some(detail) => Ok(Response::from_json(&detail)?.with_status(201)),
                None => Err(ApiError::NotFound),
            };
        }
    }
    if rest == "profile-document" && method == "POST" {
        let detail = db.profile_document(&principal.user_id).await?;
        return Ok(Response::from_json(&detail)?.with_status(201));
    }
    if rest == "media" && method == "POST" {
        let bytes = req.bytes().await?;
        let content_type = image_content_type(&bytes)
            .ok_or_else(|| ApiError::BadRequest("only PNG, JPEG, GIF, and WebP images are supported".to_owned()))?;
        let media_id = format!("{:x}", Sha256::digest(&bytes));
        store_media(env, &media_id, &bytes, content_type).await?;
        let base = env.var("MEDIA_BASE_URL").map(|v| v.to_string()).unwrap_or_else(|_| "https://docs.ntnl.io/media".to_owned());
        return Ok(Response::from_json(&json!({ "id": media_id, "url": format!("{base}/{media_id}") }))?.with_status(201));
    }
    if let Some(tail) = rest.strip_prefix("public-documents/") {
        if let Some(document_id) = tail.strip_suffix("/comments") {
            if method == "POST" {
                let input: CommentInput = req.json().await?;
                let anchor = input.anchor;
                let comment = db
                    .create_public_comment(&NewDocumentComment {
                        document_id,
                        author_id: &principal.user_id,
                        language: &input.language,
                        content: &input.content,
                        quote: anchor.as_ref().map(|a| a.quote.as_str()),
                        prefix: anchor.as_ref().map(|a| a.prefix.as_str()),
                        suffix: anchor.as_ref().map(|a| a.suffix.as_str()),
                    })
                    .await?;
                return match comment {
                    Some(comment) => Ok(Response::from_json(&comment)?.with_status(201)),
                    None => Err(ApiError::NotFound),
                };
            }
        }
    }
    if let Some(tail) = rest.strip_prefix("documents/") {
        let mut segments = tail.split('/');
        let document_id = segments.next().unwrap_or_default().to_owned();
        let action = segments.next();
        let extra = segments.next();
        let _ = &principal;
        match (action, method) {
            (None, "GET") => {
                owned(db, &principal, &document_id).await?;
                return match db.get_document(&document_id).await? {
                    Some(detail) => json(&detail),
                    None => Err(ApiError::NotFound),
                };
            }
            (None, "PUT") => {
                owned(db, &principal, &document_id).await?;
                let input: DocumentUpdateInput = req.json().await?;
                let metadata = input.metadata.unwrap_or_else(|| json!({}));
                let detail = db
                    .save_document(&DocumentSave {
                        id: &document_id,
                        title: &input.title,
                        content: &input.content,
                        message: input.message.as_deref().unwrap_or("Saved document"),
                        author_id: &principal.user_id,
                        source_language: &language_or_und(input.source_language.as_deref()),
                        metadata: &metadata,
                    })
                    .await?;
                return match detail {
                    Some(detail) => json(&detail),
                    None => Err(ApiError::NotFound),
                };
            }
            (None, "DELETE") => {
                owned(db, &principal, &document_id).await?;
                if db.delete_document(&document_id).await? {
                    return empty_ok();
                }
                return Err(ApiError::NotFound);
            }
            (Some("move"), "POST") => {
                owned(db, &principal, &document_id).await?;
                let input: MoveInput = req.json().await?;
                let outcome = db
                    .move_document(&DocumentMove { id: &document_id, parent_id: input.parent_id.as_deref(), after_id: input.after_id.as_deref() })
                    .await?;
                return match outcome {
                    crate::models::MoveDocumentOutcome::Moved(detail) => json(&detail),
                    crate::models::MoveDocumentOutcome::Missing => Err(ApiError::NotFound),
                    crate::models::MoveDocumentOutcome::InvalidTarget => Err(ApiError::BadRequest("invalid move target".to_owned())),
                };
            }
            (Some("publish"), "POST") => {
                owned(db, &principal, &document_id).await?;
                let input: PublishInput = req.json().await?;
                let visibility = publication_visibility(&input.visibility)?;
                return match db.publish_document(&document_id, visibility).await? {
                    Some(document) => json(&document),
                    None => Err(ApiError::NotFound),
                };
            }
            (Some("publication"), "GET") => {
                owned(db, &principal, &document_id).await?;
                return match db.publication_time(&document_id).await? {
                    Some(value) => json(&json!({ "published_at": value })),
                    None => Err(ApiError::NotFound),
                };
            }
            (Some("publication"), "PUT") => {
                owned(db, &principal, &document_id).await?;
                let input: PublicationTimeInput = req.json().await?;
                return match db.update_publication_time(&document_id, input.published_at).await? {
                    Some(value) => json(&json!({ "published_at": value })),
                    None => Err(ApiError::NotFound),
                };
            }
            (Some("readers"), "GET") => {
                owned(db, &principal, &document_id).await?;
                return json(&db.document_readers(&document_id).await?);
            }
            (Some("readers"), "PUT") => {
                owned(db, &principal, &document_id).await?;
                let user_id = extra.ok_or(ApiError::NotFound)?;
                db.grant_document_reader(&document_id, user_id).await?;
                return empty_ok();
            }
            (Some("readers"), "DELETE") => {
                owned(db, &principal, &document_id).await?;
                let user_id = extra.ok_or(ApiError::NotFound)?;
                db.revoke_document_reader(&document_id, user_id).await?;
                return empty_ok();
            }
            (Some("ai"), "POST") | (Some("profile-summaries"), "POST") => {
                owned(db, &principal, &document_id).await?;
                return Err(ApiError::Unavailable("AI 队列迁移中，尚未在此版本启用".to_owned()));
            }
            _ => {}
        }
    }
    if rest.starts_with("admin/") {
        return Err(ApiError::Unavailable("admin 接口迁移中，尚未在此版本启用".to_owned()));
    }
    Err(ApiError::NotFound)
}

async fn store_media(env: &Env, media_id: &str, bytes: &[u8], content_type: &str) -> std::result::Result<(), ApiError> {
    let bucket = env.bucket("MEDIA")?;
    let mut object = bucket.put(media_id, bytes.to_vec());
    object = object.http_metadata(HttpMetadata {
        content_type: Some(content_type.to_owned()),
        ..Default::default()
    });
    object.execute().await?;
    Ok(())
}

fn unused_doc_row(_row: DocRow) {}
