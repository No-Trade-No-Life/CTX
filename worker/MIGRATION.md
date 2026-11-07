# Docs → Cloudflare 原生迁移（进行中）

目标：`docs.ntnl.io` 由 Cloudflare Worker 服务（Workers + D1 + R2 + Static Assets），
旧的 axum/EC2 版本继续在 `ctx.ntnl.io` 提供服务，直到用户验收后再切。

## 已建的云资源（Cloudflare 账号 NTNL / b831a34938a75457b5029d31cea13c25）

- D1：`docs-db` = `fdfecddc-bd7c-4176-bf59-b419d8abcfac`（location hint: apac）
- R2：`docs-media`（18 个媒体对象，内容寻址 key = sha256）
- Worker：`docs`，先部署在 `docs.zccz14.workers.dev`（该域在中国网络不可达，验收必须走 docs.ntnl.io）
- wrangler 登录态：OAuth（zccz14@outlook.com），权限含 workers/workers_routes/d1/zone:read

## 数据迁移结果（已核对，与生产逐表一致）

来源：生产 `ctx-prod` EC2 的 SQLite 快照（sqlite backup API，integrity ok），
本地保存于 `~/.local/state/docs-migration/`（含 `ctx-snapshot.sqlite3`、`media/`、`ctx.key`）。

| 表 | 行数 |
|---|---|
| documents | 157 |
| document_revisions | 1231 |
| ai_runs | 2 |
| document_translations | 309 |
| document_metadata | 445 |
| ai_requests | 112 |
| document_comments | 2 |
| document_readers | 2 |
| api_keys | 4 |
| document_view_stats | 157 |
| document_view_daily | 350 |
| app_meta | 7 |

媒体 18 个文件 / 3MB 已进 R2。

## 结构

- `worker/migrations/0001_schema.sql`：生产 11 张表 + app_meta + request_metrics（新增，接口性能指标）+ 索引
- `worker/src/models.rs`：与 axum 版完全一致的 serde 结构（从 `src/db.rs` 原样复制，保证线上 JSON 不变）
- `worker/src/db.rs`：D1 数据层（进行中）
- `worker/src/http.rs`：32 条路由 + Ed25519 JWKS 鉴权（进行中）
- 构建：`pnpm --dir web run build && cd worker && worker-build --release && npx wrangler deploy`

## 待办

- db 层、http 层、AI 队列（Cron）、浏览计数（D1 直写）、接口性能指标、前端去掉系统资源页
- 绑 docs.ntnl.io 自定义域（ctx.ntnl.io 保持指向 EC2）
