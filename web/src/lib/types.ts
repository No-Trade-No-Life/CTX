export type Me = {
  user_id: string
  is_root: boolean
  setup_required: boolean
}

export type Document = {
  id: string
  owner_id: string
  title: string
  source_language: string
  status: "draft" | "published"
  metadata: Record<string, unknown>
  current_revision_id: string
  published_revision_id: string | null
  published_source_language: string | null
  created_at: number
  updated_at: number
}

export type DocumentRevision = {
  id: string
  document_id: string
  content: string
  message: string
  author_id: string
  created_at: number
}

export type DocumentDetail = {
  document: Document
  revision: DocumentRevision
}

export type PublicDocument = {
  id: string
  owner_id: string
  title: string
  metadata: PublishedMetadata
  language: string
  is_metadata_fallback: boolean
  metadata_status: "queued" | "running" | "succeeded" | "failed" | null
  published_at: number
}

export type PublishedMetadata = {
  description: string
  summary: string
  short_summary: string
  tags: string[]
  inferred_date: string
  inferred_lang: string
  key_points: string[]
  audience: string
}

export type PublicDocumentDetail = {
  id: string
  owner_id: string
  title: string
  content: string
  source_language: string
  language: string
  available_languages: string[]
  metadata: PublishedMetadata
  is_metadata_fallback: boolean
  metadata_status: "queued" | "running" | "succeeded" | "failed" | null
  requested_language: string | null
  translation_status: "queued" | "running" | "succeeded" | "failed" | null
  is_translation_fallback: boolean
  published_at: number
}

export type AiConfiguration = {
  base_url: string
  model: string
  configured: boolean
}

export type AiRun = {
  id: string
  document_id: string
  source_revision_id: string
  task: "metadata" | "detect_language"
  output: string
  proposed_content: string | null
  created_at: number
}

export type AiRequest = {
  id: string
  document_id: string
  document_title: string
  source_revision_id: string
  task: "metadata" | "translate"
  target_language: string
  status: "queued" | "running" | "succeeded" | "failed"
  result_summary: string | null
  error: string | null
  created_at: number
  started_at: number | null
  completed_at: number | null
}
