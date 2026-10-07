use std::{
    fs,
    path::{Path as FilePath, PathBuf},
    sync::{Arc, Mutex},
};

use auth_mini_axum::{AuthMiniLayer, AuthMiniVerifier};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Extension, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use reqwest::Url;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::{
    ai::{self, AiTask},
    db::{
        AiConfiguration, AiRequest, AiRun, ApiKey, CreateDocumentOutcome, Database, DatabaseError,
        Document, DocumentComment, DocumentDetail, DocumentMove, DocumentReader, DocumentSave,
        MoveDocumentOutcome, NewDocument, NewDocumentComment, PublicDocumentDetail,
        PublicDocumentSummary, PublicUserProfile, ReadDocumentOutcome,
    },
    language::normalize_language_tag,
    resources::{ResourceError, ResourceMonitor, SystemResourcesSnapshot},
    views::{ViewCounter, ViewEvent},
};

#[derive(Clone)]
struct AppState {
    database: Database,
    media_directory: Arc<PathBuf>,
    resources: Arc<Mutex<ResourceMonitor>>,
    views: Arc<ViewCounter>,
    auth_verifier: AuthMiniVerifier,
}

#[derive(Clone, Debug)]
struct Principal {
    user_id: String,
}

async fn request_auth(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    match request_principal(&state, request.headers()).await {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(error) => error.into_response(),
    }
}

async fn request_principal(state: &AppState, headers: &HeaderMap) -> Result<Principal, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ApiError::unauthorized("invalid or expired bearer token"))?;
    let Some(rest) = token.strip_prefix("ctx_") else {
        let principal = state
            .auth_verifier
            .verify(token)
            .await
            .map_err(|_| ApiError::unauthorized("invalid or expired bearer token"))?;
        return Ok(Principal {
            user_id: principal.subject,
        });
    };
    let Some((user_id, secret)) = rest
        .split_once('_')
        .filter(|(user_id, secret)| !user_id.is_empty() && !secret.is_empty())
    else {
        return Err(ApiError::unauthorized("invalid API key"));
    };
    state
        .database
        .authenticate_api_key(user_id, &hash_secret(secret))?
        .map(|_| Principal {
            user_id: user_id.to_owned(),
        })
        .ok_or_else(|| ApiError::unauthorized("invalid API key"))
}

