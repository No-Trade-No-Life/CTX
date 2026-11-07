//! D1 data layer. 只读公开面（第一阶段），字段名与 axum 版 SQL 对齐。
use serde::Deserialize;
use serde_json::Value;
use worker::wasm_bindgen::JsValue;
use worker::*;

use crate::models::{
    DocumentComment, DocumentViewStats, PublicDocumentDetail, PublicDocumentSummary,
    PublicUserProfile, PublishedMetadata,
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
