use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use fractional_index::FractionalIndex;
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
        document_kind TEXT NOT NULL DEFAULT 'article' CHECK(document_kind IN ('article', 'profile')),
        parent_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
        sort_key TEXT NOT NULL DEFAULT '',
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
        task TEXT NOT NULL CHECK(task IN ('metadata', 'translate', 'profile_experience', 'profile_personality', 'profile_mbti', 'profile_schwartz', 'profile_motivations', 'profile_philosophy', 'profile_timeline')),
        target_language TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
        result_summary TEXT,
        error TEXT,
        openai_lb_request_id TEXT,
        created_at INTEGER NOT NULL,
        started_at INTEGER,
        completed_at INTEGER,
        UNIQUE(document_id, source_revision_id, task, target_language)
    );
    CREATE TABLE IF NOT EXISTS document_comments (
        id TEXT PRIMARY KEY NOT NULL,
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        source_revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
        author_id TEXT NOT NULL,
        language TEXT NOT NULL,
        content TEXT NOT NULL,
        quote TEXT,
        prefix TEXT,
        suffix TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS document_readers (
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        user_id TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (document_id, user_id)
    );
    CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY NOT NULL,
        user_id TEXT NOT NULL,
        label TEXT NOT NULL,
        prefix TEXT NOT NULL,
        secret_hash TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        last_used_at INTEGER,
        revoked_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS document_view_stats (
        document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        views_total INTEGER NOT NULL DEFAULT 0,
        views_human INTEGER NOT NULL DEFAULT 0,
        views_unique_human INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS document_view_daily (
        document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        day TEXT NOT NULL,
        views_total INTEGER NOT NULL DEFAULT 0,
        views_human INTEGER NOT NULL DEFAULT 0,
        views_unique_human INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (document_id, day)
    );
";

const CURRENT_INDEXES_SQL: &str = "
    CREATE INDEX IF NOT EXISTS documents_owner_idx ON documents(owner_id, updated_at DESC);
    CREATE INDEX IF NOT EXISTS documents_tree_idx ON documents(owner_id, parent_id, sort_key);
    CREATE INDEX IF NOT EXISTS documents_public_idx ON documents(visibility, status, published_at DESC);
    CREATE UNIQUE INDEX IF NOT EXISTS documents_owner_profile_unique ON documents(owner_id) WHERE document_kind = 'profile';
    CREATE INDEX IF NOT EXISTS document_revisions_document_idx ON document_revisions(document_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS ai_runs_document_idx ON ai_runs(document_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS document_translations_document_idx ON document_translations(document_id, language);
    CREATE INDEX IF NOT EXISTS document_metadata_document_idx ON document_metadata(document_id, language);
    CREATE INDEX IF NOT EXISTS ai_requests_status_idx ON ai_requests(status, created_at, id);
    CREATE INDEX IF NOT EXISTS ai_requests_document_idx ON ai_requests(document_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS document_comments_document_idx ON document_comments(document_id, source_revision_id, language, created_at, id);
    CREATE INDEX IF NOT EXISTS document_readers_user_idx ON document_readers(user_id, document_id);
    CREATE INDEX IF NOT EXISTS api_keys_user_idx ON api_keys(user_id, created_at DESC);
";

const DOCUMENT_DETAIL_SQL: &str = "SELECT d.id, d.owner_id, d.title, d.source_language, d.status, d.visibility, d.document_kind, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, d.parent_id, d.sort_key, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = d.current_revision_id AND r.document_id = d.id WHERE d.id = ?1";

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
    #[error("stored document order key is invalid")]
    InvalidOrderKey,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Document {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub source_language: String,
    pub status: String,
    pub kind: String,
    pub visibility: String,
    pub metadata: Value,
    pub current_revision_id: String,
    pub published_revision_id: Option<String>,
    pub published_source_language: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub parent_id: Option<String>,
    pub sort_key: String,
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
    pub descendant_count: i64,
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
    pub visibility: String,
    pub available_languages: Vec<String>,
    pub metadata: PublishedMetadata,
    pub is_metadata_fallback: bool,
    pub metadata_status: Option<String>,
    pub requested_language: Option<String>,
    pub translation_status: Option<String>,
    pub is_translation_fallback: bool,
    pub published_at: i64,
    pub views: DocumentViewStats,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct DocumentViewStats {
    pub total: u64,
    pub human: u64,
    pub unique_human: u64,
}

#[derive(Clone, Debug)]
pub struct ViewDelta {
    pub day: String,
    pub document_id: String,
    pub total: u64,
    pub human: u64,
    pub unique_human: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicUserProfile {
    pub owner_id: String,
    pub profile: Option<PublicDocumentDetail>,
    pub published_article_dates: Vec<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SummaryEvidence {
    #[serde(default)]
    pub article_title: String,
    #[serde(default)]
    pub article_url: String,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MbtiDimension {
    #[serde(default)]
    pub axis: String,
    #[serde(default)]
    pub preference: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub evidence: Vec<SummaryEvidence>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MbtiAnalysis {
    #[serde(default)]
    pub type_code: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub dimensions: Vec<MbtiDimension>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SchwartzValue {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub score: u8,
    #[serde(default)]
    pub rank: u8,
    #[serde(default)]
    pub evidence: Vec<SummaryEvidence>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DailyTimelineEntry {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<SummaryEvidence>,
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
    #[serde(default)]
    pub experience_summary: String,
    #[serde(default)]
    pub personality_analysis: String,
    #[serde(default)]
    pub mbti_analysis: MbtiAnalysis,
    #[serde(default)]
    pub schwartz_values: Vec<SchwartzValue>,
    #[serde(default)]
    pub unconscious_motivations: String,
    #[serde(default)]
    pub philosophical_references: String,
    #[serde(default)]
    pub daily_timeline: Vec<DailyTimelineEntry>,
}

impl PublishedMetadata {
    pub fn is_empty(&self) -> bool {
        self.description.is_empty()
            && self.summary.is_empty()
            && self.short_summary.is_empty()
            && self.tags.is_empty()
            && self.key_points.is_empty()
            && self.audience.is_empty()
            && self.experience_summary.is_empty()
            && self.personality_analysis.is_empty()
            && self.mbti_analysis.type_code.is_empty()
            && self.schwartz_values.is_empty()
            && self.unconscious_motivations.is_empty()
            && self.philosophical_references.is_empty()
            && self.daily_timeline.is_empty()
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
    pub openai_lb_request_id: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

pub const PROFILE_SUMMARY_TASKS: [&str; 7] = [
    "profile_experience",
    "profile_personality",
    "profile_mbti",
    "profile_schwartz",
    "profile_motivations",
    "profile_philosophy",
    "profile_timeline",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentReader {
    pub user_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKey {
    pub id: String,
    pub label: String,
    pub prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentComment {
    pub id: String,
    pub document_id: String,
    pub author_id: String,
    pub language: String,
    pub content: String,
    pub quote: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct SqlitePageUsage {
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_count: u64,
}

#[derive(Clone, Debug)]
pub struct NewDocument<'a> {
    pub author_id: &'a str,
    pub title: &'a str,
    pub source_language: &'a str,
    pub content: &'a str,
    pub message: &'a str,
    pub parent_id: Option<&'a str>,
}

#[derive(Debug)]
pub enum CreateDocumentOutcome {
    Created(Box<DocumentDetail>),
    MissingParent,
    InvalidParent,
}

#[derive(Clone, Copy, Debug)]
pub struct DocumentMove<'a> {
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub after_id: Option<&'a str>,
}

#[derive(Debug)]
pub enum MoveDocumentOutcome {
    Moved(Box<DocumentDetail>),
    Missing,
    InvalidTarget,
}

#[derive(Debug)]
pub enum ReadDocumentOutcome {
    Found(Box<PublicDocumentDetail>),
    Private,
    Missing,
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

#[derive(Debug)]
pub struct NewDocumentComment<'a> {
    pub document_id: &'a str,
    pub author_id: &'a str,
    pub language: &'a str,
    pub content: &'a str,
    pub quote: Option<&'a str>,
    pub prefix: Option<&'a str>,
    pub suffix: Option<&'a str>,
}

#[derive(Clone, Debug)]
struct PublishedDocument {
    id: String,
    owner_id: String,
    title: String,
    content: String,
    source_language: String,
    visibility: String,
    source_revision_id: String,
    published_at: i64,
}

#[derive(Clone, Debug)]
pub struct PublishedArticle {
    pub id: String,
    pub title: String,
    pub content: String,
    pub inferred_date: String,
    pub published_date: String,
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
        add_document_tree_columns_if_needed(&connection)?;
        connection.execute_batch(CURRENT_TABLES_SQL)?;
        migrate_ai_request_task_constraint(&mut connection)?;
        add_ai_request_openai_lb_request_id_if_needed(&connection)?;
        connection.execute_batch(CURRENT_INDEXES_SQL)?;
        connection.execute("DROP TABLE IF EXISTS author_language_preferences", [])?;
        backfill_published_source_languages(&connection)?;
        backfill_published_metadata_requests(&mut connection)?;
        backfill_profile_summaries(&mut connection)?;
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

    pub fn media_directory(&self) -> PathBuf {
        self.database_path.parent().map_or_else(
            || PathBuf::from("media"),
            |directory| directory.join("media"),
        )
    }

    pub fn database_path(&self) -> PathBuf {
        self.database_path.as_ref().clone()
    }

    pub fn sqlite_page_usage(&self) -> Result<SqlitePageUsage, DatabaseError> {
        let connection = self.connection()?;
        let (page_size, page_count, freelist_count): (i64, i64, i64) = connection.query_row(
            "SELECT (SELECT * FROM pragma_page_size()), (SELECT * FROM pragma_page_count()), (SELECT * FROM pragma_freelist_count())",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(SqlitePageUsage {
            page_size: sqlite_page_value(page_size)?,
            page_count: sqlite_page_value(page_count)?,
            freelist_count: sqlite_page_value(freelist_count)?,
        })
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

    pub fn create_api_key(
        &self,
        user_id: &str,
        label: &str,
        prefix: &str,
        secret_hash: &str,
    ) -> Result<ApiKey, DatabaseError> {
        let key = ApiKey {
            id: Uuid::new_v4().to_string(),
            label: label.to_owned(),
            prefix: prefix.to_owned(),
            created_at: now(),
            last_used_at: None,
        };
        self.connection()?.execute(
            "INSERT INTO api_keys(id, user_id, label, prefix, secret_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![key.id, user_id, key.label, key.prefix, secret_hash, key.created_at],
        )?;
        Ok(key)
    }

    pub fn list_api_keys(&self, user_id: &str) -> Result<Vec<ApiKey>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, label, prefix, created_at, last_used_at FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([user_id], api_key_from_row)?;
        let mut keys = Vec::new();
        for row in rows {
            keys.push(row?);
        }
        Ok(keys)
    }

    pub fn revoke_api_key(&self, user_id: &str, id: &str) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE api_keys SET revoked_at = ?3 WHERE id = ?1 AND user_id = ?2 AND revoked_at IS NULL",
            params![id, user_id, now()],
        )?;
        Ok(changed == 1)
    }

    pub fn authenticate_api_key(
        &self,
        user_id: &str,
        secret_hash: &str,
    ) -> Result<Option<String>, DatabaseError> {
        let connection = self.connection()?;
        let key_id: Option<String> = connection
            .query_row(
                "SELECT id FROM api_keys WHERE user_id = ?1 AND secret_hash = ?2 AND revoked_at IS NULL",
                params![user_id, secret_hash],
                |row| row.get(0),
            )
            .optional()?;
        let Some(key_id) = key_id else {
            return Ok(None);
        };
        connection.execute(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            params![key_id, now()],
        )?;
        Ok(Some(key_id))
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
    ) -> Result<CreateDocumentOutcome, DatabaseError> {
        let document_id = Uuid::new_v4().to_string();
        let revision = DocumentRevision {
            id: Uuid::new_v4().to_string(),
            document_id: document_id.clone(),
            content: input.content.to_owned(),
            message: input.message.to_owned(),
            author_id: input.author_id.to_owned(),
            created_at: now(),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(parent_id) = input.parent_id {
            let parent = transaction
                .query_row(
                    "SELECT owner_id, document_kind FROM documents WHERE id = ?1",
                    [parent_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((owner_id, document_kind)) = parent else {
                return Ok(CreateDocumentOutcome::MissingParent);
            };
            if owner_id != input.author_id || document_kind != "article" {
                return Ok(CreateDocumentOutcome::InvalidParent);
            }
        }
        let document = Document {
            id: document_id,
            owner_id: input.author_id.to_owned(),
            title: input.title.to_owned(),
            source_language: input.source_language.to_owned(),
            status: "draft".to_owned(),
            kind: "article".to_owned(),
            visibility: "private".to_owned(),
            metadata: Value::Object(Default::default()),
            current_revision_id: revision.id.clone(),
            published_revision_id: None,
            published_source_language: None,
            created_at: revision.created_at,
            updated_at: revision.created_at,
            parent_id: input.parent_id.map(str::to_owned),
            sort_key: append_document_sort_key(&transaction, input.author_id, input.parent_id)?,
        };
        transaction.execute(
            "INSERT INTO documents(id, owner_id, title, source_language, status, visibility, document_kind, parent_id, sort_key, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12, ?13)",
            params![document.id, document.owner_id, document.title, document.source_language, document.status, document.visibility, document.kind, document.parent_id, document.sort_key, document.metadata.to_string(), document.current_revision_id, document.created_at, document.updated_at],
        )?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.id, revision.document_id, revision.content, revision.message, revision.author_id, revision.created_at],
        )?;
        transaction.commit()?;
        Ok(CreateDocumentOutcome::Created(Box::new(DocumentDetail {
            document,
            revision,
            descendant_count: 0,
        })))
    }

    pub fn list_documents(&self, owner_id: &str) -> Result<Vec<Document>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, owner_id, title, source_language, status, visibility, document_kind, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at, parent_id, sort_key FROM documents WHERE owner_id = ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        Ok(statement
            .query_map([owner_id], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn profile_document(&self, owner_id: &str) -> Result<DocumentDetail, DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT d.id, d.owner_id, d.title, d.source_language, d.status, d.visibility, d.document_kind, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, d.parent_id, d.sort_key, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = d.current_revision_id AND r.document_id = d.id WHERE d.owner_id = ?1 AND d.document_kind = 'profile'",
                [owner_id],
                document_detail_from_row,
            )
            .optional()?;
        if let Some(existing) = existing {
            transaction.commit()?;
            return Ok(existing);
        }
        let document_id = Uuid::new_v4().to_string();
        let created_at = now();
        let revision = DocumentRevision {
            id: Uuid::new_v4().to_string(),
            document_id: document_id.clone(),
            content: String::new(),
            message: "Created profile document".to_owned(),
            author_id: owner_id.to_owned(),
            created_at,
        };
        let document = Document {
            id: document_id,
            owner_id: owner_id.to_owned(),
            title: "Profile".to_owned(),
            source_language: "und".to_owned(),
            status: "draft".to_owned(),
            kind: "profile".to_owned(),
            visibility: "private".to_owned(),
            metadata: Value::Object(Default::default()),
            current_revision_id: revision.id.clone(),
            published_revision_id: None,
            published_source_language: None,
            created_at,
            updated_at: created_at,
            parent_id: None,
            sort_key: append_document_sort_key(&transaction, owner_id, None)?,
        };
        transaction.execute(
            "INSERT INTO documents(id, owner_id, title, source_language, status, visibility, document_kind, parent_id, sort_key, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?11, ?12)",
            params![document.id, document.owner_id, document.title, document.source_language, document.status, document.visibility, document.kind, document.sort_key, document.metadata.to_string(), document.current_revision_id, document.created_at, document.updated_at],
        )?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.id, revision.document_id, revision.content, revision.message, revision.author_id, revision.created_at],
        )?;
        transaction.commit()?;
        Ok(DocumentDetail {
            document,
            revision,
            descendant_count: 0,
        })
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentDetail>, DatabaseError> {
        let connection = self.connection()?;
        let document = connection
            .query_row(DOCUMENT_DETAIL_SQL, [id], document_detail_from_row)
            .optional()?;
        let Some(mut document) = document else {
            return Ok(None);
        };
        document.descendant_count = count_descendants(&connection, id)?;
        Ok(Some(document))
    }

    pub fn delete_document(&self, id: &str) -> Result<bool, DatabaseError> {
        Ok(self
            .connection()?
            .execute("DELETE FROM documents WHERE id = ?1", [id])?
            == 1)
    }

    pub fn move_document(
        &self,
        input: &DocumentMove<'_>,
    ) -> Result<MoveDocumentOutcome, DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let moving = transaction
            .query_row(
                "SELECT owner_id, document_kind FROM documents WHERE id = ?1",
                [input.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((owner_id, document_kind)) = moving else {
            return Ok(MoveDocumentOutcome::Missing);
        };
        if document_kind != "article" {
            return Ok(MoveDocumentOutcome::InvalidTarget);
        }
        if let Some(parent_id) = input.parent_id {
            let parent = transaction
                .query_row(
                    "SELECT owner_id, document_kind FROM documents WHERE id = ?1",
                    [parent_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((parent_owner_id, parent_kind)) = parent else {
                return Ok(MoveDocumentOutcome::InvalidTarget);
            };
            if parent_owner_id != owner_id
                || parent_kind != "article"
                || is_descendant(&transaction, input.id, parent_id)?
            {
                return Ok(MoveDocumentOutcome::InvalidTarget);
            }
        }
        let Some(sort_key) = sibling_sort_key(
            &transaction,
            &owner_id,
            input.parent_id,
            input.after_id,
            input.id,
        )?
        else {
            return Ok(MoveDocumentOutcome::InvalidTarget);
        };
        transaction.execute(
            "UPDATE documents SET parent_id = ?2, sort_key = ?3 WHERE id = ?1",
            params![input.id, input.parent_id, sort_key],
        )?;
        let mut document =
            transaction.query_row(DOCUMENT_DETAIL_SQL, [input.id], document_detail_from_row)?;
        document.descendant_count = count_descendants(&transaction, input.id)?;
        transaction.commit()?;
        Ok(MoveDocumentOutcome::Moved(Box::new(document)))
    }

    pub fn publication_time(&self, id: &str) -> Result<Option<i64>, DatabaseError> {
        self.connection()?
            .query_row(
                "SELECT published_at FROM documents WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(DatabaseError::Sqlite)
    }

    pub fn update_publication_time(
        &self,
        id: &str,
        published_at: i64,
    ) -> Result<Option<i64>, DatabaseError> {
        let updated = self.connection()?.execute(
            "UPDATE documents SET published_at = ?2 WHERE id = ?1",
            params![id, published_at],
        )?;
        Ok((updated == 1).then_some(published_at))
    }

    pub fn save_document(
        &self,
        input: &DocumentSave<'_>,
    ) -> Result<Option<DocumentDetail>, DatabaseError> {
        let Some(existing) = self.get_document(input.id)? else {
            return Ok(None);
        };
        let descendant_count = existing.descendant_count;
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
        Ok(Some(DocumentDetail {
            document,
            revision,
            descendant_count,
        }))
    }

    pub fn publish_document(
        &self,
        id: &str,
        source_revision_id: &str,
        visibility: &str,
    ) -> Result<Option<Document>, DatabaseError> {
        let Some(mut detail) = self.get_document(id)? else {
            return Ok(None);
        };
        let source_language = detail.document.source_language.clone();
        let updated_at = now();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE documents SET status = 'published', visibility = ?5, published_revision_id = ?2, published_title = title, published_at = COALESCE(published_at, ?3), source_language = ?4, published_source_language = ?4, updated_at = ?3 WHERE id = ?1 AND current_revision_id = ?2",
            params![id, source_revision_id, updated_at, source_language, visibility],
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
        detail.document.visibility = visibility.to_owned();
        detail.document.source_language = source_language.clone();
        detail.document.published_revision_id = Some(source_revision_id.to_owned());
        detail.document.published_source_language = Some(source_language);
        detail.document.updated_at = updated_at;
        Ok(Some(detail.document))
    }

    pub fn enqueue_profile_summary_tasks(
        &self,
        document_id: &str,
        source_revision_id: &str,
        task: Option<&str>,
    ) -> Result<Vec<String>, DatabaseError> {
        let tasks = match task {
            None | Some("all") => PROFILE_SUMMARY_TASKS.to_vec(),
            Some(task) if PROFILE_SUMMARY_TASKS.contains(&task) => vec![task],
            Some(_) => return Ok(vec![]),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for task in &tasks {
            force_enqueue_ai_request(
                &transaction,
                document_id,
                source_revision_id,
                task,
                "",
                now(),
            )?;
        }
        transaction.commit()?;
        Ok(tasks.into_iter().map(str::to_owned).collect())
    }

    pub fn apply_profile_summary(
        &self,
        document_id: &str,
        source_revision_id: &str,
        source_language: &str,
        metadata: &PublishedMetadata,
    ) -> Result<bool, DatabaseError> {
        let metadata_json = serde_json::to_string(metadata)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE documents SET metadata_json = ?3, updated_at = ?4 WHERE id = ?1 AND published_revision_id = ?2 AND document_kind = 'profile' AND status = 'published'",
            params![document_id, source_revision_id, metadata_json, now()],
        )?;
        if changed == 0 {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "INSERT INTO document_metadata(document_id, language, source_revision_id, metadata_json, published_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, metadata_json = excluded.metadata_json, published_at = excluded.published_at",
            params![document_id, source_language, source_revision_id, metadata_json, now()],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn requeue_running_ai_requests(&self) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'queued', started_at = NULL, openai_lb_request_id = NULL WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    pub fn requeue_failed_ai_requests(&self) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'queued', result_summary = NULL, error = NULL, openai_lb_request_id = NULL, started_at = NULL, completed_at = NULL WHERE status = 'failed'",
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
            "UPDATE ai_requests SET status = 'running', started_at = ?2, completed_at = NULL, result_summary = NULL, error = NULL, openai_lb_request_id = NULL WHERE id = ?1 AND status = 'queued'",
            params![request_id, started_at],
        )?;
        let request = transaction.query_row(
            "SELECT r.id, r.document_id, COALESCE(d.published_title, d.title), r.source_revision_id, r.task, r.target_language, r.status, r.result_summary, r.error, r.openai_lb_request_id, r.created_at, r.started_at, r.completed_at FROM ai_requests r JOIN documents d ON d.id = r.document_id WHERE r.id = ?1",
            [request_id],
            ai_request_from_row,
        )?;
        transaction.commit()?;
        Ok(Some(request))
    }

    pub fn complete_ai_request(
        &self,
        id: &str,
        summary: &str,
        openai_lb_request_id: Option<&str>,
    ) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'succeeded', result_summary = ?2, error = NULL, openai_lb_request_id = ?3, completed_at = ?4 WHERE id = ?1",
            params![id, summary, openai_lb_request_id, now()],
        )?;
        Ok(())
    }

    pub fn fail_ai_request(
        &self,
        id: &str,
        error: &str,
        openai_lb_request_id: Option<&str>,
    ) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "UPDATE ai_requests SET status = 'failed', result_summary = NULL, error = ?2, openai_lb_request_id = ?3, completed_at = ?4 WHERE id = ?1",
            params![id, error, openai_lb_request_id, now()],
        )?;
        Ok(())
    }

    pub fn ai_requests(&self) -> Result<Vec<AiRequest>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT r.id, r.document_id, COALESCE(d.published_title, d.title), r.source_revision_id, r.task, r.target_language, r.status, r.result_summary, r.error, r.openai_lb_request_id, r.created_at, r.started_at, r.completed_at FROM ai_requests r JOIN documents d ON d.id = r.document_id ORDER BY r.created_at DESC, r.id DESC LIMIT 200",
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
                "SELECT d.id, d.owner_id, d.published_title, r.content, d.published_source_language, d.visibility, d.published_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.document_kind = 'article' AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL ORDER BY d.published_at DESC, d.id DESC",
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

    pub fn read_document(
        &self,
        id: &str,
        requested_language: Option<&str>,
        viewer: Option<&str>,
    ) -> Result<ReadDocumentOutcome, DatabaseError> {
        let connection = self.connection()?;
        let publication = connection
            .query_row(
                "SELECT d.id, d.owner_id, d.published_title, r.content, d.published_source_language, d.visibility, d.published_revision_id, d.published_at, CASE WHEN d.visibility = 'public' OR d.owner_id = ?2 OR EXISTS (SELECT 1 FROM document_readers rd WHERE rd.document_id = d.id AND rd.user_id = ?2) THEN 1 ELSE 0 END FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.id = ?1 AND d.document_kind = 'article' AND d.status = 'published' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL",
                params![id, viewer],
                |row| Ok((published_document_from_row(row)?, row.get::<_, bool>(8)?)),
            )
            .optional()?;
        let Some((publication, can_read)) = publication else {
            return Ok(ReadDocumentOutcome::Missing);
        };
        if !can_read {
            return Ok(ReadDocumentOutcome::Private);
        }
        Ok(ReadDocumentOutcome::Found(Box::new(
            public_document_detail(&connection, &publication, requested_language)?,
        )))
    }

    /// Flushes one batch of in-memory view deltas into `SQLite`.
    ///
    /// Deltas for documents that no longer exist are skipped instead of
    /// failing the whole batch. Deltas are additive, so a batch retried after
    /// an ambiguous failure can at worst double-count one flush window.
    ///
    /// # Errors
    ///
    /// Returns an error when the transaction cannot be committed.
    pub fn apply_view_deltas(&self, deltas: &[ViewDelta]) -> Result<(), DatabaseError> {
        if deltas.is_empty() {
            return Ok(());
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated_at = now();
        for delta in deltas {
            let total = sqlite_count(delta.total);
            let human = sqlite_count(delta.human);
            let unique_human = sqlite_count(delta.unique_human);
            transaction.execute(
                "INSERT INTO document_view_stats(document_id, views_total, views_human, views_unique_human, updated_at) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1) ON CONFLICT(document_id) DO UPDATE SET views_total = views_total + excluded.views_total, views_human = views_human + excluded.views_human, views_unique_human = views_unique_human + excluded.views_unique_human, updated_at = excluded.updated_at",
                params![delta.document_id, total, human, unique_human, updated_at],
            )?;
            transaction.execute(
                "INSERT INTO document_view_daily(document_id, day, views_total, views_human, views_unique_human) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1) ON CONFLICT(document_id, day) DO UPDATE SET views_total = views_total + excluded.views_total, views_human = views_human + excluded.views_human, views_unique_human = views_unique_human + excluded.views_unique_human",
                params![delta.document_id, delta.day, total, human, unique_human],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns the materialized view counters for one document, or zeroes
    /// when the document has never been read.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing query fails.
    pub fn view_stats(&self, document_id: &str) -> Result<DocumentViewStats, DatabaseError> {
        let connection = self.connection()?;
        view_stats(&connection, document_id)
    }

    pub fn document_readers(
        &self,
        document_id: &str,
    ) -> Result<Vec<DocumentReader>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT user_id, created_at FROM document_readers WHERE document_id = ?1 ORDER BY created_at, user_id",
        )?;
        Ok(statement
            .query_map([document_id], |row| {
                Ok(DocumentReader {
                    user_id: row.get(0)?,
                    created_at: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn grant_document_reader(
        &self,
        document_id: &str,
        user_id: &str,
    ) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "INSERT INTO document_readers(document_id, user_id, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(document_id, user_id) DO NOTHING",
            params![document_id, user_id, now()],
        )?;
        Ok(changed == 1)
    }

    pub fn revoke_document_reader(
        &self,
        document_id: &str,
        user_id: &str,
    ) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "DELETE FROM document_readers WHERE document_id = ?1 AND user_id = ?2",
            params![document_id, user_id],
        )?;
        Ok(changed == 1)
    }

    pub fn public_comments(
        &self,
        document_id: &str,
        language: &str,
    ) -> Result<Vec<DocumentComment>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT c.id, c.document_id, c.author_id, c.language, c.content, c.quote, c.prefix, c.suffix, c.created_at FROM document_comments c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ?1 AND c.language = ?2 AND c.source_revision_id = d.published_revision_id AND d.document_kind = 'article' AND d.status = 'published' AND d.visibility = 'public' ORDER BY c.created_at, c.rowid",
        )?;
        Ok(statement
            .query_map(params![document_id, language], document_comment_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_public_comment(
        &self,
        input: &NewDocumentComment<'_>,
    ) -> Result<Option<DocumentComment>, DatabaseError> {
        let connection = self.connection()?;
        let publication: Option<(String, String)> = connection
            .query_row(
                "SELECT published_revision_id, published_source_language FROM documents WHERE id = ?1 AND document_kind = 'article' AND status = 'published' AND visibility = 'public' AND published_revision_id IS NOT NULL AND published_source_language IS NOT NULL",
                [input.document_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((source_revision_id, source_language)) = publication else {
            return Ok(None);
        };
        if input.language != source_language
            && !translation_exists(
                &connection,
                input.document_id,
                &source_revision_id,
                input.language,
            )?
        {
            return Ok(None);
        }
        let comment = DocumentComment {
            id: Uuid::new_v4().to_string(),
            document_id: input.document_id.to_owned(),
            author_id: input.author_id.to_owned(),
            language: input.language.to_owned(),
            content: input.content.to_owned(),
            quote: input.quote.map(str::to_owned),
            prefix: input.prefix.map(str::to_owned),
            suffix: input.suffix.map(str::to_owned),
            created_at: now(),
        };
        connection.execute(
            "INSERT INTO document_comments(id, document_id, source_revision_id, author_id, language, content, quote, prefix, suffix, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![comment.id, comment.document_id, source_revision_id, comment.author_id, comment.language, comment.content, comment.quote, comment.prefix, comment.suffix, comment.created_at],
        )?;
        Ok(Some(comment))
    }

    pub fn public_user_profile(
        &self,
        owner_id: &str,
        requested_language: Option<&str>,
    ) -> Result<PublicUserProfile, DatabaseError> {
        let connection = self.connection()?;
        let profile = connection
            .query_row(
                "SELECT d.id, d.owner_id, d.published_title, r.content, d.published_source_language, d.visibility, d.published_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.owner_id = ?1 AND d.document_kind = 'profile' AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL",
                [owner_id],
                published_document_from_row,
            )
            .optional()?
            .map(|publication| public_document_detail(&connection, &publication, requested_language))
            .transpose()?;
        let mut statement = connection.prepare(
            "SELECT published_at FROM documents WHERE owner_id = ?1 AND document_kind = 'article' AND status = 'published' AND visibility = 'public' AND published_at IS NOT NULL ORDER BY published_at",
        )?;
        let published_article_dates = statement
            .query_map([owner_id], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PublicUserProfile {
            owner_id: owner_id.to_owned(),
            profile,
            published_article_dates,
        })
    }

    pub fn published_articles_for_author(
        &self,
        owner_id: &str,
    ) -> Result<Vec<PublishedArticle>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT d.id, d.published_title, r.content, COALESCE(m.metadata_json, ''), strftime('%Y-%m-%d', d.published_at, 'unixepoch') FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id LEFT JOIN document_metadata m ON m.document_id = d.id AND m.source_revision_id = d.published_revision_id AND m.language = d.published_source_language WHERE d.owner_id = ?1 AND d.document_kind = 'article' AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_revision_id IS NOT NULL ORDER BY d.published_at, d.id",
        )?;
        Ok(statement
            .query_map([owner_id], published_article_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn request_public_translation(
        &self,
        document_id: &str,
        target_language: &str,
    ) -> Result<Option<String>, DatabaseError> {
        let connection = self.connection()?;
        let publication: Option<(String, String)> = connection
            .query_row(
                "SELECT published_revision_id, published_source_language FROM documents WHERE id = ?1 AND status = 'published' AND published_revision_id IS NOT NULL AND published_source_language IS NOT NULL",
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
            "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, 'translate', ?4, 'queued', ?5) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, openai_lb_request_id = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status IN ('failed', 'succeeded')",
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
                "SELECT d.id, d.owner_id, d.published_title, d.published_source_language, d.status, d.visibility, d.document_kind, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, d.parent_id, d.sort_key, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = ?2 AND r.document_id = d.id WHERE d.id = ?1 AND d.status = 'published' AND d.published_revision_id = ?2 AND d.published_title IS NOT NULL AND d.published_source_language IS NOT NULL",
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
            "INSERT INTO document_metadata(document_id, language, source_revision_id, metadata_json, published_at) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, metadata_json = excluded.metadata_json, published_at = excluded.published_at",
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

    pub fn invalidate_profile_translation_metadata(
        &self,
        document_id: &str,
        source_revision_id: &str,
    ) -> Result<(), DatabaseError> {
        self.connection()?.execute(
            "DELETE FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language != (SELECT published_source_language FROM documents WHERE id = ?1)",
            params![document_id, source_revision_id],
        )?;
        self.requeue_translations_missing_metadata(document_id, source_revision_id)
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
            "INSERT INTO document_translations(document_id, language, source_revision_id, title, content, published_at) SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, title = excluded.title, content = excluded.content, published_at = excluded.published_at",
            params![document_id, language, source_revision_id, title, content, now()],
        )?;
        transaction.execute(
            "INSERT INTO document_metadata(document_id, language, source_revision_id, metadata_json, published_at) SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1 AND published_revision_id = ?3 AND status = 'published') ON CONFLICT(document_id, language) DO UPDATE SET source_revision_id = excluded.source_revision_id, metadata_json = excluded.metadata_json, published_at = excluded.published_at",
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
        "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, openai_lb_request_id = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status = 'failed'",
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
        "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, openai_lb_request_id = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status != 'running'",
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

fn public_document_detail(
    connection: &Connection,
    publication: &PublishedDocument,
    requested_language: Option<&str>,
) -> Result<PublicDocumentDetail, DatabaseError> {
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
    let metadata_status = match metadata_status {
        Some(status) => Some(status),
        None => {
            profile_summary_status(connection, &publication.id, &publication.source_revision_id)?
        }
    };
    let translation_fallback = content_fallback || is_metadata_fallback;
    let translation_status = if translation_fallback {
        target_language
            .map(|language| {
                ai_request_status_on(
                    connection,
                    &publication.id,
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
    let mut statement = connection.prepare(
        "SELECT language FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 ORDER BY language",
    )?;
    let translation_languages = statement
        .query_map(
            params![publication.id, publication.source_revision_id],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut available_languages = vec![publication.source_language.clone()];
    available_languages.extend(translation_languages);
    Ok(PublicDocumentDetail {
        id: publication.id.clone(),
        owner_id: publication.owner_id.clone(),
        title,
        content,
        source_language: publication.source_language.clone(),
        language,
        visibility: publication.visibility.clone(),
        available_languages,
        metadata,
        is_metadata_fallback,
        metadata_status,
        requested_language: requested_language.map(str::to_owned),
        translation_status,
        is_translation_fallback: translation_fallback,
        published_at: publication.published_at,
        views: view_stats(connection, &publication.id)?,
    })
}

fn profile_summary_status(
    connection: &Connection,
    document_id: &str,
    source_revision_id: &str,
) -> Result<Option<String>, DatabaseError> {
    let status = connection
        .query_row(
            "SELECT status FROM ai_requests WHERE document_id = ?1 AND source_revision_id = ?2 AND target_language = '' AND task LIKE 'profile_%' ORDER BY CASE status WHEN 'running' THEN 1 WHEN 'queued' THEN 2 WHEN 'failed' THEN 3 ELSE 4 END, created_at DESC LIMIT 1",
            params![document_id, source_revision_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(status.filter(|status| status != "succeeded"))
}

fn view_stats(
    connection: &Connection,
    document_id: &str,
) -> Result<DocumentViewStats, DatabaseError> {
    let stats = connection
        .query_row(
            "SELECT views_total, views_human, views_unique_human FROM document_view_stats WHERE document_id = ?1",
            [document_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((total, human, unique_human)) = stats else {
        return Ok(DocumentViewStats::default());
    };
    Ok(DocumentViewStats {
        total: u64::try_from(total).unwrap_or(0),
        human: u64::try_from(human).unwrap_or(0),
        unique_human: u64::try_from(unique_human).unwrap_or(0),
    })
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
            "SELECT d.id, d.published_revision_id FROM documents d WHERE d.status = 'published' AND d.published_revision_id IS NOT NULL AND d.published_source_language IS NOT NULL AND NOT EXISTS (SELECT 1 FROM document_metadata m WHERE m.document_id = d.id AND m.source_revision_id = d.published_revision_id AND m.language = d.published_source_language)",
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

fn backfill_profile_summaries(connection: &mut Connection) -> Result<(), DatabaseError> {
    const PROFILE_SUMMARY_VERSION: &str = "5";
    let version: Option<String> = connection
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'profile_summary_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if version.as_deref() == Some(PROFILE_SUMMARY_VERSION) {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    let profiles = {
        let mut statement = transaction.prepare(
            "SELECT id, published_revision_id FROM documents WHERE document_kind = 'profile' AND status = 'published' AND published_revision_id IS NOT NULL",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (document_id, source_revision_id) in profiles {
        force_enqueue_ai_request(
            &transaction,
            &document_id,
            &source_revision_id,
            "metadata",
            "",
            now(),
        )?;
        for task in PROFILE_SUMMARY_TASKS {
            force_enqueue_ai_request(
                &transaction,
                &document_id,
                &source_revision_id,
                task,
                "",
                now(),
            )?;
        }
    }
    transaction.execute(
        "INSERT INTO app_meta(key, value) VALUES ('profile_summary_version', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [PROFILE_SUMMARY_VERSION],
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_ai_request_task_constraint(connection: &mut Connection) -> Result<(), DatabaseError> {
    let schema: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ai_requests'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if schema
        .as_deref()
        .is_some_and(|sql| sql.contains("profile_experience"))
    {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "DROP INDEX IF EXISTS ai_requests_status_idx;
         DROP INDEX IF EXISTS ai_requests_document_idx;
         ALTER TABLE ai_requests RENAME TO ai_requests_legacy;
         CREATE TABLE ai_requests (
             id TEXT PRIMARY KEY NOT NULL,
             document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
             source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
             task TEXT NOT NULL CHECK(task IN ('metadata', 'translate', 'profile_experience', 'profile_personality', 'profile_mbti', 'profile_schwartz', 'profile_motivations', 'profile_philosophy', 'profile_timeline')),
             target_language TEXT NOT NULL DEFAULT '',
             status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
             result_summary TEXT,
             error TEXT,
             openai_lb_request_id TEXT,
             created_at INTEGER NOT NULL,
             started_at INTEGER,
             completed_at INTEGER,
             UNIQUE(document_id, source_revision_id, task, target_language)
         );
         INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at)
         SELECT id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at
         FROM ai_requests_legacy;
         DROP TABLE ai_requests_legacy;",
    )?;
    transaction.commit()?;
    Ok(())
}

fn add_ai_request_openai_lb_request_id_if_needed(
    connection: &Connection,
) -> Result<(), DatabaseError> {
    if table_has_column(connection, "ai_requests", "openai_lb_request_id")? {
        return Ok(());
    }
    connection.execute(
        "ALTER TABLE ai_requests ADD COLUMN openai_lb_request_id TEXT",
        [],
    )?;
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
        ("document_kind", "TEXT NOT NULL DEFAULT 'article'"),
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

fn add_document_tree_columns_if_needed(connection: &Connection) -> Result<(), DatabaseError> {
    // COMPATIBILITY: databases created before the document tree lack the ordering columns.
    // Remove after every supported deployed database has been upgraded; verify with
    // PRAGMA table_info(documents) before removing this path.
    if !table_exists(connection, "documents")?
        || !table_has_column(connection, "documents", "owner_id")?
    {
        return Ok(());
    }
    for (column, definition) in [
        (
            "parent_id",
            "TEXT REFERENCES documents(id) ON DELETE CASCADE",
        ),
        ("sort_key", "TEXT NOT NULL DEFAULT ''"),
    ] {
        if !table_has_column(connection, "documents", column)? {
            connection.execute(
                &format!("ALTER TABLE documents ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    backfill_document_sort_keys(connection)
}

fn backfill_document_sort_keys(connection: &Connection) -> Result<(), DatabaseError> {
    let owners: Vec<String> = connection
        .prepare("SELECT DISTINCT owner_id FROM documents WHERE sort_key = ''")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for owner_id in owners {
        let mut last = connection
            .query_row(
                "SELECT sort_key FROM documents WHERE owner_id = ?1 AND sort_key != '' ORDER BY sort_key DESC LIMIT 1",
                [&owner_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|key| parse_sort_key(&key))
            .transpose()?;
        let ids: Vec<String> = connection
            .prepare("SELECT id FROM documents WHERE owner_id = ?1 AND sort_key = '' ORDER BY updated_at DESC, id DESC")?
            .query_map([&owner_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for id in ids {
            let index = match &last {
                Some(last) => FractionalIndex::new_after(last),
                None => FractionalIndex::default(),
            };
            connection.execute(
                "UPDATE documents SET sort_key = ?2 WHERE id = ?1",
                params![id, index.to_string()],
            )?;
            last = Some(index);
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

fn append_document_sort_key(
    connection: &Connection,
    owner_id: &str,
    parent_id: Option<&str>,
) -> Result<String, DatabaseError> {
    let last: Option<String> = connection
        .query_row(
            "SELECT sort_key FROM documents WHERE owner_id = ?1 AND parent_id IS ?2 AND sort_key != '' ORDER BY sort_key DESC LIMIT 1",
            params![owner_id, parent_id],
            |row| row.get(0),
        )
        .optional()?;
    let index = match last {
        Some(key) => FractionalIndex::new_after(&parse_sort_key(&key)?),
        None => FractionalIndex::default(),
    };
    Ok(index.to_string())
}

fn sibling_sort_key(
    connection: &Connection,
    owner_id: &str,
    parent_id: Option<&str>,
    after_id: Option<&str>,
    moving_id: &str,
) -> Result<Option<String>, DatabaseError> {
    let children: Vec<(String, String)> = connection
        .prepare("SELECT id, sort_key FROM documents WHERE owner_id = ?1 AND parent_id IS ?2 ORDER BY sort_key, id")?
        .query_map(params![owner_id, parent_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    let after_position = match after_id {
        None => None,
        Some(after_id) if after_id != moving_id => {
            match children.iter().position(|(id, _)| id == after_id) {
                Some(position) => Some(position),
                None => return Ok(None),
            }
        }
        Some(_) => return Ok(None),
    };
    let (lower, upper_start) = match after_position {
        Some(position) => (Some(parse_sort_key(&children[position].1)?), position + 1),
        None => (None, 0),
    };
    let upper = children[upper_start..]
        .iter()
        .find(|(id, _)| id != moving_id)
        .map(|(_, key)| parse_sort_key(key))
        .transpose()?;
    let index = match (lower, upper) {
        (Some(lower), Some(upper)) => FractionalIndex::new_between(&lower, &upper),
        (Some(lower), None) => Some(FractionalIndex::new_after(&lower)),
        (None, Some(upper)) => Some(FractionalIndex::new_before(&upper)),
        (None, None) => Some(FractionalIndex::default()),
    };
    Ok(index.map(|index| index.to_string()))
}

fn is_descendant(
    connection: &Connection,
    document_id: &str,
    candidate_id: &str,
) -> Result<bool, DatabaseError> {
    connection
        .query_row(
            "WITH RECURSIVE subtree(id) AS (SELECT id FROM documents WHERE id = ?1 UNION ALL SELECT d.id FROM documents d JOIN subtree s ON d.parent_id = s.id) SELECT EXISTS(SELECT 1 FROM subtree WHERE id = ?2)",
            params![document_id, candidate_id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::Sqlite)
}

fn count_descendants(connection: &Connection, document_id: &str) -> Result<i64, DatabaseError> {
    connection
        .query_row(
            "WITH RECURSIVE subtree(id) AS (SELECT id FROM documents WHERE parent_id = ?1 UNION ALL SELECT d.id FROM documents d JOIN subtree s ON d.parent_id = s.id) SELECT COUNT(*) FROM subtree",
            [document_id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::Sqlite)
}

fn parse_sort_key(key: &str) -> Result<FractionalIndex, DatabaseError> {
    FractionalIndex::from_string(key).map_err(|_| DatabaseError::InvalidOrderKey)
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

fn api_key_from_row(row: &Row<'_>) -> rusqlite::Result<ApiKey> {
    Ok(ApiKey {
        id: row.get(0)?,
        label: row.get(1)?,
        prefix: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
    })
}

fn document_from_row(row: &Row<'_>) -> rusqlite::Result<Document> {
    let metadata: String = row.get(7)?;
    Ok(Document {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        title: row.get(2)?,
        source_language: row.get(3)?,
        status: row.get(4)?,
        visibility: row.get(5)?,
        kind: row.get(6)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(Value::Object(Default::default())),
        current_revision_id: row.get(8)?,
        published_revision_id: row.get(9)?,
        published_source_language: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        parent_id: row.get(13)?,
        sort_key: row.get(14)?,
    })
}

fn document_detail_from_row(row: &Row<'_>) -> rusqlite::Result<DocumentDetail> {
    let document = document_from_row(row)?;
    Ok(DocumentDetail {
        document,
        revision: DocumentRevision {
            id: row.get(15)?,
            document_id: row.get(16)?,
            content: row.get(17)?,
            message: row.get(18)?,
            author_id: row.get(19)?,
            created_at: row.get(20)?,
        },
        descendant_count: 0,
    })
}

fn document_comment_from_row(row: &Row<'_>) -> rusqlite::Result<DocumentComment> {
    Ok(DocumentComment {
        id: row.get(0)?,
        document_id: row.get(1)?,
        author_id: row.get(2)?,
        language: row.get(3)?,
        content: row.get(4)?,
        quote: row.get(5)?,
        prefix: row.get(6)?,
        suffix: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn published_document_from_row(row: &Row<'_>) -> rusqlite::Result<PublishedDocument> {
    Ok(PublishedDocument {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        source_language: row.get(4)?,
        visibility: row.get(5)?,
        source_revision_id: row.get(6)?,
        published_at: row.get(7)?,
    })
}

fn published_article_from_row(row: &Row<'_>) -> rusqlite::Result<PublishedArticle> {
    Ok(PublishedArticle {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        inferred_date: metadata_from_json(&row.get::<_, String>(3)?).inferred_date,
        published_date: row.get(4)?,
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
        openai_lb_request_id: row.get(9)?,
        created_at: row.get(10)?,
        started_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

fn sqlite_count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn now() -> i64 {
    Utc::now().timestamp()
}

fn sqlite_page_value(value: i64) -> Result<u64, DatabaseError> {
    u64::try_from(value)
        .map_err(|_| DatabaseError::Migration("SQLite page values must be non-negative".to_owned()))
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use serde_json::json;

    #[test]
    fn api_keys_create_authenticate_list_and_revoke() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = database
            .create_api_key("user-a", "Cybion", "0123456789", &"a".repeat(64))
            .unwrap();
        let keys = database.list_api_keys("user-a").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, created.id);
        assert_eq!(keys[0].label, "Cybion");
        assert_eq!(keys[0].prefix, "0123456789");
        assert!(keys[0].last_used_at.is_none());

        assert!(
            database
                .authenticate_api_key("user-a", &"b".repeat(64))
                .unwrap()
                .is_none()
        );
        let key_id = database
            .authenticate_api_key("user-a", &"a".repeat(64))
            .unwrap()
            .unwrap();
        assert_eq!(key_id, created.id);
        assert!(
            database.list_api_keys("user-a").unwrap()[0]
                .last_used_at
                .is_some()
        );

        assert!(!database.revoke_api_key("user-b", &created.id).unwrap());
        assert!(
            database
                .authenticate_api_key("user-a", &"a".repeat(64))
                .unwrap()
                .is_some()
        );
        assert!(database.revoke_api_key("user-a", &created.id).unwrap());
        assert!(!database.revoke_api_key("user-a", &created.id).unwrap());
        assert!(
            database
                .authenticate_api_key("user-a", &"a".repeat(64))
                .unwrap()
                .is_none()
        );
        assert!(database.list_api_keys("user-a").unwrap().is_empty());
    }

    use super::{
        CreateDocumentOutcome, Database, Document, DocumentDetail, DocumentMove, DocumentSave,
        MoveDocumentOutcome, NewDocument, NewDocumentComment, PROFILE_SUMMARY_TASKS,
        PublicDocumentDetail, PublishedMetadata, ReadDocumentOutcome, ViewDelta, table_has_column,
    };

    fn read_public_document(
        database: &Database,
        document_id: &str,
        language: Option<&str>,
    ) -> Option<PublicDocumentDetail> {
        match database.read_document(document_id, language, None).unwrap() {
            ReadDocumentOutcome::Found(document) => Some(*document),
            ReadDocumentOutcome::Private | ReadDocumentOutcome::Missing => None,
        }
    }

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
            experience_summary: String::new(),
            personality_analysis: String::new(),
            mbti_analysis: Default::default(),
            schwartz_values: vec![],
            unconscious_motivations: String::new(),
            philosophical_references: String::new(),
            daily_timeline: vec![],
        }
    }

    fn create_document(database: &Database, input: &NewDocument<'_>) -> DocumentDetail {
        match database.create_document(input).unwrap() {
            CreateDocumentOutcome::Created(document) => *document,
            outcome => panic!("unexpected create outcome: {outcome:?}"),
        }
    }

    fn create_article(database: &Database, title: &str, parent_id: Option<&str>) -> DocumentDetail {
        create_document(
            database,
            &NewDocument {
                author_id: "author-a",
                title,
                source_language: "en-US",
                content: "# Content",
                message: "Created document",
                parent_id,
            },
        )
    }

    fn ordered_titles(database: &Database, owner_id: &str, parent_id: Option<&str>) -> Vec<String> {
        let mut documents: Vec<Document> = database
            .list_documents(owner_id)
            .unwrap()
            .into_iter()
            .filter(|document| document.parent_id.as_deref() == parent_id)
            .collect();
        documents.sort_by(|left, right| left.sort_key.cmp(&right.sort_key));
        documents
            .into_iter()
            .map(|document| document.title)
            .collect()
    }

    #[test]
    fn view_counts_accumulate_in_batches_and_follow_document_deletion() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let article = create_article(&database, "Counted article", None);
        database
            .publish_document(&article.document.id, &article.revision.id, "public")
            .unwrap();

        database
            .apply_view_deltas(&[ViewDelta {
                day: "2026-02-14".to_owned(),
                document_id: article.document.id.clone(),
                total: 3,
                human: 2,
                unique_human: 1,
            }])
            .unwrap();
        database
            .apply_view_deltas(&[ViewDelta {
                day: "2026-02-14".to_owned(),
                document_id: article.document.id.clone(),
                total: 2,
                human: 2,
                unique_human: 1,
            }])
            .unwrap();

        let stats = database.view_stats(&article.document.id).unwrap();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.human, 4);
        assert_eq!(stats.unique_human, 2);
        assert_eq!(
            read_public_document(&database, &article.document.id, None)
                .unwrap()
                .views
                .total,
            5
        );
        let daily: i64 = database
            .connection()
            .unwrap()
            .query_row(
                "SELECT views_total FROM document_view_daily WHERE document_id = ?1 AND day = ?2",
                params![article.document.id, "2026-02-14"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(daily, 5);

        // Deltas for documents that no longer exist are skipped, not errors.
        database
            .apply_view_deltas(&[ViewDelta {
                day: "2026-02-14".to_owned(),
                document_id: "missing-document".to_owned(),
                total: 1,
                human: 1,
                unique_human: 1,
            }])
            .unwrap();

        assert!(database.delete_document(&article.document.id).unwrap());
        let stats = database.view_stats(&article.document.id).unwrap();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.unique_human, 0);
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

        let created = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "First note",
                source_language: "zh-CN",
                content: "# First note",
                message: "Created document",
                parent_id: None,
            },
        );
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
            .publish_document(&created.document.id, &saved.revision.id, "public")
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

        let public = read_public_document(&database, &created.document.id, None).unwrap();
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
    fn published_articles_accept_comments_and_keep_imported_publication_times() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let article = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "Commented article",
                source_language: "zh-CN",
                content: "# Article\n\nA selected passage.",
                message: "Created document",
                parent_id: None,
            },
        );
        assert_eq!(
            database
                .update_publication_time(&article.document.id, 1_704_067_200)
                .unwrap(),
            Some(1_704_067_200)
        );
        let published = database
            .publish_document(&article.document.id, &article.revision.id, "public")
            .unwrap()
            .unwrap();
        let source_revision_id = published.published_revision_id.unwrap();
        assert_eq!(
            read_public_document(&database, &article.document.id, None)
                .unwrap()
                .published_at,
            1_704_067_200
        );

        assert_eq!(
            database
                .update_publication_time(&article.document.id, 1_704_153_600)
                .unwrap(),
            Some(1_704_153_600)
        );
        assert_eq!(
            database.publication_time(&article.document.id).unwrap(),
            Some(1_704_153_600)
        );

        let full_comment = database
            .create_public_comment(&NewDocumentComment {
                document_id: &article.document.id,
                author_id: "reader-a",
                language: "zh-CN",
                content: "全文评论",
                quote: None,
                prefix: None,
                suffix: None,
            })
            .unwrap()
            .unwrap();
        let inline_comment = database
            .create_public_comment(&NewDocumentComment {
                document_id: &article.document.id,
                author_id: "reader-b",
                language: "zh-CN",
                content: "划线评论",
                quote: Some("selected passage"),
                prefix: Some("A "),
                suffix: Some("."),
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            database
                .public_comments(&article.document.id, "zh-CN")
                .unwrap()
                .into_iter()
                .map(|comment| comment.id)
                .collect::<Vec<_>>(),
            vec![full_comment.id, inline_comment.id]
        );

        assert!(
            database
                .create_public_comment(&NewDocumentComment {
                    document_id: &article.document.id,
                    author_id: "reader-c",
                    language: "en-US",
                    content: "Unavailable translation",
                    quote: None,
                    prefix: None,
                    suffix: None,
                })
                .unwrap()
                .is_none()
        );
        database
            .store_published_translation(
                &article.document.id,
                &source_revision_id,
                "en-US",
                "Commented article",
                "# Article\n\nA selected passage.",
                &metadata("en-US", "English metadata"),
            )
            .unwrap();
        assert!(
            database
                .create_public_comment(&NewDocumentComment {
                    document_id: &article.document.id,
                    author_id: "reader-c",
                    language: "en-US",
                    content: "Translated article comment",
                    quote: None,
                    prefix: None,
                    suffix: None,
                })
                .unwrap()
                .is_some()
        );

        assert!(database.delete_document(&article.document.id).unwrap());
        assert!(
            database
                .public_comments(&article.document.id, "zh-CN")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn private_publications_are_readable_by_the_owner_and_granted_readers_only() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let article = create_article(&database, "Shared note", None);
        let saved = database
            .save_document(&DocumentSave {
                id: &article.document.id,
                author_id: "author-a",
                title: "Shared note",
                source_language: "en-US",
                content: "# Shared\n\nOnly invited readers.",
                message: "Revise",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();

        let published = database
            .publish_document(&article.document.id, &saved.revision.id, "private")
            .unwrap()
            .unwrap();
        assert_eq!(published.status, "published");
        assert_eq!(published.visibility, "private");
        assert!(database.public_documents(None).unwrap().is_empty());
        assert!(matches!(
            database
                .read_document(&article.document.id, None, None)
                .unwrap(),
            ReadDocumentOutcome::Private
        ));
        assert!(matches!(
            database
                .read_document(&article.document.id, None, Some("stranger"))
                .unwrap(),
            ReadDocumentOutcome::Private
        ));

        assert!(
            database
                .grant_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        for viewer in ["author-a", "reader-a"] {
            let ReadDocumentOutcome::Found(document) = database
                .read_document(&article.document.id, None, Some(viewer))
                .unwrap()
            else {
                panic!("expected {viewer} to read the private publication");
            };
            assert_eq!(document.title, "Shared note");
            assert_eq!(document.content, "# Shared\n\nOnly invited readers.");
            assert_eq!(document.visibility, "private");
        }

        assert!(
            database
                .revoke_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        assert!(matches!(
            database
                .read_document(&article.document.id, None, Some("reader-a"))
                .unwrap(),
            ReadDocumentOutcome::Private
        ));

        database
            .publish_document(&article.document.id, &saved.revision.id, "public")
            .unwrap()
            .unwrap();
        assert_eq!(database.public_documents(None).unwrap().len(), 1);
        let ReadDocumentOutcome::Found(document) = database
            .read_document(&article.document.id, None, None)
            .unwrap()
        else {
            panic!("expected the public republication to be readable");
        };
        assert_eq!(document.visibility, "public");
    }

    #[test]
    fn granted_readers_are_listed_and_removed_with_their_document() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let article = create_article(&database, "Team note", None);

        assert!(
            database
                .grant_document_reader(&article.document.id, "reader-b")
                .unwrap()
        );
        assert!(
            database
                .grant_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        assert!(
            !database
                .grant_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        let mut user_ids: Vec<String> = database
            .document_readers(&article.document.id)
            .unwrap()
            .into_iter()
            .map(|reader| reader.user_id)
            .collect();
        user_ids.sort();
        assert_eq!(user_ids, vec!["reader-a", "reader-b"]);

        assert!(
            database
                .revoke_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        assert!(
            !database
                .revoke_document_reader(&article.document.id, "reader-a")
                .unwrap()
        );
        assert_eq!(
            database
                .document_readers(&article.document.id)
                .unwrap()
                .len(),
            1
        );

        assert!(database.delete_document(&article.document.id).unwrap());
        assert!(
            database
                .document_readers(&article.document.id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn profile_documents_are_unique_and_do_not_appear_in_the_square() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let profile = database.profile_document("author-a").unwrap();
        let repeated_profile = database.profile_document("author-a").unwrap();
        assert_eq!(profile.document.id, repeated_profile.document.id);
        assert_eq!(profile.document.kind, "profile");

        let profile_source = database
            .save_document(&DocumentSave {
                id: &profile.document.id,
                author_id: "author-a",
                title: "About the author",
                source_language: "en-US",
                content: "# About\n\nWorked on documented software projects since 2023.",
                message: "Write profile",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        database
            .publish_document(&profile.document.id, &profile_source.revision.id, "public")
            .unwrap()
            .unwrap();
        let profile_request_id = database
            .ai_requests()
            .unwrap()
            .into_iter()
            .find(|request| {
                request.document_id == profile.document.id && request.task == "metadata"
            })
            .map(|request| request.id)
            .unwrap();
        database
            .complete_ai_request(&profile_request_id, "Initial profile metadata", None)
            .unwrap();

        let article = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "An article",
                source_language: "en-US",
                content: "# Article",
                message: "Created document",
                parent_id: None,
            },
        );
        database
            .publish_document(&article.document.id, &article.revision.id, "public")
            .unwrap()
            .unwrap();
        let published_articles = database.published_articles_for_author("author-a").unwrap();
        assert_eq!(published_articles.len(), 1);
        assert_eq!(published_articles[0].id, article.document.id);
        assert_eq!(published_articles[0].title, "An article");
        assert_eq!(published_articles[0].content, "# Article");
        assert!(!published_articles[0].published_date.is_empty());
        assert_eq!(
            database
                .ai_requests()
                .unwrap()
                .into_iter()
                .find(|request| request.id == profile_request_id)
                .map(|request| request.status),
            Some("succeeded".to_owned())
        );

        let mut profile_metadata = metadata("en-US", "Profile description");
        profile_metadata.experience_summary =
            "Software projects have been documented since 2023.".to_owned();
        profile_metadata.personality_analysis =
            "## Writing patterns\n\n- Builds software documentation from explicit project evidence.".to_owned();
        database
            .apply_published_metadata(
                &profile.document.id,
                &profile_source.revision.id,
                &profile_metadata,
                "en-US",
            )
            .unwrap();
        database
            .store_published_metadata(
                &profile.document.id,
                &profile_source.revision.id,
                "en-US",
                &profile_metadata,
            )
            .unwrap();
        database
            .store_published_translation(
                &profile.document.id,
                &profile_source.revision.id,
                "zh-CN",
                "作者简介",
                "# 简介",
                &metadata("zh-CN", "译文简介"),
            )
            .unwrap();
        database
            .invalidate_profile_translation_metadata(
                &profile.document.id,
                &profile_source.revision.id,
            )
            .unwrap();
        let localized_profile = database
            .public_user_profile("author-a", Some("zh-CN"))
            .unwrap()
            .profile
            .unwrap();
        assert!(localized_profile.is_translation_fallback);
        assert_eq!(
            localized_profile.translation_status.as_deref(),
            Some("queued")
        );

        assert_eq!(database.public_documents(None).unwrap().len(), 1);
        assert!(read_public_document(&database, &profile.document.id, None).is_none());
        let public_profile = database.public_user_profile("author-a", None).unwrap();
        assert_eq!(public_profile.published_article_dates.len(), 1);
        assert_eq!(
            public_profile
                .profile
                .as_ref()
                .map(|document| document.metadata.experience_summary.as_str()),
            Some("Software projects have been documented since 2023.")
        );
        assert_eq!(
            public_profile
                .profile
                .as_ref()
                .map(|document| document.metadata.personality_analysis.as_str()),
            Some(
                "## Writing patterns\n\n- Builds software documentation from explicit project evidence."
            )
        );
    }

    #[test]
    fn profile_summaries_are_queued_only_when_the_owner_asks() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let profile = database.profile_document("author-a").unwrap();
        let source = database
            .save_document(&DocumentSave {
                id: &profile.document.id,
                author_id: "author-a",
                title: "Profile",
                source_language: "en-US",
                content: "# Profile",
                message: "Write profile",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        database
            .publish_document(&profile.document.id, &source.revision.id, "public")
            .unwrap()
            .unwrap();
        let requests = database.ai_requests().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].task, "metadata");

        let queued = database
            .enqueue_profile_summary_tasks(&profile.document.id, &source.revision.id, None)
            .unwrap();
        assert_eq!(queued.len(), PROFILE_SUMMARY_TASKS.len());
        let requests = database.ai_requests().unwrap();
        assert!(PROFILE_SUMMARY_TASKS.iter().all(|task| {
            requests
                .iter()
                .any(|request| request.task == *task && request.target_language.is_empty())
        }));

        let queued = database
            .enqueue_profile_summary_tasks(
                &profile.document.id,
                &source.revision.id,
                Some("profile_mbti"),
            )
            .unwrap();
        assert_eq!(queued, vec!["profile_mbti"]);
    }

    #[test]
    fn migrates_existing_ai_request_tables_for_profile_summary_tasks() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        drop(database);
        let database_path = directory.path().join("ctx.sqlite3");
        let connection = Connection::open(database_path).unwrap();
        connection
            .execute_batch(
                "DROP INDEX ai_requests_status_idx;
                 DROP INDEX ai_requests_document_idx;
                 ALTER TABLE ai_requests RENAME TO ai_requests_new;
                 CREATE TABLE ai_requests (
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
                 INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at)
                 SELECT id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at FROM ai_requests_new;
                 DROP TABLE ai_requests_new;",
            )
            .unwrap();
        drop(connection);

        let reopened = Database::open(directory.path()).unwrap();
        assert!(
            table_has_column(
                &Connection::open(directory.path().join("ctx.sqlite3")).unwrap(),
                "ai_requests",
                "openai_lb_request_id"
            )
            .unwrap()
        );
        let profile = reopened.profile_document("author-a").unwrap();
        let source = reopened
            .save_document(&DocumentSave {
                id: &profile.document.id,
                author_id: "author-a",
                title: "Profile",
                source_language: "en-US",
                content: "# Profile",
                message: "Write profile",
                metadata: &json!({}),
            })
            .unwrap()
            .unwrap();
        reopened
            .publish_document(&profile.document.id, &source.revision.id, "public")
            .unwrap()
            .unwrap();
        let queued = reopened
            .enqueue_profile_summary_tasks(
                &profile.document.id,
                &source.revision.id,
                Some("profile_mbti"),
            )
            .unwrap();
        assert_eq!(queued, vec!["profile_mbti"]);
        assert!(
            reopened
                .ai_requests()
                .unwrap()
                .iter()
                .any(|request| request.task == "profile_mbti")
        );
    }

    #[test]
    fn adds_openai_lb_request_id_to_current_ai_request_tables() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        drop(database);
        let database_path = directory.path().join("ctx.sqlite3");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "DROP INDEX ai_requests_status_idx;
                 DROP INDEX ai_requests_document_idx;
                 ALTER TABLE ai_requests RENAME TO ai_requests_new;
                 CREATE TABLE ai_requests (
                     id TEXT PRIMARY KEY NOT NULL,
                     document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                     source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
                     task TEXT NOT NULL CHECK(task IN ('metadata', 'translate', 'profile_experience', 'profile_personality', 'profile_mbti', 'profile_schwartz', 'profile_motivations', 'profile_philosophy', 'profile_timeline')),
                     target_language TEXT NOT NULL DEFAULT '',
                     status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
                     result_summary TEXT,
                     error TEXT,
                     created_at INTEGER NOT NULL,
                     started_at INTEGER,
                     completed_at INTEGER,
                     UNIQUE(document_id, source_revision_id, task, target_language)
                 );
                 INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at)
                 SELECT id, document_id, source_revision_id, task, target_language, status, result_summary, error, created_at, started_at, completed_at FROM ai_requests_new;
                 DROP TABLE ai_requests_new;",
            )
            .unwrap();
        drop(connection);

        let reopened = Database::open(directory.path()).unwrap();
        assert!(
            table_has_column(
                &Connection::open(reopened.database_path()).unwrap(),
                "ai_requests",
                "openai_lb_request_id"
            )
            .unwrap()
        );
    }

    #[test]
    fn publishing_queues_metadata_and_readers_request_localized_markdown_and_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "Original",
                source_language: "zh-CN",
                content: "# 原文\n\n- [x] 已完成",
                message: "Created document",
                parent_id: None,
            },
        );
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
            .publish_document(&created.document.id, &source.revision.id, "public")
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

        let source_public = read_public_document(&database, &created.document.id, None).unwrap();
        assert_eq!(source_public.owner_id, "author-a");
        assert_eq!(source_public.language, "zh-CN");
        assert_eq!(source_public.available_languages, vec!["zh-CN"]);
        assert!(source_public.metadata.is_empty());
        assert_eq!(source_public.metadata_status.as_deref(), Some("queued"));
        let english_public =
            read_public_document(&database, &created.document.id, Some("en-US")).unwrap();
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
        let incomplete_legacy_translation =
            read_public_document(&database, &created.document.id, Some("en-US")).unwrap();
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
        let english_public =
            read_public_document(&database, &created.document.id, Some("en-US")).unwrap();
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
            read_public_document(&database, &created.document.id, None)
                .unwrap()
                .source_language,
            "zh-CN"
        );

        database
            .publish_document(&created.document.id, &next.revision.id, "public")
            .unwrap()
            .unwrap();
        assert!(
            read_public_document(&database, &created.document.id, Some("ja-JP"))
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
            read_public_document(&database, &created.document.id, Some("zh-CN"))
                .unwrap()
                .title,
            "更新的原文"
        );
    }

    #[test]
    fn inferred_source_language_is_available_before_a_reader_requests_translation() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "Untitled language",
                source_language: "und",
                content: "# こんにちは",
                message: "Created document",
                parent_id: None,
            },
        );
        let published = database
            .publish_document(&created.document.id, &created.revision.id, "public")
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
            .fail_ai_request(&claimed.id, "AI is not configured", Some("lb-request-1"))
            .unwrap();
        assert_eq!(database.ai_requests().unwrap()[0].status, "failed");
        assert_eq!(
            database.ai_requests().unwrap()[0]
                .openai_lb_request_id
                .as_deref(),
            Some("lb-request-1")
        );
        database.requeue_failed_ai_requests().unwrap();
        assert_eq!(database.ai_requests().unwrap()[0].status, "queued");
        assert!(
            database.ai_requests().unwrap()[0]
                .openai_lb_request_id
                .is_none()
        );

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
            read_public_document(&database, &created.document.id, None)
                .unwrap()
                .source_language,
            "ja-JP"
        );
    }

    #[test]
    fn publication_rejects_a_source_revision_that_changed_while_translating() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let created = create_document(
            &database,
            &NewDocument {
                author_id: "author-a",
                title: "Original",
                source_language: "en-US",
                content: "# Original",
                message: "Created document",
                parent_id: None,
            },
        );
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
                .publish_document(&created.document.id, &source.revision.id, "public")
                .unwrap()
                .is_none()
        );
        assert!(read_public_document(&database, &created.document.id, None).is_none());
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
        assert_eq!(published.kind, "article");
        assert_eq!(published.published_source_language.as_deref(), Some("und"));
        assert_eq!(draft.source_language, "und");
        assert!(draft.published_source_language.is_none());
        let public = read_public_document(&database, "published", None).unwrap();
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
        assert!(read_public_document(&database, "private-document", None).is_none());
        let changed_public_document =
            read_public_document(&database, "changed-document", None).unwrap();
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

    #[test]
    fn documents_form_an_ordered_tree_and_reject_invalid_moves() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let a = create_article(&database, "A", None);
        let b = create_article(&database, "B", None);
        let c = create_article(&database, "C", Some(&a.document.id));
        let d = create_article(&database, "D", Some(&a.document.id));
        assert_eq!(ordered_titles(&database, "author-a", None), ["A", "B"]);
        assert_eq!(
            ordered_titles(&database, "author-a", Some(&a.document.id)),
            ["C", "D"]
        );
        assert_eq!(
            database
                .get_document(&a.document.id)
                .unwrap()
                .unwrap()
                .descendant_count,
            2
        );

        let moved = database
            .move_document(&DocumentMove {
                id: &d.document.id,
                parent_id: None,
                after_id: None,
            })
            .unwrap();
        let MoveDocumentOutcome::Moved(moved) = moved else {
            panic!("moving a document to the first position must succeed");
        };
        assert_eq!(moved.document.parent_id, None);
        assert_eq!(ordered_titles(&database, "author-a", None), ["D", "A", "B"]);

        let moved = database
            .move_document(&DocumentMove {
                id: &b.document.id,
                parent_id: Some(&c.document.id),
                after_id: None,
            })
            .unwrap();
        let MoveDocumentOutcome::Moved(moved) = moved else {
            panic!("re-parenting must succeed");
        };
        assert_eq!(
            moved.document.parent_id.as_deref(),
            Some(c.document.id.as_str())
        );
        assert_eq!(
            ordered_titles(&database, "author-a", Some(&c.document.id)),
            ["B"]
        );
        assert_eq!(ordered_titles(&database, "author-a", None), ["D", "A"]);

        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: &a.document.id,
                    parent_id: Some(&c.document.id),
                    after_id: None,
                })
                .unwrap(),
            MoveDocumentOutcome::InvalidTarget
        ));
        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: &b.document.id,
                    parent_id: None,
                    after_id: Some(&c.document.id),
                })
                .unwrap(),
            MoveDocumentOutcome::InvalidTarget
        ));
        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: &b.document.id,
                    parent_id: Some(&c.document.id),
                    after_id: Some(&b.document.id),
                })
                .unwrap(),
            MoveDocumentOutcome::InvalidTarget
        ));
        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: "missing",
                    parent_id: None,
                    after_id: None,
                })
                .unwrap(),
            MoveDocumentOutcome::Missing
        ));

        let other = create_document(
            &database,
            &NewDocument {
                author_id: "author-b",
                title: "Other",
                source_language: "en-US",
                content: "",
                message: "Created document",
                parent_id: None,
            },
        );
        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: &a.document.id,
                    parent_id: Some(&other.document.id),
                    after_id: None,
                })
                .unwrap(),
            MoveDocumentOutcome::InvalidTarget
        ));

        let profile = database.profile_document("author-a").unwrap();
        assert!(matches!(
            database
                .move_document(&DocumentMove {
                    id: &profile.document.id,
                    parent_id: None,
                    after_id: None,
                })
                .unwrap(),
            MoveDocumentOutcome::InvalidTarget
        ));
        assert!(matches!(
            database
                .create_document(&NewDocument {
                    author_id: "author-a",
                    title: "Under profile",
                    source_language: "en-US",
                    content: "",
                    message: "Created document",
                    parent_id: Some(&profile.document.id),
                })
                .unwrap(),
            CreateDocumentOutcome::InvalidParent
        ));
        assert!(matches!(
            database
                .create_document(&NewDocument {
                    author_id: "author-a",
                    title: "Under missing",
                    source_language: "en-US",
                    content: "",
                    message: "Created document",
                    parent_id: Some("missing"),
                })
                .unwrap(),
            CreateDocumentOutcome::MissingParent
        ));
    }

    #[test]
    fn deleting_a_parent_document_deletes_its_subtree() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let a = create_article(&database, "A", None);
        let b = create_article(&database, "B", Some(&a.document.id));
        let c = create_article(&database, "C", Some(&b.document.id));
        let x = create_article(&database, "X", None);
        let y = create_article(&database, "Y", None);
        assert!(database.delete_document(&a.document.id).unwrap());
        assert!(database.get_document(&b.document.id).unwrap().is_none());
        assert!(database.get_document(&c.document.id).unwrap().is_none());
        assert_eq!(ordered_titles(&database, "author-a", None), ["X", "Y"]);
        assert!(database.delete_document(&y.document.id).unwrap());
        assert_eq!(ordered_titles(&database, "author-a", None), ["X"]);
        assert!(database.get_document(&x.document.id).unwrap().is_some());
    }

    #[test]
    fn existing_databases_backfill_document_order_keys() {
        let directory = tempfile::tempdir().unwrap();
        {
            let connection = Connection::open(directory.path().join("ctx.sqlite3")).unwrap();
            let legacy_ddl = super::CURRENT_TABLES_SQL
                .replace(
                    "        parent_id TEXT REFERENCES documents(id) ON DELETE CASCADE,\n",
                    "",
                )
                .replace("        sort_key TEXT NOT NULL DEFAULT '',\n", "");
            connection.execute_batch(&legacy_ddl).unwrap();
            for (id, title, updated_at) in [
                ("doc-1", "Older", 1_i64),
                ("doc-2", "Middle", 2),
                ("doc-3", "Newer", 3),
            ] {
                connection
                    .execute(
                        "INSERT INTO documents(id, owner_id, title, status, visibility, document_kind, metadata_json, current_revision_id, created_at, updated_at) VALUES (?1, 'author-a', ?2, 'draft', 'private', 'article', '{}', ?3, ?4, ?4)",
                        params![id, title, format!("{id}-rev"), updated_at],
                    )
                    .unwrap();
                connection
                    .execute(
                        "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, '', '', 'author-a', ?3)",
                        params![format!("{id}-rev"), id, updated_at],
                    )
                    .unwrap();
            }
        }
        let database = Database::open(directory.path()).unwrap();
        let documents = database.list_documents("author-a").unwrap();
        assert_eq!(documents.len(), 3);
        assert!(
            documents
                .iter()
                .all(|document| !document.sort_key.is_empty())
        );
        let mut ordered = documents;
        ordered.sort_by(|left, right| left.sort_key.cmp(&right.sort_key));
        let titles: Vec<&str> = ordered
            .iter()
            .map(|document| document.title.as_str())
            .collect();
        assert_eq!(titles, ["Newer", "Middle", "Older"]);
        create_article(&database, "Newest", None);
        assert_eq!(
            ordered_titles(&database, "author-a", None),
            ["Newer", "Middle", "Older", "Newest"]
        );
    }
}
