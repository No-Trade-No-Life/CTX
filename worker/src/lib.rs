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
async fn scheduled(_event: ScheduledEvent, _env: Env, _ctx: ScheduleContext) {}
