export type DocumentDraft = {
  title: string
  sourceLanguage: string
  content: string
}

type StoredDocumentDraft = DocumentDraft & {
  revisionId: string
}

const newDocumentDraftKey = "ctx.new-document-draft.v1"

function documentDraftKey(documentId: string) {
  return `ctx.document-draft.v1.${documentId}`
}

function hasDocumentDraft(value: unknown): value is DocumentDraft {
  if (!value || typeof value !== "object") return false
  const draft = value as Partial<DocumentDraft>
  return (
    typeof draft.title === "string" &&
    typeof draft.sourceLanguage === "string" &&
    typeof draft.content === "string"
  )
}

function readDraft(key: string): unknown {
  try {
    const value = window.localStorage.getItem(key)
    return value ? JSON.parse(value) : null
  } catch {
    // RECOVERY: Browser storage is optional. Server autosave remains available.
    return null
  }
}

function writeDraft(key: string, draft: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(draft))
  } catch {
    // RECOVERY: Browser storage is optional. Server autosave remains available.
  }
}

function clearDraft(key: string) {
  try {
    window.localStorage.removeItem(key)
  } catch {
    // RECOVERY: Browser storage is optional. There is no local copy to clear.
  }
}

export function documentDraftFrom(
  title: string,
  sourceLanguage: string,
  content: string
): DocumentDraft {
  return { title, sourceLanguage, content }
}

export function draftsMatch(left: DocumentDraft, right: DocumentDraft) {
  return (
    left.title === right.title &&
    left.sourceLanguage === right.sourceLanguage &&
    left.content === right.content
  )
}

export function hasUnsavedDraft(draft: DocumentDraft) {
  return Boolean(draft.title || draft.content || draft.sourceLanguage !== "und")
}

export function loadNewDocumentDraft(): DocumentDraft | null {
  const draft = readDraft(newDocumentDraftKey)
  return hasDocumentDraft(draft) ? draft : null
}

export function saveNewDocumentDraft(draft: DocumentDraft) {
  if (hasUnsavedDraft(draft)) writeDraft(newDocumentDraftKey, draft)
  else clearDraft(newDocumentDraftKey)
}

export function clearNewDocumentDraft() {
  clearDraft(newDocumentDraftKey)
}

export function loadDocumentDraft(
  documentId: string,
  revisionId: string
): DocumentDraft | null {
  const draft = readDraft(documentDraftKey(documentId))
  if (!hasDocumentDraft(draft)) return null
  const storedDraft = draft as Partial<StoredDocumentDraft>
  if (storedDraft.revisionId === revisionId) return draft
  clearDraft(documentDraftKey(documentId))
  return null
}

export function saveDocumentDraft(
  documentId: string,
  revisionId: string,
  draft: DocumentDraft
) {
  writeDraft(documentDraftKey(documentId), { ...draft, revisionId })
}

export function clearDocumentDraft(documentId: string) {
  clearDraft(documentDraftKey(documentId))
}