fn hash_secret(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

const MAX_IMAGE_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
const PUBLIC_MEDIA_BASE_URL: &str = "https://ctx.ntnl.io/media";

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub fn router(database: Database, views: Arc<ViewCounter>, auth: &AuthMiniLayer) -> Router {
    let media_directory = Arc::new(database.media_directory());
    let state = AppState {
        resources: Arc::new(Mutex::new(ResourceMonitor::new(database.clone()))),
        database,
        media_directory,
        views,
        auth_verifier: auth.verifier(),
    };
    let private = Router::new()
        .route("/me", get(me))
        .route("/setup", post(setup_root))
        .route("/documents", get(list_documents).post(create_document))
        .route(
            "/media",
            post(upload_media).layer(DefaultBodyLimit::max(MAX_IMAGE_UPLOAD_BYTES)),
        )
        .route("/profile-document", post(profile_document))
        .route(
            "/documents/{document_id}",
            get(get_document).put(save_document).delete(delete_document),
        )
        .route("/documents/{document_id}/move", post(move_document))
        .route(
            "/documents/{document_id}/publication",
            get(publication_time).put(update_publication_time),
        )
        .route("/documents/{document_id}/publish", post(publish_document))
        .route(
            "/documents/{document_id}/readers",
            get(list_document_readers),
        )
        .route(
            "/documents/{document_id}/readers/{user_id}",
            put(grant_document_reader).delete(revoke_document_reader),
        )
        .route(
            "/documents/{document_id}/profile-summaries",
            post(trigger_profile_summaries),
        )
        .route(
            "/public-documents/{document_id}/comments",
            post(create_public_comment),
        )
        .route("/documents/{document_id}/ai", post(run_ai_task))
        .route(
            "/admin/ai",
            get(get_ai_configuration).put(update_ai_configuration),
        )
        .route("/admin/ai/test", post(test_ai_configuration))
        .route("/admin/ai/requests", get(list_ai_requests))
        .route("/admin/system-resources", get(system_resources))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/{id}", delete(delete_api_key))
        .route_layer(middleware::from_fn_with_state(state.clone(), request_auth));
    Router::new()
        .route("/api/health", get(health))
        .route("/media/{media_id}", get(public_media))
        .route("/api/public/documents", get(list_public_documents))
        .route("/api/public/documents/{document_id}", get(public_document))
        .route(
            "/api/public/documents/{document_id}/comments",
            get(public_comments),
        )
        .route(
            "/api/public/users/{owner_id}/profile",
            get(public_user_profile),
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
    Extension(principal): Extension<Principal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    Ok(Json(Me {
        user_id: principal.user_id.clone(),
        is_root: root_user_id.as_deref() == Some(&principal.user_id),
        setup_required: root_user_id.is_none(),
    }))
}

async fn setup_root(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    if let Some(root_user_id) = root_user_id {
        if root_user_id != principal.user_id {
            return Err(ApiError::forbidden(
                "root user has already been initialized",
            ));
        }
    } else {
        state.database.initialize_root_user(&principal.user_id)?;
    }
    Ok(Json(Me {
        user_id: principal.user_id,
        is_root: true,
        setup_required: false,
    }))
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyInput {
    label: String,
}

#[derive(Debug, Serialize)]
struct CreatedApiKey {
    #[serde(flatten)]
    key: ApiKey,
    secret: String,
}

async fn list_api_keys(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<ApiKey>>, ApiError> {
    Ok(Json(state.database.list_api_keys(&principal.user_id)?))
}

async fn create_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<CreateApiKeyInput>,
) -> Result<(StatusCode, Json<CreatedApiKey>), ApiError> {
    let label = input.label.trim();
    if label.is_empty() || label.chars().count() > 80 {
        return Err(ApiError::bad_request("label must be 1-80 characters"));
    }
    let secret = Uuid::new_v4().simple().to_string();
    let prefix = secret[..10].to_owned();
    let key =
        state
            .database
            .create_api_key(&principal.user_id, label, &prefix, &hash_secret(&secret))?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedApiKey {
            key,
            secret: format!("ctx_{}_{}", principal.user_id, secret),
        }),
    ))
}

async fn delete_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if state.database.revoke_api_key(&principal.user_id, &id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

async fn list_documents(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<Document>>, ApiError> {
    Ok(Json(state.database.list_documents(&principal.user_id)?))
}

async fn create_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<DocumentInput>,
) -> Result<(StatusCode, Json<DocumentDetail>), ApiError> {
    validate_document_input(&input)?;
    let source_language = source_language_or_und(input.source_language.as_deref())?;
    let outcome = state.database.create_document(&NewDocument {
        author_id: &principal.user_id,
        title: input.title.trim(),
        source_language: &source_language,
        content: input.content.as_deref().unwrap_or_default(),
        message: "Created document",
        parent_id: input.parent_id.as_deref(),
    })?;
    match outcome {
        CreateDocumentOutcome::Created(document) => Ok((StatusCode::CREATED, Json(*document))),
        CreateDocumentOutcome::MissingParent => Err(ApiError::not_found()),
        CreateDocumentOutcome::InvalidParent => Err(ApiError::bad_request(
            "parent document must be an article you own",
        )),
    }
}

#[derive(Debug, Serialize)]
struct MediaUpload {
    id: String,
    url: String,
}

async fn upload_media(
    State(state): State<AppState>,
    Extension(_principal): Extension<Principal>,
    content: Bytes,
) -> Result<(StatusCode, Json<MediaUpload>), ApiError> {
    image_content_type(&content).ok_or_else(|| {
        ApiError::bad_request("only PNG, JPEG, GIF, and WebP images are supported")
    })?;
    let media_id = store_media(&state.media_directory, &content)?;
    Ok((
        StatusCode::CREATED,
        Json(MediaUpload {
            url: format!("{PUBLIC_MEDIA_BASE_URL}/{media_id}"),
            id: media_id,
        }),
    ))
}

async fn profile_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<DocumentDetail>, ApiError> {
    Ok(Json(state.database.profile_document(&principal.user_id)?))
}

async fn get_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
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
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<DocumentUpdateInput>,
) -> Result<Json<DocumentDetail>, ApiError> {
    validate_document_update(&input)?;
    let existing = require_document_owner(&state.database, &principal, &document_id)?;
    let source_language = match input.source_language.as_deref() {
        Some(value) => source_language_or_und(Some(value))?,
        None => existing.document.source_language.clone(),
    };
    let document = state
        .database
        .save_document(&DocumentSave {
            id: &document_id,
            author_id: &principal.user_id,
            title: input.title.trim(),
            source_language: &source_language,
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

async fn move_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<DocumentMoveInput>,
) -> Result<Json<DocumentDetail>, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    let outcome = state.database.move_document(&DocumentMove {
        id: &document_id,
        parent_id: input.parent_id.as_deref(),
        after_id: input.after_id.as_deref(),
    })?;
    match outcome {
        MoveDocumentOutcome::Moved(document) => Ok(Json(*document)),
        MoveDocumentOutcome::Missing => Err(ApiError::not_found()),
        MoveDocumentOutcome::InvalidTarget => Err(ApiError::bad_request("invalid move target")),
    }
}

async fn delete_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    if state.database.delete_document(&document_id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

async fn publication_time(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
) -> Result<Json<PublicationTime>, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    let published_at = state.database.publication_time(&document_id)?;
    Ok(Json(PublicationTime { published_at }))
}

async fn update_publication_time(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<PublicationTimeInput>,
) -> Result<Json<PublicationTime>, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    validate_publication_time(input.published_at)?;
    let published_at = state
        .database
        .update_publication_time(&document_id, input.published_at)?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(PublicationTime {
        published_at: Some(published_at),
    }))
}

