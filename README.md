# CTX

CTX is an AI-native Markdown publishing tool. Every document belongs directly
to its author, keeps immutable Markdown revisions, and can be published to a
public square for anyone to read.

## What ships in v0.1

- Auth Mini sign-in for `auth.ntnl.io` users and a first-user root setup flow.
- SQLite with WAL mode, foreign keys, a five-second busy timeout, and immutable
  Markdown revisions.
- Direct document creation and editing for each authenticated user. Saving
  creates a new revision; publishing fixes the current revision as public.
- Existing documents save after five seconds of editing inactivity, whenever
  the editor loses focus, and at least every 15 seconds while changes remain.
  Every unsaved edit also has a browser-local recovery copy, including a new
  document before its first explicit creation.
- A Notion-like block editor that keeps Markdown as the source of truth: rich
  writing, a slash-command menu, selection formatting, GFM tables and task
  lists, image and link insertion, published-document references, and an
  always-available Markdown source view. Mermaid code fences remain Markdown
  code blocks and keep their language marker when saved. Published readers
  render GFM, Mermaid diagrams, inline KaTeX formulas (`$...$`), and displayed
  KaTeX formulas (`$$...$$`). Pasting a PNG, JPEG, GIF, or WebP image up to
  10 MiB uploads it to CTX and inserts a content-addressed
  `https://ctx.ntnl.io/media/{sha256}` Markdown URL; media files live under
  `~/.ctx/media/`.
- A public square at `#/square`, a public article reader at
  `#/p/{document_id}`, and public APIs at `/api/public/documents` and
  `/api/public/documents/{document_id}`. A signed-in article author can open
  its editor directly from the public reader.
- Every author can create one dedicated profile document. Its public page at
  `#/u/{owner_id}` presents Linkit identity and avatar, a 52-week heatmap of
  published articles, and URL-state tabs for an objective AI-generated
  Markdown resume (`?tab=resume`) and the published profile article
  (`?tab=article`). The resume is derived from the author's complete published
  article set and refreshes asynchronously when an article is published.
  Profile documents are served from `/api/public/users/{owner_id}/profile` and
  do not appear in the square.
- Root-only OpenAI-compatible routing configuration for `https://openai.ntnl.io/v1`.
  The server encrypts the configured API key with a host-local AES-GCM key; it
  is never returned to the browser.
- Publishing saves the original Markdown immediately, then queues auditable AI
  metadata extraction against that fixed revision. When a reader opens an
  unavailable UI language (`zh-CN`, `en-US`, `ja-JP`, or `es-ES`), CTX queues
  one Markdown-preserving translation together with localized editorial
  metadata. Translations retain Markdown structure, including links, code,
  GFM, and Mermaid; the reader shows the original with a background-work
  notice until that localized article is ready.
- Root administrators can inspect queued, running, successful, and failed
  metadata/translation requests at **Administration → AI request audit**.
- Authors can ask AI to polish a saved document and review the proposed
  Markdown before explicitly saving it as a new immutable revision.
- Root administrators can inspect five-second CPU, memory, network, disk, and
  SQLite storage snapshots at **Administration → System resources**. This
  operational view is not exposed to other users.

## Run locally

```bash
npm --prefix web ci
npm --prefix web run build
cargo run
```

Open `http://127.0.0.1:8080/#/square` to browse published articles, or open
`#/documents` to sign in and create a document. The first authenticated Auth
Mini user chooses **Become root administrator**, then configures the OpenAI-LB
model and key in **Administration**. CTX's local database and encryption key
live under `~/.ctx/`.

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
