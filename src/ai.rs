use std::collections::BTreeSet;
use std::time::Duration;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    db::{
        AiCredentials, AiRequest, DailyTimelineEntry, Database, DocumentDetail, MbtiAnalysis,
        PROFILE_SUMMARY_TASKS, PublishedArticle, PublishedMetadata, SchwartzValue, SummaryEvidence,
    },
    language::normalize_language_tag,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTask {
    Metadata,
    DetectLanguage,
    Polish,
}

impl AiTask {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::DetectLanguage => "detect_language",
            Self::Polish => "polish",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiOutput {
    pub output: String,
    pub proposed_content: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocumentTranslation {
    pub title: String,
    pub content: String,
    pub metadata: PublishedMetadata,
}

#[derive(Clone, Debug)]
pub struct DocumentMetadata {
    pub metadata: PublishedMetadata,
    pub inferred_language: String,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("the configured AI service could not be reached")]
    Request(#[from] reqwest::Error),
    #[error("the configured AI service rejected the request: {0}")]
    Rejected(String),
    #[error("the configured AI service returned an unreadable response")]
    Response,
}

#[derive(Debug, Deserialize)]
struct ResponsesOutputTextDone {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranslationOutput {
    title: String,
    content: String,
    metadata: PublishedMetadata,
}

#[derive(Debug, Deserialize)]
struct MetadataOutput {
    inferred_lang: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    short_summary: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    inferred_date: String,
    #[serde(default)]
    key_points: Vec<String>,
    #[serde(default)]
    audience: String,
    #[serde(default)]
    experience_summary: String,
    #[serde(default)]
    personality_analysis: String,
    #[serde(default)]
    mbti_analysis: MbtiAnalysis,
    #[serde(default)]
    schwartz_values: Vec<SchwartzValue>,
    #[serde(default)]
    unconscious_motivations: String,
    #[serde(default)]
    philosophical_references: String,
    #[serde(default)]
    daily_timeline: Vec<DailyTimelineEntry>,
}

enum ProfileSummaryPatch {
    Experience(String),
    Personality(String),
    Mbti(MbtiAnalysis),
    Schwartz(Vec<SchwartzValue>),
    Motivations(String),
    Philosophy(String),
    Timeline(Vec<DailyTimelineEntry>),
}

pub async fn run(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    task: AiTask,
) -> Result<AiOutput, AiError> {
    let system = system_prompt(&task);
    let content = format!(
        "Document kind: {}\nDocument title: {}\n\nDocument Markdown:\n{}",
        document.document.kind, document.document.title, document.revision.content
    );
    let output = request_output(credentials, &system, &content).await?;
    let proposed_content = matches!(task, AiTask::Polish).then(|| output.clone());
    Ok(AiOutput {
        output,
        proposed_content,
    })
}

pub async fn translate_document(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    source_language: &str,
    target_language: &str,
    source_metadata: &PublishedMetadata,
) -> Result<DocumentTranslation, AiError> {
    let instructions = translation_instructions(source_language, target_language);
    let content = format!(
        "Source language: {source_language}\nTarget language: {target_language}\n\nDocument title:\n{}\n\nDocument Markdown:\n{}\n\nSource editorial metadata JSON:\n{}",
        document.document.title,
        document.revision.content,
        serde_json::to_string(source_metadata).map_err(|_| AiError::Response)?
    );
    let output = request_output(credentials, &instructions, &content).await?;
    translation_from_output(&output, &document.document.kind)
}

pub async fn extract_document_metadata(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    published_articles: &[PublishedArticle],
) -> Result<DocumentMetadata, AiError> {
    let instructions = metadata_instructions(&document.document.kind);
    let content = metadata_input(document, published_articles);
    let output = request_output(credentials, &instructions, &content).await?;
    metadata_from_output(&output, &document.document.kind)
}

async fn extract_profile_summary(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    published_articles: &[PublishedArticle],
    task: &str,
) -> Result<ProfileSummaryPatch, AiError> {
    let instructions = profile_summary_instructions(task).ok_or(AiError::Response)?;
    let input = metadata_input(document, published_articles);
    let output = request_output(credentials, &instructions, &input).await?;
    profile_summary_from_output_with_articles(&output, task, published_articles).map_err(|error| {
        match error {
            AiError::Response => AiError::Rejected(format!(
                "profile summary {task} response was not accepted ({} bytes): {}",
                output.len(),
                response_preview(&output)
            )),
            error => error,
        }
    })
}

fn response_preview(output: &str) -> String {
    let compact = output.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = compact.chars().take(240).collect::<String>();
    if compact.chars().count() > 240 {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
fn profile_summary_from_output(output: &str, task: &str) -> Result<ProfileSummaryPatch, AiError> {
    profile_summary_from_output_with_articles(output, task, &[])
}

fn profile_summary_from_output_with_articles(
    output: &str,
    task: &str,
    published_articles: &[PublishedArticle],
) -> Result<ProfileSummaryPatch, AiError> {
    let mut value = json_value_from_output(output)?;
    normalize_profile_summary_value(&mut value, published_articles);
    match task {
        "profile_experience"
        | "profile_personality"
        | "profile_motivations"
        | "profile_philosophy" => {
            let field = match task {
                "profile_experience" => "experience_summary",
                "profile_personality" => "personality_analysis",
                "profile_motivations" => "unconscious_motivations",
                _ => "philosophical_references",
            };
            let content = summary_string(&value, field).ok_or(AiError::Response)?;
            Ok(match task {
                "profile_experience" => ProfileSummaryPatch::Experience(content),
                "profile_personality" => ProfileSummaryPatch::Personality(content),
                "profile_motivations" => ProfileSummaryPatch::Motivations(content),
                _ => ProfileSummaryPatch::Philosophy(content),
            })
        }
        "profile_mbti" => {
            let analysis = parse_mbti_analysis(
                summary_value(&value, "mbti_analysis")
                    .or_else(|| summary_value(&value, "mbti"))
                    .or_else(|| (value.is_object()).then_some(&value))
                    .ok_or(AiError::Response)?,
            )?;
            if !valid_mbti_type(&analysis) || analysis.dimensions.len() != 4 {
                return Err(AiError::Response);
            }
            const AXES: [&str; 4] = ["I/E", "N/S", "T/F", "J/P"];
            if !analysis
                .dimensions
                .iter()
                .zip(AXES)
                .all(|(dimension, axis)| valid_mbti_dimension(dimension, axis))
            {
                return Err(AiError::Response);
            }
            Ok(ProfileSummaryPatch::Mbti(analysis))
        }
        "profile_schwartz" => {
            const VALUES: [&str; 10] = [
                "self_direction",
                "stimulation",
                "hedonism",
                "achievement",
                "power",
                "security",
                "conformity",
                "tradition",
                "benevolence",
                "universalism",
            ];
            let values = parse_schwartz_values(
                summary_value(&value, "schwartz_values")
                    .or_else(|| summary_value(&value, "values"))
                    .or_else(|| (value.is_array()).then_some(&value))
                    .ok_or(AiError::Response)?,
            )?;
            if !valid_schwartz_values(&values, &VALUES) {
                return Err(AiError::Response);
            }
            Ok(ProfileSummaryPatch::Schwartz(values))
        }
        "profile_timeline" => {
            let entries = parse_daily_timeline(
                summary_value(&value, "daily_timeline")
                    .or_else(|| summary_value(&value, "timeline"))
                    .or_else(|| summary_value(&value, "entries"))
                    .or_else(|| (value.is_array()).then_some(&value))
                    .ok_or(AiError::Response)?,
            )?;
            if !valid_daily_timeline(&entries) {
                return Err(AiError::Response);
            }
            Ok(ProfileSummaryPatch::Timeline(entries))
        }
        _ => Err(AiError::Response),
    }
}

fn normalize_profile_summary_value(value: &mut Value, published_articles: &[PublishedArticle]) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    normalize_confidence(object, "confidence");
    if let Some(dimensions) = object
        .get_mut("mbti_analysis")
        .and_then(Value::as_object_mut)
    {
        normalize_confidence(dimensions, "confidence");
        if let Some(items) = dimensions
            .get_mut("dimensions")
            .and_then(Value::as_array_mut)
        {
            for item in items {
                let Some(item) = item.as_object_mut() else {
                    continue;
                };
                if !item.contains_key("axis")
                    && let Some(axis) = item.remove("dimension")
                {
                    item.insert("axis".to_owned(), axis);
                }
                normalize_confidence(item, "confidence");
                normalize_evidence(item, published_articles);
            }
        }
    }
    if let Some(values) = object.remove("schwartz_values") {
        object.insert(
            "schwartz_values".to_owned(),
            normalize_schwartz_values(values, published_articles),
        );
    }
    if let Some(entries) = object.remove("daily_timeline") {
        object.insert(
            "daily_timeline".to_owned(),
            normalize_daily_timeline(entries, published_articles),
        );
    }
}

fn normalize_confidence(object: &mut serde_json::Map<String, Value>, field: &str) {
    let Some(value) = object.get(field).cloned() else {
        return;
    };
    let confidence = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        .map(confidence_label);
    if let Some(confidence) = confidence {
        object.insert(field.to_owned(), Value::String(confidence.to_owned()));
    }
}

fn confidence_label(value: f64) -> &'static str {
    if value >= 0.8 {
        "high"
    } else if value >= 0.5 {
        "medium"
    } else if value > 0.0 {
        "low"
    } else {
        "undetermined"
    }
}

fn normalize_evidence(
    object: &mut serde_json::Map<String, Value>,
    published_articles: &[PublishedArticle],
) {
    let Some(items) = object.get_mut("evidence").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        if !item.contains_key("explanation")
            && let Some(observation) = item.remove("observation")
        {
            item.insert("explanation".to_owned(), observation);
        }
        if !item.contains_key("explanation")
            && let Some(reason) = item.remove("reason")
        {
            item.insert("explanation".to_owned(), reason);
        }
        if !item.contains_key("explanation")
            && let Some(quote) = item.get("quote").cloned()
        {
            item.insert("explanation".to_owned(), quote);
        }
        if !item.contains_key("article_title") {
            let title = item
                .get("article_url")
                .and_then(Value::as_str)
                .and_then(|url| url.strip_prefix("#/p/"))
                .and_then(|id| published_articles.iter().find(|article| article.id == id))
                .map(|article| article.title.clone())
                .unwrap_or_else(|| "Source article".to_owned());
            item.insert("article_title".to_owned(), Value::String(title));
        }
        if !item.contains_key("explanation") {
            item.insert(
                "explanation".to_owned(),
                Value::String("Source article evidence.".to_owned()),
            );
        }
    }
}

fn normalize_schwartz_values(value: Value, published_articles: &[PublishedArticle]) -> Value {
    match value {
        Value::Array(mut items) => {
            for item in &mut items {
                if let Some(item) = item.as_object_mut() {
                    normalize_evidence(item, published_articles);
                }
            }
            Value::Array(items)
        }
        Value::Object(values) => Value::Array(
            values
                .into_iter()
                .map(|(key, value)| {
                    let mut item = value.as_object().cloned().unwrap_or_default();
                    item.entry("key".to_owned())
                        .or_insert_with(|| Value::String(key));
                    normalize_evidence(&mut item, published_articles);
                    Value::Object(item)
                })
                .collect(),
        ),
        value => value,
    }
}

fn normalize_daily_timeline(value: Value, published_articles: &[PublishedArticle]) -> Value {
    let Value::Array(mut entries) = value else {
        return value;
    };
    for entry in &mut entries {
        if let Some(entry) = entry.as_object_mut() {
            if !entry.contains_key("summary")
                && let Some(description) = entry.remove("description")
            {
                entry.insert("summary".to_owned(), description);
            }
            if let Some(evidence) = entry.get("evidence").cloned()
                && !evidence.is_array()
            {
                entry.insert("evidence".to_owned(), Value::Array(vec![evidence]));
            }
            normalize_evidence(entry, published_articles);
        }
    }
    Value::Array(entries)
}

fn json_value_from_output(output: &str) -> Result<Value, AiError> {
    let trimmed = output.trim();
    let unwrapped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .map(|value| value.strip_suffix("```").unwrap_or(value).trim())
        .unwrap_or(trimmed);
    serde_json::from_str(unwrapped).map_err(|_| AiError::Response)
}

fn summary_value<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    value
        .get(field)
        .or_else(|| {
            value
                .get("metadata")
                .and_then(|metadata| metadata.get(field))
        })
        .or_else(|| value.get("result").and_then(|result| result.get(field)))
        .or_else(|| value.get("data").and_then(|data| data.get(field)))
}

fn summary_string(value: &Value, field: &str) -> Option<String> {
    summary_value(value, field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| value.as_str().map(str::to_owned))
}

fn parse_mbti_analysis(value: &Value) -> Result<MbtiAnalysis, AiError> {
    if let Ok(analysis) = serde_json::from_value::<MbtiAnalysis>(value.clone()) {
        return Ok(analysis);
    }
    let Some(object) = value.as_object() else {
        return Err(AiError::Response);
    };
    let mut normalized = object.clone();
    if !normalized.contains_key("type_code") {
        for alias in ["type", "classification", "result"] {
            if let Some(type_code) = object.get(alias).and_then(Value::as_str) {
                normalized.insert("type_code".to_owned(), Value::String(type_code.to_owned()));
                break;
            }
        }
    }
    if !normalized.contains_key("dimensions")
        && let Some(dimensions) = object.get("dimension_analysis")
    {
        normalized.insert("dimensions".to_owned(), dimensions.clone());
    }
    let Some(dimensions) = normalized.get("dimensions") else {
        return Err(AiError::Response);
    };
    if let Some(dimensions) = dimensions.as_object() {
        let normalized_dimensions = dimensions
            .iter()
            .map(|(axis, dimension)| {
                let mut dimension = dimension.as_object().cloned().unwrap_or_default();
                dimension
                    .entry("axis".to_owned())
                    .or_insert_with(|| Value::String(axis.clone()));
                Value::Object(dimension)
            })
            .collect::<Vec<_>>();
        normalized.insert("dimensions".to_owned(), Value::Array(normalized_dimensions));
    }
    serde_json::from_value(Value::Object(normalized)).map_err(|_| AiError::Response)
}

fn parse_schwartz_values(value: &Value) -> Result<Vec<SchwartzValue>, AiError> {
    if let Ok(values) = serde_json::from_value::<Vec<SchwartzValue>>(value.clone()) {
        return Ok(values);
    }
    let Some(values) = value.as_array() else {
        return Err(AiError::Response);
    };
    let normalized = values
        .iter()
        .map(|value| {
            let mut object = value.as_object().cloned().unwrap_or_default();
            if !object.contains_key("key") {
                for alias in ["name", "value", "dimension"] {
                    if let Some(name) = value.get(alias) {
                        object.insert("key".to_owned(), name.clone());
                        break;
                    }
                }
            }
            for field in ["score", "rank"] {
                if let Some(number) = object.get(field).and_then(Value::as_str)
                    && let Ok(number) = number.parse::<u8>()
                {
                    object.insert(field.to_owned(), Value::from(number));
                }
            }
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    serde_json::from_value(Value::Array(normalized)).map_err(|_| AiError::Response)
}

fn parse_daily_timeline(value: &Value) -> Result<Vec<DailyTimelineEntry>, AiError> {
    if let Ok(entries) = serde_json::from_value::<Vec<DailyTimelineEntry>>(value.clone()) {
        return Ok(entries);
    }
    let Some(object) = value.as_object() else {
        return Err(AiError::Response);
    };
    let entries = object
        .iter()
        .map(|(date, summary)| {
            let text = summary
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    summary
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            json!({"date": date, "summary": text, "evidence": []})
        })
        .collect::<Vec<_>>();
    serde_json::from_value(Value::Array(entries)).map_err(|_| AiError::Response)
}

fn apply_profile_summary_patch(
    metadata: &mut PublishedMetadata,
    patch: ProfileSummaryPatch,
) -> &'static str {
    match patch {
        ProfileSummaryPatch::Experience(content) => {
            metadata.experience_summary = content;
            "experience"
        }
        ProfileSummaryPatch::Personality(content) => {
            metadata.personality_analysis = content;
            "personality"
        }
        ProfileSummaryPatch::Mbti(analysis) => {
            metadata.mbti_analysis = analysis;
            "MBTI"
        }
        ProfileSummaryPatch::Schwartz(values) => {
            metadata.schwartz_values = values;
            "Schwartz values"
        }
        ProfileSummaryPatch::Motivations(content) => {
            metadata.unconscious_motivations = content;
            "unconscious motivations"
        }
        ProfileSummaryPatch::Philosophy(content) => {
            metadata.philosophical_references = content;
            "philosophical references"
        }
        ProfileSummaryPatch::Timeline(entries) => {
            metadata.daily_timeline = entries;
            "daily timeline"
        }
    }
}

pub async fn run_worker(database: Database) {
    if let Err(error) = database.requeue_running_ai_requests() {
        eprintln!("CTX could not recover interrupted AI requests: {error}");
    }
    if let Err(error) = database.schedule_profile_summary_tasks_if_due() {
        eprintln!("CTX could not schedule daily profile summaries: {error}");
    }
    let mut next_daily_schedule = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if tokio::time::Instant::now() >= next_daily_schedule {
            if let Err(error) = database.schedule_profile_summary_tasks_if_due() {
                eprintln!("CTX could not schedule daily profile summaries: {error}");
            }
            next_daily_schedule = tokio::time::Instant::now() + Duration::from_secs(60);
        }
        match database.claim_next_ai_request() {
            Ok(Some(request)) => {
                let result = process_ai_request(&database, &request).await;
                let completion = match result {
                    Ok(summary) => database.complete_ai_request(&request.id, &summary),
                    Err(error) => database.fail_ai_request(&request.id, &error),
                };
                if let Err(error) = completion {
                    eprintln!("CTX could not record AI request completion: {error}");
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(500)).await,
            Err(error) => {
                eprintln!("CTX could not claim an AI request: {error}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn process_ai_request(database: &Database, request: &AiRequest) -> Result<String, String> {
    let document = database
        .published_document_detail(&request.document_id, &request.source_revision_id)
        .map_err(|error| error.to_string())?;
    let Some(document) = document else {
        return Ok("Superseded by a newer published revision".to_owned());
    };
    let credentials = database
        .ai_credentials()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "AI is not configured by the root administrator".to_owned())?;
    match request.task.as_str() {
        task if PROFILE_SUMMARY_TASKS.contains(&task) => {
            if document.document.kind != "profile" {
                return Err("profile summary tasks require a profile document".to_owned());
            }
            let published_articles = database
                .published_articles_for_author(&document.document.owner_id)
                .map_err(|error| error.to_string())?;
            if document.document.source_language == "und" {
                return Err("profile metadata is still being extracted".to_owned());
            }
            let patch = extract_profile_summary(&credentials, &document, &published_articles, task)
                .await
                .map_err(|error| error.to_string())?;
            let Some(mut metadata) = database
                .published_source_metadata(
                    &request.document_id,
                    &request.source_revision_id,
                    &document.document.source_language,
                )
                .map_err(|error| error.to_string())?
            else {
                return Err("profile metadata is still being extracted".to_owned());
            };
            let summary_name = apply_profile_summary_patch(&mut metadata, patch);
            let stored = database
                .apply_profile_summary(
                    &request.document_id,
                    &request.source_revision_id,
                    &document.document.source_language,
                    &metadata,
                )
                .map_err(|error| error.to_string())?;
            if !stored {
                return Ok("Superseded by a newer published revision".to_owned());
            }
            database
                .invalidate_profile_translation_metadata(
                    &request.document_id,
                    &request.source_revision_id,
                )
                .map_err(|error| error.to_string())?;
            Ok(format!("Updated {summary_name} profile summary"))
        }
        "metadata" => {
            let published_articles = (document.document.kind == "profile")
                .then(|| database.published_articles_for_author(&document.document.owner_id))
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or_default();
            let extracted = extract_document_metadata(&credentials, &document, &published_articles)
                .await
                .map_err(|error| error.to_string())?;
            let source_language = database
                .apply_published_metadata(
                    &request.document_id,
                    &request.source_revision_id,
                    &extracted.metadata,
                    &extracted.inferred_language,
                )
                .map_err(|error| error.to_string())?;
            let Some(source_language) = source_language else {
                return Ok("Superseded by a newer published revision".to_owned());
            };
            database
                .store_published_metadata(
                    &request.document_id,
                    &request.source_revision_id,
                    &source_language,
                    &extracted.metadata,
                )
                .map_err(|error| error.to_string())?;
            if document.document.kind == "profile" {
                database
                    .invalidate_profile_translation_metadata(
                        &request.document_id,
                        &request.source_revision_id,
                    )
                    .map_err(|error| error.to_string())?;
                database
                    .enqueue_profile_summary_tasks(
                        &request.document_id,
                        &request.source_revision_id,
                        None,
                    )
                    .map_err(|error| error.to_string())?;
            } else {
                database
                    .requeue_translations_missing_metadata(
                        &request.document_id,
                        &request.source_revision_id,
                    )
                    .map_err(|error| error.to_string())?;
            }
            let profile_summary_refresh = if document.document.kind == "profile" {
                "; refreshed all profile summaries from published articles"
            } else {
                ""
            };
            Ok(format!(
                "Metadata extracted; published source language is {source_language}{profile_summary_refresh}"
            ))
        }
        "translate" => {
            if document.document.source_language == "und" {
                return Err("the source language is still being inferred".to_owned());
            }
            let metadata = database
                .published_source_metadata(
                    &request.document_id,
                    &request.source_revision_id,
                    &document.document.source_language,
                )
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "source metadata is still being extracted".to_owned())?;
            let translation = translate_document(
                &credentials,
                &document,
                &document.document.source_language,
                &request.target_language,
                &metadata,
            )
            .await
            .map_err(|error| error.to_string())?;
            if translation.metadata.is_empty()
                || translation.metadata.inferred_lang != request.target_language
            {
                return Err(
                    "the translation response did not include metadata for the requested language"
                        .to_owned(),
                );
            }
            let stored = database
                .store_published_translation(
                    &request.document_id,
                    &request.source_revision_id,
                    &request.target_language,
                    &translation.title,
                    &translation.content,
                    &translation.metadata,
                )
                .map_err(|error| error.to_string())?;
            Ok(if stored {
                format!("Translated Markdown to {}", request.target_language)
            } else {
                "Superseded by a newer published revision".to_owned()
            })
        }
        _ => Err("unsupported AI request task".to_owned()),
    }
}

fn translation_from_output(
    output: &str,
    _document_kind: &str,
) -> Result<DocumentTranslation, AiError> {
    let translation: TranslationOutput =
        serde_json::from_str(output).map_err(|_| AiError::Response)?;
    if translation.title.trim().is_empty()
        || translation.content.trim().is_empty()
        || translation.metadata.is_empty()
    {
        return Err(AiError::Response);
    }
    Ok(DocumentTranslation {
        title: translation.title,
        content: translation.content,
        metadata: translation.metadata,
    })
}

fn metadata_from_output(output: &str, _document_kind: &str) -> Result<DocumentMetadata, AiError> {
    let output: MetadataOutput = serde_json::from_str(output).map_err(|_| AiError::Response)?;
    let inferred_language = output.inferred_lang;
    let inferred_language = normalize_language_tag(&inferred_language)
        .filter(|language| language != "und")
        .ok_or(AiError::Response)?;
    let metadata = PublishedMetadata {
        description: output.description,
        summary: output.summary,
        short_summary: output.short_summary,
        tags: output.tags,
        inferred_date: output.inferred_date,
        inferred_lang: inferred_language.clone(),
        key_points: output.key_points,
        audience: output.audience,
        experience_summary: output.experience_summary,
        personality_analysis: output.personality_analysis,
        mbti_analysis: output.mbti_analysis,
        schwartz_values: output.schwartz_values,
        unconscious_motivations: output.unconscious_motivations,
        philosophical_references: output.philosophical_references,
        daily_timeline: output.daily_timeline,
    };
    if metadata.is_empty() {
        return Err(AiError::Response);
    }
    Ok(DocumentMetadata {
        metadata,
        inferred_language,
    })
}

fn valid_mbti_type(analysis: &MbtiAnalysis) -> bool {
    let type_code = analysis.type_code.as_bytes();
    valid_confidence(&analysis.confidence)
        && (analysis.type_code == "undetermined"
            || (type_code.len() == 4
                && matches!(type_code[0], b'I' | b'E')
                && matches!(type_code[1], b'N' | b'S')
                && matches!(type_code[2], b'T' | b'F')
                && matches!(type_code[3], b'J' | b'P')))
}

fn valid_mbti_dimension(dimension: &crate::db::MbtiDimension, axis: &str) -> bool {
    let preference_is_valid = dimension.preference == "undetermined"
        || axis
            .split('/')
            .any(|preference| preference == dimension.preference);
    dimension.axis == axis
        && preference_is_valid
        && valid_confidence(&dimension.confidence)
        && dimension.evidence.iter().all(valid_evidence)
}

fn valid_schwartz_values(values: &[SchwartzValue], expected_values: &[&str]) -> bool {
    let mut keys = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    values.iter().all(|value| {
        keys.insert(value.key.as_str())
            && ranks.insert(value.rank)
            && value.rank > 0
            && value.rank <= expected_values.len() as u8
            && value.score <= 100
            && value.evidence.iter().all(valid_evidence)
    }) && keys.len() == expected_values.len()
        && expected_values.iter().all(|key| keys.contains(key))
        && ranks.len() == expected_values.len()
}

fn valid_daily_timeline(entries: &[DailyTimelineEntry]) -> bool {
    entries.iter().all(|entry| {
        NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").is_ok()
            && !entry.summary.trim().is_empty()
            && entry.evidence.iter().all(valid_evidence)
    })
}

fn valid_evidence(evidence: &SummaryEvidence) -> bool {
    evidence.article_url.starts_with("#/p/")
        && !evidence.article_title.trim().is_empty()
        && !evidence.explanation.trim().is_empty()
}

fn valid_confidence(confidence: &str) -> bool {
    matches!(confidence, "high" | "medium" | "low" | "undetermined")
}

fn metadata_instructions(document_kind: &str) -> String {
    let profile_instructions = (document_kind == "profile").then_some(
        "This is a personal profile document. The input includes the complete set of ordinary articles published by this author. The metadata task only extracts the common editorial fields (description, summary, short_summary, tags, inferred_date, inferred_lang, key_points, and audience). Return empty values for all profile summary fields: experience_summary, personality_analysis, mbti_analysis, schwartz_values, unconscious_motivations, philosophical_references, and daily_timeline. Those seven summaries are generated by independent scheduled or manually triggered tasks, so do not combine them here. Do not invent facts, dates, sources, or links.",
    );
    format!(
        "You are CTX's editorial metadata assistant. Extract structured metadata from Markdown without inventing facts, sources, or dates. Return JSON only with these fields: description (one sentence, at most 100 characters when practical), summary (one paragraph), short_summary (2-3 sentences for an article list or RSS description), tags (3-8 concise strings), inferred_date (YYYY-MM-DD or an empty string), inferred_lang (the document's original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES), key_points (3-5 concise strings), audience (a short description), experience_summary, personality_analysis, mbti_analysis, schwartz_values, unconscious_motivations, philosophical_references, and daily_timeline. For this metadata task, return empty values for all seven profile summary fields; independent profile summary tasks populate them later. Preserve the author's title; do not generate or change it. Summary is part of this metadata extraction; do not create a separate summary artifact. {}",
        profile_instructions.unwrap_or_default()
    )
}

fn profile_summary_instructions(task: &str) -> Option<String> {
    let output_shape = match task {
        "profile_experience" => {
            "experience_summary (concise GitHub Flavored Markdown in a factual resume format)"
        }
        "profile_personality" => {
            "personality_analysis (concise GitHub Flavored Markdown; begin with a non-clinical AI-reading disclaimer and describe only observable writing patterns)"
        }
        "profile_mbti" => {
            "mbti_analysis for observable writing-style preferences only (type_code, confidence, and exactly four dimensions in I/E, N/S, T/F, J/P order; every dimension includes preference, confidence, and evidence; this is not a claim about private personality or a diagnosis)"
        }
        "profile_schwartz" => {
            "schwartz_values (exactly ten values with keys self_direction, stimulation, hedonism, achievement, power, security, conformity, tradition, benevolence, universalism; each has score 0-100, unique rank 1-10, and evidence)"
        }
        "profile_motivations" => {
            "unconscious_motivations (concise GitHub Flavored Markdown with explicitly tentative, source-grounded interpretations; never infer trauma or mental-health conditions)"
        }
        "profile_philosophy" => {
            "philosophical_references (concise GitHub Flavored Markdown identifying expressed propositions and supported philosophical references with evidence links)"
        }
        "profile_timeline" => {
            "daily_timeline (zero or more entries; each has an explicit YYYY-MM-DD date, one sentence describing the day's action and result, and evidence; dates need not be ordered)"
        }
        _ => return None,
    };
    let constraints = match task {
        "profile_mbti" => {
            " Return only the compact JSON object, with at most one strongest evidence item per dimension."
        }
        "profile_schwartz" => {
            " Return only the compact JSON object, with at most one strongest evidence item per value."
        }
        "profile_timeline" => {
            " Return only the compact JSON object, with at most one concise entry per distinct source date and at most one evidence item per entry."
        }
        _ => " Return only the compact JSON object and keep Markdown concise.",
    };
    Some(format!(
        "You are CTX's independent editorial corpus-analysis assistant. Read the profile document and the complete set of the author's published ordinary articles supplied in the input. Generate only {output_shape}. Use only explicit facts from those sources; every evidence item must have article_title, article_url, and explanation, and article_url must exactly match a supplied #/p/... link. MBTI confidence fields must be one of high, medium, low, or undetermined. Return JSON only with exactly the requested field. An empty Markdown string or empty timeline is valid when the sources do not support a conclusion. Do not add a Markdown code fence or commentary.{constraints}"
    ))
}

fn metadata_input(document: &DocumentDetail, published_articles: &[PublishedArticle]) -> String {
    if document.document.kind != "profile" {
        return format!(
            "Document title:\n{}\n\nDocument Markdown:\n{}",
            document.document.title, document.revision.content
        );
    }
    let articles = if published_articles.is_empty() {
        "No ordinary published articles.".to_owned()
    } else {
        published_articles
            .iter()
            .enumerate()
            .map(|(index, article)| {
                format!(
                    "## Published article {}\nSource title: {}\nSource link: #/p/{}\nDocument date: {}\nPublic publication date: {}\n\nMarkdown:\n{}",
                    index + 1,
                    article.title,
                    article.id,
                    article.inferred_date,
                    article.published_date,
                    article.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    format!(
        "Profile document title:\n{}\n\nProfile document Markdown:\n{}\n\nAll ordinary articles published by this author:\n{}",
        document.document.title, document.revision.content, articles
    )
}

fn translation_instructions(source_language: &str, target_language: &str) -> String {
    format!(
        "You are CTX's Markdown translation assistant. Translate the title, complete GitHub Flavored Markdown document, and editorial metadata from {source_language} into {target_language}. Preserve every Markdown structure and all factual meaning: headings, links and their URLs, code blocks, inline code, formulas, images, task lists, tables, HTML, and frontmatter. For Mermaid fenced code blocks, preserve valid Mermaid syntax, identifiers, directives, relationships, and edge operators; translate only reader-facing labels, titles, and subgraph labels. Do not leave Mermaid labels in the source language. Do not change non-text elements or code. Translate description, summary, short_summary, tags, key_points, audience, experience_summary, personality_analysis, unconscious_motivations, philosophical_references, the explanations and article titles inside mbti_analysis and schwartz_values, and the summaries and evidence inside daily_timeline. Preserve all type codes, MBTI axes, value keys, scores, ranks, ISO dates, and `#/p/...` evidence links exactly. Preserve inferred_date and set metadata.inferred_lang to {target_language}. Keep the non-clinical and tentative limitations in each profile summary. Use fluent, idiomatic language for a reader familiar with the subject. {} Return only a JSON object with string fields title and content and an object field metadata.",
        japanese_translation_rules(target_language)
    )
}

fn japanese_translation_rules(target_language: &str) -> &'static str {
    if target_language == "ja-JP" {
        "For Japanese, use natural native phrasing in a polite, formal です・ます style appropriate for technical or professional documentation. Use Japanese-standard Jōyō kanji forms, never traditional Chinese glyph variants."
    } else {
        ""
    }
}

async fn request_output(
    credentials: &AiCredentials,
    instructions: &str,
    input: &str,
) -> Result<String, AiError> {
    let endpoint = format!("{}/responses", credentials.base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(&credentials.api_key)
        .json(&request_body(&credentials.model, instructions, input))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AiError::Rejected(response.text().await?));
    }
    let body = response.text().await?;
    output_text_from_sse(&body).ok_or_else(|| {
        AiError::Rejected(format!(
            "AI response contained no text output events: {}",
            sse_event_types(&body)
        ))
    })
}

fn request_body(model: &str, instructions: &str, input: &str) -> serde_json::Value {
    json!({
        "model": model,
        "instructions": instructions,
        "stream": true,
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": input
            }]
        }]
    })
}

fn output_text_from_sse(sse: &str) -> Option<String> {
    let output = sse
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<ResponsesOutputTextDone>(data.trim()).ok())
        .filter(|event| event.kind == "response.output_text.done")
        .filter_map(|event| event.text)
        .collect::<String>();
    (!output.trim().is_empty()).then_some(output)
}

fn sse_event_types(sse: &str) -> String {
    let mut kinds = BTreeSet::new();
    for data in sse.lines().filter_map(|line| line.strip_prefix("data:")) {
        if let Ok(value) = serde_json::from_str::<Value>(data.trim())
            && let Some(kind) = value.get("type").and_then(Value::as_str)
        {
            kinds.insert(kind.to_owned());
        }
    }
    if kinds.is_empty() {
        "none".to_owned()
    } else {
        kinds.into_iter().collect::<Vec<_>>().join(",")
    }
}

fn system_prompt(task: &AiTask) -> String {
    match task {
        AiTask::Metadata => "You are CTX's editorial metadata assistant. Return valid JSON only with description, summary, short_summary, tags, inferred_date, inferred_lang, key_points, audience, experience_summary, personality_analysis, mbti_analysis, schwartz_values, unconscious_motivations, philosophical_references, and daily_timeline. Preserve the author's factual claims and do not invent sources. Profile structured summaries are generated by independent scheduled tasks; return empty profile summary fields here.".to_owned(),
        AiTask::DetectLanguage => "You are CTX's language detector. Read the document title and Markdown, then return only its original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES. Do not add explanation, punctuation, or Markdown. If the language cannot be determined, return und.".to_owned(),
        AiTask::Polish => "You are CTX's Markdown editing assistant. Polish the complete document for clarity, flow, precision, and concise professional tone while preserving the author's facts, intent, and voice. Return only the revised GitHub Flavored Markdown. Preserve every Markdown structure and meaning: headings, links and URLs, inline code, code blocks, Mermaid syntax, images, task lists, tables, HTML, frontmatter, and formulas. Do not add sources, claims, or editorial commentary.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        AiTask, ProfileSummaryPatch, metadata_from_output, metadata_instructions,
        output_text_from_sse, profile_summary_from_output,
        profile_summary_from_output_with_articles, profile_summary_instructions, request_body,
        system_prompt, translation_from_output, translation_instructions,
    };
    use crate::db::PublishedArticle;

    fn complete_profile_metadata() -> Value {
        let evidence = json!({
            "article_title": "A source article",
            "article_url": "#/p/article-1",
            "explanation": "The article states the observation explicitly."
        });
        let values = [
            "self_direction",
            "stimulation",
            "hedonism",
            "achievement",
            "power",
            "security",
            "conformity",
            "tradition",
            "benevolence",
            "universalism",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, key)| {
            json!({
                "key": key,
                "score": 100 - index * 10,
                "rank": index + 1,
                "evidence": [evidence.clone()]
            })
        })
        .collect::<Vec<_>>();
        json!({
            "description": "A profile",
            "summary": "A profile summary.",
            "short_summary": "A short profile summary.",
            "tags": ["Profile"],
            "inferred_date": "",
            "inferred_lang": "en-US",
            "key_points": ["A point"],
            "audience": "Readers",
            "experience_summary": "### Experience\n\n- Documented projects.",
            "personality_analysis": "### Writing patterns\n\n- Evidence-based writing.",
            "mbti_analysis": {
                "type_code": "INTJ",
                "confidence": "medium",
                "dimensions": [
                    {"axis": "I/E", "preference": "I", "confidence": "medium", "evidence": [evidence.clone()]},
                    {"axis": "N/S", "preference": "N", "confidence": "medium", "evidence": [evidence.clone()]},
                    {"axis": "T/F", "preference": "T", "confidence": "medium", "evidence": [evidence.clone()]},
                    {"axis": "J/P", "preference": "J", "confidence": "medium", "evidence": [evidence.clone()]}
                ]
            },
            "schwartz_values": values,
            "unconscious_motivations": "### Tentative motivation\n\n- A source-grounded interpretation.",
            "philosophical_references": "### Proposition\n\n- A source-grounded reference.",
            "daily_timeline": [{
                "date": "2026-01-01",
                "summary": "The author published a documented project result.",
                "evidence": [evidence]
            }]
        })
    }

