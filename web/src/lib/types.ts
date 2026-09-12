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
  kind: "article" | "profile"
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

export type MediaUpload = {
  id: string
  url: string
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
  experience_summary: string
  personality_analysis: string
  mbti_analysis: MbtiAnalysis
  schwartz_values: SchwartzValue[]
  unconscious_motivations: string
  philosophical_references: string
  daily_timeline: DailyTimelineEntry[]
}

export type SummaryEvidence = {
  article_title: string
  article_url: string
  explanation: string
}

export type MbtiDimension = {
  axis: string
  preference: string
  confidence: string
  evidence: SummaryEvidence[]
}

export type MbtiAnalysis = {
  type_code: string
  confidence: string
  dimensions: MbtiDimension[]
}

export type SchwartzValue = {
  key: string
  score: number
  rank: number
  evidence: SummaryEvidence[]
}

export type DailyTimelineEntry = {
  date: string
  summary: string
  evidence: SummaryEvidence[]
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

export type PublicUserProfile = {
  owner_id: string
  profile: PublicDocumentDetail | null
  published_article_dates: number[]
}

export type PublicationTime = {
  published_at: number | null
}

export type DocumentComment = {
  id: string
  document_id: string
  author_id: string
  language: string
  content: string
  quote: string | null
  prefix: string | null
  suffix: string | null
  created_at: number
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
  task: "metadata" | "detect_language" | "polish"
  output: string
  proposed_content: string | null
  created_at: number
}

export type AiRequest = {
  id: string
  document_id: string
  document_title: string
  source_revision_id: string
  task:
    | "metadata"
    | "translate"
    | "profile_experience"
    | "profile_personality"
    | "profile_mbti"
    | "profile_schwartz"
    | "profile_motivations"
    | "profile_philosophy"
    | "profile_timeline"
  target_language: string
  status: "queued" | "running" | "succeeded" | "failed"
  result_summary: string | null
  error: string | null
  openai_lb_request_id: string | null
  created_at: number
  started_at: number | null
  completed_at: number | null
}

export type ProfileSummaryTaskResponse = {
  tasks: string[]
}

export type SystemResources = {
  sampled_at: number
  sample_interval_ms: number
  cpu: {
    usage_percent: number
    load_1m: number
    logical_cpus: number
  }
  memory: {
    used_bytes: number
    total_bytes: number
    available_bytes: number
    process_used_bytes: number
    other_used_bytes: number
    usage_percent: number
    swap_used_bytes: number
    swap_total_bytes: number
  }
  network: {
    receive_bytes_per_second: number
    transmit_bytes_per_second: number
    interfaces: number
  }
  disk: {
    mount_point: string
    used_bytes: number
    total_bytes: number
    available_bytes: number
    usage_percent: number
  } | null
  sqlite: {
    main_bytes: number
    wal_bytes: number
    shm_bytes: number
    total_bytes: number
    freelist_bytes: number
    freelist_percent: number
  }
}
