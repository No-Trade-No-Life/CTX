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
  published_at: number
}

export type PublicDocumentDetail = {
  id: string
  owner_id: string
  title: string
  content: string
  source_language: string
  language: string
  available_languages: string[]
  published_at: number
}

export type LanguagePreferences = {
  languages: string[]
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
  task: "metadata" | "summary" | "detect_language"
  output: string
  proposed_content: string | null
  created_at: number
}
