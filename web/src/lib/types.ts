export type Me = {
  user_id: string
  is_root: boolean
  setup_required: boolean
}

export type Context = {
  id: string
  owner_id: string
  name: string
  slug: string
  description: string
  instructions: string
  visibility: "private" | "public"
  document_count: number
  created_at: number
  updated_at: number
}

export type Document = {
  id: string
  context_id: string
  title: string
  slug: string
  language: string
  kind: "docs" | "blog"
  status: "draft" | "published"
  metadata: Record<string, unknown>
  current_revision_id: string
  published_revision_id: string | null
  created_at: number
  updated_at: number
}

export type DocumentDetail = {
  document: Document
  revision: {
    id: string
    document_id: string
    content: string
    message: string
    author_id: string
    created_at: number
  }
}

export type AiConfiguration = {
  base_url: string
  model: string
  configured: boolean
}

export type AiRun = {
  id: string
  context_id: string
  document_id: string
  source_revision_id: string
  task: "metadata" | "summary" | "translate"
  output: string
  proposed_content: string | null
  created_at: number
}