#[derive(Debug, Deserialize)]
struct PublishDocumentInput {
    visibility: String,
}

async fn publish_document(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<PublishDocumentInput>,
) -> Result<Json<Document>, ApiError> {
    let visibility = publication_visibility(&input.visibility)?;
    let document = require_document_owner(&state.database, &principal, &document_id)?;
    Ok(Json(
        state
            .database
            .publish_document(&document_id, &document.revision.id, visibility)?
            .ok_or_else(ApiError::conflict)?,
    ))
}

async fn list_document_readers(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
) -> Result<Json<Vec<DocumentReader>>, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    Ok(Json(state.database.document_readers(&document_id)?))
}

async fn grant_document_reader(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((document_id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err(ApiError::bad_request("reader user_id is required"));
    }
    state
        .database
        .grant_document_reader(&document_id, user_id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_document_reader(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((document_id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_document_owner(&state.database, &principal, &document_id)?;
    state
        .database
        .revoke_document_reader(&document_id, user_id.trim())?;
    Ok(StatusCode::NO_CONTENT)
}

async fn trigger_profile_summaries(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<ProfileSummaryTaskInput>,
) -> Result<(StatusCode, Json<ProfileSummaryTaskResponse>), ApiError> {
    let document = require_document_owner(&state.database, &principal, &document_id)?;
    if document.document.kind != "profile" || document.document.status != "published" {
        return Err(ApiError::bad_request(
            "profile summaries require a published profile document",
        ));
    }
    let source_revision_id = document
        .document
        .published_revision_id
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("profile document has no published revision"))?;
    let task = input
        .task
        .as_deref()
        .map(profile_summary_task_name)
        .transpose()?;
    let tasks = state.database.enqueue_profile_summary_tasks(
        &document_id,
        source_revision_id,
        task.as_deref(),
    )?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ProfileSummaryTaskResponse { tasks }),
    ))
}

async fn run_ai_task(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<AiTaskInput>,
) -> Result<Json<AiRun>, ApiError> {
    let document = require_document_owner(&state.database, &principal, &document_id)?;
    let credentials = state
        .database
        .ai_credentials()?
        .ok_or_else(|| ApiError::unavailable("AI is not configured by the root administrator"))?;
    let output = ai::run(&credentials, &document, input.task.clone()).await?;
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
    Extension(principal): Extension<Principal>,
) -> Result<Json<AiConfiguration>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    Ok(Json(state.database.ai_configuration()?))
}

