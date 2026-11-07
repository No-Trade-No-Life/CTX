//! D1 data layer. 只读公开面（第一阶段），字段名与 axum 版 SQL 对齐。
use serde::Deserialize;
use serde_json::Value;
use worker::wasm_bindgen::JsValue;
use worker::*;

use crate::models::{
    ApiKey, Document, DocumentComment, DocumentDetail, DocumentMove, DocumentReader, DocumentRevision,
    DocumentSave, DocumentViewStats, MoveDocumentOutcome, NewDocument, NewDocumentComment,
    PublicDocumentDetail, PublicDocumentSummary, PublicUserProfile, PublishedMetadata,
};

pub fn now() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

pub fn jstr(v: &str) -> JsValue {
    JsValue::from_str(v)
}

pub struct Db {
    d1: D1Database,
}

#[derive(Deserialize)]
struct PublicationRow {
    id: String,
    owner_id: String,
    title: String,
    content: String,
    source_language: String,
    visibility: String,
    source_revision_id: String,
    published_at: i64,
}

#[derive(Deserialize)]
struct CommentRow {
    id: String,
    document_id: String,
    author_id: String,
    language: String,
    content: String,
    quote: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    created_at: i64,
}

#[derive(Deserialize)]
struct StatsRow {
    views_total: i64,
    views_human: i64,
    views_unique_human: i64,
}

impl Db {
    pub fn new(env: &Env) -> Result<Self> {
        Ok(Self { d1: env.d1("DB")? })
    }

    async fn all<T: for<'a> Deserialize<'a>>(&self, sql: &str, params: Vec<JsValue>) -> Result<Vec<T>> {
        let stmt = self.d1.prepare(sql);
        let stmt = if params.is_empty() { stmt } else { stmt.bind(&params)? };
        stmt.all().await?.results::<T>()
    }

    async fn first<T: for<'a> Deserialize<'a>>(&self, sql: &str, params: Vec<JsValue>) -> Result<Option<T>> {
        let stmt = self.d1.prepare(sql);
        let stmt = if params.is_empty() { stmt } else { stmt.bind(&params)? };
        stmt.first::<T>(None).await
    }

    pub async fn run(&self, sql: &str, params: Vec<JsValue>) -> Result<usize> {
        let stmt = self.d1.prepare(sql);
        let stmt = if params.is_empty() { stmt } else { stmt.bind(&params)? };
        let result = stmt.run().await?;
        if let Some(error) = result.error() {
            return Err(Error::RustError(error));
        }
        Ok(result.meta()?.and_then(|m| m.changes).unwrap_or(0))
    }

    async fn view_stats(&self, document_id: &str) -> Result<DocumentViewStats> {
        let row: Option<StatsRow> = self
            .first(
                "SELECT views_total, views_human, views_unique_human FROM document_view_stats WHERE document_id = ?1",
                vec![jstr(document_id)],
            )
            .await?;
        Ok(match row {
            Some(r) => DocumentViewStats {
                total: u64::try_from(r.views_total).unwrap_or(0),
                human: u64::try_from(r.views_human).unwrap_or(0),
                unique_human: u64::try_from(r.views_unique_human).unwrap_or(0),
            },
            None => DocumentViewStats::default(),
        })
    }

    async fn translation(&self, id: &str, revision: &str, language: &str) -> Result<Option<(String, String)>> {
        #[derive(Deserialize)]
        struct T { title: String, content: String }
        let row: Option<T> = self
            .first(
                "SELECT title, content FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
                vec![jstr(id), jstr(revision), jstr(language)],
            )
            .await?;
        Ok(row.map(|t| (t.title, t.content)))
    }

    async fn metadata_exists(&self, id: &str, revision: &str, language: &str) -> Result<bool> {
        #[derive(Deserialize)]
        struct One { #[allow(dead_code)] one: i64 }
        let row: Option<One> = self
            .first(
                "SELECT 1 AS one FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
                vec![jstr(id), jstr(revision), jstr(language)],
            )
            .await?;
        Ok(row.is_some())
    }

    async fn metadata_json(&self, id: &str, revision: &str, language: &str) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct M { metadata_json: String }
        let row: Option<M> = self
            .first(
                "SELECT metadata_json FROM document_metadata WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
                vec![jstr(id), jstr(revision), jstr(language)],
            )
            .await?;
        Ok(row.map(|m| m.metadata_json))
    }

    async fn published_metadata(&self, p: &PublicationRow, language: &str) -> Result<(PublishedMetadata, bool)> {
        if let Some(json) = self.metadata_json(&p.id, &p.source_revision_id, language).await? {
            return Ok((metadata_from_json(&json), false));
        }
        if language == p.source_language {
            return Ok((PublishedMetadata::default(), false));
        }
        let source = self.metadata_json(&p.id, &p.source_revision_id, &p.source_language).await?;
        Ok((source.as_deref().map(metadata_from_json).unwrap_or_default(), true))
    }

