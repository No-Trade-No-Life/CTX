# CTX

CTX is an AI-native Markdown Context CMS. A Context is a bounded collection of
Markdown documents, editorial instructions, AI work, and public publishing
state. Docs and blog documents share one revisioned source model.

## What ships in v0.1

- Auth Mini sign-in for `auth.ntnl.io` users and a first-user root setup flow.
- SQLite with WAL mode, foreign keys, a five-second busy timeout, and immutable
  Markdown revisions.
- Contexts, online Markdown editing, Docs/Blog document types, and explicit
  publication of the current revision.
- Public read API at
  `/api/public/contexts/{context_slug}/documents/{document_slug}` and the
  matching `#/p/{context_slug}/{document_slug}` reader.
- Root-only OpenAI-compatible routing configuration for `https://openai.ntnl.io/v1`.
  The server encrypts the configured API key with a host-local AES-GCM key; it
  is never returned to the browser.
- CZON-inspired AI tasks: metadata extraction, Markdown summaries, and
  Markdown-preserving translation. Every result is recorded against the source
  revision; translation can become a separate draft rather than overwriting
  the original.

## Run locally

```bash
npm --prefix web ci
npm --prefix web run build
cargo run
```

Open `http://127.0.0.1:8080`. The first authenticated Auth Mini user chooses
**Become root administrator**, then configures the OpenAI-LB model and key in
**Administration**. CTX's local database and encryption key live under
`~/.ctx/`.

## Validation

```bash
npm --prefix web run build
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
```

The frontend is built before Rust because its assets are embedded in the single
binary.

## Deployment

Merging `main` creates a GitHub Release and deploys it to the one configured
Tokyo EC2 instance through AWS Systems Manager. There is intentionally no
second environment. The deployment uses the instance's AWS public DNS name as
the target of the proxied Cloudflare CNAME for `ctx.ntnl.io`; it does not use
an Elastic IP.

The repository requires these GitHub Actions variables:

- `AWS_DEPLOY_ROLE_ARN`
- `AWS_REGION` (`ap-northeast-1`)
- `EC2_INSTANCE_ID`

Bootstrap the instance once using `deploy/bootstrap-ubuntu.sh`. Caddy listens
at the Cloudflare origin and proxies the local CTX listener at port `8080`.
