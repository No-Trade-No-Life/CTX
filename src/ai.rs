use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::db::{AiCredentials, Context, DocumentDetail};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTask {
    Metadata,
    Summary,
    Translate,
}

impl AiTask {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Summary => "summary",
            Self::Translate => "translate",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiOutput {
    pub output: String,
    pub proposed_content: Option<String>,
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
struct ResponsesResponse {
    output: Vec<ResponseOutput>,
}

#[derive(Debug, Deserialize)]
struct ResponseOutput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub async fn run(
    credentials: &AiCredentials,
    context: &Context,
    document: &DocumentDetail,
    task: AiTask,
    target_language: Option<&str>,
) -> Result<AiOutput, AiError> {
    let system = system_prompt(&task, target_language);
    let content = format!(
        "Context name: {}\nContext instructions:\n{}\n\nDocument title: {}\nDocument language: {}\nDocument Markdown:\n{}",
        context.name,
        context.instructions,
        document.document.title,
        document.document.language,
        document.revision.content
    );
    let endpoint = format!("{}/responses", credentials.base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(&credentials.api_key)
        .json(&request_body(&credentials.model, &system, &content))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AiError::Rejected(response.text().await?));
    }
    let response: ResponsesResponse = response.json().await?;
    let output = output_text(response).ok_or(AiError::Response)?;
    let proposed_content = matches!(task, AiTask::Translate).then(|| output.clone());
    Ok(AiOutput {
        output,
        proposed_content,
    })
}

fn request_body(model: &str, instructions: &str, input: &str) -> serde_json::Value {
    json!({
        "model": model,
        "instructions": instructions,
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": input
            }]
        }]
    })
}

fn output_text(response: ResponsesResponse) -> Option<String> {
    let output = response
        .output
        .into_iter()
        .filter(|item| item.kind == "message")
        .flat_map(|item| item.content)
        .filter(|content| content.kind == "output_text")
        .filter_map(|content| content.text)
        .collect::<String>();
    (!output.trim().is_empty()).then_some(output)
}

fn system_prompt(task: &AiTask, target_language: Option<&str>) -> String {
    match task {
        AiTask::Metadata => "You are CTX's editorial metadata assistant. Return valid JSON only with title, description, tags, and category. Preserve the author's factual claims and do not invent sources.".to_owned(),
        AiTask::Summary => "You are CTX's editorial summary assistant. Write a concise Markdown summary with the document's actual argument, key points, and unresolved questions. Do not invent facts or citations.".to_owned(),
        AiTask::Translate => format!("You are CTX's Markdown translation assistant. Translate the complete Markdown document into {}. Preserve Markdown syntax, code blocks, links, formulas, Mermaid diagrams, and factual meaning. Return only the translated Markdown.", target_language.unwrap_or("the requested target language")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ResponsesResponse, output_text, request_body};

    #[test]
    fn sends_responses_input_as_message_items() {
        assert_eq!(
            request_body("gpt-5.6-luna", "follow the context", "# Document"),
            json!({
                "model": "gpt-5.6-luna",
                "instructions": "follow the context",
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
    fn extracts_output_text_from_response_messages_in_order() {
        let response = serde_json::from_value::<ResponsesResponse>(json!({
            "output": [
                {"type": "reasoning"},
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "first "},
                        {"type": "refusal", "refusal": "ignored"},
                        {"type": "output_text", "text": "second"}
                    ]
                }
            ]
        }))
        .unwrap();

        assert_eq!(output_text(response).as_deref(), Some("first second"));
    }

    #[test]
    fn rejects_response_without_visible_output_text() {
        let response = serde_json::from_value::<ResponsesResponse>(json!({
            "output": [{"type": "reasoning"}]
        }))
        .unwrap();

        assert_eq!(output_text(response), None);
    }
}
