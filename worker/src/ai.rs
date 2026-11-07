//! AI queue: metadata extraction and Markdown translation.
//!
//! Credentials live in D1 `app_meta` in plain text (`ai_base_url`, `ai_model`,
//! `ai_api_key`): the operator explicitly chose plaintext over the old
//! AES-GCM column, so this module never decrypts anything.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use worker::wasm_bindgen::JsValue;
use worker::*;

use crate::db::Db;
use crate::models::PublishedMetadata;

#[derive(Clone, Debug, Deserialize)]
pub struct AiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
struct MetadataOutput {
    #[serde(default)]
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
}

#[derive(Debug, Deserialize, Serialize)]
struct TranslationOutput {
    title: String,
    content: String,
}

pub struct AiOutcome {
    pub summary: String,
    pub metadata: Option<(String, PublishedMetadata)>,
    pub translation: Option<(String, String, String)>,
}

pub async fn process(db: &Db, request_id: &str) -> std::result::Result<AiOutcome, String> {
    let Some(request) = db.ai_request(request_id).await.map_err(|e| e.to_string())? else {
        return Err("AI 请求已不存在".to_owned());
    };
    let Some(config) = db.ai_config().await.map_err(|e| e.to_string())? else {
        return Err("AI 尚未配置（管理页填写 base_url / model / api_key）".to_owned());
    };
    let Some(document) = db
        .published_revision(&request.document_id, &request.source_revision_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(AiOutcome {
            summary: "已被更新的发布版本取代".to_owned(),
            metadata: None,
            translation: None,
        });
    };

    match request.task.as_str() {
        "metadata" => {
            let instructions = metadata_instructions();
            let input = format!(
                "Document title:\n{}\n\nDocument Markdown:\n{}",
                document.title, document.content
            );
            let output = call(&config, &instructions, &input).await?;
            let parsed: MetadataOutput = serde_json::from_value(structured_output(&output, "inferred_lang")?)
                .map_err(|_| format!("元数据响应缺少必需字段：{}", preview(&output)))?;
            let language = normalize_language(&parsed.inferred_lang)
                .ok_or_else(|| format!("元数据响应里的 inferred_lang 不可用：{}", preview(&output)))?;
            let metadata = PublishedMetadata {
                description: parsed.description,
                summary: parsed.summary,
                short_summary: parsed.short_summary,
                tags: parsed.tags,
                inferred_date: parsed.inferred_date,
                inferred_lang: language.clone(),
                key_points: parsed.key_points,
                audience: parsed.audience,
                ..Default::default()
            };
            if metadata.is_empty() {
                return Err(format!("元数据响应为空：{}", preview(&output)));
            }
            Ok(AiOutcome {
                summary: format!("已提取元数据（{}）", language),
                metadata: Some((language, metadata)),
                translation: None,
            })
        }
        "translate" => {
            let target = request.target_language.clone();
            if target.trim().is_empty() {
                return Err("翻译任务缺少目标语言".to_owned());
            }
            let source = if document.source_language.trim().is_empty() {
                "und".to_owned()
            } else {
                document.source_language.clone()
            };
            let instructions = translation_instructions(&source, &target);
            let input = format!(
                "Document title:\n{}\n\nDocument Markdown:\n{}",
                document.title, document.content
            );
            let output = call(&config, &instructions, &input).await?;
            let parsed: TranslationOutput = serde_json::from_value(structured_output(&output, "title")?)
                .map_err(|_| format!("翻译响应缺少必需字段：{}", preview(&output)))?;
            if parsed.title.trim().is_empty() || parsed.content.trim().is_empty() {
                return Err(format!("翻译响应不完整：{}", preview(&output)));
            }
            Ok(AiOutcome {
                summary: format!("已翻译为 {}", target),
                metadata: None,
                translation: Some((target, parsed.title, parsed.content)),
            })
        }
        other => Err(format!("该版本暂不支持任务 {other}")),
    }
}

async fn call(config: &AiConfig, instructions: &str, input: &str) -> std::result::Result<String, String> {
    let endpoint = format!("{}/responses", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.model,
        "instructions": instructions,
        "stream": true,
        "input": [{ "role": "user", "content": [{ "type": "input_text", "text": input }] }]
    });
    let headers = Headers::new();
    headers
        .set("content-type", "application/json")
        .map_err(|e| e.to_string())?;
    headers
        .set("authorization", &format!("Bearer {}", config.api_key))
        .map_err(|e| e.to_string())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&body.to_string())));
    let request = Request::new_with_init(&endpoint, &init).map_err(|e| e.to_string())?;
    let mut response = Fetch::Request(request).send().await.map_err(|e| e.to_string())?;
    let status = response.status_code();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("AI 服务返回 {status}：{}", preview(&text)));
    }
    output_text(&text).ok_or_else(|| format!("AI 响应里没有文本输出：{}", preview(&text)))
}

