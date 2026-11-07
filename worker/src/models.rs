//! Wire types shared with the web layer (copied verbatim from the axum app).
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