async fn update_ai_configuration(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<AiConfigurationInput>,
) -> Result<Json<AiConfiguration>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    let base_url = validate_ai_base_url(&input.base_url)?;
    if input.model.trim().is_empty() {
        return Err(ApiError::bad_request("AI model is required"));
    }
    let configuration = state.database.update_ai_configuration(
        &base_url,
        input.model.trim(),
        input.api_key.as_deref(),
    )?;
    state.database.requeue_failed_ai_requests()?;
    Ok(Json(configuration))
}

async fn test_ai_configuration(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<AiConfigurationInput>,
) -> Result<Json<AiConfigurationTest>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    let base_url = validate_ai_base_url(&input.base_url)?;
    let model = input.model.trim();
    if model.is_empty() {
        return Err(ApiError::bad_request("AI model is required"));
    }
    let saved = state
        .database
        .ai_credentials()?
        .ok_or_else(|| ApiError::unavailable("AI API key is not configured"))?;
    let api_key = input
        .api_key
        .filter(|value| !value.is_empty())
        .unwrap_or(saved.api_key);
    let request_id = ai::test_configuration(&crate::db::AiCredentials {
        base_url,
        model: model.to_owned(),
        api_key,
    })
    .await?;
    Ok(Json(AiConfigurationTest { request_id }))
}

fn validate_ai_base_url(value: &str) -> Result<String, ApiError> {
    let url = Url::parse(value.trim())
        .map_err(|_| ApiError::bad_request("AI base URL must be a valid HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ApiError::bad_request(
            "AI base URL must be a valid HTTP(S) URL",
        ));
    }
    Ok(value.trim().trim_end_matches('/').to_owned())
}

async fn list_ai_requests(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<AiRequest>>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    Ok(Json(state.database.ai_requests()?))
}

async fn system_resources(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<SystemResourcesSnapshot>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    let mut monitor = state
        .resources
        .lock()
        .map_err(|_| ResourceError::Poisoned)?;
    Ok(Json(monitor.sample()?))
}

async fn list_public_documents(
    State(state): State<AppState>,
    Query(query): Query<PublicDocumentQuery>,
) -> Result<Json<Vec<PublicDocumentSummary>>, ApiError> {
    Ok(Json(state.database.public_documents(
        requested_language(query.language.as_deref())?.as_deref(),
    )?))
}

async fn public_document(
    State(state): State<AppState>,
    Path(document_id): Path<String>,
    Query(query): Query<PublicDocumentQuery>,
    headers: HeaderMap,
) -> Result<Json<PublicDocumentDetail>, ApiError> {
    let language = requested_language(query.language.as_deref())?;
    let viewer = optional_viewer(&state, &headers).await;
    let mut document =
        match state
            .database
            .read_document(&document_id, language.as_deref(), viewer.as_deref())?
        {
            ReadDocumentOutcome::Found(document) => *document,
            ReadDocumentOutcome::Private => {
                return Err(ApiError::forbidden("document is private"));
            }
            ReadDocumentOutcome::Missing => return Err(ApiError::not_found()),
        };
    if viewer.as_deref() != Some(document.owner_id.as_str()) {
        state.views.record(ViewEvent {
            document_id: document_id.as_str(),
            client: client_address(&headers),
            user_agent: user_agent(&headers),
        });
    }
    state.views.merge_pending(&document_id, &mut document.views);
    if document.is_translation_fallback && language.as_deref().is_some_and(is_reader_language) {
        document.translation_status = state
            .database
            .request_public_translation(&document_id, language.as_deref().unwrap_or_default())?
            .or(document.translation_status);
    }
    Ok(Json(document))
}

async fn optional_viewer(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let token = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())?;
    state
        .auth_verifier
        .verify(token)
        .await
        .ok()
        .map(|principal| principal.subject)
}

/// Best-effort client hint used only to deduplicate unique readers; the value
/// is hashed before it is remembered and never stored verbatim.
fn client_address(headers: &HeaderMap) -> Option<&str> {
    const FORWARDED_CLIENT_HEADERS: [&str; 3] =
        ["cf-connecting-ip", "x-forwarded-for", "x-real-ip"];
    for name in FORWARDED_CLIENT_HEADERS {
        let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        let Some(client) = value
            .split(',')
            .map(str::trim)
            .find(|client| !client.is_empty())
        else {
            continue;
        };
        return Some(client);
    }
    None
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
}