    async fn ai_request_status(&self, id: &str, revision: &str, task: &str, language: &str) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct S { status: String }
        let row: Option<S> = self
            .first(
                "SELECT status FROM ai_requests WHERE document_id = ?1 AND source_revision_id = ?2 AND task = ?3 AND target_language = ?4 ORDER BY created_at DESC LIMIT 1",
                vec![jstr(id), jstr(revision), jstr(task), jstr(language)],
            )
            .await?;
        Ok(row.map(|s| s.status).filter(|s| s != "succeeded"))
    }

    async fn profile_summary_status(&self, id: &str, revision: &str) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct S { status: String }
        let row: Option<S> = self
            .first(
                "SELECT status FROM ai_requests WHERE document_id = ?1 AND source_revision_id = ?2 AND target_language = '' AND task LIKE 'profile_%' ORDER BY CASE status WHEN 'running' THEN 1 WHEN 'queued' THEN 2 WHEN 'failed' THEN 3 ELSE 4 END, created_at DESC LIMIT 1",
                vec![jstr(id), jstr(revision)],
            )
            .await?;
        Ok(row.map(|s| s.status).filter(|s| s != "succeeded"))
    }

    async fn translation_languages(&self, id: &str, revision: &str) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct L { language: String }
        let rows: Vec<L> = self
            .all(
                "SELECT language FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 ORDER BY language",
                vec![jstr(id), jstr(revision)],
            )
            .await?;
        Ok(rows.into_iter().map(|l| l.language).collect())
    }

    pub async fn public_documents(&self, language: Option<&str>) -> Result<Vec<PublicDocumentSummary>> {
        let publications: Vec<PublicationRow> = self
            .all(
                "SELECT d.id, d.owner_id, d.published_title AS title, r.content, d.published_source_language AS source_language, d.visibility, d.published_revision_id AS source_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.document_kind = 'article' AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL ORDER BY d.published_at DESC, d.id DESC",
                vec![],
            )
            .await?;
        let mut summaries = Vec::with_capacity(publications.len());
        for p in &publications {
            summaries.push(self.summary(p, language).await?);
        }
        Ok(summaries)
    }

    async fn summary(&self, p: &PublicationRow, requested: Option<&str>) -> Result<PublicDocumentSummary> {
        let target = requested.filter(|l| *l != p.source_language);
        let translated = match target {
            Some(l) => self.translation(&p.id, &p.source_revision_id, l).await?,
            None => None,
        };
        let has_metadata = match target {
            Some(l) => self.metadata_exists(&p.id, &p.source_revision_id, l).await?,
            None => false,
        };
        let (title, language) = match (translated, has_metadata) {
            (Some((title, _)), true) => (title, target.unwrap_or_default().to_owned()),
            _ => (p.title.clone(), p.source_language.clone()),
        };
        let (metadata, is_metadata_fallback) = self.published_metadata(p, &language).await?;
        let metadata_status = if metadata.is_empty() {
            self.ai_request_status(&p.id, &p.source_revision_id, "metadata", "").await?
        } else {
            None
        };
        Ok(PublicDocumentSummary {
            id: p.id.clone(),
            owner_id: p.owner_id.clone(),
            title,
            metadata,
            language,
            is_metadata_fallback,
            metadata_status,
            published_at: p.published_at,
        })
    }

    async fn detail(&self, p: &PublicationRow, requested: Option<&str>) -> Result<PublicDocumentDetail> {
        let target = requested.filter(|l| *l != p.source_language);
        let translated = match target {
            Some(l) => self.translation(&p.id, &p.source_revision_id, l).await?,
            None => None,
        };
        let has_metadata = match target {
            Some(l) => self.metadata_exists(&p.id, &p.source_revision_id, l).await?,
            None => false,
        };
        let (title, content, language, content_fallback) = match (translated, has_metadata) {
            (Some((title, content)), true) => (title, content, target.unwrap_or_default().to_owned(), false),
            _ => (p.title.clone(), p.content.clone(), p.source_language.clone(), target.is_some()),
        };
        let (metadata, is_metadata_fallback) = self.published_metadata(p, &language).await?;
        let metadata_status = if metadata.is_empty() {
            match self.ai_request_status(&p.id, &p.source_revision_id, "metadata", "").await? {
                Some(status) => Some(status),
                None => self.profile_summary_status(&p.id, &p.source_revision_id).await?,
            }
        } else {
            None
        };
        let translation_fallback = content_fallback || is_metadata_fallback;
        let translation_status = if translation_fallback {
            match target {
                Some(l) => self.ai_request_status(&p.id, &p.source_revision_id, "translate", l).await?,
                None => None,
            }
        } else {
            None
        };
        let mut available_languages = vec![p.source_language.clone()];
        available_languages.extend(self.translation_languages(&p.id, &p.source_revision_id).await?);
        Ok(PublicDocumentDetail {
            id: p.id.clone(),
            owner_id: p.owner_id.clone(),
            title,
            content,
            source_language: p.source_language.clone(),
            language,
            visibility: p.visibility.clone(),
            available_languages,
            metadata,
            is_metadata_fallback,
            metadata_status,
            requested_language: requested.map(str::to_owned),
            translation_status,
            is_translation_fallback: translation_fallback,
            published_at: p.published_at,
            views: self.view_stats(&p.id).await?,
        })
    }

    pub async fn read_document(
        &self,
        id: &str,
        requested: Option<&str>,
        viewer: Option<&str>,
    ) -> Result<Option<Result<PublicDocumentDetail, ()>>> {
        #[derive(Deserialize)]
        struct Row {
            id: String,
            owner_id: String,
            title: String,
            content: String,
            source_language: String,
            visibility: String,
            source_revision_id: String,
            published_at: i64,
            can_read: i64,
        }
        let row: Option<Row> = self
            .first(
                "SELECT d.id, d.owner_id, d.published_title AS title, r.content, d.published_source_language AS source_language, d.visibility, d.published_revision_id AS source_revision_id, d.published_at, CASE WHEN d.visibility = 'public' OR d.owner_id = ?2 OR EXISTS (SELECT 1 FROM document_readers rd WHERE rd.document_id = d.id AND rd.user_id = ?2) THEN 1 ELSE 0 END AS can_read FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.id = ?1 AND d.document_kind = 'article' AND d.status = 'published' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL",
                vec![jstr(id), jstr(viewer.unwrap_or(""))],
            )
            .await?;
        let Some(row) = row else { return Ok(None) };
        if row.can_read == 0 {
            return Ok(Some(Err(())));
        }
        let publication = PublicationRow {
            id: row.id,
            owner_id: row.owner_id,
            title: row.title,
            content: row.content,
            source_language: row.source_language,
            visibility: row.visibility,
            source_revision_id: row.source_revision_id,
            published_at: row.published_at,
        };
        Ok(Some(Ok(self.detail(&publication, requested).await?)))
    }

    pub async fn public_comments(&self, document_id: &str, language: &str) -> Result<Vec<DocumentComment>> {
        let rows: Vec<CommentRow> = self
            .all(
                "SELECT c.id, c.document_id, c.author_id, c.language, c.content, c.quote, c.prefix, c.suffix, c.created_at FROM document_comments c JOIN documents d ON d.id = c.document_id WHERE c.document_id = ?1 AND c.language = ?2 AND c.source_revision_id = d.published_revision_id AND d.document_kind = 'article' AND d.status = 'published' AND d.visibility = 'public' ORDER BY c.created_at, c.rowid",
                vec![jstr(document_id), jstr(language)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| DocumentComment {
                id: r.id,
                document_id: r.document_id,
                author_id: r.author_id,
                language: r.language,
                content: r.content,
                quote: r.quote,
                prefix: r.prefix,
                suffix: r.suffix,
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn public_user_profile(&self, owner_id: &str, requested: Option<&str>) -> Result<PublicUserProfile> {
        let profile_row: Option<PublicationRow> = self
            .first(
                "SELECT d.id, d.owner_id, d.published_title AS title, r.content, d.published_source_language AS source_language, d.visibility, d.published_revision_id AS source_revision_id, d.published_at FROM documents d JOIN document_revisions r ON r.id = d.published_revision_id AND r.document_id = d.id WHERE d.owner_id = ?1 AND d.document_kind = 'profile' AND d.status = 'published' AND d.visibility = 'public' AND d.published_title IS NOT NULL AND d.published_at IS NOT NULL AND d.published_source_language IS NOT NULL",
                vec![jstr(owner_id)],
            )
            .await?;
        let profile = match profile_row {
            Some(row) => Some(self.detail(&row, requested).await?),
            None => None,
        };
        #[derive(Deserialize)]
        struct D { published_at: i64 }
        let dates: Vec<D> = self
            .all(
                "SELECT published_at FROM documents WHERE owner_id = ?1 AND document_kind = 'article' AND status = 'published' AND visibility = 'public' AND published_at IS NOT NULL ORDER BY published_at",
                vec![jstr(owner_id)],
            )
            .await?;
        Ok(PublicUserProfile {
            owner_id: owner_id.to_owned(),
            profile,
            published_article_dates: dates.into_iter().map(|d| d.published_at).collect(),
        })
    }

    /// 每次阅读直接落库（替代原来的内存 5s 批量）。同一文档同一秒内的多次读取仍会累加。
    pub async fn record_view(&self, document_id: &str) -> Result<()> {
        let updated_at = now();
        let day = chrono::DateTime::from_timestamp(updated_at, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "1970-01-01".to_owned());
        self.run(
            "INSERT INTO document_view_stats(document_id, views_total, views_human, views_unique_human, updated_at) SELECT ?1, 1, 0, 0, ?2 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1) ON CONFLICT(document_id) DO UPDATE SET views_total = views_total + 1, updated_at = excluded.updated_at",
            vec![jstr(document_id), JsValue::from_f64(updated_at as f64)],
        )
        .await?;
        self.run(
            "INSERT INTO document_view_daily(document_id, day, views_total, views_human, views_unique_human) SELECT ?1, ?2, 1, 0, 0 WHERE EXISTS (SELECT 1 FROM documents WHERE id = ?1) ON CONFLICT(document_id, day) DO UPDATE SET views_total = views_total + 1",
            vec![jstr(document_id), jstr(&day)],
        )
        .await?;
        Ok(())
    }

    pub async fn record_metric(&self, route: &str, method: &str, status: u16, duration_ms: f64) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        self.run(
            "INSERT INTO request_metrics(id, route, method, status, duration_ms, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            vec![
                jstr(&id),
                jstr(route),
                jstr(method),
                JsValue::from_f64(f64::from(status)),
                JsValue::from_f64(duration_ms),
                JsValue::from_f64(now() as f64),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn route_performance(&self) -> Result<Value> {
        #[derive(Deserialize)]
        struct P {
            route: String,
            requests: i64,
            avg_ms: f64,
            p50_ms: f64,
            p95_ms: f64,
            errors: i64,
        }
        let rows: Vec<P> = self
            .all(
                "SELECT route, COUNT(*) AS requests, AVG(duration_ms) AS avg_ms, MAX(duration_ms) AS p95_ms, MAX(duration_ms) AS p50_ms, SUM(CASE WHEN status >= 400 THEN 1 ELSE 0 END) AS errors FROM request_metrics WHERE created_at >= ?1 GROUP BY route ORDER BY requests DESC LIMIT 40",
                vec![JsValue::from_f64((now() - 86_400) as f64)],
            )
            .await?;
        Ok(serde_json::json!({
            "window_seconds": 86400,
            "routes": rows.into_iter().map(|r| serde_json::json!({
                "route": r.route,
                "requests": r.requests,
                "avg_ms": r.avg_ms,
                "p50_ms": r.p50_ms,
                "p95_ms": r.p95_ms,
                "errors": r.errors,
            })).collect::<Vec<_>>()
        }))
    }
}

pub fn metadata_from_json(value: &str) -> PublishedMetadata {
    serde_json::from_str(value).unwrap_or_default()
}

#[derive(Deserialize)]
pub struct DocRow {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub source_language: String,
    pub status: String,
    pub document_kind: String,
    pub metadata_json: String,
    pub current_revision_id: String,
    pub published_revision_id: Option<String>,
    pub published_source_language: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub parent_id: Option<String>,
    pub sort_key: String,
}

const DOC_COLUMNS: &str = "d.id, d.owner_id, d.title, d.source_language, d.status, d.document_kind, d.metadata_json, d.current_revision_id, d.published_revision_id, d.published_source_language, d.created_at, d.updated_at, d.parent_id, d.sort_key";

pub fn document_from_row(row: DocRow) -> Document {
    Document {
        id: row.id,
        owner_id: row.owner_id,
        title: row.title,
        source_language: row.source_language,
        status: row.status,
        kind: row.document_kind,
        visibility: String::new(),
        metadata: metadata_json_value(&row.metadata_json),
        current_revision_id: row.current_revision_id,
        published_revision_id: row.published_revision_id,
        published_source_language: row.published_source_language,
        created_at: row.created_at,
        updated_at: row.updated_at,
        parent_id: row.parent_id,
        sort_key: row.sort_key,
    }
}

pub fn metadata_json_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or(Value::Object(Default::default()))
}

fn frac(value: &str) -> Option<fractional_index::FractionalIndex> {
    fractional_index::FractionalIndex::from_string(value).ok()
}

impl Db {
    async fn one<T: for<'a> Deserialize<'a>>(&self, sql: &str, params: Vec<JsValue>) -> Result<Option<T>> {
        self.first(sql, params).await
    }

    pub async fn root_user_id(&self) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct V { value: String }
        let row: Option<V> = self.first("SELECT value FROM app_meta WHERE key = 'root_user_id'", vec![]).await?;
        Ok(row.map(|r| r.value))
    }

    pub async fn initialize_root_user(&self, user_id: &str) -> Result<bool> {
        let changed = self.run(
            "INSERT INTO app_meta(key, value) VALUES ('root_user_id', ?1) ON CONFLICT(key) DO NOTHING",
            vec![jstr(user_id)],
        ).await?;
        Ok(changed == 1)
    }

    pub async fn authenticate_api_key(&self, user_id: &str, secret_hash: &str) -> Result<bool> {
        #[derive(Deserialize)]
        struct Id { id: String }
        let row: Option<Id> = self
            .first(
                "SELECT id FROM api_keys WHERE user_id = ?1 AND secret_hash = ?2 AND revoked_at IS NULL",
                vec![jstr(user_id), jstr(secret_hash)],
            )
            .await?;
        let Some(row) = row else { return Ok(false) };
        self.run(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            vec![jstr(&row.id), JsValue::from_f64(now() as f64)],
        )
        .await?;
        Ok(true)
    }

    pub async fn create_api_key(&self, user_id: &str, label: &str, prefix: &str, secret_hash: &str) -> Result<ApiKey> {
        let key = ApiKey { id: uuid::Uuid::new_v4().to_string(), label: label.to_owned(), prefix: prefix.to_owned(), created_at: now(), last_used_at: None };
        self.run(
            "INSERT INTO api_keys(id, user_id, label, prefix, secret_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            vec![jstr(&key.id), jstr(user_id), jstr(&key.label), jstr(&key.prefix), jstr(secret_hash), JsValue::from_f64(key.created_at as f64)],
        )
        .await?;
        Ok(key)
    }

    pub async fn list_api_keys(&self, user_id: &str) -> Result<Vec<ApiKey>> {
        self.all(
            "SELECT id, label, prefix, created_at, last_used_at FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL ORDER BY created_at DESC, id DESC",
            vec![jstr(user_id)],
        )
        .await
    }

    pub async fn revoke_api_key(&self, user_id: &str, id: &str) -> Result<bool> {
        let changed = self
            .run(
                "UPDATE api_keys SET revoked_at = ?3 WHERE id = ?1 AND user_id = ?2 AND revoked_at IS NULL",
                vec![jstr(id), jstr(user_id), JsValue::from_f64(now() as f64)],
            )
            .await?;
        Ok(changed == 1)
    }

    pub async fn list_documents(&self, owner_id: &str) -> Result<Vec<Document>> {
        let sql = format!("SELECT {DOC_COLUMNS} FROM documents d WHERE d.owner_id = ?1 ORDER BY d.updated_at DESC, d.id DESC");
        let rows: Vec<DocRow> = self.all(&sql, vec![jstr(owner_id)]).await?;
        Ok(rows.into_iter().map(|row| {
            let visibility = visibility_of(&row.status);
            let mut document = document_from_row(row);
            document.visibility = visibility;
            document
        }).collect())
    }

    pub async fn get_document(&self, id: &str) -> Result<Option<DocumentDetail>> {
        let sql = format!("SELECT {DOC_COLUMNS}, d.visibility AS visibility_col, r.id AS revision_id, r.document_id AS revision_document_id, r.content AS revision_content, r.message AS revision_message, r.author_id AS revision_author_id, r.created_at AS revision_created_at FROM documents d JOIN document_revisions r ON r.id = d.current_revision_id AND r.document_id = d.id WHERE d.id = ?1");
        #[derive(Deserialize)]
        struct DetailRow {
            #[serde(flatten)]
            doc: DocRow,
            visibility_col: String,
            revision_id: String,
            revision_document_id: String,
            revision_content: String,
            revision_message: String,
            revision_author_id: String,
            revision_created_at: i64,
        }
        let row: Option<DetailRow> = self.first(&sql, vec![jstr(id)]).await?;
        let Some(row) = row else { return Ok(None) };
        let mut document = document_from_row(row.doc);
        document.visibility = row.visibility_col;
        let descendant_count = self.count_descendants(id).await?;
        Ok(Some(DocumentDetail {
            document,
            revision: DocumentRevision {
                id: row.revision_id,
                document_id: row.revision_document_id,
                content: row.revision_content,
                message: row.revision_message,
                author_id: row.revision_author_id,
                created_at: row.revision_created_at,
            },
            descendant_count,
        }))
    }

    async fn count_descendants(&self, document_id: &str) -> Result<i64> {
        #[derive(Deserialize)]
        struct N { n: i64 }
        let row: Option<N> = self
            .first(
                "WITH RECURSIVE subtree(id) AS (SELECT id FROM documents WHERE parent_id = ?1 UNION ALL SELECT d.id FROM documents d JOIN subtree s ON d.parent_id = s.id) SELECT COUNT(*) AS n FROM subtree",
                vec![jstr(document_id)],
            )
            .await?;
        Ok(row.map(|r| r.n).unwrap_or(0))
    }

    async fn is_descendant(&self, document_id: &str, candidate_id: &str) -> Result<bool> {
        #[derive(Deserialize)]
        struct E { e: i64 }
        let row: Option<E> = self
            .first(
                "WITH RECURSIVE subtree(id) AS (SELECT id FROM documents WHERE id = ?1 UNION ALL SELECT d.id FROM documents d JOIN subtree s ON d.parent_id = s.id) SELECT EXISTS(SELECT 1 FROM subtree WHERE id = ?2) AS e",
                vec![jstr(document_id), jstr(candidate_id)],
            )
            .await?;
        Ok(row.map(|r| r.e == 1).unwrap_or(false))
    }

    async fn append_sort_key(&self, owner_id: &str, parent_id: Option<&str>) -> Result<String> {
        #[derive(Deserialize)]
        struct K { sort_key: String }
        let row: Option<K> = self
            .first(
                "SELECT sort_key FROM documents WHERE owner_id = ?1 AND parent_id IS ?2 AND sort_key != '' ORDER BY sort_key DESC LIMIT 1",
                vec![jstr(owner_id), parent_id.map_or(JsValue::NULL, jstr)],
            )
            .await?;
        Ok(match row.and_then(|r| frac(&r.sort_key)) {
            Some(key) => fractional_index::FractionalIndex::new_after(&key).to_string(),
            None => fractional_index::FractionalIndex::default().to_string(),
        })
    }

    pub async fn profile_document(&self, owner_id: &str) -> Result<DocumentDetail> {
        #[derive(Deserialize)]
        struct Id { id: String }
        let existing: Option<Id> = self.first("SELECT id FROM documents WHERE owner_id = ?1 AND document_kind = 'profile'", vec![jstr(owner_id)]).await?;
        if let Some(existing) = existing {
            if let Some(detail) = self.get_document(&existing.id).await? {
                return Ok(detail);
            }
        }
        let now_ts = now();
        let document_id = uuid::Uuid::new_v4().to_string();
        let revision_id = uuid::Uuid::new_v4().to_string();
        let sort_key = self.append_sort_key(owner_id, None).await?;
        self.run(
            "INSERT INTO documents(id, owner_id, title, source_language, status, visibility, document_kind, parent_id, sort_key, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at) VALUES (?1, ?2, 'Profile', 'und', 'draft', 'private', 'profile', NULL, ?3, '{}', ?4, NULL, NULL, ?5, ?6)",
            vec![jstr(&document_id), jstr(owner_id), jstr(&sort_key), jstr(&revision_id), JsValue::from_f64(now_ts as f64), JsValue::from_f64(now_ts as f64)],
        ).await?;
        self.run(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, '', 'Created profile document', ?3, ?4)",
            vec![jstr(&revision_id), jstr(&document_id), jstr(owner_id), JsValue::from_f64(now_ts as f64)],
        ).await?;
        self.get_document(&document_id).await?.ok_or_else(|| Error::RustError("profile document missing".to_owned()))
    }

    pub async fn create_document(&self, input: &NewDocument<'_>) -> Result<Option<DocumentDetail>> {
        if let Some(parent_id) = input.parent_id {
            #[derive(Deserialize)]
            struct P { owner_id: String, document_kind: String }
            let parent: Option<P> = self
                .first("SELECT owner_id, document_kind FROM documents WHERE id = ?1", vec![jstr(parent_id)])
                .await?;
            match parent {
                Some(p) if p.owner_id == input.author_id && p.document_kind == "article" => {}
                _ => return Ok(None),
            }
        }
        let now_ts = now();
        let document_id = uuid::Uuid::new_v4().to_string();
        let revision_id = uuid::Uuid::new_v4().to_string();
        let sort_key = self.append_sort_key(input.author_id, input.parent_id).await?;
        self.run(
            "INSERT INTO documents(id, owner_id, title, source_language, status, visibility, document_kind, parent_id, sort_key, metadata_json, current_revision_id, published_revision_id, published_source_language, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'draft', 'private', 'article', ?5, ?6, '{}', ?7, NULL, NULL, ?8, ?9)",
            vec![jstr(&document_id), jstr(input.author_id), jstr(input.title), jstr(input.source_language), input.parent_id.map_or(JsValue::NULL, jstr), jstr(&sort_key), jstr(&revision_id), JsValue::from_f64(now_ts as f64), JsValue::from_f64(now_ts as f64)],
        )
        .await?;
        self.run(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            vec![jstr(&revision_id), jstr(&document_id), jstr(input.content), jstr(input.message), jstr(input.author_id), JsValue::from_f64(now_ts as f64)],
        )
        .await?;
        self.get_document(&document_id).await
    }

    pub async fn save_document(&self, input: &DocumentSave<'_>) -> Result<Option<DocumentDetail>> {
        let Some(existing) = self.get_document(input.id).await? else { return Ok(None) };
        let now_ts = now();
        let revision_id = uuid::Uuid::new_v4().to_string();
        self.run(
            "INSERT INTO document_revisions(id, document_id, content, message, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            vec![jstr(&revision_id), jstr(input.id), jstr(input.content), jstr(input.message), jstr(input.author_id), JsValue::from_f64(now_ts as f64)],
        )
        .await?;
        self.run(
            "UPDATE documents SET title = ?2, source_language = ?3, metadata_json = ?4, current_revision_id = ?5, updated_at = ?6 WHERE id = ?1",
            vec![jstr(input.id), jstr(input.title), jstr(input.source_language), jstr(&input.metadata.to_string()), jstr(&revision_id), JsValue::from_f64(now_ts as f64)],
        )
        .await?;
        let mut detail = self.get_document(input.id).await?.unwrap_or_else(|| existing.clone());
        detail.descendant_count = existing.descendant_count;
        Ok(Some(detail))
    }

    pub async fn publish_document(&self, id: &str, visibility: &str) -> Result<Option<Document>> {
        let Some(existing) = self.get_document(id).await? else { return Ok(None) };
        let revision_id = existing.document.current_revision_id.clone();
        let source_language = existing.document.source_language.clone();
        let now_ts = now();
        let changed = self.run(
            "UPDATE documents SET status = 'published', visibility = ?5, published_revision_id = ?2, published_title = title, published_at = COALESCE(published_at, ?3), source_language = ?4, published_source_language = ?4, updated_at = ?3 WHERE id = ?1 AND current_revision_id = ?2",
            vec![jstr(id), jstr(&revision_id), JsValue::from_f64(now_ts as f64), jstr(&source_language), jstr(visibility)],
        ).await?;
        if changed == 0 { return Ok(None); }
        self.run("DELETE FROM document_translations WHERE document_id = ?1 AND source_revision_id != ?2", vec![jstr(id), jstr(&revision_id)]).await?;
        self.run("DELETE FROM document_metadata WHERE document_id = ?1 AND source_revision_id != ?2", vec![jstr(id), jstr(&revision_id)]).await?;
        self.run(
            "INSERT INTO ai_requests(id, document_id, source_revision_id, task, target_language, status, created_at) VALUES (?1, ?2, ?3, 'metadata', '', 'queued', ?4) ON CONFLICT(document_id, source_revision_id, task, target_language) DO UPDATE SET status = 'queued', result_summary = NULL, error = NULL, openai_lb_request_id = NULL, started_at = NULL, completed_at = NULL WHERE ai_requests.status = 'failed'",
            vec![jstr(&uuid::Uuid::new_v4().to_string()), jstr(id), jstr(&revision_id), JsValue::from_f64(now_ts as f64)],
        ).await?;
        Ok(self.get_document(id).await?.map(|d| d.document))
    }

    pub async fn delete_document(&self, id: &str) -> Result<bool> {
        Ok(self.run("DELETE FROM documents WHERE id = ?1", vec![jstr(id)]).await? == 1)
    }

    pub async fn move_document(&self, input: &DocumentMove<'_>) -> Result<MoveDocumentOutcome> {
        #[derive(Deserialize)]
        struct O { owner_id: String, document_kind: String }
        let moving: Option<O> = self.first("SELECT owner_id, document_kind FROM documents WHERE id = ?1", vec![jstr(input.id)]).await?;
        let Some(moving) = moving else { return Ok(MoveDocumentOutcome::Missing) };
        if moving.document_kind != "article" { return Ok(MoveDocumentOutcome::InvalidTarget) }
        if let Some(parent_id) = input.parent_id {
            let parent: Option<O> = self.first("SELECT owner_id, document_kind FROM documents WHERE id = ?1", vec![jstr(parent_id)]).await?;
            let Some(parent) = parent else { return Ok(MoveDocumentOutcome::InvalidTarget) };
            if parent.owner_id != moving.owner_id || parent.document_kind != "article" || self.is_descendant(input.id, parent_id).await? {
                return Ok(MoveDocumentOutcome::InvalidTarget);
            }
        }
        let Some(sort_key) = self.sibling_sort_key(&moving.owner_id, input.parent_id, input.after_id, input.id).await? else {
            return Ok(MoveDocumentOutcome::InvalidTarget);
        };
        self.run(
            "UPDATE documents SET parent_id = ?2, sort_key = ?3 WHERE id = ?1",
            vec![jstr(input.id), input.parent_id.map_or(JsValue::NULL, jstr), jstr(&sort_key)],
        ).await?;
        match self.get_document(input.id).await? {
            Some(document) => Ok(MoveDocumentOutcome::Moved(Box::new(document))),
            None => Ok(MoveDocumentOutcome::Missing),
        }
    }

    async fn sibling_sort_key(&self, owner_id: &str, parent_id: Option<&str>, after_id: Option<&str>, moving_id: &str) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct C { id: String, sort_key: String }
        let children: Vec<C> = self
            .all("SELECT id, sort_key FROM documents WHERE owner_id = ?1 AND parent_id IS ?2 ORDER BY sort_key, id", vec![jstr(owner_id), parent_id.map_or(JsValue::NULL, jstr)])
            .await?;
        let after_position = match after_id {
            None => None,
            Some(after) if after != moving_id => match children.iter().position(|c| c.id == after) {
                Some(position) => Some(position),
                None => return Ok(None),
            },
            Some(_) => return Ok(None),
        };
        let (lower, start) = match after_position {
            Some(position) => (frac(&children[position].sort_key), position + 1),
            None => (None, 0),
        };
        let upper = children[start..].iter().find(|c| c.id != moving_id).and_then(|c| frac(&c.sort_key));
        let index = match (lower, upper) {
            (Some(lower), Some(upper)) => fractional_index::FractionalIndex::new_between(&lower, &upper),
            (Some(lower), None) => Some(fractional_index::FractionalIndex::new_after(&lower)),
            (None, Some(upper)) => Some(fractional_index::FractionalIndex::new_before(&upper)),
            (None, None) => Some(fractional_index::FractionalIndex::default()),
        };
        Ok(index.map(|index| index.to_string()))
    }

    pub async fn document_readers(&self, document_id: &str) -> Result<Vec<DocumentReader>> {
        self.all(
            "SELECT user_id, created_at FROM document_readers WHERE document_id = ?1 ORDER BY created_at, user_id",
            vec![jstr(document_id)],
        ).await
    }

    pub async fn grant_document_reader(&self, document_id: &str, user_id: &str) -> Result<bool> {
        let changed = self.run(
            "INSERT INTO document_readers(document_id, user_id, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(document_id, user_id) DO NOTHING",
            vec![jstr(document_id), jstr(user_id), JsValue::from_f64(now() as f64)],
        ).await?;
        Ok(changed == 1)
    }

    pub async fn revoke_document_reader(&self, document_id: &str, user_id: &str) -> Result<bool> {
        let changed = self.run(
            "DELETE FROM document_readers WHERE document_id = ?1 AND user_id = ?2",
            vec![jstr(document_id), jstr(user_id)],
        ).await?;
        Ok(changed == 1)
    }

    pub async fn create_public_comment(&self, input: &NewDocumentComment<'_>) -> Result<Option<DocumentComment>> {
        #[derive(Deserialize)]
        struct P { published_revision_id: String, published_source_language: String }
        let publication: Option<P> = self.first(
            "SELECT published_revision_id, published_source_language FROM documents WHERE id = ?1 AND document_kind = 'article' AND status = 'published' AND visibility = 'public' AND published_revision_id IS NOT NULL AND published_source_language IS NOT NULL",
            vec![jstr(input.document_id)],
        ).await?;
        let Some(publication) = publication else { return Ok(None) };
        if input.language != publication.published_source_language {
            #[derive(Deserialize)]
            struct One { #[allow(dead_code)] one: i64 }
            let exists: Option<One> = self.first(
                "SELECT 1 AS one FROM document_translations WHERE document_id = ?1 AND source_revision_id = ?2 AND language = ?3",
                vec![jstr(input.document_id), jstr(&publication.published_revision_id), jstr(input.language)],
            ).await?;
            if exists.is_none() { return Ok(None) }
        }
        let comment = DocumentComment {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: input.document_id.to_owned(),
            author_id: input.author_id.to_owned(),
            language: input.language.to_owned(),
            content: input.content.to_owned(),
            quote: input.quote.map(str::to_owned),
            prefix: input.prefix.map(str::to_owned),
            suffix: input.suffix.map(str::to_owned),
            created_at: now(),
        };
        self.run(
            "INSERT INTO document_comments(id, document_id, source_revision_id, author_id, language, content, quote, prefix, suffix, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            vec![jstr(&comment.id), jstr(&comment.document_id), jstr(&publication.published_revision_id), jstr(&comment.author_id), jstr(&comment.language), jstr(&comment.content), comment.quote.as_deref().map_or(JsValue::NULL, jstr), comment.prefix.as_deref().map_or(JsValue::NULL, jstr), comment.suffix.as_deref().map_or(JsValue::NULL, jstr), JsValue::from_f64(comment.created_at as f64)],
        ).await?;
        Ok(Some(comment))
    }

    pub async fn publication_time(&self, id: &str) -> Result<Option<Option<i64>>> {
        #[derive(Deserialize)]
        struct P { published_at: Option<i64> }
        let row: Option<P> = self.first("SELECT published_at FROM documents WHERE id = ?1", vec![jstr(id)]).await?;
        Ok(row.map(|r| r.published_at))
    }

    pub async fn update_publication_time(&self, id: &str, published_at: i64) -> Result<Option<i64>> {
        let changed = self.run(
            "UPDATE documents SET published_at = ?2 WHERE id = ?1",
            vec![jstr(id), JsValue::from_f64(published_at as f64)],
        ).await?;
        Ok((changed == 1).then_some(published_at))
    }
}

pub fn visibility_of(status: &str) -> String {
    if status == "published" { "public".to_owned() } else { "private".to_owned() }
}
