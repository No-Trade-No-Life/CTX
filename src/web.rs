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
        AiConfiguration, AiRun, Context, Database, DatabaseError, Document, DocumentDetail,
        DocumentSave, NewDocument,
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
        .route("/contexts", get(list_contexts).post(create_context))
        .route(
            "/contexts/{context_id}/documents",
            get(list_documents).post(create_document),
        )
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
        .route(
            "/api/public/contexts/{context_slug}/documents/{document_slug}",
            get(public_document),
        )
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

async fn list_contexts(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Vec<Context>>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    Ok(Json(state.database.list_contexts(actor.owner_filter())?))
}

async fn create_context(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<ContextInput>,
) -> Result<(StatusCode, Json<Context>), ApiError> {
    validate_context_input(&input)?;
    let context = state.database.create_context(
        &principal.subject,
        input.name.trim(),
        input.slug.trim(),
        input.description.trim(),
        input.instructions.trim(),
        &input.visibility,
    )?;
    Ok((StatusCode::CREATED, Json(context)))
}

async fn list_documents(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(context_id): Path<String>,
) -> Result<Json<Vec<Document>>, ApiError> {
    require_context_access(&state.database, &principal, &context_id)?;
    Ok(Json(state.database.list_documents(&context_id)?))
}

async fn create_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(context_id): Path<String>,
    Json(input): Json<DocumentInput>,
) -> Result<(StatusCode, Json<DocumentDetail>), ApiError> {
    require_context_access(&state.database, &principal, &context_id)?;
    validate_document_input(&input)?;
    let document = state.database.create_document(&NewDocument {
        context_id: &context_id,
        author_id: &principal.subject,
        title: input.title.trim(),
        slug: input.slug.trim(),
        language: input.language.trim(),
        kind: &input.kind,
        content: &input.content,
        message: "Created in CTX",
    })?;
    Ok((StatusCode::CREATED, Json(document)))
}

async fn get_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
) -> Result<Json<DocumentDetail>, ApiError> {
    let document = state
        .database
        .get_document(&document_id)?
        .ok_or_else(ApiError::not_found)?;
    require_context_access(&state.database, &principal, &document.document.context_id)?;
    Ok(Json(document))
}

async fn save_document(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(document_id): Path<String>,
    Json(input): Json<DocumentUpdateInput>,
) -> Result<Json<DocumentDetail>, ApiError> {
    validate_document_update(&input)?;
    let existing = state
        .database
        .get_document(&document_id)?
        .ok_or_else(ApiError::not_found)?;
    require_context_access(&state.database, &principal, &existing.document.context_id)?;
    let document = state
        .database
        .save_document(&DocumentSave {
            id: &document_id,
            author_id: &principal.subject,
            title: input.title.trim(),
            slug: input.slug.trim(),
            content: &input.content,
            message: input.message.as_deref().unwrap_or("Saved in CTX"),
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
    let existing = state
        .database
        .get_document(&document_id)?
        .ok_or_else(ApiError::not_found)?;
    require_context_access(&state.database, &principal, &existing.document.context_id)?;
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
    let document = state
        .database
        .get_document(&document_id)?
        .ok_or_else(ApiError::not_found)?;
    let context =
        require_context_access(&state.database, &principal, &document.document.context_id)?;
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
        &context,
        &document,
        input.task.clone(),
        input.target_language.as_deref(),
    )
    .await?;
    Ok(Json(state.database.record_ai_run(
        &context.id,
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

async fn public_document(
    State(state): State<AppState>,
    Path((context_slug, document_slug)): Path<(String, String)>,
) -> Result<Json<DocumentDetail>, ApiError> {
    Ok(Json(
        state
            .database
            .public_document(&context_slug, &document_slug)?
            .ok_or_else(ApiError::not_found)?,
    ))
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
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
struct ContextInput {
    name: String,
    slug: String,
    description: String,
    instructions: String,
    visibility: String,
}

#[derive(Debug, Deserialize)]
struct DocumentInput {
    title: String,
    slug: String,
    language: String,
    kind: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct DocumentUpdateInput {
    title: String,
    slug: String,
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
    user_id: String,
    is_root: bool,
}

impl Actor {
    fn from_principal(
        database: &Database,
        principal: &AuthMiniPrincipal,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            user_id: principal.subject.clone(),
            is_root: database.root_user_id()?.as_deref() == Some(&principal.subject),
        })
    }

    fn owner_filter(&self) -> Option<&str> {
        (!self.is_root).then_some(self.user_id.as_str())
    }

    fn assert_root(&self) -> Result<(), ApiError> {
        if self.is_root {
            Ok(())
        } else {
            Err(ApiError::forbidden("root user is required"))
        }
    }

    fn assert_context_access(&self, context: &Context) -> Result<(), ApiError> {
        if self.is_root || self.user_id == context.owner_id {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "you do not have access to this Context",
            ))
        }
    }
}

fn require_context_access(
    database: &Database,
    principal: &AuthMiniPrincipal,
    context_id: &str,
) -> Result<Context, ApiError> {
    let actor = Actor::from_principal(database, principal)?;
    let context = database
        .get_context(context_id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_context_access(&context)?;
    Ok(context)
}

fn validate_context_input(input: &ContextInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Context name is required"));
    }
    validate_slug(&input.slug)?;
    if !matches!(input.visibility.as_str(), "private" | "public") {
        return Err(ApiError::bad_request(
            "visibility must be private or public",
        ));
    }
    Ok(())
}

fn validate_document_input(input: &DocumentInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::bad_request("document title is required"));
    }
    validate_slug(&input.slug)?;
    if input.language.trim().is_empty() {
        return Err(ApiError::bad_request("document language is required"));
    }
    if !matches!(input.kind.as_str(), "docs" | "blog") {
        return Err(ApiError::bad_request("document kind must be docs or blog"));
    }
    Ok(())
}

fn validate_document_update(input: &DocumentUpdateInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::bad_request("document title is required"));
    }
    validate_slug(&input.slug)
}

fn validate_slug(slug: &str) -> Result<(), ApiError> {
    let valid = !slug.is_empty()
        && slug.len() <= 80
        && slug.bytes().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'-'
        });
    if valid {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "slug must use lowercase letters, numbers, and hyphens",
        ))
    }
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
