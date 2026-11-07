mod ai;
mod auth;
mod db;
mod http;
mod models;

use worker::*;

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    let url = req.url()?;
    let path = url.path().to_owned();
    let method = req.method().to_string();
    let api = path.starts_with("/api") || path.starts_with("/media");
    let started = js_sys::Date::now();
    let response = if api {
        http::route(req, env.clone()).await?
    } else {
        http::assets(req, &env).await?
    };
    let elapsed = js_sys::Date::now() - started;
    let status = response.status_code();
    let route = http::normalize_route(&path);
    if let Ok(db) = db::Db::new(&env) {
        ctx.wait_until(async move {
            let _ = db.record_metric(&route, &method, status, elapsed).await;
        });
    }
    Ok(response)
}

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let Ok(db) = db::Db::new(&env) else { return };
    for _ in 0..3 {
        let Ok(Some(request)) = db.claim_next_ai_request().await else { break };
        match ai::process(&db, &request.id).await {
            Ok(outcome) => {
                if let Some((language, metadata)) = outcome.metadata {
                    let _ = db
                        .apply_ai_metadata(&request.document_id, &request.source_revision_id, &language, &serde_json::to_string(&metadata).unwrap_or_default())
                        .await;
                }
                if let Some((language, title, content)) = outcome.translation {
                    let _ = db
                        .apply_ai_translation(&request.document_id, &request.source_revision_id, &language, &title, &content)
                        .await;
                }
                let _ = db.complete_ai_request(&request.id, &outcome.summary).await;
            }
            Err(error) => {
                let _ = db.fail_ai_request(&request.id, &error).await;
            }
        }
    }
}