async fn public_comments(
    State(state): State<AppState>,
    Path(document_id): Path<String>,
    Query(query): Query<PublicDocumentQuery>,
) -> Result<Json<Vec<DocumentComment>>, ApiError> {
    let language = requested_language(query.language.as_deref())?
        .ok_or_else(|| ApiError::bad_request("language is required"))?;
    Ok(Json(
        state.database.public_comments(&document_id, &language)?,
    ))
}

async fn create_public_comment(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(document_id): Path<String>,
    Json(input): Json<DocumentCommentInput>,
) -> Result<(StatusCode, Json<DocumentComment>), ApiError> {
    let language = normalize_language_tag(&input.language)
        .ok_or_else(|| ApiError::bad_request("language must be a BCP 47 tag"))?;
    validate_comment_input(&input)?;
    let anchor = input.anchor.as_ref();
    let comment = state
        .database
        .create_public_comment(&NewDocumentComment {
            document_id: &document_id,
            author_id: &principal.user_id,
            language: &language,
            content: input.content.trim(),
            quote: anchor.map(|anchor| anchor.quote.trim()),
            prefix: anchor.map(|anchor| anchor.prefix.as_str()),
            suffix: anchor.map(|anchor| anchor.suffix.as_str()),
        })?
        .ok_or_else(ApiError::not_found)?;
    Ok((StatusCode::CREATED, Json(comment)))
}

async fn public_user_profile(
    State(state): State<AppState>,
    Path(owner_id): Path<String>,
    Query(query): Query<PublicDocumentQuery>,
) -> Result<Json<PublicUserProfile>, ApiError> {
    let language = requested_language(query.language.as_deref())?;
    let mut profile = state
        .database
        .public_user_profile(&owner_id, language.as_deref())?;
    if let Some(document) = profile.profile.as_mut()
        && document.is_translation_fallback
        && language.as_deref().is_some_and(is_reader_language)
    {
        document.translation_status = state
            .database
            .request_public_translation(&document.id, language.as_deref().unwrap_or_default())?
            .or(document.translation_status.clone());
    }
    Ok(Json(profile))
}

async fn public_media(
    State(state): State<AppState>,
    Path(media_id): Path<String>,
) -> Result<Response, ApiError> {
    if !is_media_id(&media_id) {
        return Err(ApiError::not_found());
    }
    let content = match fs::read(state.media_directory.join(media_id)) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::not_found());
        }
        Err(error) => return Err(ApiError::Media(error)),
    };
    let content_type = image_content_type(&content).ok_or_else(ApiError::not_found)?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(content))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()))
}

fn image_content_type(content: &[u8]) -> Option<&'static str> {
    if content.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if content.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if content.starts_with(b"GIF87a") || content.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if content.starts_with(b"RIFF") && content.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

fn store_media(directory: &FilePath, content: &[u8]) -> Result<String, std::io::Error> {
    fs::create_dir_all(directory)?;
    let media_id = format!("{:x}", Sha256::digest(content));
    let path = directory.join(&media_id);
    if !path.exists() {
        fs::write(path, content)?;
    }
    Ok(media_id)
}

