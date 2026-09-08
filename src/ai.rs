use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    db::{AiCredentials, DocumentDetail},
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

pub async fn detect_language(
    credentials: &AiCredentials,
    document: &DocumentDetail,
) -> Result<String, AiError> {
    let output = run(credentials, document, AiTask::DetectLanguage)
        .await?
        .output;
    detected_language_from_output(&output)
}

pub async fn translate_document(
    credentials: &AiCredentials,
    document: &DocumentDetail,
    source_language: &str,
    target_language: &str,
) -> Result<DocumentTranslation, AiError> {
    let instructions = format!(
        "You are CTX's Markdown translation assistant. Translate the title and complete GitHub Flavored Markdown document from {source_language} into {target_language}. Preserve Markdown syntax, code blocks, links, formulas, Mermaid diagrams, task lists, tables, and factual meaning. Return only a JSON object with string fields title and content."
    );
    let content = format!(
        "Source language: {source_language}\nTarget language: {target_language}\n\nDocument title:\n{}\n\nDocument Markdown:\n{}",
        document.document.title, document.revision.content
    );
    let output = request_output(credentials, &instructions, &content).await?;
    translation_from_output(&output)
}

fn detected_language_from_output(output: &str) -> Result<String, AiError> {
    normalize_language_tag(output.trim().trim_matches(['`', '"']))
        .filter(|language| language != "und")
        .ok_or(AiError::Response)
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
        detected_language_from_output, output_text_from_sse, request_body, translation_from_output,
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
    fn accepts_a_canonical_detected_source_language_only() {
        assert_eq!(detected_language_from_output("`zh-cn`").unwrap(), "zh-CN");
        assert!(detected_language_from_output("und").is_err());
        assert!(detected_language_from_output("Chinese").is_err());
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
}