    #[test]
    fn sends_responses_input_as_message_items() {
        assert_eq!(
            request_body("gpt-5.6-luna", "follow the context", "# Document"),
            json!({
                "model": "gpt-5.6-luna",
                "instructions": "follow the context",
                "stream": true,
                "input": [{
                    "role": "user",
                    "content": [{
                        "type": "input_text",
                        "text": "# Document"
                    }]
                }]
            })
        );
    }

    #[test]
    fn extracts_output_text_done_events_in_order() {
        let sse = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ignored\"}\n\n",
            "event: response.output_text.done\n",
            "data: {\"type\":\"response.output_text.done\",\"text\":\"first \"}\n\n",
            "data: {\"type\":\"response.output_text.done\",\"text\":\"second\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n"
        );

        assert_eq!(output_text_from_sse(sse).as_deref(), Some("first second"));
    }

    #[test]
    fn rejects_sse_without_output_text_done_events() {
        let sse = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n"
        );

        assert_eq!(output_text_from_sse(sse), None);
    }

    #[test]
    fn requires_a_complete_structured_translation() {
        let translation = translation_from_output(
            r##"{"title":"Translated title","content":"# Translated\n\n- [x] Done","metadata":{"description":"Translated description","summary":"Translated summary","short_summary":"Translated short summary","tags":["CTX"],"inferred_date":"","inferred_lang":"en-US","key_points":["Done"],"audience":"Readers","personality_analysis":"Writing patterns"}}"##,
            "article",
        )
        .unwrap();
        assert_eq!(translation.title, "Translated title");
        assert_eq!(translation.content, "# Translated\n\n- [x] Done");
        assert_eq!(
            translation.metadata.personality_analysis,
            "Writing patterns"
        );
        assert!(
            translation_from_output(r#"{"title":"Only a title","content":""}"#, "article").is_err()
        );
        assert!(translation_from_output(r##"{"title":"No metadata","content":"# Content","metadata":{"inferred_lang":"en-US"}}"##, "article").is_err());
        assert!(translation_from_output("not JSON", "article").is_err());
    }

    #[test]
    fn keeps_mermaid_syntax_and_translates_reader_facing_labels() {
        let instructions = translation_instructions("en-US", "ja-JP");
        assert!(instructions.contains("preserve valid Mermaid syntax, identifiers, directives, relationships, and edge operators"));
        assert!(
            instructions
                .contains("translate only reader-facing labels, titles, and subgraph labels")
        );
        assert!(instructions.contains("Do not leave Mermaid labels in the source language"));
        assert!(instructions.contains("experience_summary"));
        assert!(instructions.contains("personality_analysis"));
        assert!(instructions.contains("です・ます"));
    }

    #[test]
    fn polish_task_returns_a_markdown_preserving_instruction() {
        let prompt = system_prompt(&AiTask::Polish);
        assert_eq!(AiTask::Polish.as_str(), "polish");
        assert!(prompt.contains("Return only the revised GitHub Flavored Markdown"));
        assert!(prompt.contains("Mermaid syntax"));
    }

    #[test]
    fn requires_structured_metadata_with_a_canonical_original_language() {
        let metadata = metadata_from_output(
            r###"{"description":"A summary","inferred_lang":"ja-jp","tags":["CTX"],"experience_summary":"Worked on CTX documentation.","personality_analysis":"## Writing patterns"}"###,
            "article",
        )
        .unwrap();
        assert_eq!(metadata.inferred_language, "ja-JP");
        assert_eq!(
            metadata.metadata.experience_summary,
            "Worked on CTX documentation."
        );
        assert_eq!(
            metadata.metadata.personality_analysis,
            "## Writing patterns"
        );
        assert!(metadata_from_output(r#"{"inferred_lang":"ja-JP"}"#, "article").is_err());
        assert!(metadata_from_output(r#"{"description":"No language"}"#, "article").is_err());
        assert!(metadata_from_output(r#"{"inferred_lang":"Japanese"}"#, "article").is_err());
    }

    #[test]
    fn profile_metadata_requires_all_article_summaries() {
        let instructions = metadata_instructions("profile");
        assert!(instructions.contains("complete set of ordinary articles"));
        assert!(instructions.contains("common editorial fields"));
        assert!(instructions.contains("personality_analysis"));
        assert!(instructions.contains("mbti_analysis"));
        assert!(instructions.contains("schwartz_values"));
        assert!(instructions.contains("daily_timeline"));
        assert!(instructions.contains("independent scheduled or manually triggered tasks"));
        assert!(metadata_instructions("article").contains("independent profile summary tasks"));
    }

    #[test]
    fn profile_metadata_can_be_completed_by_independent_tasks() {
        let output = complete_profile_metadata();
        let metadata = metadata_from_output(&output.to_string(), "profile").unwrap();
        assert_eq!(metadata.metadata.mbti_analysis.type_code, "INTJ");
        assert_eq!(metadata.metadata.mbti_analysis.dimensions.len(), 4);
        assert_eq!(metadata.metadata.schwartz_values.len(), 10);
        assert_eq!(metadata.metadata.daily_timeline[0].date, "2026-01-01");

        let mut malformed = complete_profile_metadata();
        malformed["schwartz_values"][0]["rank"] = json!(0);
        assert!(metadata_from_output(&malformed.to_string(), "profile").is_ok());
    }

    #[test]
    fn parses_independent_profile_summary_tasks() {
        let markdown = profile_summary_from_output(
            "{\"experience_summary\":\"### Experience\\n\\n- Built CTX.\"}",
            "profile_experience",
        )
        .unwrap();
        assert!(matches!(
            markdown,
            super::ProfileSummaryPatch::Experience(_)
        ));
        assert!(
            profile_summary_instructions("profile_mbti")
                .unwrap()
                .contains("mbti_analysis")
        );
        assert!(
            profile_summary_instructions("profile_timeline")
                .unwrap()
                .contains("daily_timeline")
        );
        assert!(
            profile_summary_from_output(
                "```json\n{\"experience_summary\":\"Built CTX.\"}\n```",
                "profile_experience"
            )
            .is_ok()
        );
        assert!(profile_summary_from_output("{}", "profile_mbti").is_err());
    }

    #[test]
    fn normalizes_common_profile_response_aliases() {
        let output = json!({
            "mbti_analysis": {
                "type_code": "INTJ",
                "confidence": 0.76,
                "dimensions": [
                    {"dimension": "I/E", "preference": "I", "confidence": 0.62, "evidence": [{"article_url": "#/p/article-1", "observation": "Observed pattern."}]},
                    {"dimension": "N/S", "preference": "N", "confidence": 0.62, "evidence": [{"article_url": "#/p/article-1", "observation": "Observed pattern."}]},
                    {"dimension": "T/F", "preference": "T", "confidence": 0.62, "evidence": [{"article_url": "#/p/article-1", "observation": "Observed pattern."}]},
                    {"dimension": "J/P", "preference": "J", "confidence": 0.62, "evidence": [{"article_url": "#/p/article-1", "observation": "Observed pattern."}]}
                ]
            }
        });
        let articles = [PublishedArticle {
            id: "article-1".to_owned(),
            title: "Source article".to_owned(),
            content: String::new(),
            inferred_date: String::new(),
            published_date: String::new(),
        }];
        let patch = profile_summary_from_output_with_articles(
            &output.to_string(),
            "profile_mbti",
            &articles,
        )
        .unwrap();
        let ProfileSummaryPatch::Mbti(analysis) = patch else {
            panic!("expected MBTI patch");
        };
        assert_eq!(analysis.confidence, "medium");
        assert_eq!(analysis.dimensions[0].axis, "I/E");
        assert_eq!(
            analysis.dimensions[0].evidence[0].article_title,
            "Source article"
        );
        assert_eq!(
            analysis.dimensions[0].evidence[0].explanation,
            "Observed pattern."
        );
    }

    #[test]
    fn normalizes_schwartz_objects_and_sparse_timeline_evidence() {
        let output = json!({
            "schwartz_values": {
                "self_direction": {"score": 98, "rank": 1, "evidence": [{"article_url": "#/p/article-1", "reason": "Independent exploration."}]},
                "stimulation": {"score": 80, "rank": 2, "evidence": []},
                "hedonism": {"score": 70, "rank": 3, "evidence": []},
                "achievement": {"score": 60, "rank": 4, "evidence": []},
                "power": {"score": 50, "rank": 5, "evidence": []},
                "security": {"score": 40, "rank": 6, "evidence": []},
                "conformity": {"score": 30, "rank": 7, "evidence": []},
                "tradition": {"score": 20, "rank": 8, "evidence": []},
                "benevolence": {"score": 10, "rank": 9, "evidence": []},
                "universalism": {"score": 1, "rank": 10, "evidence": []}
            }
        });
        let articles = [PublishedArticle {
            id: "article-1".to_owned(),
            title: "Source article".to_owned(),
            content: String::new(),
            inferred_date: String::new(),
            published_date: String::new(),
        }];
        let patch = profile_summary_from_output_with_articles(
            &output.to_string(),
            "profile_schwartz",
            &articles,
        )
        .unwrap();
        let ProfileSummaryPatch::Schwartz(values) = patch else {
            panic!("expected Schwartz patch");
        };
        assert_eq!(values.len(), 10);
        let self_direction = values
            .iter()
            .find(|value| value.key == "self_direction")
            .unwrap();
        assert_eq!(self_direction.evidence[0].article_title, "Source article");
        assert_eq!(
            self_direction.evidence[0].explanation,
            "Independent exploration."
        );
    }

    #[test]
    fn normalizes_timeline_descriptions_and_object_evidence() {
        let output = json!({"daily_timeline": [{
            "date": "2025-08-10",
            "description": "提出交易框架。",
            "evidence": {"article_url": "#/p/article-1"}
        }]});
        let articles = [PublishedArticle {
            id: "article-1".to_owned(),
            title: "Source article".to_owned(),
            content: String::new(),
            inferred_date: String::new(),
            published_date: String::new(),
        }];
        let patch = profile_summary_from_output_with_articles(
            &output.to_string(),
            "profile_timeline",
            &articles,
        )
        .unwrap();
        let ProfileSummaryPatch::Timeline(entries) = patch else {
            panic!("expected timeline patch");
        };
        assert_eq!(entries[0].summary, "提出交易框架。");
        assert_eq!(entries[0].evidence[0].article_title, "Source article");
    }

    #[test]
    fn accepts_daily_timeline_entries_in_any_order() {
        let mut output = complete_profile_metadata();
        output["daily_timeline"] = json!([
            {"date": "2026-01-02", "summary": "Second day.", "evidence": []},
            {"date": "2026-01-01", "summary": "First day.", "evidence": []}
        ]);
        assert!(metadata_from_output(&output.to_string(), "profile").is_ok());
    }

    #[test]
    fn requires_translated_profile_summaries_to_keep_their_structure() {
        let translation = json!({
            "title": "Translated profile",
            "content": "# Profile",
            "metadata": complete_profile_metadata()
        });
        assert!(translation_from_output(&translation.to_string(), "profile").is_ok());
    }
}