fn is_media_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let asset = WebAssets::get(path);
    if asset.is_none() && FilePath::new(path).extension().is_some() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let served_shell = asset.is_none();
    let Some(asset) = asset.or_else(|| WebAssets::get("index.html")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = mime_guess::from_path(if path.is_empty() { "index.html" } else { path })
        .first_or_octet_stream();
    let cache_control = if !served_shell && path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(content_type.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(asset.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Debug, Deserialize)]
struct DocumentInput {
    title: String,
    source_language: Option<String>,
    content: Option<String>,
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DocumentMoveInput {
    parent_id: Option<String>,
    after_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DocumentUpdateInput {
    title: String,
    source_language: Option<String>,
    content: String,
    message: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PublicationTimeInput {
    published_at: i64,
}

#[derive(Debug, Serialize)]
struct PublicationTime {
    published_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DocumentCommentInput {
    language: String,
    content: String,
    anchor: Option<DocumentCommentAnchorInput>,
}

#[derive(Debug, Deserialize)]
struct DocumentCommentAnchorInput {
    quote: String,
    prefix: String,
    suffix: String,
}

#[derive(Debug, Deserialize)]
struct AiTaskInput {
    task: AiTask,
}

#[derive(Debug, Deserialize)]
struct ProfileSummaryTaskInput {
    task: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProfileSummaryTaskResponse {
    tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PublicDocumentQuery {
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AiConfigurationInput {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct AiConfigurationTest {
    request_id: Option<String>,
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
    fn from_principal(database: &Database, principal: &Principal) -> Result<Self, ApiError> {
        Ok(Self {
            is_root: database.root_user_id()?.as_deref() == Some(&principal.user_id),
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
    principal: &Principal,
    document_id: &str,
) -> Result<DocumentDetail, ApiError> {
    let document = database
        .get_document(document_id)?
        .ok_or_else(ApiError::not_found)?;
    if document.document.owner_id == principal.user_id {
        Ok(document)
    } else {
        Err(ApiError::forbidden(
            "you do not have access to this document",
        ))
    }
}

fn is_reader_language(language: &str) -> bool {
    matches!(language, "zh-CN" | "en-US" | "ja-JP" | "es-ES")
}

fn source_language_or_und(value: Option<&str>) -> Result<String, ApiError> {
    match value {
        Some(value) => normalize_language_tag(value)
            .ok_or_else(|| ApiError::bad_request("source_language must be a BCP 47 tag")),
        None => Ok("und".to_owned()),
    }
}

fn requested_language(value: Option<&str>) -> Result<Option<String>, ApiError> {
    value
        .map(|language| {
            normalize_language_tag(language)
                .ok_or_else(|| ApiError::bad_request("language must be a BCP 47 tag"))
        })
        .transpose()
}

fn profile_summary_task_name(value: &str) -> Result<String, ApiError> {
    let task = match value {
        "all" => "all",
        "experience" | "profile_experience" => "profile_experience",
        "personality" | "profile_personality" => "profile_personality",
        "mbti" | "profile_mbti" => "profile_mbti",
        "schwartz" | "profile_schwartz" => "profile_schwartz",
        "motivations" | "profile_motivations" => "profile_motivations",
        "philosophy" | "profile_philosophy" => "profile_philosophy",
        "timeline" | "profile_timeline" => "profile_timeline",
        _ => return Err(ApiError::bad_request("unsupported profile summary task")),
    };
    Ok(task.to_owned())
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

fn publication_visibility(value: &str) -> Result<&str, ApiError> {
    match value {
        "public" | "private" => Ok(value),
        _ => Err(ApiError::bad_request(
            "visibility must be public or private",
        )),
    }
}

fn validate_publication_time(published_at: i64) -> Result<(), ApiError> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(published_at, 0)
        .is_some()
        .then_some(())
        .ok_or_else(|| ApiError::bad_request("published_at must be a valid Unix timestamp"))
}

fn validate_comment_input(input: &DocumentCommentInput) -> Result<(), ApiError> {
    if input.content.trim().is_empty() {
        return Err(ApiError::bad_request("comment content is required"));
    }
    if input.content.chars().count() > 4_000 {
        return Err(ApiError::bad_request(
            "comment content must be at most 4000 characters",
        ));
    }
    let Some(anchor) = input.anchor.as_ref() else {
        return Ok(());
    };
    if anchor.quote.trim().is_empty() {
        return Err(ApiError::bad_request("comment anchor quote is required"));
    }
    if anchor.quote.chars().count() > 1_200
        || anchor.prefix.chars().count() > 160
        || anchor.suffix.chars().count() > 160
    {
        return Err(ApiError::bad_request("comment anchor is too long"));
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
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("conflict: the document changed before publication completed")]
    Conflict,
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("database error")]
    Database(#[from] DatabaseError),
    #[error("AI request failed")]
    Ai(#[from] ai::AiError),
    #[error("system resource request failed")]
    Resource(#[from] ResourceError),
    #[error("media request failed")]
    Media(#[from] std::io::Error),
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
    fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }
    fn conflict() -> Self {
        Self::Conflict
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
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Database(_) | Self::Ai(_) | Self::Resource(_) => StatusCode::BAD_GATEWAY,
            Self::Media(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = self.to_string();
        (status, Json(json!({"error": message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, PublicDocumentQuery, WebAssets, client_address, hash_secret, image_content_type,
        is_media_id, public_document, public_media, publication_visibility, request_principal,
        requested_language, source_language_or_und, static_asset, store_media,
        validate_ai_base_url,
    };
    use crate::{
        db::{CreateDocumentOutcome, Database, NewDocument},
        resources::ResourceMonitor,
        views::ViewCounter,
    };
    use axum::{
        extract::{Path, Query, State},
        http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header},
    };
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn serves_the_app_shell_without_cache_and_hashed_assets_immutably() {
        let shell = static_asset(Uri::from_static("/")).await;
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()[header::CACHE_CONTROL], "no-cache");

        let missing_asset = static_asset(Uri::from_static("/assets/missing.js")).await;
        assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);

        let index_html = WebAssets::get("index.html").unwrap();
        let index_html = std::str::from_utf8(&index_html.data).unwrap();
        let asset_name = index_html
            .split("/assets/")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let asset = static_asset(Uri::try_from(format!("/assets/{asset_name}")).unwrap()).await;
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(
            asset.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }

    fn test_auth_verifier() -> auth_mini_axum::AuthMiniVerifier {
        auth_mini_axum::AuthMiniVerifier::from_issuer_background(
            "https://auth.ntnl.io",
            "ctx.ntnl.io".to_owned(),
            auth_mini_axum::JwksCachePolicy::default(),
        )
        .unwrap()
    }

    #[test]
    fn accepts_only_public_and_private_publication_visibility() {
        assert_eq!(publication_visibility("public").unwrap(), "public");
        assert_eq!(publication_visibility("private").unwrap(), "private");
        assert!(publication_visibility("unlisted").is_err());
    }

    #[test]
    fn normalizes_requested_reader_languages() {
        assert_eq!(
            requested_language(Some("es-es")).unwrap().as_deref(),
            Some("es-ES")
        );
        assert!(requested_language(Some("Spanish")).is_err());
    }

    #[test]
    fn accepts_und_only_as_an_unset_document_source_language() {
        assert_eq!(source_language_or_und(None).unwrap(), "und");
        assert_eq!(source_language_or_und(Some("ja-jp")).unwrap(), "ja-JP");
        assert!(source_language_or_und(Some("Japanese")).is_err());
    }

    #[test]
    fn accepts_arbitrary_http_base_urls_and_normalizes_trailing_slashes() {
        assert_eq!(
            validate_ai_base_url(" https://ai.example.test/v1/// ").unwrap(),
            "https://ai.example.test/v1"
        );
        assert_eq!(
            validate_ai_base_url("http://127.0.0.1:11434/v1").unwrap(),
            "http://127.0.0.1:11434/v1"
        );
    }

    #[test]
    fn rejects_non_http_base_urls() {
        assert!(validate_ai_base_url("ftp://ai.example.test/v1").is_err());
        assert!(validate_ai_base_url("not a url").is_err());
    }

    #[test]
    fn accepts_supported_image_bytes_and_content_hash_identifiers() {
        assert_eq!(
            image_content_type(b"\x89PNG\r\n\x1a\nimage"),
            Some("image/png")
        );
        assert_eq!(
            image_content_type(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(image_content_type(b"GIF89aimage"), Some("image/gif"));
        assert_eq!(image_content_type(b"RIFF____WEBPimage"), Some("image/webp"));
        assert!(image_content_type(b"not an image").is_none());
        assert!(is_media_id(&"a".repeat(64)));
        assert!(!is_media_id(&"g".repeat(64)));
        assert!(!is_media_id(&"A".repeat(64)));
        assert!(!is_media_id("../media"));
    }

    #[tokio::test]
    async fn stored_images_are_content_addressed_and_publicly_readable() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let media_directory = Arc::new(database.media_directory());
        let media_id = store_media(&media_directory, b"\x89PNG\r\n\x1a\nimage").unwrap();
        let response = public_media(
            State(AppState {
                resources: Arc::new(Mutex::new(ResourceMonitor::new(database.clone()))),
                database,
                media_directory,
                views: Arc::new(ViewCounter::new()),
                auth_verifier: test_auth_verifier(),
            }),
            Path(media_id),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }

    fn test_state(database: Database) -> AppState {
        AppState {
            media_directory: Arc::new(database.media_directory()),
            resources: Arc::new(Mutex::new(ResourceMonitor::new(database.clone()))),
            database,
            views: Arc::new(ViewCounter::new()),
            auth_verifier: test_auth_verifier(),
        }
    }

    #[tokio::test]
    async fn api_keys_authenticate_the_private_api_without_auth_mini() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = database
            .create_api_key("user-a", "Cybion", "0123456789", &hash_secret("secret"))
            .unwrap();
        let state = test_state(database);
        let headers = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer ctx_user-a_secret"),
        )]);

        let principal = request_principal(&state, &headers).await.unwrap();
        assert_eq!(principal.user_id, "user-a");

        let wrong_secret = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer ctx_user-a-other"),
        )]);
        assert!(request_principal(&state, &wrong_secret).await.is_err());

        let wrong_user = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer ctx_user-b_secret"),
        )]);
        assert!(request_principal(&state, &wrong_user).await.is_err());

        assert!(
            state
                .database
                .revoke_api_key("user-a", &created.id)
                .unwrap()
        );
        assert!(request_principal(&state, &headers).await.is_err());
    }

    #[test]
    fn prefers_cloudflare_client_headers_for_unique_reader_dedup() {
        let headers = HeaderMap::from_iter([
            (
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("198.51.100.9, 10.0.0.1"),
            ),
            (
                HeaderName::from_static("cf-connecting-ip"),
                HeaderValue::from_static("203.0.113.7"),
            ),
        ]);
        assert_eq!(client_address(&headers), Some("203.0.113.7"));

        let headers = HeaderMap::from_iter([(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static("198.51.100.9, 10.0.0.1"),
        )]);
        assert_eq!(client_address(&headers), Some("198.51.100.9"));
        assert_eq!(client_address(&HeaderMap::new()), None);
    }

    #[tokio::test]
    async fn public_document_reads_are_counted_in_memory_and_served_with_the_pending_total() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let CreateDocumentOutcome::Created(article) = database
            .create_document(&NewDocument {
                author_id: "author-a",
                title: "Counted article",
                source_language: "en-US",
                content: "# Counted",
                message: "Created document",
                parent_id: None,
            })
            .unwrap()
        else {
            panic!("expected the document to be created");
        };
        database
            .publish_document(&article.document.id, &article.revision.id, "public")
            .unwrap();
        let state = test_state(database);
        let reader_headers = HeaderMap::from_iter([
            (
                header::USER_AGENT,
                HeaderValue::from_static("Mozilla/5.0 (X11; Linux x86_64)"),
            ),
            (
                HeaderName::from_static("cf-connecting-ip"),
                HeaderValue::from_static("203.0.113.7"),
            ),
        ]);

        let first = public_document(
            State(state.clone()),
            Path(article.document.id.clone()),
            Query(PublicDocumentQuery { language: None }),
            reader_headers.clone(),
        )
        .await
        .unwrap();
        assert_eq!(first.0.views.total, 1);
        assert_eq!(first.0.views.human, 1);
        assert_eq!(first.0.views.unique_human, 1);

        // The same reader again counts as a read but not as a new unique one.
        let second = public_document(
            State(state.clone()),
            Path(article.document.id.clone()),
            Query(PublicDocumentQuery { language: None }),
            reader_headers,
        )
        .await
        .unwrap();
        assert_eq!(second.0.views.total, 2);
        assert_eq!(second.0.views.unique_human, 1);

        // Machine agents count as reads but not as human or unique readers.
        let agent_headers =
            HeaderMap::from_iter([(header::USER_AGENT, HeaderValue::from_static("GPTBot/1.0"))]);
        let third = public_document(
            State(state),
            Path(article.document.id),
            Query(PublicDocumentQuery { language: None }),
            agent_headers,
        )
        .await
        .unwrap();
        assert_eq!(third.0.views.total, 3);
        assert_eq!(third.0.views.human, 2);
        assert_eq!(third.0.views.unique_human, 1);
    }
}
