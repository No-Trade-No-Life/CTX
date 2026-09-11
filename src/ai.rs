use std::collections::BTreeSet;
use std::time::Duration;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    db::{
        AiCredentials, AiRequest, DailyTimelineEntry, Database, DocumentDetail, MbtiAnalysis,
        PublishedArticle, PublishedMetadata, SchwartzValue, SummaryEvidence,
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

pub async fn run_worker(database: Database) {
    if let Err(error) = database.requeue_running_ai_requests() {
        eprintln!("CTX could not recover interrupted AI requests: {error}");
    }
    loop {
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
    document_kind: &str,
) -> Result<DocumentTranslation, AiError> {
    let translation: TranslationOutput =
        serde_json::from_str(output).map_err(|_| AiError::Response)?;
    if translation.title.trim().is_empty()
        || translation.content.trim().is_empty()
        || translation.metadata.is_empty()
        || (document_kind == "profile" && !profile_summaries_are_complete(&translation.metadata))
    {
        return Err(AiError::Response);
    }
    Ok(DocumentTranslation {
        title: translation.title,
        content: translation.content,
        metadata: translation.metadata,
    })
}

fn metadata_from_output(output: &str, document_kind: &str) -> Result<DocumentMetadata, AiError> {
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
    if metadata.is_empty()
        || (document_kind == "profile" && !profile_summaries_are_complete(&metadata))
    {
        return Err(AiError::Response);
    }
    Ok(DocumentMetadata {
        metadata,
        inferred_language,
    })
}

fn profile_summaries_are_complete(metadata: &PublishedMetadata) -> bool {
    const MBTI_AXES: [&str; 4] = ["I/E", "N/S", "T/F", "J/P"];
    const SCHWARTZ_VALUES: [&str; 10] = [
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

    valid_mbti_type(&metadata.mbti_analysis)
        && metadata.mbti_analysis.dimensions.len() == MBTI_AXES.len()
        && metadata
            .mbti_analysis
            .dimensions
            .iter()
            .zip(MBTI_AXES)
            .all(|(dimension, axis)| valid_mbti_dimension(dimension, axis))
        && metadata.schwartz_values.len() == SCHWARTZ_VALUES.len()
        && valid_schwartz_values(&metadata.schwartz_values, &SCHWARTZ_VALUES)
        && valid_daily_timeline(&metadata.daily_timeline)
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
    let mut previous_date = None;
    entries.iter().all(|entry| {
        let parsed_date = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").ok();
        let is_ordered = parsed_date
            .as_ref()
            .is_some_and(|date| previous_date.is_none_or(|previous| *date > previous));
        previous_date = parsed_date;
        is_ordered && !entry.summary.trim().is_empty() && entry.evidence.iter().all(valid_evidence)
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
        "This is a personal profile document. The input includes the complete set of ordinary articles published by this author. Build experience_summary from all of those sources and the profile document, not from the profile document alone. experience_summary must be concise GitHub Flavored Markdown in a resume format. Use only sections supported by explicit facts, such as `### Overview`, `### Experience`, `### Projects`, and `### Milestones`. Use factual bullet points and dates only when stated. Do not use first person, emotional language, subjective evaluation, or speculation. Do not invent employers, roles, education, skills, chronology, or links. If no experience facts are stated anywhere, return an empty string. Also build personality_analysis from the same complete source set. personality_analysis must be concise GitHub Flavored Markdown for a public personal page. Start with a blockquote stating that it is an AI reading of published writing, not a clinical or diagnostic assessment. Then use only evidence-supported sections such as `### Writing patterns`, `### Values and priorities`, `### Decision and work style`, and `### Expression and collaboration`. For every substantive point, cite one or more of the supplied source article links. Describe observable patterns in the writing, not private facts about the author. Do not diagnose or assess mental health; do not infer conditions, trauma, psychological defence mechanisms, cognitive functions, or risks. Do not make claims about relationships or emotions unless the text explicitly states them. If the articles do not contain enough explicit material for a useful, evidence-supported analysis, return an empty string. Also return a complete structured MBTI analysis in mbti_analysis. It must contain type_code (one of the sixteen four-letter MBTI codes or `undetermined`), confidence (`high`, `medium`, `low`, or `undetermined`), and exactly four dimensions in this order: I/E, N/S, T/F, J/P. Each dimension must include its axis, one preference from that axis or `undetermined`, confidence, and zero or more evidence objects. Also return exactly ten schwartz_values, with the keys self_direction, stimulation, hedonism, achievement, power, security, conformity, tradition, benevolence, and universalism. Each value must include an integer score from 0 to 100, a unique integer rank from 1 (strongest) to 10, and zero or more evidence objects. Every evidence object must have article_title, article_url, and explanation; article_url must exactly use one of the supplied `#/p/...` links. MBTI is a tentative textual classification, never a medical or scientific diagnosis. Build unconscious_motivations as concise GitHub Flavored Markdown that offers explicitly tentative interpretations of possible motivations in the writing, with source links for each conclusion. Do not infer trauma, mental-health conditions, or private facts. Build philosophical_references as concise GitHub Flavored Markdown that identifies the author's expressed propositions, relevant philosophical references or traditions, and evidence links; do not attribute an author to a philosopher or school without textual support. Build daily_timeline as an ascending list of zero or more entries. Each entry must have an explicit YYYY-MM-DD date, one concise sentence saying what the author did and what result was recorded that day, and zero or more evidence objects. Use only dates stated in the Markdown, the supplied document date, or the supplied public publication date; merge multiple article events from the same day and never invent dates, events, or results.",
    );
    format!(
        "You are CTX's editorial metadata assistant. Extract structured metadata from Markdown without inventing facts, sources, or dates. Return JSON only with these fields: description (one sentence, at most 100 characters when practical), summary (one paragraph), short_summary (2-3 sentences for an article list or RSS description), tags (3-8 concise strings), inferred_date (YYYY-MM-DD or an empty string), inferred_lang (the document's original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES), key_points (3-5 concise strings), audience (a short description), experience_summary (an empty string unless this is a personal profile document), personality_analysis (an empty string unless this is a personal profile document), mbti_analysis (an empty object unless this is a personal profile document), schwartz_values (an empty array unless this is a personal profile document), unconscious_motivations (an empty string unless this is a personal profile document), philosophical_references (an empty string unless this is a personal profile document), and daily_timeline (an empty array unless this is a personal profile document). Preserve the author's title; do not generate or change it. Summary is part of this metadata extraction; do not create a separate summary artifact. {}",
        profile_instructions.unwrap_or_default()
    )
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
    output_text_from_sse(&response.text().await?).ok_or(AiError::Response)
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

fn system_prompt(task: &AiTask) -> String {
    match task {
        AiTask::Metadata => "You are CTX's editorial metadata assistant. Return valid JSON only with description, summary, short_summary, tags, inferred_date, inferred_lang, key_points, audience, experience_summary, personality_analysis, mbti_analysis, schwartz_values, unconscious_motivations, philosophical_references, and daily_timeline. Preserve the author's factual claims and do not invent sources. For a profile document, its structured summaries must remain source-grounded and non-clinical; keep evidence links, ranks, scores, and dates factual. Return empty profile summary fields for other documents.".to_owned(),
        AiTask::DetectLanguage => "You are CTX's language detector. Read the document title and Markdown, then return only its original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES. Do not add explanation, punctuation, or Markdown. If the language cannot be determined, return und.".to_owned(),
        AiTask::Polish => "You are CTX's Markdown editing assistant. Polish the complete document for clarity, flow, precision, and concise professional tone while preserving the author's facts, intent, and voice. Return only the revised GitHub Flavored Markdown. Preserve every Markdown structure and meaning: headings, links and URLs, inline code, code blocks, Mermaid syntax, images, task lists, tables, HTML, frontmatter, and formulas. Do not add sources, claims, or editorial commentary.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        AiTask, metadata_from_output, metadata_instructions, output_text_from_sse, request_body,
        system_prompt, translation_from_output, translation_instructions,
    };

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
        assert!(instructions.contains("resume format"));
        assert!(instructions.contains("GitHub Flavored Markdown"));
        assert!(instructions.contains("personality_analysis"));
        assert!(instructions.contains("mbti_analysis"));
        assert!(instructions.contains("schwartz_values"));
        assert!(instructions.contains("daily_timeline"));
        assert!(instructions.contains("not a clinical or diagnostic assessment"));
        assert!(instructions.contains("supplied source article links"));
        assert!(metadata_instructions("article").contains("empty string unless"));
    }

    #[test]
    fn requires_complete_structured_profile_summaries() {
        let output = complete_profile_metadata();
        let metadata = metadata_from_output(&output.to_string(), "profile").unwrap();
        assert_eq!(metadata.metadata.mbti_analysis.type_code, "INTJ");
        assert_eq!(metadata.metadata.mbti_analysis.dimensions.len(), 4);
        assert_eq!(metadata.metadata.schwartz_values.len(), 10);
        assert_eq!(metadata.metadata.daily_timeline[0].date, "2026-01-01");

        let mut malformed = complete_profile_metadata();
        malformed["schwartz_values"][0]["rank"] = json!(0);
        assert!(metadata_from_output(&malformed.to_string(), "profile").is_err());
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