fn output_text(body: &str) -> Option<String> {
    let events: Vec<Value> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .collect();
    let collect = |kind: &str, pick: fn(&Value) -> Option<String>| -> String {
        events
            .iter()
            .filter(|event| event.get("type").and_then(Value::as_str) == Some(kind))
            .filter_map(pick)
            .collect()
    };
    let done = collect("response.output_text.done", |event| {
        event.get("text").and_then(Value::as_str).map(str::to_owned)
    });
    if !done.trim().is_empty() {
        return Some(done);
    }
    let items = collect("response.output_item.done", |event| {
        event.get("item").and_then(item_text)
    });
    if !items.trim().is_empty() {
        return Some(items);
    }
    if let Some(response) = events
        .iter()
        .find(|event| event.get("type").and_then(Value::as_str) == Some("response.completed"))
        .and_then(|event| event.get("response"))
    {
        if let Some(output) = response.get("output").and_then(Value::as_array) {
            let text: String = output.iter().filter_map(item_text).collect();
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("output")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(item_text).collect::<String>())
            .filter(|text| !text.trim().is_empty())
    })
}

fn item_text(item: &Value) -> Option<String> {
    item.get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .filter(|text| !text.trim().is_empty())
}

fn structured_output(output: &str, required: &str) -> std::result::Result<Value, String> {
    let trimmed = output.trim();
    let candidate = trimmed
        .strip_prefix("```json")
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    let value: Value = serde_json::from_str(candidate)
        .map_err(|_| format!("AI 输出不是 JSON：{}", preview(output)))?;
    if value.get(required).is_none() {
        return Err(format!("AI 输出缺少字段 {required}：{}", preview(output)));
    }
    Ok(value)
}

fn normalize_language(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("und") {
        return None;
    }
    let mut parts = value.split(['-', '_']);
    let language = parts.next()?.to_ascii_lowercase();
    if language.len() < 2 {
        return None;
    }
    let mut normalized = language;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if part.len() == 2 {
            normalized.push('-');
            normalized.push_str(&part.to_ascii_uppercase());
        } else {
            normalized.push('-');
            normalized.push_str(part);
        }
    }
    Some(normalized)
}

fn metadata_instructions() -> String {
    "You are CTX's editorial metadata assistant. Extract structured metadata from Markdown without inventing facts, sources, or dates. Return JSON only with these fields: description (one sentence, at most 100 characters when practical), summary (one paragraph), short_summary (2-3 sentences for an article list or RSS description), tags (3-8 concise strings), inferred_date (YYYY-MM-DD or an empty string), inferred_lang (the document's original language as a canonical BCP 47 tag such as zh-CN, en-US, ja-JP, or es-ES), key_points (3-5 concise strings), audience (a short description). Do not return any profile summary fields.".to_owned()
}

fn translation_instructions(source_language: &str, target_language: &str) -> String {
    format!(
        "You are CTX's Markdown translation assistant. Translate the title, complete GitHub Flavored Markdown document, and editorial metadata from {source_language} into {target_language}. Preserve every Markdown structure and all factual meaning: headings, links and their URLs, code blocks, inline code, formulas, images, task lists, tables, HTML, and frontmatter. For Mermaid fenced code blocks, preserve valid Mermaid syntax, identifiers, directives, relationships, and edge operators; translate only reader-facing labels, titles, and subgraph labels. Do not change non-text elements or code. Translate description, summary, short_summary, tags, key_points and audience. Preserve inferred_date and set metadata.inferred_lang to {target_language}. Use fluent, idiomatic language for a reader familiar with the subject. Return only a JSON object with string fields title and content and an object field metadata."
    )
}

pub fn validate_base_url(value: &str) -> std::result::Result<(), crate::http::ApiError> {
    let trimmed = value.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(crate::http::ApiError::BadRequest(
            "base_url must start with http:// or https://".to_owned(),
        ));
    }
    Ok(())
}

fn preview(output: &str) -> String {
    let mut preview: String = output.chars().take(300).collect();
    if output.chars().count() > 300 {
        preview.push('…');
    }
    preview
}
