CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('draft', 'published')),
    visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('private', 'public')),
    document_kind TEXT NOT NULL DEFAULT 'article' CHECK(document_kind IN ('article', 'profile')),
    parent_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
    sort_key TEXT NOT NULL DEFAULT '',
    metadata_json TEXT NOT NULL,
    current_revision_id TEXT NOT NULL,
    published_revision_id TEXT,
    published_title TEXT,
    published_at INTEGER,
    source_language TEXT NOT NULL DEFAULT 'und',
    published_source_language TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS document_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    message TEXT NOT NULL,
    author_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS ai_runs (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
    task TEXT NOT NULL,
    output TEXT NOT NULL,
    proposed_content TEXT,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS document_translations (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    language TEXT NOT NULL,
    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    published_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, language)
);
CREATE TABLE IF NOT EXISTS document_metadata (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    language TEXT NOT NULL,
    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
    metadata_json TEXT NOT NULL,
    published_at INTEGER NOT NULL,
    PRIMARY KEY(document_id, language)
);
CREATE TABLE IF NOT EXISTS ai_requests (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id),
    task TEXT NOT NULL CHECK(task IN ('metadata', 'translate', 'profile_experience', 'profile_personality', 'profile_mbti', 'profile_schwartz', 'profile_motivations', 'profile_philosophy', 'profile_timeline')),
    target_language TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed')),
    result_summary TEXT,
    error TEXT,
    openai_lb_request_id TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    UNIQUE(document_id, source_revision_id, task, target_language)
);
CREATE TABLE IF NOT EXISTS document_comments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    source_revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL,
    language TEXT NOT NULL,
    content TEXT NOT NULL,
    quote TEXT,
    prefix TEXT,
    suffix TEXT,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS document_readers (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (document_id, user_id)
);
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    label TEXT NOT NULL,
    prefix TEXT NOT NULL,
    secret_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS document_view_stats (
    document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    views_total INTEGER NOT NULL DEFAULT 0,
    views_human INTEGER NOT NULL DEFAULT 0,
    views_unique_human INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS document_view_daily (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    day TEXT NOT NULL,
    views_total INTEGER NOT NULL DEFAULT 0,
    views_human INTEGER NOT NULL DEFAULT 0,
    views_unique_human INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (document_id, day)
);
CREATE TABLE IF NOT EXISTS request_metrics (
    id TEXT PRIMARY KEY NOT NULL,
    route TEXT NOT NULL,
    method TEXT NOT NULL,
    status INTEGER NOT NULL,
    duration_ms REAL NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS documents_owner_idx ON documents(owner_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS documents_tree_idx ON documents(owner_id, parent_id, sort_key);
CREATE INDEX IF NOT EXISTS documents_public_idx ON documents(visibility, status, published_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS documents_owner_profile_unique ON documents(owner_id) WHERE document_kind = 'profile';
CREATE INDEX IF NOT EXISTS document_revisions_document_idx ON document_revisions(document_id, created_at DESC);
CREATE INDEX IF NOT EXISTS ai_runs_document_idx ON ai_runs(document_id, created_at DESC);
CREATE INDEX IF NOT EXISTS document_translations_document_idx ON document_translations(document_id, language);
CREATE INDEX IF NOT EXISTS document_metadata_document_idx ON document_metadata(document_id, language);
CREATE INDEX IF NOT EXISTS ai_requests_status_idx ON ai_requests(status, created_at, id);
CREATE INDEX IF NOT EXISTS ai_requests_document_idx ON ai_requests(document_id, created_at DESC);
CREATE INDEX IF NOT EXISTS document_comments_document_idx ON document_comments(document_id, source_revision_id, language, created_at, id);
CREATE INDEX IF NOT EXISTS document_readers_user_idx ON document_readers(user_id, document_id);
CREATE INDEX IF NOT EXISTS request_metrics_created_idx ON request_metrics(created_at DESC);
