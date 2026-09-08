use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    db::{AiCredentials, AiRequest, Database, DocumentDetail},
    language::normalize_language_tag,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTask {
    Metadata,
    Summary,
    DetectLanguage,
}

impl AiTask {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Summary => "summary",
            Self::DetectLanguage => "detect_language",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiOutput {
    pub output: String,
}

#[derive(Clone, Debug)]
pub struct DocumentTranslation {
    pub title: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct DocumentMetadata {
    pub metadata: Value,
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
}

#[derive(Debug, Deserialize)]
struct MetadataOutput {
    inferred_lang: String,
}

pub async fn run(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    task: AiTask,
) -> Result<AiOutput, AiError> {
    let system = system_prompt(&task);
    let content = format!(
        "Document title: {}\n\nDocument Markdown:\n{}",
        document.document.title, document.revision.content
    );
    let output = request_output(credentials, &system, &content).await?;
    Ok(AiOutput { output })
}

pub async fn translate_document(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    source_language: &str,
    target_language: &str,
) -> Result<DocumentTranslation, AiError> {
    let instructions = format!(
        "You are CTX's Markdown translation assistant. Translate the title and complete GitHub Flavored Markdown document from {source_language} into {target_language}. Preserve every Markdown structure and all factual meaning: headings, links and their URLs, code blocks, inline code, formulas, images, Mermaid diagrams, task lists, tables, HTML, and frontmatter. Do not change non-text elements or code. Use fluent, idiomatic language for a reader familiar with the subject. {} Return only a JSON object with string fields title and content.",
        japanese_translation_rules(target_language)
    );
    let content = format!(
        "Source language: {source_language}\nTarget language: {target_language}\n\nDocument title:\n{}\n\nDocument Markdown:\n{}",
        document.document.title, document.revision.content
    );
    let output = request_output(credentials, &instructions, &content).await?;
    translation_from_output(&output)
}

pub async fn extract_document_metadata(
    credentials: &AiCredentials,
    document: &DocumentDetail,
) -> Result<DocumentMetadata, AiError> {
    let instructions = "You are CTX's editorial metadata assistant. Extract structured metadata from the Markdown without inventing facts, sources, or dates. Return JSON only with these fields: description (one sentence, at most 100 characters when practical), summary (one paragraph), short_summary (2-3 sentences), tags (3-8 concise strings), inferred_date (YYYY-MM-DD or an empty string), inferred_lang (the document's original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES), key_points (3-5 concise strings), and audience (a short description). Preserve the author's title; do not generate or change it.";
    let content = format!(
        "Document title:\n{}\n\nDocument Markdown:\n{}",
        document.document.title, document.revision.content
    );
    let output = request_output(credentials, instructions, &content).await?;
    metadata_from_output(&output)
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
            let extracted = extract_document_metadata(&credentials, &document)
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
                .enqueue_published_translation_matrix(
                    &request.document_id,
                    &request.source_revision_id,
                )
                .map_err(|error| error.to_string())?;
            Ok(format!(
                "Metadata extracted; published source language is {source_language}"
            ))
        }
        "translate" => {
            if document.document.source_language == "und" {
                return Err("the source language is still being inferred".to_owned());
            }
            let translation = translate_document(
                &credentials,
                &document,
                &document.document.source_language,
                &request.target_language,
            )
            .await
            .map_err(|error| error.to_string())?;
            let stored = database
                .store_published_translation(
                    &request.document_id,
                    &request.source_revision_id,
                    &request.target_language,
                    &translation.title,
                    &translation.content,
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

fn translation_from_output(output: &str) -> Result<DocumentTranslation, AiError> {
    let translation: TranslationOutput =
        serde_json::from_str(output).map_err(|_| AiError::Response)?;
    if translation.title.trim().is_empty() || translation.content.trim().is_empty() {
        return Err(AiError::Response);
    }
    Ok(DocumentTranslation {
        title: translation.title,
        content: translation.content,
    })
}

fn metadata_from_output(output: &str) -> Result<DocumentMetadata, AiError> {
    let metadata: Value = serde_json::from_str(output).map_err(|_| AiError::Response)?;
    let inferred_language = serde_json::from_value::<MetadataOutput>(metadata.clone())
        .map_err(|_| AiError::Response)?
        .inferred_lang;
    let inferred_language = normalize_language_tag(&inferred_language)
        .filter(|language| language != "und")
        .ok_or(AiError::Response)?;
    if !metadata.is_object() {
        return Err(AiError::Response);
    }
    Ok(DocumentMetadata {
        metadata,
        inferred_language,
    })
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
        AiTask::Metadata => "You are CTX's editorial metadata assistant. Return valid JSON only with title, description, tags, and category. Preserve the author's factual claims and do not invent sources.".to_owned(),
        AiTask::Summary => "You are CTX's editorial summary assistant. Write a concise Markdown summary with the document's actual argument, key points, and unresolved questions. Do not invent facts or citations.".to_owned(),
        AiTask::DetectLanguage => "You are CTX's language detector. Read the document title and Markdown, then return only its original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES. Do not add explanation, punctuation, or Markdown. If the language cannot be determined, return und.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        metadata_from_output, output_text_from_sse, request_body, translation_from_output,
    };

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
            r##"{"title":"Translated title","content":"# Translated\n\n- [x] Done"}"##,
        )
        .unwrap();
        assert_eq!(translation.title, "Translated title");
        assert_eq!(translation.content, "# Translated\n\n- [x] Done");
        assert!(translation_from_output(r#"{"title":"Only a title","content":""}"#).is_err());
        assert!(translation_from_output("not JSON").is_err());
    }

    #[test]
    fn requires_structured_metadata_with_a_canonical_original_language() {
        let metadata = metadata_from_output(
            r#"{"description":"A summary","inferred_lang":"ja-jp","tags":["CTX"]}"#,
        )
        .unwrap();
        assert_eq!(metadata.inferred_language, "ja-JP");
        assert!(metadata_from_output(r#"{"description":"No language"}"#).is_err());
        assert!(metadata_from_output(r#"{"inferred_lang":"Japanese"}"#).is_err());
    }
}
