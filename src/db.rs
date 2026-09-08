use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{Cipher, CipherError};

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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Context {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub instructions: String,
    pub visibility: String,
    pub document_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Document {
    pub id: String,
    pub context_id: String,
    pub title: String,
    pub slug: String,
    pub language: String,
    pub kind: String,
    pub status: String,
    pub metadata: Value,
    pub current_revision_id: String,
    pub published_revision_id: Option<String>,
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
    pub context_id: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub task: String,
    pub output: String,
    pub proposed_content: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewDocument<'a> {
    pub context_id: &'a str,
    pub author_id: &'a str,
    pub title: &'a str,
    pub slug: &'a str,
    pub language: &'a str,
    pub kind: &'a str,
    pub content: &'a str,
    pub message: &'a str,
}

#[derive(Clone, Debug)]
pub struct DocumentSave<'a> {
    pub id: &'a str,
    pub author_id: &'a str,
    pub title: &'a str,
    pub slug: &'a str,
    pub content: &'a str,
    pub message: &'a str,
    pub metadata: &'a Value,
}

impl Database {
    pub fn open(state_directory: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let state_directory = state_directory.as_ref();
        let cipher = Cipher::load_or_create(state_directory)?;
        let database_path = state_directory.join("ctx.sqlite3");
        let connection = Connection::open(&database_path)?;
        connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS contexts (
                id TEXT PRIMARY KEY NOT NULL,
                owner_id TEXT NOT NULL,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL,
                instructions TEXT NOT NULL,
                visibility TEXT NOT NULL CHECK(visibility IN ('private', 'public')),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS contexts_owner_id_idx ON contexts(owner_id, updated_at DESC);
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY NOT NULL,
                context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                slug TEXT NOT NULL,
                language TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('docs', 'blog')),
                status TEXT NOT NULL CHECK(status IN ('draft', 'published')),
                metadata_json TEXT NOT NULL,
                current_revision_id TEXT NOT NULL,
                published_revision_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(context_id, slug)
            );
            CREATE INDEX IF NOT EXISTS documents_context_idx ON documents(context_id, kind, updated_at DESC);
            CREATE TABLE IF NOT EXISTS document_revisions (
                id TEXT PRIMARY KEY NOT NULL,
                document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                message TEXT NOT NULL,
                author_id TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS document_revisions_document_idx ON document_revisions(document_id, created_at DESC);
            CREATE TABLE IF NOT EXISTS ai_runs (
                id TEXT PRIMARY KEY NOT NULL,
                context_id TEXT NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
                document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
                task TEXT NOT NULL,
                output TEXT NOT NULL,
                proposed_content TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ai_runs_document_idx ON ai_runs(document_id, created_at DESC);
            ",
        )?;
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

    pub fn create_context(
        &self,
        owner_id: &str,
        name: &str,
        slug: &str,
        description: &str,
        instructions: &str,
        visibility: &str,
    ) -> Result<Context, DatabaseError> {
        let context = Context {
            id: Uuid::new_v4().to_string(),
            owner_id: owner_id.to_owned(),
            name: name.to_owned(),
            slug: slug.to_owned(),
            description: description.to_owned(),
            instructions: instructions.to_owned(),
            visibility: visibility.to_owned(),
            document_count: 0,
            created_at: now(),
            updated_at: now(),
        };
        self.connection()?.execute(
            "INSERT INTO contexts(id, owner_id, name, slug, description, instructions, visibility, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![context.id, context.owner_id, context.name, context.slug, context.description, context.instructions, context.visibility, context.created_at, context.updated_at],
        )?;
        Ok(context)
    }

    pub fn list_contexts(&self, owner_id: Option<&str>) -> Result<Vec<Context>, DatabaseError> {
        let connection = self.connection()?;
        let sql = "SELECT c.id, c.owner_id, c.name, c.slug, c.description, c.instructions, c.visibility, COUNT(d.id), c.created_at, c.updated_at FROM contexts c LEFT JOIN documents d ON d.context_id = c.id";
        let suffix = " GROUP BY c.id ORDER BY c.updated_at DESC";
        let rows = if let Some(owner_id) = owner_id {
            let mut statement =
                connection.prepare(&format!("{sql} WHERE c.owner_id = ?1{suffix}"))?;
            statement
                .query_map([owner_id], context_from_row)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut statement = connection.prepare(&format!("{sql}{suffix}"))?;
            statement
                .query_map([], context_from_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    pub fn get_context(&self, id: &str) -> Result<Option<Context>, DatabaseError> {
        self.connection()?
            .query_row(
                "SELECT c.id, c.owner_id, c.name, c.slug, c.description, c.instructions, c.visibility, COUNT(d.id), c.created_at, c.updated_at FROM contexts c LEFT JOIN documents d ON d.context_id = c.id WHERE c.id = ?1 GROUP BY c.id",
                [id],
                context_from_row,
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
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
            context_id: input.context_id.to_owned(),
            title: input.title.to_owned(),
            slug: input.slug.to_owned(),
            language: input.language.to_owned(),
            kind: input.kind.to_owned(),
            status: "draft".to_owned(),
            metadata: Value::Object(Default::default()),
            current_revision_id: revision.id.clone(),
            published_revision_id: None,
            created_at: revision.created_at,
            updated_at: revision.created_at,
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO documents(id, context_id, title, slug, language, kind, status, metadata_json, current_revision_id, published_revision_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)",
            params![document.id, document.context_id, document.title, document.slug, document.language, document.kind, document.status, document.metadata.to_string(), document.current_revision_id, document.created_at, document.updated_at],
        )?;
        transaction.execute(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![revision.id, revision.document_id, revision.content, revision.message, revision.author_id, revision.created_at],
        )?;
        transaction.execute(
            "UPDATE contexts SET updated_at = ?2 WHERE id = ?1",
            params![input.context_id, now()],
        )?;
        transaction.commit()?;
        Ok(DocumentDetail { document, revision })
    }

    pub fn list_documents(&self, context_id: &str) -> Result<Vec<Document>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id, context_id, title, slug, language, kind, status, metadata_json, current_revision_id, published_revision_id, created_at, updated_at FROM documents WHERE context_id = ?1 ORDER BY kind, updated_at DESC")?;
        Ok(statement
            .query_map([context_id], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentDetail>, DatabaseError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT d.id, d.context_id, d.title, d.slug, d.language, d.kind, d.status, d.metadata_json, d.current_revision_id, d.published_revision_id, d.created_at, d.updated_at, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM documents d JOIN document_revisions r ON r.id = d.current_revision_id WHERE d.id = ?1",
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
            "UPDATE documents SET title = ?2, slug = ?3, metadata_json = ?4, current_revision_id = ?5, updated_at = ?6 WHERE id = ?1",
            params![input.id, input.title, input.slug, input.metadata.to_string(), revision.id, revision.created_at],
        )?;
        transaction.execute(
            "UPDATE contexts SET updated_at = ?2 WHERE id = ?1",
            params![existing.document.context_id, revision.created_at],
        )?;
        transaction.commit()?;
        let document = Document {
            title: input.title.to_owned(),
            slug: input.slug.to_owned(),
            metadata: input.metadata.clone(),
            current_revision_id: revision.id.clone(),
            updated_at: revision.created_at,
            ..existing.document
        };
        Ok(Some(DocumentDetail { document, revision }))
    }

    pub fn publish_document(&self, id: &str) -> Result<Option<Document>, DatabaseError> {
        let Some(mut detail) = self.get_document(id)? else {
            return Ok(None);
        };
        let now = now();
        self.connection()?.execute(
            "UPDATE documents SET status = 'published', published_revision_id = current_revision_id, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        detail.document.status = "published".to_owned();
        detail.document.published_revision_id = Some(detail.document.current_revision_id.clone());
        detail.document.updated_at = now;
        Ok(Some(detail.document))
    }

    pub fn public_document(
        &self,
        context_slug: &str,
        document_slug: &str,
    ) -> Result<Option<DocumentDetail>, DatabaseError> {
        self.connection()?.query_row(
            "SELECT d.id, d.context_id, d.title, d.slug, d.language, d.kind, d.status, d.metadata_json, d.current_revision_id, d.published_revision_id, d.created_at, d.updated_at, r.id, r.document_id, r.content, r.message, r.author_id, r.created_at FROM contexts c JOIN documents d ON d.context_id = c.id JOIN document_revisions r ON r.id = d.published_revision_id WHERE c.slug = ?1 AND c.visibility = 'public' AND d.slug = ?2 AND d.status = 'published'",
            params![context_slug, document_slug],
            document_detail_from_row,
        ).optional().map_err(DatabaseError::Sqlite)
    }

    pub fn record_ai_run(
        &self,
        context_id: &str,
        document_id: &str,
        source_revision_id: &str,
        task: &str,
        output: &str,
        proposed_content: Option<&str>,
    ) -> Result<AiRun, DatabaseError> {
        let run = AiRun {
            id: Uuid::new_v4().to_string(),
            context_id: context_id.to_owned(),
            document_id: document_id.to_owned(),
            source_revision_id: source_revision_id.to_owned(),
            task: task.to_owned(),
            output: output.to_owned(),
            proposed_content: proposed_content.map(str::to_owned),
            created_at: now(),
        };
        self.connection()?.execute(
            "INSERT INTO ai_runs(id, context_id, document_id, source_revision_id, task, output, proposed_content, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![run.id, run.context_id, run.document_id, run.source_revision_id, run.task, run.output, run.proposed_content, run.created_at],
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

fn context_from_row(row: &Row<'_>) -> rusqlite::Result<Context> {
    Ok(Context {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        name: row.get(2)?,
        slug: row.get(3)?,
        description: row.get(4)?,
        instructions: row.get(5)?,
        visibility: row.get(6)?,
        document_count: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn document_from_row(row: &Row<'_>) -> rusqlite::Result<Document> {
    let metadata: String = row.get(7)?;
    Ok(Document {
        id: row.get(0)?,
        context_id: row.get(1)?,
        title: row.get(2)?,
        slug: row.get(3)?,
        language: row.get(4)?,
        kind: row.get(5)?,
        status: row.get(6)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(Value::Object(Default::default())),
        current_revision_id: row.get(8)?,
        published_revision_id: row.get(9)?,
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

fn now() -> i64 {
    Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::{Database, NewDocument};

    #[test]
    fn database_uses_wal_and_keeps_a_published_revision() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let journal_mode: String = database
            .connection()
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");
        database.initialize_root_user("root").unwrap();
        let context = database
            .create_context("root", "Research", "research", "", "", "public")
            .unwrap();
        let document = database
            .create_document(&NewDocument {
                context_id: &context.id,
                author_id: "root",
                title: "First note",
                slug: "first-note",
                language: "en",
                kind: "docs",
                content: "# First note",
                message: "Initial draft",
            })
            .unwrap();
        let published = database
            .publish_document(&document.document.id)
            .unwrap()
            .unwrap();
        assert_eq!(published.status, "published");
        let public = database
            .public_document("research", "first-note")
            .unwrap()
            .unwrap();
        assert_eq!(public.revision.content, "# First note");
    }
}
