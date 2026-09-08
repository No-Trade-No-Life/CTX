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
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Option<String>,
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
    let endpoint = format!(
        "{}/chat/completions",
        credentials.base_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(&credentials.api_key)
        .json(&json!({
            "model": credentials.model,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": content}
            ]
        }))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AiError::Rejected(response.text().await?));
    }
    let completion: CompletionResponse = response.json().await?;
    let output = completion
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or(AiError::Response)?;
    let proposed_content = matches!(task, AiTask::Translate).then(|| output.clone());
    Ok(AiOutput {
        output,
        proposed_content,
    })
}

fn system_prompt(task: &AiTask, target_language: Option<&str>) -> String {
    match task {
        AiTask::Metadata => "You are CTX's editorial metadata assistant. Return valid JSON only with title, description, tags, and category. Preserve the author's factual claims and do not invent sources.".to_owned(),
        AiTask::Summary => "You are CTX's editorial summary assistant. Write a concise Markdown summary with the document's actual argument, key points, and unresolved questions. Do not invent facts or citations.".to_owned(),
        AiTask::Translate => format!("You are CTX's Markdown translation assistant. Translate the complete Markdown document into {}. Preserve Markdown syntax, code blocks, links, formulas, Mermaid diagrams, and factual meaning. Return only the translated Markdown.", target_language.unwrap_or("the requested target language")),
    }
}
