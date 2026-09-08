use auth_mini_axum::{AuthMiniLayer, AuthMiniPrincipal};
use axum::{
    Json, Router,
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tower_http::trace::TraceLayer;

use crate::{
    ai::{self, AiTask},
    db::{
        AiConfiguration, AiRun, Database, DatabaseError, Document, DocumentDetail, DocumentSave,
        NewDocument, PublicDocumentDetail, PublicDocumentSummary,
    },
};

#[derive(Clone, Debug)]
struct AppState {
    database: Database,
}

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub fn router(database: Database, auth: AuthMiniLayer) -> Router {
    let state = AppState { database };
    let private = Router::new()
        .route("/me", get(me))
        .route("/setup", post(setup_root))
        .route("/documents", get(list_documents).post(create_document))
        .route(
            "/documents/{document_id}",
            get(get_document).put(save_document),
        )
        .route("/documents/{document_id}/publish", post(publish_document))
        .route("/documents/{document_id}/ai", post(run_ai_task))
        .route(
            "/admin/ai",
            get(get_ai_configuration).put(update_ai_configuration),
        )
        .route_layer(auth);
    Router::new()
        .route("/api/health", get(health))
        .route("/api/public/documents", get(list_public_documents))
        .route("/api/public/documents/{document_id}", get(public_document))
        .nest("/api/v1", private)
        .fallback(static_asset)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "ctx"}))
}

async fn me(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    Ok(Json(Me {
        user_id: principal.subject.clone(),
        is_root: root_user_id.as_deref() == Some(&principal.subject),
        setup_required: root_user_id.is_none(),
    }))
}

async fn setup_root(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    if let Some(root_user_id) = root_user_id {
        if root_user_id != principal.subject {
            return Err(ApiError::forbidden(
                "root user has already been initialized",
            ));
        }
    } else {
        state.database.initialize_root_user(&principal.subject)?;
    }
    Ok(Json(Me {
        user_id: principal.subject,
        is_root: true,
        setup_required: false,
    }))
}

async fn list_documents(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Vec<Document>>, ApiError> {
    Ok(Json(state.database.list_documents(&principal.subject)?))
}

async fn create_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<DocumentInput>,
) -> Result<(StatusCode, Json<DocumentDetail>), ApiError> {
    validate_document_input(&input)?;
    let document = state.database.create_document(&NewDocument {
        author_id: &principal.subject,
        title: input.title.trim(),
        content: input.content.as_deref().unwrap_or_default(),
        message: "Created document",
    })?;
    Ok((StatusCode::CREATED, Json(document)))
}

async fn get_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
) -> Result<Json<DocumentDetail>, ApiError> {
    Ok(Json(require_document_owner(
        &state.database,
        &principal,
        &document_id,
    )?))
}

async fn save_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
    Json(input): Json<DocumentUpdateInput>,
) -> Result<Json<DocumentDetail>, ApiError> {
    validate_document_update(&input)?;
    let existing = require_document_owner(&state.database, &principal, &document_id)?;
    let document = state
        .database
        .save_document(&DocumentSave {
            id: &document_id,
            author_id: &principal.subject,
            title: input.title.trim(),
            content: &input.content,
            message: input.message.as_deref().unwrap_or("Saved document"),
            metadata: input
                .metadata
                .as_ref()
                .unwrap_or(&existing.document.metadata),
        })?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(document))
}

async fn publish_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
) -> Result<Json<Document>, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    Ok(Json(
        state
            .database
            .publish_document(&document_id)?
            .ok_or_else(ApiError::not_found)?,
    ))
}

async fn run_ai_task(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
    Json(input): Json<AiTaskInput>,
) -> Result<Json<AiRun>, ApiError> {
    let document = require_document_owner(&state.database, &principal, &document_id)?;
    let credentials = state
        .database
        .ai_credentials()?
        .ok_or_else(|| ApiError::unavailable("AI is not configured by the root administrator"))?;
    if matches!(&input.task, AiTask::Translate)
        && input.target_language.as_deref().is_none_or(str::is_empty)
    {
        return Err(ApiError::bad_request(
            "target_language is required for translation",
        ));
    }
    let output = ai::run(
        &credentials,
        &document,
        input.task.clone(),
        input.target_language.as_deref(),
    )
    .await?;
    Ok(Json(state.database.record_ai_run(
        &document.document.id,
        &document.revision.id,
        input.task.as_str(),
        &output.output,
        output.proposed_content.as_deref(),
    )?))
}

async fn get_ai_configuration(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<AiConfiguration>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    Ok(Json(state.database.ai_configuration()?))
}

async fn update_ai_configuration(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<AiConfigurationInput>,
) -> Result<Json<AiConfiguration>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    if !input.base_url.starts_with("https://openai.ntnl.io") {
        return Err(ApiError::bad_request("AI base URL must use openai.ntnl.io"));
    }
    if input.model.trim().is_empty() {
        return Err(ApiError::bad_request("AI model is required"));
    }
    Ok(Json(state.database.update_ai_configuration(
        input.base_url.trim_end_matches('/'),
        input.model.trim(),
        input.api_key.as_deref(),
    )?))
}

async fn list_public_documents(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicDocumentSummary>>, ApiError> {
    Ok(Json(state.database.public_documents()?))
}

async fn public_document(
    State(state): State<AppState>,
    Path(document_id): Path<String>,
) -> Result<Json<PublicDocumentDetail>, ApiError> {
    Ok(Json(
        state
            .database
            .public_document(&document_id)?
            .ok_or_else(ApiError::not_found)?,
    ))
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let asset = WebAssets::get(path).or_else(|| WebAssets::get("index.html"));
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = mime_guess::from_path(if path.is_empty() { "index.html" } else { path })
        .first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(content_type.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .body(Body::from(asset.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Debug, Deserialize)]
struct DocumentInput {
    title: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DocumentUpdateInput {
    title: String,
    content: String,
    message: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct AiTaskInput {
    task: AiTask,
    target_language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AiConfigurationInput {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct Me {
    user_id: String,
    is_root: bool,
    setup_required: bool,
}

#[derive(Debug)]
struct Actor {
    is_root: bool,
}

impl Actor {
    fn from_principal(
        database: &Database,
        principal: &AuthMiniPrincipal,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            is_root: database.root_user_id()?.as_deref() == Some(&principal.subject),
        })
    }

    fn assert_root(&self) -> Result<(), ApiError> {
        if self.is_root {
            Ok(())
        } else {
            Err(ApiError::forbidden("root user is required"))
        }
    }
}

fn require_document_owner(
    database: &Database,
    principal: &AuthMiniPrincipal,
    document_id: &str,
) -> Result<DocumentDetail, ApiError> {
    let document = database
        .get_document(document_id)?
        .ok_or_else(ApiError::not_found)?;
    if document.document.owner_id == principal.subject {
        Ok(document)
    } else {
        Err(ApiError::forbidden(
            "you do not have access to this document",
        ))
    }
}

fn validate_document_input(input: &DocumentInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::bad_request("document title is required"));
    }
    Ok(())
}

fn validate_document_update(input: &DocumentUpdateInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::bad_request("document title is required"));
    }
    Ok(())
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("database error")]
    Database(#[from] DatabaseError),
    #[error("AI request failed")]
    Ai(#[from] ai::AiError),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
    fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }
    fn not_found() -> Self {
        Self::NotFound
    }
    fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Database(_) | Self::Ai(_) => StatusCode::BAD_GATEWAY,
        };
        let message = self.to_string();
        (status, Json(json!({"error": message}))).into_response()
    }
}
