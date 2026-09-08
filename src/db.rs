use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{Cipher, CipherError};

const CURRENT_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS documents (
        id TEXT PRIMARY KEY NOT NULL,
        owner_id TEXT NOT NULL,
        title TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('draft', 'published')),
        visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('private', 'public')),
        metadata_json TEXT NOT NULL,
        current_revision_id TEXT NOT NULL,
        published_revision_id TEXT,
        published_title TEXT,
        published_at INTEGER,
        source_language TEXT NOT NULL DEFAULT 'und',
        published_source_language TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS document_revisions (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        message TEXT NOT NULL,
        author_id TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS ai_runs (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
        task TEXT NOT NULL,
        output TEXT NOT NULL,
        proposed_content TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS document_translations (
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        language TEXT NOT NULL,
        source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        published_at INTEGER NOT NULL,
        PRIMARY KEY(document_id, language)
    );
    CREATE TABLE IF NOT EXISTS document_metadata (
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        language TEXT NOT NULL,
        source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
        metadata_json TEXT NOT NULL,
        published_at INTEGER NOT NULL,
        PRIMARY KEY(document_id, language)
    );
    CREATE TABLE IF NOT EXISTS ai_requests (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
        task TEXT NOT NULL CHECK(task IN ('metadata', 'translate')),
        target_language TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
        result_summary TEXT,
        error TEXT,
        created_at INTEGER NOT NULL,
        started_at INTEGER,
        completed_at INTEGER,
        UNIQUE(document_id, source_revision_id, task, target_language)
    );
";

const CURRENT_INDEXES_SQL: &str = "
    CREATE INDEX IF NOT EXISTS documents_owner_idx ON documents(owner_id, updated_at DESC);
    CREATE INDEX IF NOT EXISTS documents_public_idx ON documents(visibility, status, published_at DESC);
    CREATE INDEX IF NOT EXISTS document_revisions_document_idx ON document_revisions(document_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS ai_runs_document_idx ON ai_runs(document_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS document_translations_document_idx ON document_translations(document_id, language);
    CREATE INDEX IF NOT EXISTS document_metadata_document_idx ON document_metadata(document_id, language);
    CREATE INDEX IF NOT EXISTS ai_requests_status_idx ON ai_requests(status, created_at, id);
    CREATE INDEX IF NOT EXISTS ai_requests_document_idx ON ai_requests(document_id, created_at DESC);
";

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    cipher: Cipher,
    database_path: Arc<PathBuf>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Database").finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite operation failed")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database lock is poisoned")]
    Poisoned,
    #[error("failed to protect an instance secret")]
    Cipher(#[from] CipherError),
    #[error("stored JSON is invalid")]
    Json(#[from] serde_json::Error),
    #[error("database migration failed: {0}")]
    Migration(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Document {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub source_language: String,
    pub status: String,
    #[serde(skip_serializing)]
    pub visibility: String,
    pub metadata: Value,
    pub current_revision_id: String,
    pub published_revision_id: Option<String>,
    pub published_source_language: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentRevision {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub message: String,
    pub author_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentDetail {
    pub document: Document,
    pub revision: DocumentRevision,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicDocumentSummary {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub metadata: PublishedMetadata,
    pub language: String,
    pub is_metadata_fallback: bool,
    pub metadata_status: Option<String>,
    pub published_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicDocumentDetail {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub content: String,
    pub source_language: String,
    pub language: String,
    pub available_languages: Vec<String>,
    pub metadata: PublishedMetadata,
    pub is_metadata_fallback: bool,
    pub metadata_status: Option<String>,
    pub requested_language: Option<String>,
    pub translation_status: Option<String>,
    pub is_translation_fallback: bool,
    pub published_at: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PublishedMetadata {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub short_summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub inferred_date: String,
    #[serde(default)]
    pub inferred_lang: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub audience: String,
}

impl PublishedMetadata {
    pub fn is_empty(&self) -> bool {
        self.description.is_empty()
            && self.summary.is_empty()
            && self.short_summary.is_empty()
            && self.tags.is_empty()
            && self.key_points.is_empty()
            && self.audience.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AiConfiguration {
    pub base_url: String,
    pub model: String,
    pub configured: bool,
}

#[derive(Clone, Debug)]
pub struct AiCredentials {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AiRun {
    pub id: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub task: String,
    pub output: String,
    pub proposed_content: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AiRequest {
    pub id: String,
    pub document_id: String,
    pub document_title: String,
    pub source_revision_id: String,
    pub task: String,
    pub target_language: String,
    pub status: String,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewDocument<'a> {
    pub author_id: &'a str,
    pub title: &'a str,
    pub source_language: &'a str,
    pub content: &'a str,
    pub message: &'a str,
}

#[derive(Clone, Debug)]
pub struct DocumentSave<'a> {
    pub id: &'a str,
    pub author_id: &'a str,
    pub title: &'a str,
    pub source_language: &'a str,
    pub content: &'a str,
    pub message: &'a str,
    pub metadata: &'a Value,
}

#[derive(Clone, Debug)]
struct PublishedDocument {
    id: String,
    owner_id: String,
    title: String,
    content: String,
    source_language: String,
    source_revision_id: String,
    published_at: i64,
}

impl Database {
    pub fn open(state_directory: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let state_directory = state_directory.as_ref();
        let cipher = Cipher::load_or_create(state_directory)?;
        let database_path = state_directory.join("ctx.sqlite3");
        let mut connection = Connection::open(&database_path)?;
        connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            ",
        )?;
        migrate_legacy_context_schema(&mut connection)?;
        add_direct_document_columns_if_needed(&connection)?;
        connection.execute_batch(CURRENT_TABLES_SQL)?;
        connection.execute_batch(CURRENT_INDEXES_SQL)?;
        connection.execute("DROP TABLE IF EXISTS author_language_preferences", [])?;
        backfill_published_source_languages(&connection)?;
        backfill_published_metadata_requests(&mut connection)?;
        connection.execute(
            "INSERT INTO app_meta(key, value) VALUES ('ai_base_url', 'https://openai.ntnl.io/v1') ON CONFLICT(key) DO NOTHING",
            [],
        )?;
        connection.execute(
            "INSERT INTO app_meta(key, value) VALUES ('ai_model', '') ON CONFLICT(key) DO NOTHING",
            [],
        )?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher,
            database_path: Arc::new(database_path),
        })
    }

    pub fn database_path(&self) -> PathBuf {
        self.database_path.as_ref().clone()
    }

    pub fn root_user_id(&self) -> Result<Option<String>, DatabaseError> {
        self.meta("root_user_id")
    }

    pub fn initialize_root_user(&self, user_id: &str) -> Result<bool, DatabaseError> {
        Ok(self.connection()?.execute(
            "INSERT INTO app_meta(key, value) VALUES ('root_user_id', ?1) ON CONFLICT(key) DO NOTHING",
            [user_id],
        )? == 1)
    }

    pub fn ai_configuration(&self) -> Result<AiConfiguration, DatabaseError> {
        let base_url = self.meta("ai_base_url")?.unwrap_or_default();
        let model = self.meta("ai_model")?.unwrap_or_default();
        let configured = self
            .meta("ai_api_key_ciphertext")?
            .is_some_and(|key| !key.is_empty())
            && !model.is_empty();
        Ok(AiConfiguration {
            base_url,
            model,
            configured,
        })
    }

    pub fn update_ai_configuration(
        &self,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
    ) -> Result<AiConfiguration, DatabaseError> {
        self.put_meta("ai_base_url", base_url)?;
        self.put_meta("ai_model", model)?;
        if let Some(api_key) = api_key.filter(|value| !value.is_empty()) {
            self.put_meta("ai_api_key_ciphertext", &self.cipher.encrypt(api_key)?)?;
        }
        self.ai_configuration()
    }

    pub fn ai_credentials(&self) -> Result<Option<AiCredentials>, DatabaseError> {
        let configuration = self.ai_configuration()?;
        let Some(ciphertext) = self.meta("ai_api_key_ciphertext")? else {
            return Ok(None);
        };
        if !configuration.configured {
            return Ok(None);
        }
        Ok(Some(AiCredentials {
            base_url: configuration.base_url,
            model: configuration.model,
            api_key: self.cipher.decrypt(&ciphertext)?,
        }))
    }

    pub fn create_document(
        &self,
        input: &NewDocument<'_>,
    ) -> Result<DocumentDetail, DatabaseError> {
        let document_id = Uuid::new_v4().to_string();
        let revision = DocumentRevision {
            id: Uuid::new_v4().to_string(),
            document_id: document_id.clone(),
            content: input.content.to_owned(),
            message: input.message.to_owned(),
            author_id: input.author_id.to_owned(),
            created_at: now(),
        };
        let document = Document {
            id: document_id,
            owner_id: input.author_id.to_owned(),
            title: input.title.to_owned(),
            source_language: input.source_language.to_owned(),
            status: "draft".to_owned(),
            visibility: "private".to_owned(),
            metadata: Value::Object(Default::default()),
            current_revision_id: revision.id.clone(),
            published_revision_id: None,
            published_source_language: None,
            created_at: revision.created_at,
            updated_at: revision.created_at,
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO documents(id, owner_id, title, source_language, status, visibility, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, ?9, ?10)",
            params![document.id, document.owner_id, document.title, document.source_language, document.status, document.visibility, document.metadata.to_string(), document.current_revision_id, document.created_at, document.updated_at],
        )?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.id, revision.document_id, revision.content, revision.message, revision.author_id, revision.created_at],
        )?;
        transaction.commit()?;
        Ok(DocumentDetail { document, revision })
    }

    pub fn list_documents(&self, owner_id: &str) -> Result<Vec<Document>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, owner_id, title, source_language, status, visibility, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at FROM documents WHERE owner_id = ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        Ok(statement
            .query_map([owner_id], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentDetail>, DatabaseError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT d.id, d.owner_id, d.title, d.source_language, d.status, d.visibility, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = d.current_revision_id AND r.document_id = d.id WHERE d.id = ?1",
                [id],
                document_detail_from_row,
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn save_document(
        &self,
        input: &DocumentSave<'_>,
    ) -> Result<Option<DocumentDetail>, DatabaseError> {
        let Some(existing) = self.get_document(input.id)? else {
            return Ok(None);
        };
        let revision = DocumentRevision {
            id: Uuid::new_v4().to_string(),
            document_id: existing.document.id.clone(),
            content: input.content.to_owned(),
            message: input.message.to_owned(),
            author_id: input.author_id.to_owned(),
            created_at: now(),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.id, revision.document_id, revision.content, revision.message, revision.author_id, revision.created_at],
        )?;
        transaction.execute(
            "UPDATE documents SET title = ?2, source_language = ?3, metadata_json = ?4, current_revision_id = ?5, updated_at = ?6 WHERE id = ?1",
            params![input.id, input.title, input.source_language, input.metadata.to_string(), revision.id, revision.created_at],
        )?;
        transaction.commit()?;
        let document = Document {
            title: input.title.to_owned(),
            source_language: input.source_language.to_owned(),
            metadata: input.metadata.clone(),
            current_revision_id: revision.id.clone(),
            updated_at: revision.created_at,
            ..existing.document
        };
        Ok(Some(DocumentDetail { document, revision }))
    }

    pub fn publish_document(
        &self,
        id: &str,
        source_revision_id: &str,
    ) -> Result<Option<Document>, DatabaseError> {
        let Some(mut detail) = self.get_document(id)? else {
            return Ok(None);
        };
        let source_language = detail.document.source_language.clone();
        let updated_at = now();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE documents SET status = 'published', visibility = 'public', published_revision_id = ?2, published_title = title, published_at = ?3, source_language = ?4, published_source_language = ?4, updated_at = ?3 WHERE id = ?1 AND current_revision_id = ?2",
            params![id, source_revision_id, updated_at, source_language],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        transaction.execute(
            "DELETE FROM document_translations WHERE document_id = ?1 AND source_revision_id != ?2",
            params![id, source_revision_id],
        )?;
        transaction.execute(
            "DELETE FROM document_metadata WHERE document_id = ?1 AND source_revision_id != ?2",
            params![id, source_revision_id],
        )?;
        enqueue_ai_request(
            &transaction,
            id,
            source_revision_id,
            "metadata",
            "",
            updated_at,
        )?;
        transaction.commit()?;
        detail.document.status = "published".to_owned();
        detail.document.visibility = "public".to_owned();
        detail.document.source_language = source_language.clone();
        detail.document.published_revision_id = Some(source_revision_id.to_owned());
        detail.document.published_source_language = Some(source_language);
        detail.document.updated_at = updated_at;
        Ok(Some(detail.document))
    }

    pub fn requeue_running_ai_requests(&self) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'queued', started_at = NULL WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    pub fn requeue_failed_ai_requests(&self) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'queued', result_summary = NULL, error = NULL, started_at = NULL, completed_at = NULL WHERE status = 'failed'",
            [],
        )?;
        Ok(())
    }

    pub fn claim_next_ai_request(&self) -> Result<Option<AiRequest>, DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request_id: Option<String> = transaction
            .query_row(
                "SELECT id FROM ai_requests WHERE status = 'queued' ORDER BY created_at, id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(request_id) = request_id else {
            transaction.commit()?;
            return Ok(None);
        };
        let started_at = now();
        transaction.execute(
            "UPDATE ai_requests SET status = 'running', started_at = ?2, completed_at = NULL, result_summary = NULL, error = NULL WHERE id = ?1 AND status = 'queued'",
            params![request_id, started_at],
        )?;
        let request = transaction.query_row(
            "SELECT r.id, r.document_id, COALESCE(d.published_title, d.title), r.source_revision_id, r.task, r.target_language, r.status, r.result_summary, r.error, r.created_at, r.started_at, r.completed_at FROM ai_requests r JOIN documents d ON d.id = r.document_id WHERE r.id = ?1",
            [request_id],
            ai_request_from_row,
        )?;
        transaction.commit()?;
        Ok(Some(request))
    }

    pub fn complete_ai_request(&self, id: &str, summary: &str) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'succeeded', result_summary = ?2, error = NULL, completed_at = ?3 WHERE id = ?1",
            params![id, summary, now()],
        )?;
        Ok(())
    }

    pub fn fail_ai_request(&self, id: &str, error: &str) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'failed', result_summary = NULL, error = ?2, completed_at = ?3 WHERE id = ?1",
            params![id, error, now()],
        )?;
        Ok(())
    }

    pub fn ai_requests(&self) -> Result<Vec<AiRequest>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT r.id, r.document_id, COALESCE(d.published_title, d.title), r.source_revision_id, r.task, r.target_language, r.status, r.result_summary, r.error, r.created_at, r.started_at, r.completed_at FROM ai_requests r JOIN documents d ON d.id = r.document_id ORDER BY r.created_at DESC, r.id DESC LIMIT 200",
        )?;
        Ok(statement
            .query_map([], ai_request_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn public_documents(
        &self,
        requested_language: Option<&str>,
    ) -> Result<Vec<PublicDocumentSummary>, DatabaseError> {
        let connection = self.connection()?;
        let publications = {
            let mut statement = connection.prepare(
                "SELECT d.id, d.owner_id, d.published_title, r.content, d.published_source_language, d.published_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL ORDER BY d.published_at DESC, d.id DESC",
            )?;
            statement
                .query_map([], published_document_from_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        publications
            .iter()
            .map(|publication| {
                public_document_summary(&connection, publication, requested_language)
            })
            .collect()
    }

    pub fn public_document(
        &self,
        id: &str,
        requested_language: Option<&str>,
    ) -> Result<Option<PublicDocumentDetail>, DatabaseError> {
        let connection = self.connection()?;
        let Some(publication) = connection
            .query_row(
                "SELECT d.id, d.owner_id, d.published_title, r.content, d.published_source_language, d.published_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.id = ?1 AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL",
                [id],
                published_document_from_row,
            )
            .optional()?
        else {
            return Ok(None);
        };
        let target_language =
            requested_language.filter(|language| *language != publication.source_language);
        let translated = target_language
            .map(|language| published_translation(&connection, &publication, language))
            .transpose()?;
        let translated_metadata_exists = target_language
            .map(|language| {
                metadata_exists(
                    &connection,
                    &publication.id,
                    &publication.source_revision_id,
                    language,
                )
            })
            .transpose()?
            .unwrap_or(false);
        let (title, content, language, content_fallback) =
            match (translated, translated_metadata_exists) {
                (Some(Some((title, content))), true) => (
                    title,
                    content,
                    target_language.unwrap_or_default().to_owned(),
                    false,
                ),
                _ => (
                    publication.title.clone(),
                    publication.content.clone(),
                    publication.source_language.clone(),
                    target_language.is_some(),
                ),
            };
        let (metadata, is_metadata_fallback) =
            published_metadata(&connection, &publication, &language)?;
        let metadata_status = metadata
            .is_empty()
            .then(|| {
                ai_request_status_on(
                    &connection,
                    &publication.id,
                    &publication.source_revision_id,
                    "metadata",
                    "",
                )
            })
            .transpose()?
            .flatten();
        let translation_fallback = content_fallback || is_metadata_fallback;
        let translation_status = if translation_fallback {
            target_language
                .map(|language| {
                    ai_request_status_on(
                        &connection,
                        id,
                        &publication.source_revision_id,
                        "translate",
                        language,
                    )
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        let translation_languages = {
            let mut statement = connection.prepare(
                "SELECT language FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 ORDER BY language",
            )?;
            statement
                .query_map(params![id, publication.source_revision_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut available_languages = vec![publication.source_language.clone()];
        available_languages.extend(translation_languages);
        Ok(Some(PublicDocumentDetail {
            id: publication.id,
            owner_id: publication.owner_id,
            title,
            content,
            source_language: publication.source_language,
            language,
            available_languages,
            metadata,
            is_metadata_fallback,
            metadata_status,
            requested_language: requested_language.map(str::to_owned),
            translation_status,
            is_translation_fallback: translation_fallback,
            published_at: publication.published_at,
        }))
    }

    pub fn request_public_translation(
        &self,
        document_id: &str,
        target_language: &str,
    ) -> Result<Option<String>, DatabaseError> {
        let connection = self.connection()?;
        let publication: Option<(String, String)> = connection
            .query_row(
                "SELECT published_revision_id, published_source_language FROM documents WHERE id = ?1 AND status = 'published' AND visibility = 'public' AND published_revision_id IS NOT NULL AND published_source_language IS NOT NULL",
                [document_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((source_revision_id, source_language)) = publication else {
            return Ok(None);
        };
        if source_language == target_language {
            return Ok(Some("succeeded".to_owned()));
        }
        let translation_exists = translation_exists(
            &connection,
            document_id,
            &source_revision_id,
            target_language,
        )?;
        let metadata_exists = metadata_exists(
            &connection,
            document_id,
            &source_revision_id,
            target_language,
        )?;
        if translation_exists && metadata_exists {
            return Ok(Some("succeeded".to_owned()));
        }
        connection.execute(
            "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, 'translate', ?4, 'queued', ?5) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status IN ('failed', 'succeeded')",
            params![Uuid::new_v4().to_string(), document_id, source_revision_id, target_language, now()],
        )?;
        ai_request_status_on(
            &connection,
            document_id,
            &source_revision_id,
            "translate",
            target_language,
        )
    }

    pub fn published_document_detail(
        &self,
        document_id: &str,
        source_revision_id: &str,
    ) -> Result<Option<DocumentDetail>, DatabaseError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT d.id, d.owner_id, d.published_title, d.published_source_language, d.status, d.visibility, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = ?2 AND r.document_id = d.id WHERE d.id = ?1 AND d.status = 'published' AND d.visibility = 'public' AND d.published_revision_id = ?2 AND d.published_title IS NOT NULL AND d.published_source_language IS NOT NULL",
                params![document_id, source_revision_id],
                document_detail_from_row,
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn apply_published_metadata(
        &self,
        document_id: &str,
        source_revision_id: &str,
        metadata: &PublishedMetadata,
        inferred_language: &str,
    ) -> Result<Option<String>, DatabaseError> {
        self.connection()?.execute(
            "UPDATE documents SET metadata_json = ?3, source_language = CASE WHEN source_language = 'und' THEN ?4 ELSE source_language END, published_source_language = CASE WHEN published_source_language = 'und' THEN ?4 ELSE published_source_language END, updated_at = ?5 WHERE id = ?1 AND published_revision_id = ?2",
            params![document_id, source_revision_id, serde_json::to_string(metadata)?, inferred_language, now()],
        )?;
        self.connection()?
            .query_row(
                "SELECT published_source_language FROM documents WHERE id = ?1 AND published_revision_id = ?2",
                params![document_id, source_revision_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn store_published_metadata(
        &self,
        document_id: &str,
        source_revision_id: &str,
        language: &str,
        metadata: &PublishedMetadata,
    ) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "INSERT INTO document_metadata(document_id, language, source_revision_id, metadata_json, published_at) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published' AND visibility = 'public') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, metadata_json = excluded.metadata_json, published_at = excluded.published_at",
            params![document_id, language, source_revision_id, serde_json::to_string(metadata)?, now()],
        )?;
        Ok(changed == 1)
    }

    pub fn published_source_metadata(
        &self,
        document_id: &str,
        source_revision_id: &str,
        source_language: &str,
    ) -> Result<Option<PublishedMetadata>, DatabaseError> {
        self.connection()?
            .query_row(
                "SELECT metadata_json FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
                params![document_id, source_revision_id, source_language],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|metadata| metadata.as_deref().map(metadata_from_json))
            .map_err(DatabaseError::Sqlite)
    }

    pub fn requeue_translations_missing_metadata(
        &self,
        document_id: &str,
        source_revision_id: &str,
    ) -> Result<(), DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let languages = {
            let mut statement = transaction.prepare(
                "SELECT language FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 AND NOT EXISTS (SELECT 1 FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = document_translations.language)",
            )?;
            statement
                .query_map(params![document_id, source_revision_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        for language in languages {
            force_enqueue_ai_request(
                &transaction,
                document_id,
                source_revision_id,
                "translate",
                &language,
                now(),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn store_published_translation(
        &self,
        document_id: &str,
        source_revision_id: &str,
        language: &str,
        title: &str,
        content: &str,
        metadata: &PublishedMetadata,
    ) -> Result<bool, DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "INSERT INTO document_translations(document_id, language, source_revision_id, title, content, published_at) SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published' AND visibility = 'public') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, title = excluded.title, content = excluded.content, published_at = excluded.published_at",
            params![document_id, language, source_revision_id, title, content, now()],
        )?;
        transaction.execute(
            "INSERT INTO document_metadata(document_id, language, source_revision_id, metadata_json, published_at) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published' AND visibility = 'public') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, metadata_json = excluded.metadata_json, published_at = excluded.published_at",
            params![document_id, language, source_revision_id, serde_json::to_string(metadata)?, now()],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub fn record_ai_run(
        &self,
        document_id: &str,
        source_revision_id: &str,
        task: &str,
        output: &str,
        proposed_content: Option<&str>,
    ) -> Result<AiRun, DatabaseError> {
        let run = AiRun {
            id: Uuid::new_v4().to_string(),
            document_id: document_id.to_owned(),
            source_revision_id: source_revision_id.to_owned(),
            task: task.to_owned(),
            output: output.to_owned(),
            proposed_content: proposed_content.map(str::to_owned),
            created_at: now(),
        };
        self.connection()?.execute(
            "INSERT INTO ai_runs(id, document_id, source_revision_id, task, output, proposed_content, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![run.id, run.document_id, run.source_revision_id, run.task, run.output, run.proposed_content, run.created_at],
        )?;
        Ok(run)
    }

    fn meta(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        self.connection()?
            .query_row("SELECT value FROM app_meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    fn put_meta(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "INSERT INTO app_meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, DatabaseError> {
        self.connection.lock().map_err(|_| DatabaseError::Poisoned)
    }
}

fn enqueue_ai_request(
    transaction: &rusqlite::Transaction<'_>,
    document_id: &str,
    source_revision_id: &str,
    task: &str,
    target_language: &str,
    created_at: i64,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status = 'failed'",
        params![Uuid::new_v4().to_string(), document_id, source_revision_id, task, target_language, created_at],
    )?;
    Ok(())
}

fn force_enqueue_ai_request(
    transaction: &rusqlite::Transaction<'_>,
    document_id: &str,
    source_revision_id: &str,
    task: &str,
    target_language: &str,
    created_at: i64,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status != 'running'",
        params![Uuid::new_v4().to_string(), document_id, source_revision_id, task, target_language, created_at],
    )?;
    Ok(())
}

fn ai_request_status_on(
    connection: &Connection,
    document_id: &str,
    source_revision_id: &str,
    task: &str,
    target_language: &str,
) -> Result<Option<String>, DatabaseError> {
    connection
        .query_row(
            "SELECT status FROM ai_requests WHERE document_id = ?1 AND source_revision_id = ?2 AND task = ?3 AND target_language = ?4",
            params![document_id, source_revision_id, task, target_language],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::Sqlite)
}

fn translation_exists(
    connection: &Connection,
    document_id: &str,
    source_revision_id: &str,
    language: &str,
) -> Result<bool, DatabaseError> {
    connection
        .query_row(
            "SELECT 1 FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
            params![document_id, source_revision_id, language],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(DatabaseError::Sqlite)
}

fn metadata_exists(
    connection: &Connection,
    document_id: &str,
    source_revision_id: &str,
    language: &str,
) -> Result<bool, DatabaseError> {
    connection
        .query_row(
            "SELECT 1 FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
            params![document_id, source_revision_id, language],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(DatabaseError::Sqlite)
}

fn published_translation(
    connection: &Connection,
    publication: &PublishedDocument,
    language: &str,
) -> Result<Option<(String, String)>, DatabaseError> {
    connection
        .query_row(
            "SELECT title, content FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
            params![publication.id, publication.source_revision_id, language],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(DatabaseError::Sqlite)
}

fn published_metadata(
    connection: &Connection,
    publication: &PublishedDocument,
    language: &str,
) -> Result<(PublishedMetadata, bool), DatabaseError> {
    let metadata = connection
        .query_row(
            "SELECT metadata_json FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
            params![publication.id, publication.source_revision_id, language],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(metadata) = metadata {
        return Ok((metadata_from_json(&metadata), false));
    }
    if language == publication.source_language {
        return Ok((PublishedMetadata::default(), false));
    }
    let source_metadata = connection
        .query_row(
            "SELECT metadata_json FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
            params![publication.id, publication.source_revision_id, publication.source_language],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok((
        source_metadata
            .as_deref()
            .map(metadata_from_json)
            .unwrap_or_default(),
        true,
    ))
}

fn public_document_summary(
    connection: &Connection,
    publication: &PublishedDocument,
    requested_language: Option<&str>,
) -> Result<PublicDocumentSummary, DatabaseError> {
    let target_language =
        requested_language.filter(|language| *language != publication.source_language);
    let translated = target_language
        .map(|language| published_translation(connection, publication, language))
        .transpose()?;
    let translated_metadata_exists = target_language
        .map(|language| {
            metadata_exists(
                connection,
                &publication.id,
                &publication.source_revision_id,
                language,
            )
        })
        .transpose()?
        .unwrap_or(false);
    let (title, language) = match (translated, translated_metadata_exists) {
        (Some(Some((title, _))), true) => (title, target_language.unwrap_or_default().to_owned()),
        _ => (
            publication.title.clone(),
            publication.source_language.clone(),
        ),
    };
    let (metadata, is_metadata_fallback) = published_metadata(connection, publication, &language)?;
    let metadata_status = metadata
        .is_empty()
        .then(|| {
            ai_request_status_on(
                connection,
                &publication.id,
                &publication.source_revision_id,
                "metadata",
                "",
            )
        })
        .transpose()?
        .flatten();
    Ok(PublicDocumentSummary {
        id: publication.id.clone(),
        owner_id: publication.owner_id.clone(),
        title,
        metadata,
        language,
        is_metadata_fallback,
        metadata_status,
        published_at: publication.published_at,
    })
}

fn metadata_from_json(value: &str) -> PublishedMetadata {
    serde_json::from_str(value).unwrap_or_default()
}

fn backfill_published_metadata_requests(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    let publications = {
        let mut statement = transaction.prepare(
            "SELECT d.id, d.published_revision_id FROM documents d WHERE d.status = 'published' AND d.visibility = 'public' AND d.published_revision_id IS NOT NULL AND d.published_source_language IS NOT NULL AND NOT EXISTS (SELECT 1 FROM document_metadata m WHERE m.document_id = d.id AND m.source_revision_id = d.published_revision_id AND m.language = d.published_source_language)",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (document_id, source_revision_id) in publications {
        force_enqueue_ai_request(
            &transaction,
            &document_id,
            &source_revision_id,
            "metadata",
            "",
            now(),
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn migrate_legacy_context_schema(connection: &mut Connection) -> Result<(), DatabaseError> {
    // COMPATIBILITY: CTX 0.1 stored documents below contexts. This runs only for databases
    // without documents.owner_id. Remove after every supported deployed database has been
    // upgraded; verify with PRAGMA table_info(documents) before removing this path.
    if !table_exists(connection, "documents")?
        || table_has_column(connection, "documents", "owner_id")?
    {
        return Ok(());
    }
    for table in ["contexts", "document_revisions"] {
        if !table_exists(connection, table)? {
            return Err(DatabaseError::Migration(format!(
                "legacy documents require the {table} table"
            )));
        }
    }
    if !table_has_column(connection, "documents", "context_id")? {
        return Err(DatabaseError::Migration(
            "documents has neither owner_id nor context_id".to_owned(),
        ));
    }
    let documents_without_owner: i64 = connection.query_row(
        "SELECT COUNT(*) FROM documents d LEFT JOIN contexts c ON c.id = d.context_id WHERE c.id IS NULL",
        [],
        |row| row.get(0),
    )?;
    if documents_without_owner != 0 {
        return Err(DatabaseError::Migration(
            "legacy documents without a Context owner cannot be migrated safely".to_owned(),
        ));
    }
    let documents_without_current_revision: i64 = connection.query_row(
        "SELECT COUNT(*) FROM documents d LEFT JOIN document_revisions r ON r.id = d.current_revision_id WHERE r.id IS NULL OR r.document_id != d.id",
        [],
        |row| row.get(0),
    )?;
    if documents_without_current_revision != 0 {
        return Err(DatabaseError::Migration(
            "legacy documents with a current revision from another document cannot be migrated safely"
                .to_owned(),
        ));
    }
    let document_count = table_count(connection, "documents")?;
    let revision_count = table_count(connection, "document_revisions")?;
    let has_ai_runs = table_exists(connection, "ai_runs")?;
    let ai_run_count = has_ai_runs
        .then(|| table_count(connection, "ai_runs"))
        .transpose()?
        .unwrap_or_default();

    connection.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let migration = (|| -> Result<(), DatabaseError> {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if has_ai_runs {
            transaction.execute_batch("ALTER TABLE ai_runs RENAME TO ai_runs_legacy;")?;
        }
        transaction.execute_batch(
            "
            ALTER TABLE document_revisions RENAME TO document_revisions_legacy;
            ALTER TABLE documents RENAME TO documents_legacy;
            ",
        )?;
        transaction.execute_batch(CURRENT_TABLES_SQL)?;
        transaction.execute(
            "INSERT INTO documents(id, owner_id, title, status, visibility, metadata_json, current_revision_id, published_revision_id, published_title, published_at, created_at, updated_at) SELECT d.id, c.owner_id, d.title, CASE WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL THEN 'published' ELSE 'draft' END, CASE WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL THEN 'public' ELSE 'private' END, d.metadata_json, d.current_revision_id, CASE WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL THEN d.published_revision_id END, CASE WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL AND d.current_revision_id = d.published_revision_id THEN d.title WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL THEN 'Published document' END, CASE WHEN c.visibility = 'public' AND d.status = 'published' AND published_revision.id IS NOT NULL THEN published_revision.created_at END, d.created_at, d.updated_at FROM documents_legacy d JOIN contexts c ON c.id = d.context_id LEFT JOIN document_revisions_legacy published_revision ON published_revision.id = d.published_revision_id AND published_revision.document_id = d.id",
            [],
        )?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) SELECT id, document_id, content, message, author_id, created_at FROM document_revisions_legacy",
            [],
        )?;
        if has_ai_runs {
            transaction.execute(
                "INSERT INTO ai_runs(id, document_id, source_revision_id, task, output, proposed_content, created_at) SELECT id, document_id, source_revision_id, task, output, proposed_content, created_at FROM ai_runs_legacy",
                [],
            )?;
        }
        verify_migration_counts(
            &transaction,
            document_count,
            revision_count,
            ai_run_count,
            has_ai_runs,
        )?;
        verify_document_revision_links(&transaction)?;
        if has_ai_runs {
            transaction.execute_batch("DROP TABLE ai_runs_legacy;")?;
        }
        transaction.execute_batch(
            "
            DROP TABLE document_revisions_legacy;
            DROP TABLE documents_legacy;
            DROP TABLE contexts;
            ",
        )?;
        verify_foreign_keys(&transaction)?;
        transaction.commit()?;
        Ok(())
    })();
    let foreign_keys = connection.execute_batch("PRAGMA foreign_keys = ON;");
    migration?;
    foreign_keys?;
    verify_connection_foreign_keys(connection)?;
    Ok(())
}

fn add_direct_document_columns_if_needed(connection: &Connection) -> Result<(), DatabaseError> {
    // COMPATIBILITY: early direct-document builds did not persist publication snapshots.
    // Remove after no supported database lacks these columns; verify with PRAGMA table_info.
    if !table_exists(connection, "documents")?
        || !table_has_column(connection, "documents", "owner_id")?
    {
        return Ok(());
    }
    for (column, definition) in [
        ("visibility", "TEXT NOT NULL DEFAULT 'private'"),
        ("published_title", "TEXT"),
        ("published_at", "INTEGER"),
        ("source_language", "TEXT NOT NULL DEFAULT 'und'"),
        ("published_source_language", "TEXT"),
    ] {
        if !table_has_column(connection, "documents", column)? {
            connection.execute(
                &format!("ALTER TABLE documents ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    Ok(())
}

fn backfill_published_source_languages(connection: &Connection) -> Result<(), DatabaseError> {
    // COMPATIBILITY: direct-document releases before v0.1.0-7 had immutable content snapshots
    // but no immutable source-language snapshot. Existing published rows receive `und` instead
    // of a guessed language. Remove after every supported database has these columns populated;
    // verify with this query returning zero rows before removal.
    connection.execute(
        "UPDATE documents SET published_source_language = source_language WHERE published_revision_id IS NOT NULL AND published_source_language IS NULL",
        [],
    )?;
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, DatabaseError> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(DatabaseError::Sqlite)
}

fn table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, DatabaseError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

fn table_count(connection: &Connection, table: &str) -> Result<i64, DatabaseError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(DatabaseError::Sqlite)
}

fn verify_migration_counts(
    transaction: &rusqlite::Transaction<'_>,
    document_count: i64,
    revision_count: i64,
    ai_run_count: i64,
    has_ai_runs: bool,
) -> Result<(), DatabaseError> {
    let migrated_document_count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
    let migrated_revision_count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM document_revisions", [], |row| {
            row.get(0)
        })?;
    let migrated_ai_run_count = if has_ai_runs {
        transaction.query_row("SELECT COUNT(*) FROM ai_runs", [], |row| row.get(0))?
    } else {
        0
    };
    if (
        migrated_document_count,
        migrated_revision_count,
        migrated_ai_run_count,
    ) == (document_count, revision_count, ai_run_count)
    {
        Ok(())
    } else {
        Err(DatabaseError::Migration(
            "legacy row counts changed during migration".to_owned(),
        ))
    }
}

fn verify_foreign_keys(transaction: &rusqlite::Transaction<'_>) -> Result<(), DatabaseError> {
    let has_violation = {
        let mut statement = transaction.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        rows.next()?.is_some()
    };
    if has_violation {
        Err(DatabaseError::Migration(
            "foreign_key_check found a migrated row without its parent".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn verify_document_revision_links(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<(), DatabaseError> {
    let invalid_current_revision_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM documents d LEFT JOIN document_revisions r ON r.id = d.current_revision_id WHERE r.id IS NULL OR r.document_id != d.id",
        [],
        |row| row.get(0),
    )?;
    let invalid_published_revision_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM documents d LEFT JOIN document_revisions r ON r.id = d.published_revision_id WHERE d.published_revision_id IS NOT NULL AND (r.id IS NULL OR r.document_id != d.id)",
        [],
        |row| row.get(0),
    )?;
    if invalid_current_revision_count == 0 && invalid_published_revision_count == 0 {
        Ok(())
    } else {
        Err(DatabaseError::Migration(
            "a document revision link points at another document".to_owned(),
        ))
    }
}

fn verify_connection_foreign_keys(connection: &Connection) -> Result<(), DatabaseError> {
    let has_violation = {
        let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        rows.next()?.is_some()
    };
    if has_violation {
        Err(DatabaseError::Migration(
            "foreign_key_check failed after migration commit".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn document_from_row(row: &Row<'_>) -> rusqlite::Result<Document> {
    let metadata: String = row.get(6)?;
    Ok(Document {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        title: row.get(2)?,
        source_language: row.get(3)?,
        status: row.get(4)?,
        visibility: row.get(5)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(Value::Object(Default::default())),
        current_revision_id: row.get(7)?,
        published_revision_id: row.get(8)?,
        published_source_language: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn document_detail_from_row(row: &Row<'_>) -> rusqlite::Result<DocumentDetail> {
    let document = document_from_row(row)?;
    Ok(DocumentDetail {
        document,
        revision: DocumentRevision {
            id: row.get(12)?,
            document_id: row.get(13)?,
            content: row.get(14)?,
            message: row.get(15)?,
            author_id: row.get(16)?,
            created_at: row.get(17)?,
        },
    })
}

fn published_document_from_row(row: &Row<'_>) -> rusqlite::Result<PublishedDocument> {
    Ok(PublishedDocument {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        source_language: row.get(4)?,
        source_revision_id: row.get(5)?,
        published_at: row.get(6)?,
    })
}

fn ai_request_from_row(row: &Row<'_>) -> rusqlite::Result<AiRequest> {
    Ok(AiRequest {
        id: row.get(0)?,
        document_id: row.get(1)?,
        document_title: row.get(2)?,
        source_revision_id: row.get(3)?,
        task: row.get(4)?,
        target_language: row.get(5)?,
        status: row.get(6)?,
        result_summary: row.get(7)?,
        error: row.get(8)?,
        created_at: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
    })
}

fn now() -> i64 {
    Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use serde_json::json;

    use super::{Database, DocumentSave, NewDocument, PublishedMetadata};

    fn metadata(language: &str, description: &str) -> PublishedMetadata {
        PublishedMetadata {
            description: description.to_owned(),
            summary: format!("{description} summary"),
            short_summary: description.to_owned(),
            tags: vec!["CTX".to_owned()],
            inferred_date: String::new(),
            inferred_lang: language.to_owned(),
            key_points: vec![description.to_owned()],
            audience: "Readers".to_owned(),
        }
    }

    #[test]
    fn documents_belong_to_their_owner_and_keep_published_revisions_immutable() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let journal_mode: String = database
            .connection()
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");

        let created = database
            .create_document(&NewDocument {
                author_id: "author-a",
                title: "First note",
                source_language: "zh-CN",
                content: "# First note",
                message: "Created document",
            })
            .unwrap();
        assert_eq!(created.document.owner_id, "author-a");
        assert!(database.list_documents("author-b").unwrap().is_empty());

        let metadata = json!({"topic": "testing"});
        let saved = database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Published note",
                source_language: "zh-CN",
                content: "# Published note",
                message: "Ready to publish",
                metadata: &metadata,
            })
            .unwrap()
            .unwrap();
        let published = database
            .publish_document(&created.document.id, &saved.revision.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            published.published_revision_id.as_deref(),
            Some(saved.revision.id.as_str())
        );

        let later_metadata = json!({"topic": "later"});
        database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Later draft",
                source_language: "en-US",
                content: "# Later draft",
                message: "Continue editing",
                metadata: &later_metadata,
            })
            .unwrap();

        let public = database
            .public_document(&created.document.id, None)
            .unwrap()
            .unwrap();
        assert_eq!(public.title, "Published note");
        assert_eq!(public.content, "# Published note");
        assert_eq!(public.source_language, "zh-CN");
        assert_eq!(public.language, "zh-CN");
        assert_eq!(public.available_languages, vec!["zh-CN"]);
        assert_eq!(
            database
                .get_document(&created.document.id)
                .unwrap()
                .unwrap()
                .document
                .source_language,
            "en-US"
        );
        assert_eq!(database.public_documents(None).unwrap().len(), 1);
    }

    #[test]
    fn publishing_queues_metadata_and_readers_request_localized_markdown_and_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = database
            .create_document(&NewDocument {
                author_id: "author-a",
                title: "Original",
                source_language: "zh-CN",
                content: "# 原文\n\n- [x] 已完成",
                message: "Created document",
            })
            .unwrap();
        let source = database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Original",
                source_language: "zh-CN",
                content: "# 原文\n\n- [x] 已完成",
                message: "Ready to publish",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        let published = database
            .publish_document(&created.document.id, &source.revision.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            published.published_revision_id.as_deref(),
            Some(source.revision.id.as_str())
        );
        assert_eq!(
            published.published_source_language.as_deref(),
            Some("zh-CN")
        );

        let requests = database.ai_requests().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].task, "metadata");
        assert_eq!(requests[0].target_language, "");
        assert_eq!(requests[0].status, "queued");

        let source_public = database
            .public_document(&created.document.id, None)
            .unwrap()
            .unwrap();
        assert_eq!(source_public.owner_id, "author-a");
        assert_eq!(source_public.language, "zh-CN");
        assert_eq!(source_public.available_languages, vec!["zh-CN"]);
        assert!(source_public.metadata.is_empty());
        assert_eq!(source_public.metadata_status.as_deref(), Some("queued"));
        let english_public = database
            .public_document(&created.document.id, Some("en-US"))
            .unwrap()
            .unwrap();
        assert_eq!(english_public.language, "zh-CN");
        assert!(english_public.is_translation_fallback);
        assert_eq!(english_public.requested_language.as_deref(), Some("en-US"));
        assert_eq!(english_public.content, "# 原文\n\n- [x] 已完成");
        assert_eq!(
            database
                .request_public_translation(&created.document.id, "en-US")
                .unwrap()
                .as_deref(),
            Some("queued")
        );

        let source_metadata = metadata("zh-CN", "原文摘要");
        database
            .apply_published_metadata(
                &created.document.id,
                &source.revision.id,
                &source_metadata,
                "zh-CN",
            )
            .unwrap();
        database
            .store_published_metadata(
                &created.document.id,
                &source.revision.id,
                "zh-CN",
                &source_metadata,
            )
            .unwrap();
        database
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO document_translations(document_id, language, source_revision_id, title, content, published_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    created.document.id,
                    "en-US",
                    source.revision.id,
                    "Legacy title",
                    "# Legacy translation",
                    1,
                ],
            )
            .unwrap();
        let incomplete_legacy_translation = database
            .public_document(&created.document.id, Some("en-US"))
            .unwrap()
            .unwrap();
        assert_eq!(incomplete_legacy_translation.language, "zh-CN");
        assert_eq!(
            incomplete_legacy_translation.content,
            "# 原文\n\n- [x] 已完成"
        );
        assert!(incomplete_legacy_translation.is_translation_fallback);
        assert_eq!(
            database.public_documents(Some("en-US")).unwrap()[0].language,
            "zh-CN"
        );
        let english_metadata = metadata("en-US", "English description");
        assert!(
            database
                .store_published_translation(
                    &created.document.id,
                    &source.revision.id,
                    "en-US",
                    "Original",
                    "# Original\n\n- [x] Done",
                    &english_metadata,
                )
                .unwrap()
        );
        let english_public = database
            .public_document(&created.document.id, Some("en-US"))
            .unwrap()
            .unwrap();
        assert_eq!(english_public.language, "en-US");
        assert_eq!(english_public.content, "# Original\n\n- [x] Done");
        assert_eq!(english_public.metadata.description, "English description");
        assert!(!english_public.is_translation_fallback);
        assert!(!english_public.is_metadata_fallback);
        let english_square = database.public_documents(Some("en-US")).unwrap();
        assert_eq!(english_square[0].title, "Original");
        assert_eq!(
            english_square[0].metadata.short_summary,
            "English description"
        );

        let next = database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Updated original",
                source_language: "en-US",
                content: "# Updated original",
                message: "New source",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            database
                .public_document(&created.document.id, None)
                .unwrap()
                .unwrap()
                .source_language,
            "zh-CN"
        );

        database
            .publish_document(&created.document.id, &next.revision.id)
            .unwrap()
            .unwrap();
        assert!(
            database
                .public_document(&created.document.id, Some("ja-JP"))
                .unwrap()
                .unwrap()
                .is_translation_fallback
        );
        let chinese_metadata = metadata("zh-CN", "更新后的摘要");
        assert!(
            database
                .store_published_translation(
                    &created.document.id,
                    &next.revision.id,
                    "zh-CN",
                    "更新的原文",
                    "# 更新的原文",
                    &chinese_metadata,
                )
                .unwrap()
        );
        assert_eq!(
            database
                .public_document(&created.document.id, Some("zh-CN"))
                .unwrap()
                .unwrap()
                .title,
            "更新的原文"
        );
    }

    #[test]
    fn inferred_source_language_is_available_before_a_reader_requests_translation() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = database
            .create_document(&NewDocument {
                author_id: "author-a",
                title: "Untitled language",
                source_language: "und",
                content: "# こんにちは",
                message: "Created document",
            })
            .unwrap();
        let published = database
            .publish_document(&created.document.id, &created.revision.id)
            .unwrap()
            .unwrap();
        assert_eq!(published.published_source_language.as_deref(), Some("und"));
        assert_eq!(database.ai_requests().unwrap().len(), 1);
        assert_eq!(database.ai_requests().unwrap()[0].task, "metadata");

        let claimed = database.claim_next_ai_request().unwrap().unwrap();
        assert_eq!(claimed.status, "running");
        database.requeue_running_ai_requests().unwrap();
        assert_eq!(database.ai_requests().unwrap()[0].status, "queued");
        let claimed = database.claim_next_ai_request().unwrap().unwrap();
        database
            .fail_ai_request(&claimed.id, "AI is not configured")
            .unwrap();
        assert_eq!(database.ai_requests().unwrap()[0].status, "failed");
        database.requeue_failed_ai_requests().unwrap();
        assert_eq!(database.ai_requests().unwrap()[0].status, "queued");

        assert_eq!(
            database
                .apply_published_metadata(
                    &created.document.id,
                    &created.revision.id,
                    &metadata("ja-JP", "日本語の要約"),
                    "ja-JP",
                )
                .unwrap()
                .as_deref(),
            Some("ja-JP")
        );
        database
            .store_published_metadata(
                &created.document.id,
                &created.revision.id,
                "ja-JP",
                &metadata("ja-JP", "日本語の要約"),
            )
            .unwrap();
        assert_eq!(database.ai_requests().unwrap().len(), 1);
        assert_eq!(
            database
                .request_public_translation(&created.document.id, "es-ES")
                .unwrap()
                .as_deref(),
            Some("queued")
        );
        assert!(
            database.ai_requests().unwrap().iter().any(|request| {
                request.task == "translate" && request.target_language == "es-ES"
            })
        );
        assert_eq!(
            database
                .public_document(&created.document.id, None)
                .unwrap()
                .unwrap()
                .source_language,
            "ja-JP"
        );
    }

    #[test]
    fn publication_rejects_a_source_revision_that_changed_while_translating() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = database
            .create_document(&NewDocument {
                author_id: "author-a",
                title: "Original",
                source_language: "en-US",
                content: "# Original",
                message: "Created document",
            })
            .unwrap();
        let source = database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Original",
                source_language: "en-US",
                content: "# Original",
                message: "Source for translation",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        database
            .save_document(&DocumentSave {
                id: &created.document.id,
                author_id: "author-a",
                title: "Changed while translating",
                source_language: "en-US",
                content: "# Changed",
                message: "New revision",
                metadata: &json!({}),
            })
            .unwrap();
        assert!(
            database
                .publish_document(&created.document.id, &source.revision.id)
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .public_document(&created.document.id, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn direct_document_migration_adds_language_snapshots_without_guessing_old_content() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ctx.sqlite3");
        let legacy = Connection::open(&database_path).unwrap();
        legacy
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE documents (
                    id TEXT PRIMARY KEY NOT NULL,
                    owner_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    visibility TEXT NOT NULL DEFAULT 'private',
                    metadata_json TEXT NOT NULL,
                    current_revision_id TEXT NOT NULL,
                    published_revision_id TEXT,
                    published_title TEXT,
                    published_at INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE document_revisions (
                    id TEXT PRIMARY KEY NOT NULL,
                    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                    content TEXT NOT NULL,
                    message TEXT NOT NULL,
                    author_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE ai_runs (
                    id TEXT PRIMARY KEY NOT NULL,
                    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
                    task TEXT NOT NULL,
                    output TEXT NOT NULL,
                    proposed_content TEXT,
                    created_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO documents(id, owner_id, title, status, visibility, metadata_json, current_revision_id, published_revision_id, published_title, published_at, created_at, updated_at) VALUES ('published', 'author-a', 'Current draft title', 'published', 'public', '{\"topic\":\"migration\"}', 'published-draft', 'published-source', 'Published title', 10, 1, 11), ('draft', 'author-b', 'Draft title', 'draft', 'private', '{}', 'draft-source', NULL, NULL, NULL, 2, 2)",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES ('published-source', 'published', '# Published', 'Published', 'author-a', 10), ('published-draft', 'published', '# Draft', 'Saved', 'author-a', 11), ('draft-source', 'draft', '# Draft', 'Created', 'author-b', 2)",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO ai_runs(id, document_id, source_revision_id, task, output, proposed_content, created_at) VALUES ('run-1', 'published', 'published-source', 'summary', 'Summary', NULL, 11)",
                [],
            )
            .unwrap();
        drop(legacy);

        let database = Database::open(directory.path()).unwrap();
        let published = database
            .get_document("published")
            .unwrap()
            .unwrap()
            .document;
        let draft = database.get_document("draft").unwrap().unwrap().document;
        assert_eq!(published.source_language, "und");
        assert_eq!(published.published_source_language.as_deref(), Some("und"));
        assert_eq!(draft.source_language, "und");
        assert!(draft.published_source_language.is_none());
        let public = database
            .public_document("published", None)
            .unwrap()
            .unwrap();
        assert_eq!(public.title, "Published title");
        assert_eq!(public.content, "# Published");
        assert_eq!(public.source_language, "und");
        let connection = database.connection().unwrap();
        let translation_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'document_translations')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(translation_table_exists);
        let ai_run_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ai_run_count, 1);
        let mut foreign_key_check = connection.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(
            foreign_key_check
                .query([])
                .unwrap()
                .next()
                .unwrap()
                .is_none()
        );
        drop(foreign_key_check);
        drop(connection);
        drop(database);

        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .get_document("published")
                .unwrap()
                .unwrap()
                .document
                .published_source_language
                .as_deref(),
            Some("und")
        );
        assert_eq!(reopened.public_documents(None).unwrap().len(), 1);
    }

    #[test]
    fn legacy_context_migration_preserves_owners_and_private_publication_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_path = directory.path().join("ctx.sqlite3");
        let legacy = Connection::open(&legacy_path).unwrap();
        legacy
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE contexts (
                    id TEXT PRIMARY KEY NOT NULL,
                    owner_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    slug TEXT NOT NULL UNIQUE,
                    description TEXT NOT NULL,
                    instructions TEXT NOT NULL,
                    visibility TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE documents (
                    id TEXT PRIMARY KEY NOT NULL,
                    context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
                    title TEXT NOT NULL,
                    slug TEXT NOT NULL,
                    language TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    current_revision_id TEXT NOT NULL,
                    published_revision_id TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE document_revisions (
                    id TEXT PRIMARY KEY NOT NULL,
                    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                    content TEXT NOT NULL,
                    message TEXT NOT NULL,
                    author_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE ai_runs (
                    id TEXT PRIMARY KEY NOT NULL,
                    context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
                    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
                    task TEXT NOT NULL,
                    output TEXT NOT NULL,
                    proposed_content TEXT,
                    created_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO contexts(id, owner_id, name, slug, description, instructions, visibility, created_at, updated_at) VALUES ('public-context', 'author-a', 'Public', 'public', '', '', 'public', 1, 1), ('private-context', 'author-b', 'Private', 'private', '', '', 'private', 1, 1)",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO documents(id, context_id, title, slug, language, kind, status, metadata_json, current_revision_id, published_revision_id, created_at, updated_at) VALUES ('public-document', 'public-context', 'Public note', 'public-note', 'en', 'docs', 'published', '{}', 'public-revision', 'public-revision', 2, 2), ('private-document', 'private-context', 'Private note', 'private-note', 'en', 'docs', 'published', '{}', 'private-revision', 'private-revision', 3, 3), ('changed-document', 'public-context', 'Unpublished title', 'changed-note', 'en', 'docs', 'published', '{}', 'changed-draft-revision', 'changed-published-revision', 4, 5)",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES ('public-revision', 'public-document', '# Public', 'Created', 'author-a', 2), ('private-revision', 'private-document', '# Private', 'Created', 'author-b', 3), ('changed-published-revision', 'changed-document', '# Original public title', 'Published', 'author-a', 4), ('changed-draft-revision', 'changed-document', '# Draft title', 'Saved draft', 'author-a', 5)",
                [],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO ai_runs(id, context_id, document_id, source_revision_id, task, output, proposed_content, created_at) VALUES ('run-1', 'public-context', 'public-document', 'public-revision', 'summary', 'Summary', NULL, 4)",
                [],
            )
            .unwrap();
        drop(legacy);

        let database = Database::open(directory.path()).unwrap();
        assert_eq!(
            database
                .get_document("public-document")
                .unwrap()
                .unwrap()
                .document
                .owner_id,
            "author-a"
        );
        assert_eq!(
            database
                .get_document("private-document")
                .unwrap()
                .unwrap()
                .document
                .owner_id,
            "author-b"
        );
        let public_documents = database.public_documents(None).unwrap();
        assert_eq!(public_documents.len(), 2);
        assert!(
            public_documents
                .iter()
                .any(|document| document.title == "Public note" && document.published_at == 2)
        );
        assert!(public_documents.iter().any(|document| {
            document.title == "Published document" && document.published_at == 4
        }));
        assert!(
            database
                .public_document("private-document", None)
                .unwrap()
                .is_none()
        );
        let changed_public_document = database
            .public_document("changed-document", None)
            .unwrap()
            .unwrap();
        assert_eq!(changed_public_document.title, "Published document");
        assert_eq!(changed_public_document.content, "# Original public title");
        assert_eq!(changed_public_document.published_at, 4);
        let public_document = database
            .get_document("public-document")
            .unwrap()
            .unwrap()
            .document;
        let private_document = database
            .get_document("private-document")
            .unwrap()
            .unwrap()
            .document;
        let changed_document = database
            .get_document("changed-document")
            .unwrap()
            .unwrap()
            .document;
        let connection = database.connection().unwrap();
        assert_eq!(public_document.status, "published");
        assert_eq!(public_document.source_language, "und");
        assert_eq!(
            public_document.published_source_language.as_deref(),
            Some("und")
        );
        assert_eq!(
            public_document.published_revision_id.as_deref(),
            Some("public-revision")
        );
        assert_eq!(private_document.status, "draft");
        assert_eq!(private_document.source_language, "und");
        assert!(private_document.published_source_language.is_none());
        assert!(private_document.published_revision_id.is_none());
        assert_eq!(changed_document.status, "published");
        assert_eq!(
            changed_document.published_source_language.as_deref(),
            Some("und")
        );
        assert_eq!(
            changed_document.published_revision_id.as_deref(),
            Some("changed-published-revision")
        );
        for (document_id, revision_id) in [
            (
                "public-document",
                public_document.current_revision_id.as_str(),
            ),
            (
                "public-document",
                public_document.published_revision_id.as_deref().unwrap(),
            ),
            (
                "private-document",
                private_document.current_revision_id.as_str(),
            ),
            (
                "changed-document",
                changed_document.current_revision_id.as_str(),
            ),
            (
                "changed-document",
                changed_document.published_revision_id.as_deref().unwrap(),
            ),
        ] {
            let revision_document_id: String = connection
                .query_row(
                    "SELECT document_id FROM document_revisions WHERE id = ?1",
                    [revision_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(revision_document_id, document_id);
        }
        let contexts_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'contexts')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!contexts_exists);
        let ai_run_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ai_run_count, 1);
        let mut statement = connection.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(statement.query([]).unwrap().next().unwrap().is_none());
        drop(statement);
        drop(connection);
        drop(database);

        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(reopened.list_documents("author-a").unwrap().len(), 2);
        assert_eq!(reopened.public_documents(None).unwrap().len(), 2);
    }
}
