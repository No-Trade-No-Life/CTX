import {
  Children,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAuthMini } from "auth-mini-react-components"
import { LinkitMyInfo, LinkitUserInfo } from "linkit-react-components"
import {
  ArrowLeftIcon,
  BookOpenIcon,
  CalendarClockIcon,
  CpuIcon,
  DatabaseIcon,
  ExternalLinkIcon,
  FileTextIcon,
  Globe2Icon,
  HardDriveIcon,
  LanguagesIcon,
  LoaderCircleIcon,
  MemoryStickIcon,
  MessageSquareIcon,
  NetworkIcon,
  PencilIcon,
  PlusIcon,
  RefreshCwIcon,
  SaveIcon,
  Settings2Icon,
  ShieldCheckIcon,
  SparklesIcon,
  Trash2Icon,
  UserRoundIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react"
import ReactMarkdown from "react-markdown"
import rehypeKatex from "rehype-katex"
import remarkGfm from "remark-gfm"
import remarkMath from "remark-math"
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom"
import { toast } from "sonner"

import { request, upload } from "./lib/api"
import {
  clearDocumentDraft,
  clearNewDocumentDraft,
  documentDraftFrom,
  draftsMatch,
  loadDocumentDraft,
  loadNewDocumentDraft,
  saveDocumentDraft,
  saveNewDocumentDraft,
  type DocumentDraft,
} from "./lib/document-drafts"
import { useI18n, type Locale } from "./lib/i18n"
import type {
  AiConfiguration,
  AiRequest,
  AiRun,
  DailyTimelineEntry,
  Document,
  DocumentComment,
  DocumentDetail,
  MediaUpload,
  MbtiAnalysis,
  Me,
  PublishedMetadata,
  PublicDocument,
  PublicDocumentDetail,
  PublicUserProfile,
  ProfileSummaryTaskResponse,
  PublicationTime,
  SchwartzValue,
  SummaryEvidence,
  SystemResources,
} from "./lib/types"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { MarkdownEditor } from "@/components/markdown-editor"
import { Separator } from "@/components/ui/separator"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Toaster } from "@/components/ui/sonner"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"

export default function App() {
  return (
    <>
      <Routes>
        <Route path="*" element={<PrivateApp />} />
      </Routes>
      <Toaster />
    </>
  )
}

export function PublicApp() {
  return (
    <>
      <PublicShell />
      <Toaster />
    </>
  )
}

function LanguageSelect() {
  const { locale, setLocale, t } = useI18n()
  return (
    <Select
      value={locale}
      onValueChange={(value) => setLocale(value as Locale)}
    >
      <SelectTrigger
        size="sm"
        aria-label={t("language")}
        className="size-8 justify-center border-transparent p-0 shadow-none hover:bg-accent [&>svg:last-child]:hidden"
      >
        <LanguagesIcon />
        <SelectValue className="sr-only" />
      </SelectTrigger>
      <SelectContent align="end">
        <SelectGroup>
          <SelectItem value="en-US">{t("languageEnglish")}</SelectItem>
          <SelectItem value="zh-CN">{t("languageChinese")}</SelectItem>
          <SelectItem value="ja-JP">{t("languageJapanese")}</SelectItem>
          <SelectItem value="es-ES">{t("languageSpanish")}</SelectItem>
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

function useImageUpload(token: string) {
  return useCallback(
    async (image: File) =>
      (await upload<MediaUpload>("/api/v1/media", image, token)).url,
    [token]
  )
}

function PrivateApp() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  if (!isReady || !isAuthenticated || !session?.accessToken)
    return <LoadingPage />
  return <CtxShell token={session.accessToken} />
}

function PublicShell() {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
  const { isReady, isAuthenticated, session } = useAuthMini()
  const { t } = useI18n()
  const title = publicPageTitle(location.pathname, t)
  useBrowserTitle(
    location.pathname.startsWith("/p/") || location.pathname.startsWith("/u/")
      ? undefined
      : title
  )
  const me = useQuery({
    queryKey: ["me", session?.accessToken],
    queryFn: () => request<Me>("/api/v1/me", session?.accessToken ?? undefined),
    enabled: isReady && isAuthenticated && Boolean(session?.accessToken),
  })
  const signedIn = Boolean(me.data && !me.data.setup_required)

  return (
    <TooltipProvider>
      <SidebarProvider>
        <Sidebar collapsible="icon">
          <SidebarHeader className="px-3 py-4">
            <Button
              variant="ghost"
              className="w-full justify-start px-1.5 font-semibold"
              onClick={() => navigate("/square")}
            >
              <span className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">
                C
              </span>
              <span className="group-data-[collapsible=icon]:hidden">
                {t("appName")}
              </span>
            </Button>
          </SidebarHeader>
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  {signedIn ? (
                    <NavItem
                      active={false}
                      icon={FileTextIcon}
                      onClick={() => navigate("/documents")}
                    >
                      {t("navigationDocuments")}
                    </NavItem>
                  ) : null}
                  <NavItem
                    active={location.pathname === "/square"}
                    icon={Globe2Icon}
                    onClick={() => navigate("/square")}
                  >
                    {t("navigationSquare")}
                  </NavItem>
                  {signedIn && me.data ? (
                    <NavItem
                      active={location.pathname === `/u/${me.data.user_id}`}
                      icon={UserRoundIcon}
                      onClick={() => navigate(`/u/${me.data.user_id}`)}
                    >
                      {t("navigationPersonalPage")}
                    </NavItem>
                  ) : null}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            {me.data?.is_root ? (
              <SidebarGroup>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <NavItem
                      active={false}
                      icon={Settings2Icon}
                      onClick={() => navigate("/admin")}
                    >
                      {t("navigationAdministration")}
                    </NavItem>
                    <NavItem
                      active={false}
                      icon={SparklesIcon}
                      onClick={() => navigate("/admin/ai-requests")}
                    >
                      {t("navigationAiRequests")}
                    </NavItem>
                    <NavItem
                      active={false}
                      icon={HardDriveIcon}
                      onClick={() => navigate("/admin/system-resources")}
                    >
                      {t("navigationSystemResources")}
                    </NavItem>
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            ) : null}
          </SidebarContent>
        </Sidebar>
        <SidebarInset>
          <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background px-4">
            <SidebarTrigger />
            <Separator orientation="vertical" className="h-5" />
            <p className="min-w-0 flex-1 truncate text-sm font-medium">
              {publicPageTitle(location.pathname, t)}
            </p>
            {me.data?.is_root ? (
              <Badge variant="outline" className="hidden sm:inline-flex">
                <ShieldCheckIcon data-icon="inline-start" />
                {t("root")}
              </Badge>
            ) : null}
            <LanguageSelect />
            <LinkitMyInfo />
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("refreshDocuments")}
                    onClick={() => void queryClient.invalidateQueries()}
                  />
                }
              >
                <RefreshCwIcon />
              </TooltipTrigger>
              <TooltipContent>{t("refreshDocuments")}</TooltipContent>
            </Tooltip>
          </header>
          <Routes>
            <Route path="/" element={<Navigate to="/square" replace />} />
            <Route path="/square" element={<SquarePage />} />
            <Route path="/p/:documentId" element={<PublicDocumentPage />} />
            <Route path="/u/:ownerId" element={<PublicUserPage />} />
            <Route path="*" element={<Navigate to="/square" replace />} />
          </Routes>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  )
}

function CtxShell({ token }: { token: string }) {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
  const { t } = useI18n()
  const title = pageTitle(location.pathname, t)
  useBrowserTitle(
    location.pathname.startsWith("/documents/") ? undefined : title
  )
  const me = useQuery({
    queryKey: ["me", token],
    queryFn: () => request<Me>("/api/v1/me", token),
  })
  const refresh = () => void queryClient.invalidateQueries()

  if (me.isPending) return <LoadingPage />
  if (me.error) return <PageError error={me.error} />
  if (!me.data) return <LoadingPage />
  if (me.data.setup_required)
    return <SetupPage token={token} onDone={refresh} />

  return (
    <TooltipProvider>
      <SidebarProvider>
        <Sidebar collapsible="icon">
          <SidebarHeader className="px-3 py-4">
            <Button
              variant="ghost"
              className="w-full justify-start px-1.5 font-semibold"
              onClick={() => navigate("/documents")}
            >
              <span className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">
                C
              </span>
              <span className="group-data-[collapsible=icon]:hidden">
                {t("appName")}
              </span>
            </Button>
          </SidebarHeader>
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  <NavItem
                    active={location.pathname.startsWith("/documents")}
                    icon={FileTextIcon}
                    onClick={() => navigate("/documents")}
                  >
                    {t("navigationDocuments")}
                  </NavItem>
                  <NavItem
                    active={location.pathname === "/square"}
                    icon={Globe2Icon}
                    onClick={() => navigate("/square")}
                  >
                    {t("navigationSquare")}
                  </NavItem>
                  <NavItem
                    active={location.pathname === `/u/${me.data.user_id}`}
                    icon={UserRoundIcon}
                    onClick={() => navigate(`/u/${me.data.user_id}`)}
                  >
                    {t("navigationPersonalPage")}
                  </NavItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            {me.data.is_root ? (
              <SidebarGroup>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <NavItem
                      active={location.pathname === "/admin"}
                      icon={Settings2Icon}
                      onClick={() => navigate("/admin")}
                    >
                      {t("navigationAdministration")}
                    </NavItem>
                    <NavItem
                      active={location.pathname === "/admin/ai-requests"}
                      icon={SparklesIcon}
                      onClick={() => navigate("/admin/ai-requests")}
                    >
                      {t("navigationAiRequests")}
                    </NavItem>
                    <NavItem
                      active={location.pathname === "/admin/system-resources"}
                      icon={HardDriveIcon}
                      onClick={() => navigate("/admin/system-resources")}
                    >
                      {t("navigationSystemResources")}
                    </NavItem>
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            ) : null}
          </SidebarContent>
        </Sidebar>
        <SidebarInset>
          <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background px-4">
            <SidebarTrigger />
            <Separator orientation="vertical" className="h-5" />
            <p className="min-w-0 flex-1 truncate text-sm font-medium">
              {pageTitle(location.pathname, t)}
            </p>
            {me.data.is_root ? (
              <Badge variant="outline" className="hidden sm:inline-flex">
                <ShieldCheckIcon data-icon="inline-start" />
                {t("root")}
              </Badge>
            ) : null}
            <LanguageSelect />
            <LinkitMyInfo />
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("refreshDocuments")}
                    onClick={refresh}
                  />
                }
              >
                <RefreshCwIcon />
              </TooltipTrigger>
              <TooltipContent>{t("refreshDocuments")}</TooltipContent>
            </Tooltip>
          </header>
          <Routes>
            <Route
              path="/documents"
              element={<DocumentListPage token={token} />}
            />
            <Route
              path="/documents/new"
              element={<NewDocumentPage token={token} />}
            />
            <Route
              path="/documents/:documentId"
              element={<ExistingDocumentPage token={token} />}
            />
            <Route
              path="/admin"
              element={
                me.data.is_root ? (
                  <AdministrationPage token={token} />
                ) : (
                  <Navigate to="/documents" replace />
                )
              }
            />
            <Route
              path="/admin/ai-requests"
              element={
                me.data.is_root ? (
                  <AiRequestAuditPage token={token} />
                ) : (
                  <Navigate to="/documents" replace />
                )
              }
            />
            <Route
              path="/admin/system-resources"
              element={
                me.data.is_root ? (
                  <SystemResourcesPage token={token} />
                ) : (
                  <Navigate to="/documents" replace />
                )
              }
            />
            <Route path="*" element={<Navigate to="/documents" replace />} />
          </Routes>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  )
}

function DocumentListPage({ token }: { token: string }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale, t } = useI18n()
  const documents = useQuery({
    queryKey: ["documents", token],
    queryFn: () => request<Document[]>("/api/v1/documents", token),
  })
  const profileDocument = useMutation({
    mutationFn: () =>
      request<DocumentDetail>("/api/v1/profile-document", token, {
        method: "POST",
      }),
    onSuccess: (detail) => {
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      navigate(`/documents/${detail.document.id}`)
    },
    onError: showError,
  })
  if (documents.isPending) return <PageSkeleton />
  if (documents.error) return <PageError error={documents.error} />
  const items = documents.data ?? []

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-8 p-4 md:p-6">
      <section className="flex flex-wrap items-end justify-between gap-4">
        <div className="max-w-2xl">
          <h1 className="text-2xl font-semibold tracking-tight text-balance">
            {t("documentsTitle")}
          </h1>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {t("documentsDescription")}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={profileDocument.isPending}
            onClick={() => profileDocument.mutate()}
          >
            {profileDocument.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <UserRoundIcon data-icon="inline-start" />
            )}
            {t("editPersonalPage")}
          </Button>
          <Button onClick={() => navigate("/documents/new")}>
            <PlusIcon data-icon="inline-start" />
            {t("newDocument")}
          </Button>
        </div>
      </section>
      {items.length === 0 ? (
        <Empty className="min-h-80">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <FileTextIcon />
            </EmptyMedia>
            <EmptyTitle>{t("emptyDocumentsTitle")}</EmptyTitle>
            <EmptyDescription>
              {t("emptyDocumentsDescription")}
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => navigate("/documents/new")}>
              <PlusIcon data-icon="inline-start" />
              {t("createDocument")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <section aria-label={t("documentsTitle")} className="flex flex-col">
          {items.map((document, index) => (
            <div key={document.id}>
              {index > 0 ? <Separator /> : null}
              <DocumentRow
                document={document}
                updatedAt={formatDate(document.updated_at, locale)}
                onOpen={() => navigate(`/documents/${document.id}`)}
                onViewPublic={() =>
                  navigate(
                    document.kind === "profile"
                      ? `/u/${document.owner_id}`
                      : `/p/${document.id}`
                  )
                }
              />
            </div>
          ))}
        </section>
      )}
    </main>
  )
}

function DocumentRow({
  document,
  updatedAt,
  onOpen,
  onViewPublic,
}: {
  document: Document
  updatedAt: string
  onOpen: () => void
  onViewPublic: () => void
}) {
  const { t } = useI18n()
  const isPublished = document.status === "published"
  const isProfile = document.kind === "profile"
  return (
    <article className="group flex flex-wrap items-center gap-3 py-4 sm:flex-nowrap">
      <Button
        variant="ghost"
        className="min-w-0 flex-1 justify-start px-0 text-left hover:bg-transparent"
        onClick={onOpen}
      >
        <FileTextIcon data-icon="inline-start" />
        <span className="min-w-0">
          <span className="block truncate font-medium">{document.title}</span>
          <span className="mt-0.5 block text-xs font-normal text-muted-foreground">
            {t("updatedOn").replace("{date}", updatedAt)}
          </span>
        </span>
      </Button>
      {isProfile ? <Badge variant="outline">{t("personalPage")}</Badge> : null}
      <Badge variant={isPublished ? "secondary" : "outline"}>
        {isPublished ? t("published") : t("draft")}
      </Badge>
      {isPublished ? (
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("viewPublicArticle")}
          onClick={onViewPublic}
        >
          <ExternalLinkIcon />
        </Button>
      ) : null}
    </article>
  )
}

function NewDocumentPage({ token }: { token: string }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { t } = useI18n()
  useBrowserTitle(t("newDocument"))
  const imageUpload = useImageUpload(token)
  const documents = useQuery({
    queryKey: ["documents", token],
    queryFn: () => request<Document[]>("/api/v1/documents", token),
  })
  const [recoveredDraft] = useState(loadNewDocumentDraft)
  const [title, setTitle] = useState(recoveredDraft?.title ?? "")
  const [sourceLanguage, setSourceLanguage] = useState(
    recoveredDraft?.sourceLanguage ?? "und"
  )
  const [content, setContent] = useState(recoveredDraft?.content ?? "")
  const updateDraft = (draft: DocumentDraft) => {
    saveNewDocumentDraft(draft)
    setTitle(draft.title)
    setSourceLanguage(draft.sourceLanguage)
    setContent(draft.content)
  }
  useEffect(() => {
    if (recoveredDraft) toast.info(t("recoveredDocumentDraft"))
  }, [recoveredDraft, t])
  const create = useMutation({
    mutationFn: () =>
      request<DocumentDetail>("/api/v1/documents", token, {
        method: "POST",
        body: JSON.stringify({
          title: title.trim(),
          source_language: sourceLanguage.trim(),
          content,
        }),
      }),
    onSuccess: (detail) => {
      clearNewDocumentDraft()
      toast.success(t("documentCreated"))
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      navigate(`/documents/${detail.document.id}`, { replace: true })
    },
    onError: showError,
  })

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-6 p-4 md:p-6">
      <EditorTopbar
        title={t("newDocument")}
        onBack={() => navigate("/documents")}
        actions={
          <Button
            disabled={create.isPending || !title.trim()}
            onClick={() => create.mutate()}
          >
            {create.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <SaveIcon data-icon="inline-start" />
            )}
            {t("createDocument")}
          </Button>
        }
      />
      <EditorFields
        title={title}
        sourceLanguage={sourceLanguage}
        content={content}
        documentReferences={documents.data ?? []}
        onTitleChange={(value) =>
          updateDraft(documentDraftFrom(value, sourceLanguage, content))
        }
        onSourceLanguageChange={(value) =>
          updateDraft(documentDraftFrom(title, value, content))
        }
        onContentChange={(value) =>
          updateDraft(documentDraftFrom(title, sourceLanguage, value))
        }
        onImageUpload={imageUpload}
      />
    </main>
  )
}

function ExistingDocumentPage({ token }: { token: string }) {
  const { documentId = "" } = useParams()
  const { t } = useI18n()
  const document = useQuery({
    queryKey: ["document", token, documentId],
    queryFn: () =>
      request<DocumentDetail>(`/api/v1/documents/${documentId}`, token),
    enabled: Boolean(documentId),
    refetchInterval: (query) =>
      query.state.data?.document.status === "published" &&
      !hasEditorialMetadata(query.state.data.document.metadata)
        ? 2_000
        : false,
  })

  if (document.isPending) return <EditorSkeleton />
  if (document.error || !document.data)
    return (
      <PageError error={document.error ?? new Error(t("documentNotFound"))} />
    )

  return <ExistingDocumentEditor token={token} detail={document.data} />
}

function ExistingDocumentEditor({
  token,
  detail,
}: {
  token: string
  detail: DocumentDetail
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const imageUpload = useImageUpload(token)
  const isPublished = detail.document.status === "published"
  const [deleteArmed, setDeleteArmed] = useState(false)
  const savedServerDraft = documentDraftFrom(
    detail.document.title,
    detail.document.source_language,
    detail.revision.content
  )
  const [recoveredDraft] = useState(() =>
    loadDocumentDraft(detail.document.id, detail.revision.id)
  )
  const initialDraft = recoveredDraft ?? savedServerDraft
  const [title, setTitle] = useState(initialDraft.title)
  useBrowserTitle(title || t("document"))
  const [sourceLanguage, setSourceLanguage] = useState(
    initialDraft.sourceLanguage
  )
  const [content, setContent] = useState(initialDraft.content)
  const [savedDraft, setSavedDraft] = useState(savedServerDraft)
  const [savedRevisionId, setSavedRevisionId] = useState(detail.revision.id)
  const [autosaveFailed, setAutosaveFailed] = useState(false)
  const draftRef = useRef(initialDraft)
  const savedDraftRef = useRef(savedServerDraft)
  const savedRevisionRef = useRef(detail.revision.id)
  const documents = useQuery({
    queryKey: ["documents", token],
    queryFn: () => request<Document[]>("/api/v1/documents", token),
  })
  const currentDraft = documentDraftFrom(title, sourceLanguage, content)
  const isDirty = !draftsMatch(currentDraft, savedDraft)
  const updateDraft = useCallback(
    (draft: DocumentDraft) => {
      draftRef.current = draft
      saveDocumentDraft(detail.document.id, savedRevisionRef.current, draft)
      setTitle(draft.title)
      setSourceLanguage(draft.sourceLanguage)
      setContent(draft.content)
    },
    [detail.document.id]
  )
  const saveCurrentRevision = useCallback(
    (draft: DocumentDraft) =>
      request<DocumentDetail>(
        `/api/v1/documents/${detail.document.id}`,
        token,
        {
          method: "PUT",
          body: JSON.stringify({
            title: draft.title.trim(),
            source_language: draft.sourceLanguage.trim(),
            content: draft.content,
          }),
        }
      ),
    [detail.document.id, token]
  )
  const markSaved = useCallback(
    (savedDetail: DocumentDetail, saved: DocumentDraft) => {
      savedDraftRef.current = saved
      savedRevisionRef.current = savedDetail.revision.id
      setSavedDraft(saved)
      setSavedRevisionId(savedDetail.revision.id)
      setAutosaveFailed(false)
      queryClient.setQueryData<DocumentDetail>(
        ["document", token, detail.document.id],
        savedDetail
      )
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      if (draftsMatch(draftRef.current, saved))
        clearDocumentDraft(detail.document.id)
      else
        saveDocumentDraft(
          detail.document.id,
          savedDetail.revision.id,
          draftRef.current
        )
    },
    [detail.document.id, queryClient, token]
  )
  const save = useMutation({
    mutationFn: ({ draft }: { draft: DocumentDraft; notify: boolean }) =>
      saveCurrentRevision(draft),
    onSuccess: (savedDetail, { draft, notify }) => {
      markSaved(savedDetail, draft)
      if (notify) toast.success(t("savedNewRevision"))
    },
    onError: (error, { notify }) => {
      setAutosaveFailed(true)
      if (notify) showError(error)
    },
  })
  const publish = useMutation({
    mutationFn: async () => {
      const draft = draftRef.current
      const savedDetail = draftsMatch(draft, savedDraftRef.current)
        ? null
        : await saveCurrentRevision(draft)
      await request<Document>(
        `/api/v1/documents/${detail.document.id}/publish`,
        token,
        {
          method: "POST",
        }
      )
      return { draft, savedDetail }
    },
    onSuccess: ({ draft, savedDetail }) => {
      if (savedDetail) markSaved(savedDetail, draft)
      toast.success(t("publishedCurrentRevision"))
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
      void queryClient.invalidateQueries({ queryKey: ["public-documents"] })
    },
    onError: (error) => {
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
      showError(error)
    },
  })
  const remove = useMutation({
    mutationFn: () =>
      request<void>(`/api/v1/documents/${detail.document.id}`, token, {
        method: "DELETE",
      }),
    onSuccess: () => {
      clearDocumentDraft(detail.document.id)
      toast.success(t("documentDeleted"))
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      void queryClient.invalidateQueries({ queryKey: ["public-documents"] })
      void queryClient.invalidateQueries({
        queryKey: ["public-document", detail.document.id],
      })
      navigate("/documents", { replace: true })
    },
    onError: showError,
  })
  const saveMutationRef = useRef(save.mutate)
  const publishMutationRef = useRef(publish.mutate)
  const savePendingRef = useRef(save.isPending)
  const publishPendingRef = useRef(publish.isPending)
  useEffect(() => {
    saveMutationRef.current = save.mutate
    savePendingRef.current = save.isPending
  }, [save.isPending, save.mutate])
  useEffect(() => {
    publishMutationRef.current = publish.mutate
    publishPendingRef.current = publish.isPending
  }, [publish.isPending, publish.mutate])
  const saveIfNeeded = useCallback((notify = false) => {
    const draft = draftRef.current
    if (
      !draft.title.trim() ||
      savePendingRef.current ||
      publishPendingRef.current ||
      draftsMatch(draft, savedDraftRef.current)
    )
      return
    savePendingRef.current = true
    saveMutationRef.current({ draft, notify })
  }, [])
  const publishCurrentRevision = useCallback(() => {
    if (savePendingRef.current || publishPendingRef.current) return
    publishPendingRef.current = true
    publishMutationRef.current()
  }, [])
  const saveAfterBlur = useCallback(() => {
    window.setTimeout(() => saveIfNeeded(), 0)
  }, [saveIfNeeded])
  useEffect(() => {
    if (recoveredDraft) toast.info(t("recoveredDocumentDraft"))
  }, [recoveredDraft, t])
  useEffect(() => {
    const timeout = window.setTimeout(() => saveIfNeeded(), 5_000)
    return () => window.clearTimeout(timeout)
  }, [content, saveIfNeeded, sourceLanguage, title])
  useEffect(() => {
    const interval = window.setInterval(() => saveIfNeeded(), 15_000)
    return () => window.clearInterval(interval)
  }, [saveIfNeeded])
  useEffect(() => {
    const saveWhenHidden = () => {
      if (document.visibilityState === "hidden") saveIfNeeded()
    }
    document.addEventListener("visibilitychange", saveWhenHidden)
    return () =>
      document.removeEventListener("visibilitychange", saveWhenHidden)
  }, [saveIfNeeded])
  useEffect(() => {
    if (
      detail.revision.id === savedRevisionRef.current ||
      !draftsMatch(draftRef.current, savedDraftRef.current)
    )
      return
    const nextDraft = documentDraftFrom(
      detail.document.title,
      detail.document.source_language,
      detail.revision.content
    )
    draftRef.current = nextDraft
    savedDraftRef.current = nextDraft
    savedRevisionRef.current = detail.revision.id
    setTitle(nextDraft.title)
    setSourceLanguage(nextDraft.sourceLanguage)
    setContent(nextDraft.content)
    setSavedDraft(nextDraft)
    setSavedRevisionId(detail.revision.id)
    clearDocumentDraft(detail.document.id)
  }, [
    detail.document.id,
    detail.document.source_language,
    detail.document.title,
    detail.revision.content,
    detail.revision.id,
  ])
  const isWriting = save.isPending || publish.isPending || remove.isPending
  const autosaveStatus = isWriting
    ? t("autosaving")
    : autosaveFailed
      ? t("autosaveFailed")
      : isDirty
        ? t("autosavePending")
        : t("autosaved")

  return (
    <main className="mx-auto grid w-full max-w-7xl gap-8 p-4 md:p-6 xl:grid-cols-[minmax(0,1fr)_19rem]">
      <section className="min-w-0">
        <EditorTopbar
          title={title || t("document")}
          onBack={() => navigate("/documents")}
          actions={
            <div className="flex flex-wrap items-center justify-end gap-2">
              <Badge variant={isPublished ? "secondary" : "outline"}>
                {isPublished ? t("published") : t("draft")}
              </Badge>
              <span
                role="status"
                className="inline-flex text-xs text-muted-foreground"
              >
                {save.isPending ? (
                  <LoaderCircleIcon className="mr-1 size-3 animate-spin" />
                ) : null}
                {autosaveStatus}
              </span>
              <Button
                variant="outline"
                disabled={isWriting || !title.trim() || !isDirty}
                onClick={() => saveIfNeeded(true)}
              >
                {save.isPending ? (
                  <LoaderCircleIcon
                    className="animate-spin"
                    data-icon="inline-start"
                  />
                ) : (
                  <SaveIcon data-icon="inline-start" />
                )}
                {t("save")}
              </Button>
              <Button
                disabled={isWriting || !title.trim()}
                onClick={publishCurrentRevision}
              >
                {publish.isPending ? (
                  <LoaderCircleIcon
                    className="animate-spin"
                    data-icon="inline-start"
                  />
                ) : (
                  <Globe2Icon data-icon="inline-start" />
                )}
                {t("publish")}
              </Button>
              {deleteArmed ? (
                <>
                  <Button
                    variant="ghost"
                    disabled={remove.isPending}
                    onClick={() => setDeleteArmed(false)}
                  >
                    <XIcon data-icon="inline-start" />
                    {t("cancel")}
                  </Button>
                  <Button
                    variant="destructive"
                    disabled={remove.isPending}
                    onClick={() => remove.mutate()}
                  >
                    {remove.isPending ? (
                      <LoaderCircleIcon
                        className="animate-spin"
                        data-icon="inline-start"
                      />
                    ) : (
                      <Trash2Icon data-icon="inline-start" />
                    )}
                    {t("deleteDocumentConfirm")}
                  </Button>
                </>
              ) : (
                <Button
                  variant="ghost"
                  disabled={isWriting}
                  onClick={() => setDeleteArmed(true)}
                >
                  <Trash2Icon data-icon="inline-start" />
                  {t("deleteDocument")}
                </Button>
              )}
            </div>
          }
        />
        <div className="mt-4 flex items-center gap-2 text-xs text-muted-foreground">
          <span>{t("revision")}</span>
          <span className="font-mono">{savedRevisionId.slice(0, 8)}</span>
          {isPublished ? (
            <Button
              variant="link"
              size="xs"
              className="ml-auto"
              onClick={() => navigate(`/p/${detail.document.id}`)}
            >
              <ExternalLinkIcon data-icon="inline-start" />
              {t("viewPublicArticle")}
            </Button>
          ) : null}
        </div>
        <Separator className="my-4" />
        <EditorFields
          title={title}
          sourceLanguage={sourceLanguage}
          content={content}
          disabled={publish.isPending}
          documentReferences={documents.data ?? []}
          currentDocumentId={detail.document.id}
          onTitleChange={(value) =>
            updateDraft(documentDraftFrom(value, sourceLanguage, content))
          }
          onSourceLanguageChange={(value) =>
            updateDraft(documentDraftFrom(title, value, content))
          }
          onContentChange={(value) =>
            updateDraft(documentDraftFrom(title, sourceLanguage, value))
          }
          onImageUpload={imageUpload}
          onBlur={saveAfterBlur}
        />
        <PublicationTimeEditor token={token} documentId={detail.document.id} />
      </section>
      <aside className="min-w-0 xl:pt-14">
        <EditorialMetadata
          metadata={publishedMetadata(detail.document.metadata)}
          pending={
            isPublished && !hasEditorialMetadata(detail.document.metadata)
          }
          variant="private"
        />
        <AiPanel
          token={token}
          detail={detail}
          title={title}
          sourceLanguage={sourceLanguage}
          content={content}
          onSourceLanguageChange={(value) =>
            updateDraft(documentDraftFrom(title, value, content))
          }
          isDirty={isDirty}
          isSaving={save.isPending}
          isPublishing={publish.isPending}
        />
      </aside>
    </main>
  )
}

function EditorTopbar({
  title,
  onBack,
  actions,
}: {
  title: string
  onBack: () => void
  actions: ReactNode
}) {
  const { t } = useI18n()
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-center gap-2">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("backToDocuments")}
          onClick={onBack}
        >
          <ArrowLeftIcon />
        </Button>
        <h1 className="min-w-0 truncate text-xl font-semibold tracking-tight">
          {title}
        </h1>
      </div>
      {actions}
    </div>
  )
}

function EditorFields({
  title,
  sourceLanguage,
  content,
  disabled = false,
  documentReferences,
  currentDocumentId,
  onTitleChange,
  onSourceLanguageChange,
  onContentChange,
  onImageUpload,
  onBlur,
}: {
  title: string
  sourceLanguage: string
  content: string
  disabled?: boolean
  documentReferences?: Document[]
  currentDocumentId?: string
  onTitleChange: (value: string) => void
  onSourceLanguageChange: (value: string) => void
  onContentChange: (value: string) => void
  onImageUpload: (image: File) => Promise<string>
  onBlur?: () => void
}) {
  const { t } = useI18n()
  return (
    <section
      className="mx-auto flex w-full max-w-4xl flex-col gap-5"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) onBlur?.()
      }}
    >
      <Field className="gap-1">
        <FieldLabel className="sr-only" htmlFor="document-title">
          {t("title")}
        </FieldLabel>
        <Input
          id="document-title"
          value={title}
          placeholder={t("titlePlaceholder")}
          disabled={disabled}
          className="h-auto rounded-none border-0 bg-transparent px-0 py-1 text-3xl font-semibold tracking-[-0.025em] shadow-none placeholder:text-muted-foreground/70 focus-visible:ring-0 md:text-4xl"
          onChange={(event) => onTitleChange(event.target.value)}
        />
      </Field>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
        <Field className="flex-row items-center gap-2">
          <FieldLabel htmlFor="document-source-language" className="shrink-0">
            {t("sourceLanguage")}
          </FieldLabel>
          <Input
            id="document-source-language"
            value={sourceLanguage}
            placeholder={t("sourceLanguagePlaceholder")}
            disabled={disabled}
            className="h-7 w-24 rounded-md bg-muted/60 text-xs shadow-none"
            onChange={(event) => onSourceLanguageChange(event.target.value)}
          />
        </Field>
        <span aria-hidden="true">·</span>
        <span>{t("sourceLanguageDescription")}</span>
      </div>
      <MarkdownEditor
        value={content}
        disabled={disabled}
        onChange={onContentChange}
        onImageUpload={onImageUpload}
        documentReferences={documentReferences}
        currentDocumentId={currentDocumentId}
        writeLabel={t("editorWrite")}
        markdownLabel={t("markdown")}
        placeholder={t("editorPlaceholder")}
        commandsLabel={t("editorCommands")}
        insertLinkLabel={t("editorInsertLink")}
        insertDocumentLinkLabel={t("editorInsertDocument")}
        insertImageLabel={t("editorInsertImage")}
        imageUploadingLabel={t("editorUploadingImage")}
        imageUploadFailedLabel={t("editorImageUploadFailed")}
        insertUrlLabel={t("editorInsertUrl")}
        insertLabel={t("editorInsert")}
        cancelLabel={t("cancel")}
        searchDocumentsLabel={t("editorSearchDocuments")}
        noReferenceDocumentsLabel={t("editorNoReferenceDocuments")}
        draftLabel={t("draft")}
      />
      <FieldDescription>{t("markdownDescription")}</FieldDescription>
    </section>
  )
}

function PublicationTimeEditor({
  token,
  documentId,
}: {
  token: string
  documentId: string
}) {
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const publicationTime = useQuery({
    queryKey: ["publication-time", token, documentId],
    queryFn: () =>
      request<PublicationTime>(
        `/api/v1/documents/${documentId}/publication`,
        token
      ),
  })
  const [value, setValue] = useState<string | null>(null)
  const publicationValue =
    value ??
    (publicationTime.data?.published_at != null
      ? datetimeLocalValue(publicationTime.data.published_at)
      : "")
  const update = useMutation({
    mutationFn: () =>
      request<PublicationTime>(
        `/api/v1/documents/${documentId}/publication`,
        token,
        {
          method: "PUT",
          body: JSON.stringify({
            published_at: Math.floor(
              new Date(publicationValue).getTime() / 1_000
            ),
          }),
        }
      ),
    onSuccess: () => {
      toast.success(t("publicationTimeSaved"))
      void queryClient.invalidateQueries({ queryKey: ["public-documents"] })
      void queryClient.invalidateQueries({
        queryKey: ["public-document", documentId],
      })
      void queryClient.invalidateQueries({ queryKey: ["public-user-profile"] })
    },
    onError: showError,
  })

  if (publicationTime.isPending)
    return <Skeleton className="mt-8 h-24 w-full" />
  if (publicationTime.error) return <PageError error={publicationTime.error} />

  return (
    <section className="mt-8 max-w-4xl border-t pt-6">
      <Field className="gap-1">
        <FieldLabel htmlFor="document-publication-time">
          <CalendarClockIcon data-icon="inline-start" />
          {t("publicationTime")}
        </FieldLabel>
        <FieldDescription>{t("publicationTimeDescription")}</FieldDescription>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <Input
            id="document-publication-time"
            type="datetime-local"
            value={publicationValue}
            disabled={update.isPending}
            className="w-auto"
            onChange={(event) => setValue(event.target.value)}
          />
          <Button
            variant="outline"
            disabled={
              update.isPending ||
              !publicationValue ||
              Number.isNaN(new Date(publicationValue).getTime())
            }
            onClick={() => update.mutate()}
          >
            {update.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <SaveIcon data-icon="inline-start" />
            )}
            {t("savePublicationTime")}
          </Button>
        </div>
      </Field>
    </section>
  )
}

function AiPanel({
  token,
  detail,
  title,
  sourceLanguage,
  content,
  onSourceLanguageChange,
  isDirty,
  isSaving,
  isPublishing,
}: {
  token: string
  detail: DocumentDetail
  title: string
  sourceLanguage: string
  content: string
  onSourceLanguageChange: (value: string) => void
  isDirty: boolean
  isSaving: boolean
  isPublishing: boolean
}) {
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const [run, setRun] = useState<AiRun>()
  const ai = useMutation({
    mutationFn: (task: AiRun["task"]) =>
      request<AiRun>(`/api/v1/documents/${detail.document.id}/ai`, token, {
        method: "POST",
        body: JSON.stringify({ task }),
      }),
    onSuccess: (result) => {
      setRun(result)
      toast.success(t("aiWorkRecorded"))
    },
    onError: showError,
  })
  const applyMetadata = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(
        `/api/v1/documents/${detail.document.id}`,
        token,
        {
          method: "PUT",
          body: JSON.stringify({
            title: title.trim(),
            source_language: sourceLanguage.trim(),
            content,
            message: t("appliedAiMetadata"),
            metadata: JSON.parse(run?.output ?? "{}"),
          }),
        }
      ),
    onSuccess: () => {
      toast.success(t("metadataSaved"))
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
    },
    onError: showError,
  })
  const applyPolish = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(
        `/api/v1/documents/${detail.document.id}`,
        token,
        {
          method: "PUT",
          body: JSON.stringify({
            title: title.trim(),
            source_language: sourceLanguage.trim(),
            content: run?.proposed_content ?? content,
            message: t("appliedAiPolish"),
          }),
        }
      ),
    onSuccess: () => {
      toast.success(t("polishSaved"))
      void queryClient.invalidateQueries({
        queryKey: ["documents", token],
      })
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
    },
    onError: showError,
  })
  const needsSaveBeforeAi = isDirty || isSaving || isPublishing
  const isAiActionPending =
    ai.isPending || applyMetadata.isPending || applyPolish.isPending
  const action = (task: AiRun["task"], label: string) => (
    <Button
      key={task}
      variant="outline"
      className="w-full justify-start"
      disabled={needsSaveBeforeAi || isAiActionPending}
      onClick={() => ai.mutate(task)}
    >
      {ai.isPending ? (
        <LoaderCircleIcon className="animate-spin" data-icon="inline-start" />
      ) : (
        <SparklesIcon data-icon="inline-start" />
      )}
      {label}
    </Button>
  )

  return (
    <section className="flex min-h-80 flex-col">
      <div>
        <h2 className="text-sm font-medium">{t("documentAi")}</h2>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {t("documentAiDescription")}
        </p>
      </div>
      <Tabs defaultValue="actions" className="mt-4 min-h-0 flex-1">
        <TabsList>
          <TabsTrigger value="actions">{t("aiActions")}</TabsTrigger>
          <TabsTrigger value="output">{t("aiOutput")}</TabsTrigger>
        </TabsList>
        <TabsContent value="actions" className="pt-4">
          <div className="flex flex-col gap-3">
            {needsSaveBeforeAi ? (
              <Alert>
                <AlertTitle>{t("saveBeforeAiTitle")}</AlertTitle>
                <AlertDescription>{t("saveBeforeAi")}</AlertDescription>
              </Alert>
            ) : null}
            {action("polish", t("polishDocument"))}
            {action("metadata", t("extractMetadata"))}
            {action("detect_language", t("detectSourceLanguage"))}
          </div>
        </TabsContent>
        <TabsContent value="output" className="pt-4">
          {run ? (
            <div className="flex flex-col gap-3">
              <pre className="max-h-125 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap">
                {run.output}
              </pre>
              {run.task === "metadata" ? (
                <Button
                  variant="outline"
                  disabled={needsSaveBeforeAi || isAiActionPending}
                  onClick={() => applyMetadata.mutate()}
                >
                  {applyMetadata.isPending ? (
                    <LoaderCircleIcon
                      className="animate-spin"
                      data-icon="inline-start"
                    />
                  ) : (
                    <SaveIcon data-icon="inline-start" />
                  )}
                  {t("applyMetadata")}
                </Button>
              ) : null}
              {run.task === "polish" && run.proposed_content ? (
                <Button
                  variant="outline"
                  disabled={needsSaveBeforeAi || isAiActionPending}
                  onClick={() => applyPolish.mutate()}
                >
                  {applyPolish.isPending ? (
                    <LoaderCircleIcon
                      className="animate-spin"
                      data-icon="inline-start"
                    />
                  ) : (
                    <SaveIcon data-icon="inline-start" />
                  )}
                  {t("applyPolish")}
                </Button>
              ) : null}
              {run.task === "detect_language" ? (
                <Button
                  variant="outline"
                  disabled={needsSaveBeforeAi || isAiActionPending}
                  onClick={() => onSourceLanguageChange(run.output.trim())}
                >
                  <LanguagesIcon data-icon="inline-start" />
                  {t("applyDetectedLanguage")}
                </Button>
              ) : null}
            </div>
          ) : (
            <p className="text-sm leading-6 text-muted-foreground">
              {t("aiOutputEmpty")}
            </p>
          )}
        </TabsContent>
      </Tabs>
    </section>
  )
}

function publishedMetadata(value: Record<string, unknown>): PublishedMetadata {
  const strings = (key: string) =>
    typeof value[key] === "string" ? value[key] : ""
  const textList = (key: string) =>
    Array.isArray(value[key])
      ? value[key].filter((item): item is string => typeof item === "string")
      : []
  const record = (candidate: unknown): Record<string, unknown> | null =>
    candidate && typeof candidate === "object" && !Array.isArray(candidate)
      ? (candidate as Record<string, unknown>)
      : null
  const evidence = (candidate: unknown): SummaryEvidence[] =>
    Array.isArray(candidate)
      ? candidate.flatMap((item) => {
          const itemRecord = record(item)
          if (!itemRecord) return []
          return [
            {
              article_title:
                typeof itemRecord.article_title === "string"
                  ? itemRecord.article_title
                  : "",
              article_url:
                typeof itemRecord.article_url === "string"
                  ? itemRecord.article_url
                  : "",
              explanation:
                typeof itemRecord.explanation === "string"
                  ? itemRecord.explanation
                  : "",
            },
          ]
        })
      : []
  const mbtiRecord = record(value.mbti_analysis)
  const mbti_analysis: MbtiAnalysis = {
    type_code:
      typeof mbtiRecord?.type_code === "string" ? mbtiRecord.type_code : "",
    confidence:
      typeof mbtiRecord?.confidence === "string" ? mbtiRecord.confidence : "",
    dimensions: Array.isArray(mbtiRecord?.dimensions)
      ? mbtiRecord.dimensions.flatMap((item) => {
          const dimension = record(item)
          if (!dimension) return []
          return [
            {
              axis: typeof dimension.axis === "string" ? dimension.axis : "",
              preference:
                typeof dimension.preference === "string"
                  ? dimension.preference
                  : "",
              confidence:
                typeof dimension.confidence === "string"
                  ? dimension.confidence
                  : "",
              evidence: evidence(dimension.evidence),
            },
          ]
        })
      : [],
  }
  const schwartz_values: SchwartzValue[] = Array.isArray(value.schwartz_values)
    ? value.schwartz_values.flatMap((item) => {
        const schwartzValue = record(item)
        if (!schwartzValue) return []
        return [
          {
            key:
              typeof schwartzValue.key === "string" ? schwartzValue.key : "",
            score:
              typeof schwartzValue.score === "number" ? schwartzValue.score : 0,
            rank: typeof schwartzValue.rank === "number" ? schwartzValue.rank : 0,
            evidence: evidence(schwartzValue.evidence),
          },
        ]
      })
    : []
  const daily_timeline: DailyTimelineEntry[] = Array.isArray(value.daily_timeline)
    ? value.daily_timeline.flatMap((item) => {
        const entry = record(item)
        if (!entry) return []
        return [
          {
            date: typeof entry.date === "string" ? entry.date : "",
            summary: typeof entry.summary === "string" ? entry.summary : "",
            evidence: evidence(entry.evidence),
          },
        ]
      })
    : []
  return {
    description: strings("description"),
    summary: strings("summary"),
    short_summary: strings("short_summary"),
    tags: textList("tags"),
    inferred_date: strings("inferred_date"),
    inferred_lang: strings("inferred_lang"),
    key_points: textList("key_points"),
    audience: strings("audience"),
    experience_summary: strings("experience_summary"),
    personality_analysis: strings("personality_analysis"),
    mbti_analysis,
    schwartz_values,
    unconscious_motivations: strings("unconscious_motivations"),
    philosophical_references: strings("philosophical_references"),
    daily_timeline,
  }
}

function hasEditorialMetadata(value: Record<string, unknown>) {
  const metadata = publishedMetadata(value)
  return Boolean(
    metadata.description ||
    metadata.summary ||
    metadata.short_summary ||
    metadata.tags.length ||
    metadata.key_points.length ||
    metadata.audience ||
    metadata.experience_summary ||
    metadata.personality_analysis ||
    metadata.mbti_analysis.type_code ||
    metadata.schwartz_values.length ||
    metadata.unconscious_motivations ||
    metadata.philosophical_references ||
    metadata.daily_timeline.length
  )
}

function EditorialMetadata({
  metadata,
  pending = false,
  variant = "detail",
}: {
  metadata: PublishedMetadata
  pending?: boolean
  variant?: "list" | "detail" | "private"
}) {
  const { t } = useI18n()
  const hasMetadata = Boolean(
    metadata.description ||
    metadata.summary ||
    metadata.short_summary ||
    metadata.tags.length ||
    metadata.key_points.length ||
    metadata.audience
  )
  if (!hasMetadata)
    return pending ? (
      <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
        <LoaderCircleIcon
          className="size-3.5 animate-spin"
          aria-hidden="true"
        />
        {t("metadataPreparing")}
      </p>
    ) : null

  if (variant === "list")
    return (
      <div className="mt-3 space-y-2">
        <p className="line-clamp-2 text-sm leading-6 text-muted-foreground">
          {metadata.short_summary || metadata.description}
        </p>
        {metadata.tags.length ? (
          <div
            className="flex flex-wrap gap-1.5"
            aria-label={t("metadataTags")}
          >
            {metadata.tags.map((tag) => (
              <Badge key={tag} variant="secondary" className="font-normal">
                {tag}
              </Badge>
            ))}
          </div>
        ) : null}
      </div>
    )

  return (
    <section aria-label={t("metadata")} className="mt-6 border-t pt-5">
      <h2 className="text-sm font-medium">{t("metadata")}</h2>
      {metadata.description ? (
        <p className="mt-2 text-base leading-7 text-foreground/90">
          {metadata.description}
        </p>
      ) : null}
      {metadata.summary && metadata.summary !== metadata.description ? (
        <p className="mt-3 text-sm leading-6 text-muted-foreground">
          {metadata.summary}
        </p>
      ) : null}
      {metadata.tags.length ? (
        <div
          className="mt-4 flex flex-wrap gap-1.5"
          aria-label={t("metadataTags")}
        >
          {metadata.tags.map((tag) => (
            <Badge key={tag} variant="secondary" className="font-normal">
              {tag}
            </Badge>
          ))}
        </div>
      ) : null}
      {metadata.key_points.length ? (
        <div className="mt-5">
          <h3 className="text-sm font-medium">{t("metadataKeyPoints")}</h3>
          <ul className="mt-2 list-disc space-y-1 pl-5 text-sm leading-6 text-muted-foreground">
            {metadata.key_points.map((point) => (
              <li key={point}>{point}</li>
            ))}
          </ul>
        </div>
      ) : null}
      {metadata.audience ? (
        <p className="mt-5 text-sm leading-6 text-muted-foreground">
          <span className="font-medium text-foreground">
            {t("metadataAudience")}:{" "}
          </span>
          {metadata.audience}
        </p>
      ) : null}
    </section>
  )
}

function SquarePage() {
  const navigate = useNavigate()
  const { locale, t } = useI18n()
  useBrowserTitle(t("pageTitleSquare"))
  const documents = useQuery({
    queryKey: ["public-documents", locale],
    queryFn: () =>
      request<PublicDocument[]>(
        `/api/public/documents?language=${encodeURIComponent(locale)}`
      ),
  })

  return (
    <main className="mx-auto flex w-full max-w-4xl flex-col gap-10 p-6 md:py-14">
      <section className="max-w-2xl">
        <h1 className="text-3xl font-semibold tracking-tight text-balance">
          {t("squareTitle")}
        </h1>
        <p className="mt-3 text-base leading-7 text-muted-foreground">
          {t("squareDescription")}
        </p>
      </section>
      {documents.isPending ? (
        <PublicListSkeleton />
      ) : documents.error ? (
        <PageError error={documents.error} />
      ) : (documents.data ?? []).length === 0 ? (
        <Empty className="min-h-80">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <BookOpenIcon />
            </EmptyMedia>
            <EmptyTitle>{t("squareEmptyTitle")}</EmptyTitle>
            <EmptyDescription>{t("squareEmptyDescription")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => navigate("/documents")}>
              <PlusIcon data-icon="inline-start" />
              {t("startWriting")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <section aria-label={t("squareTitle")} className="flex flex-col">
          {(documents.data ?? []).map((document, index) => (
            <div key={document.id}>
              {index > 0 ? <Separator /> : null}
              <PublicDocumentRow
                document={document}
                date={formatDate(document.published_at, locale)}
                onOpen={() => navigate(`/p/${document.id}`)}
              />
            </div>
          ))}
        </section>
      )}
    </main>
  )
}

function PublicDocumentRow({
  document,
  date,
  onOpen,
}: {
  document: PublicDocument
  date: string
  onOpen: () => void
}) {
  const navigate = useNavigate()
  const { t } = useI18n()
  return (
    <article className="group flex items-center gap-4 py-5">
      <div className="min-w-0 flex-1">
        <Button
          variant="ghost"
          className="w-full justify-start px-0 text-left hover:bg-transparent"
          onClick={onOpen}
        >
          <span className="min-w-0">
            <span className="block truncate font-medium">{document.title}</span>
            <span className="mt-1 block text-sm font-normal text-muted-foreground">
              {t("publishedOn").replace("{date}", date)}
            </span>
          </span>
        </Button>
        <div className="mt-2 flex flex-wrap items-center gap-1.5">
          <LinkitUserInfo userId={document.owner_id} compact />
          <Button
            variant="ghost"
            size="sm"
            onClick={() => navigate(`/u/${document.owner_id}`)}
          >
            <UserRoundIcon data-icon="inline-start" />
            {t("viewAuthorPage")}
          </Button>
        </div>
        <EditorialMetadata
          metadata={document.metadata}
          pending={Boolean(document.metadata_status)}
          variant="list"
        />
      </div>
      <Button variant="ghost" size="sm" onClick={onOpen}>
        {t("readArticle")}
        <ArrowLeftIcon className="rotate-180" data-icon="inline-end" />
      </Button>
    </article>
  )
}

function PublicDocumentPage() {
  const { documentId = "" } = useParams()
  const navigate = useNavigate()
  const { locale, t } = useI18n()
  const { isReady, isAuthenticated, session } = useAuthMini()
  const articleRef = useRef<HTMLElement>(null)
  const [commentAnchor, setCommentAnchor] = useState<{
    documentId: string
    anchor: CommentAnchor
  } | null>(null)
  const document = useQuery({
    queryKey: ["public-document", documentId, locale],
    queryFn: () =>
      request<PublicDocumentDetail>(
        `/api/public/documents/${documentId}?language=${encodeURIComponent(locale)}`
      ),
    enabled: Boolean(documentId),
    refetchInterval: (query) =>
      query.state.data?.is_translation_fallback ||
      query.state.data?.metadata_status
        ? 2_000
        : false,
  })
  useBrowserTitle(document.data?.title ?? t("pageTitleEditor"))
  const me = useQuery({
    queryKey: ["me", session?.accessToken],
    queryFn: () => request<Me>("/api/v1/me", session?.accessToken ?? undefined),
    enabled: isReady && isAuthenticated && Boolean(session?.accessToken),
  })
  const activeCommentAnchor =
    commentAnchor?.documentId === documentId ? commentAnchor.anchor : null

  if (document.isPending) return <LoadingPage />
  if (document.error || !document.data)
    return (
      <PageError
        error={document.error ?? new Error(t("publishedDocumentNotFound"))}
        action={
          <Button variant="outline" onClick={() => navigate("/square")}>
            <ArrowLeftIcon data-icon="inline-start" />
            {t("backToSquare")}
          </Button>
        }
      />
    )

  return (
    <main className="mx-auto w-full max-w-3xl p-6 md:py-14">
      <Button variant="ghost" size="sm" onClick={() => navigate("/square")}>
        <ArrowLeftIcon data-icon="inline-start" />
        {t("backToSquare")}
      </Button>
      <header className="mt-10 max-w-[72ch]">
        <h1 className="text-3xl font-semibold tracking-tight text-balance md:text-4xl">
          {document.data.title}
        </h1>
        <p className="mt-3 text-sm text-muted-foreground">
          {t("publishedOn").replace(
            "{date}",
            formatDate(document.data.published_at, locale)
          )}
        </p>
        <div className="mt-4 flex flex-wrap items-center gap-3">
          <LinkitUserInfo userId={document.data.owner_id} compact />
          <Button
            variant="ghost"
            size="sm"
            onClick={() => navigate(`/u/${document.data.owner_id}`)}
          >
            <UserRoundIcon data-icon="inline-start" />
            {t("viewAuthorPage")}
          </Button>
          <Badge variant="outline">{document.data.language}</Badge>
          {me.data?.user_id === document.data.owner_id ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => navigate(`/documents/${document.data.id}`)}
            >
              <PencilIcon data-icon="inline-start" />
              {t("editDocument")}
            </Button>
          ) : null}
        </div>
        <EditorialMetadata
          metadata={document.data.metadata}
          pending={Boolean(document.data.metadata_status)}
        />
      </header>
      {document.data.is_translation_fallback ? (
        <Alert className="mt-6 max-w-[72ch]">
          <LanguagesIcon data-icon="inline-start" />
          <AlertTitle>{t("translationInProgressTitle")}</AlertTitle>
          <AlertDescription>{t("translationInProgress")}</AlertDescription>
        </Alert>
      ) : null}
      <article
        ref={articleRef}
        data-commentable
        className="mt-10 max-w-[72ch]"
        onMouseUp={() => {
          const anchor = readCommentAnchor(articleRef.current)
          setCommentAnchor(anchor ? { documentId, anchor } : null)
        }}
        onKeyUp={() => {
          const anchor = readCommentAnchor(articleRef.current)
          setCommentAnchor(anchor ? { documentId, anchor } : null)
        }}
        onClick={(event) => {
          const mark = (event.target as HTMLElement).closest<HTMLElement>(
            "mark[data-comment-id]"
          )
          const commentId = mark?.dataset.commentId
          if (commentId)
            window.document
              .getElementById(`comment-${commentId}`)
              ?.scrollIntoView({ behavior: "smooth", block: "center" })
        }}
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkMath]}
          rehypePlugins={[rehypeKatex]}
          components={markdownComponents}
        >
          {document.data.content}
        </ReactMarkdown>
      </article>
      <DocumentComments
        documentId={document.data.id}
        language={document.data.language}
        token={session?.accessToken ?? undefined}
        canComment={isReady && isAuthenticated && Boolean(session?.accessToken)}
        articleRef={articleRef}
        anchor={activeCommentAnchor}
        onClearAnchor={() => setCommentAnchor(null)}
      />
    </main>
  )
}

type CommentAnchor = {
  quote: string
  prefix: string
  suffix: string
}

function DocumentComments({
  documentId,
  language,
  token,
  canComment,
  articleRef,
  anchor,
  onClearAnchor,
}: {
  documentId: string
  language: string
  token?: string
  canComment: boolean
  articleRef: { current: HTMLElement | null }
  anchor: CommentAnchor | null
  onClearAnchor: () => void
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale, t } = useI18n()
  const [content, setContent] = useState("")
  const comments = useQuery({
    queryKey: ["public-document-comments", documentId, language],
    queryFn: () =>
      request<DocumentComment[]>(
        `/api/public/documents/${documentId}/comments?language=${encodeURIComponent(language)}`
      ),
    enabled: Boolean(documentId && language),
  })
  const post = useMutation({
    mutationFn: () => {
      if (!token) throw new Error(t("signInToComment"))
      return request<DocumentComment>(
        `/api/v1/public-documents/${documentId}/comments`,
        token,
        {
          method: "POST",
          body: JSON.stringify({
            language,
            content,
            anchor: anchor ?? undefined,
          }),
        }
      )
    },
    onSuccess: () => {
      setContent("")
      onClearAnchor()
      toast.success(t("commentPosted"))
      void queryClient.invalidateQueries({
        queryKey: ["public-document-comments", documentId, language],
      })
    },
    onError: showError,
  })
  useEffect(() => {
    const article = articleRef.current
    if (!article) return
    return applyCommentHighlights(article, comments.data ?? [])
  }, [articleRef, comments.data])

  return (
    <section
      className="mt-14 max-w-[72ch] border-t pt-8"
      aria-labelledby="comments-title"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <h2
            id="comments-title"
            className="text-xl font-semibold tracking-tight"
          >
            <MessageSquareIcon data-icon="inline-start" />
            {t("commentsTitle")}
          </h2>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {t("commentsDescription")}
          </p>
        </div>
        <span className="text-sm text-muted-foreground">
          {comments.data?.length ?? 0}
        </span>
      </div>
      {canComment ? (
        <form
          className="mt-6 border-y py-5"
          onSubmit={(event) => {
            event.preventDefault()
            post.mutate()
          }}
        >
          {anchor ? (
            <div className="mb-3 rounded-md bg-muted px-3 py-2 text-sm">
              <div className="flex items-center justify-between gap-3">
                <span className="font-medium">{t("selectedText")}</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={onClearAnchor}
                >
                  <XIcon data-icon="inline-start" />
                  {t("clearSelectionComment")}
                </Button>
              </div>
              <blockquote className="mt-2 border-l border-primary/35 pl-3 leading-6 text-muted-foreground">
                {anchor.quote}
              </blockquote>
            </div>
          ) : null}
          <Field>
            <FieldLabel htmlFor="new-document-comment">
              {anchor ? t("commentOnSelection") : t("writeComment")}
            </FieldLabel>
            <Textarea
              id="new-document-comment"
              value={content}
              maxLength={4_000}
              placeholder={t("commentPlaceholder")}
              disabled={post.isPending}
              className="min-h-24 resize-y"
              onChange={(event) => setContent(event.target.value)}
            />
          </Field>
          <div className="mt-3 flex justify-end">
            <Button disabled={post.isPending || !content.trim()} type="submit">
              {post.isPending ? (
                <LoaderCircleIcon
                  className="animate-spin"
                  data-icon="inline-start"
                />
              ) : (
                <MessageSquareIcon data-icon="inline-start" />
              )}
              {t("postComment")}
            </Button>
          </div>
        </form>
      ) : (
        <div className="mt-6 border-y py-5">
          <Button variant="outline" onClick={() => navigate("/documents")}>
            <MessageSquareIcon data-icon="inline-start" />
            {t("signInToComment")}
          </Button>
        </div>
      )}
      {comments.isPending ? (
        <div className="mt-6 space-y-4">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-20 w-full" />
        </div>
      ) : comments.error ? (
        <PageError error={comments.error} />
      ) : (comments.data ?? []).length === 0 ? (
        <p className="mt-6 text-sm leading-6 text-muted-foreground">
          {t("commentsEmpty")}
        </p>
      ) : (
        <div className="mt-2 divide-y">
          {(comments.data ?? []).map((comment) => (
            <article
              id={`comment-${comment.id}`}
              key={comment.id}
              className="py-5"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <LinkitUserInfo userId={comment.author_id} compact />
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">
                    {comment.quote ? t("inlineComment") : t("fullComment")}
                  </Badge>
                  <time
                    dateTime={new Date(
                      comment.created_at * 1_000
                    ).toISOString()}
                  >
                    {formatDateTime(comment.created_at, locale)}
                  </time>
                </div>
              </div>
              {comment.quote ? (
                <blockquote className="mt-3 border-l border-primary/35 pl-3 text-sm leading-6 text-muted-foreground">
                  {comment.quote}
                </blockquote>
              ) : null}
              <p className="mt-3 leading-7 whitespace-pre-wrap">
                {comment.content}
              </p>
            </article>
          ))}
        </div>
      )}
    </section>
  )
}

function readCommentAnchor(root: HTMLElement | null): CommentAnchor | null {
  if (!root) return null
  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed)
    return null
  const range = selection.getRangeAt(0)
  if (
    !root.contains(range.startContainer) ||
    !root.contains(range.endContainer)
  )
    return null
  const selected = range.toString()
  const quote = selected.trim()
  if (!quote || quote.length > 1_200) return null
  const leading = selected.length - selected.trimStart().length
  const before = range.cloneRange()
  before.selectNodeContents(root)
  before.setEnd(range.startContainer, range.startOffset)
  const start = before.toString().length + leading
  const fullText = root.textContent ?? ""
  return {
    quote,
    prefix: fullText.slice(Math.max(0, start - 80), start),
    suffix: fullText.slice(start + quote.length, start + quote.length + 80),
  }
}

function applyCommentHighlights(
  root: HTMLElement,
  comments: DocumentComment[]
) {
  clearCommentHighlights(root)
  const text = root.textContent ?? ""
  const anchored = comments
    .filter((comment) => comment.quote)
    .flatMap((comment) => {
      const start = commentAnchorOffset(text, comment)
      return start === null ? [] : [{ comment, start }]
    })
    .sort((left, right) => right.start - left.start)
  for (const { comment, start } of anchored) {
    if (comment.quote)
      highlightTextRange(root, start, start + comment.quote.length, comment)
  }
  return () => clearCommentHighlights(root)
}

function clearCommentHighlights(root: HTMLElement) {
  for (const mark of root.querySelectorAll("mark[data-comment-id]")) {
    const parent = mark.parentNode
    if (!parent) continue
    while (mark.firstChild) parent.insertBefore(mark.firstChild, mark)
    parent.removeChild(mark)
    parent.normalize()
  }
}

function commentAnchorOffset(
  text: string,
  comment: DocumentComment
): number | null {
  const quote = comment.quote
  if (!quote) return null
  const fallback = text.indexOf(quote)
  let offset = fallback
  while (offset >= 0) {
    const prefixMatches =
      !comment.prefix ||
      text.slice(Math.max(0, offset - comment.prefix.length), offset) ===
        comment.prefix
    const suffixMatches =
      !comment.suffix ||
      text.slice(
        offset + quote.length,
        offset + quote.length + comment.suffix.length
      ) === comment.suffix
    if (prefixMatches && suffixMatches) return offset
    offset = text.indexOf(quote, offset + quote.length)
  }
  return fallback >= 0 ? fallback : null
}

function highlightTextRange(
  root: HTMLElement,
  start: number,
  end: number,
  comment: DocumentComment
) {
  const walker = window.document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  const nodes: Array<{ node: Text; start: number; end: number }> = []
  let node = walker.nextNode() as Text | null
  let offset = 0
  while (node) {
    const nextOffset = offset + node.data.length
    nodes.push({ node, start: offset, end: nextOffset })
    offset = nextOffset
    node = walker.nextNode() as Text | null
  }
  for (const entry of nodes) {
    if (entry.end <= start || entry.start >= end) continue
    if (entry.node.parentElement?.closest("mark[data-comment-id]")) continue
    const localStart = Math.max(0, start - entry.start)
    const localEnd = Math.min(entry.node.data.length, end - entry.start)
    const fragment = window.document.createDocumentFragment()
    if (localStart) fragment.append(entry.node.data.slice(0, localStart))
    const mark = window.document.createElement("mark")
    mark.dataset.commentId = comment.id
    mark.className = "ctx-inline-comment"
    mark.tabIndex = 0
    mark.textContent = entry.node.data.slice(localStart, localEnd)
    fragment.append(mark)
    if (localEnd < entry.node.data.length)
      fragment.append(entry.node.data.slice(localEnd))
    entry.node.replaceWith(fragment)
  }
}

const profileTabs = [
  "resume",
  "personality",
  "mbti",
  "schwartz",
  "motivations",
  "philosophy",
  "timeline",
  "article",
] as const

type ProfileTab = (typeof profileTabs)[number]

const schwartzValueOrder = [
  "self_direction",
  "stimulation",
  "hedonism",
  "achievement",
  "power",
  "security",
  "conformity",
  "tradition",
  "benevolence",
  "universalism",
]

function isProfileTab(value: string | null): value is ProfileTab {
  return profileTabs.some((tab) => tab === value)
}

function SummaryEvidenceList({ evidence }: { evidence: SummaryEvidence[] }) {
  const { t } = useI18n()
  if (!evidence.length)
    return <span className="text-sm text-muted-foreground">{t("summaryNoEvidence")}</span>

  return (
    <ul className="space-y-2 text-sm leading-6">
      {evidence.map((item, index) => (
        <li key={`${item.article_url}-${index}`}>
          <a
            href={item.article_url}
            target="_blank"
            rel="noreferrer"
            className="font-medium text-primary underline-offset-4 hover:underline focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none"
          >
            {item.article_title}
            <ExternalLinkIcon
              className="ml-1 inline size-3 align-text-top"
              aria-hidden="true"
            />
          </a>{" "}
          <span className="text-muted-foreground">{item.explanation}</span>
        </li>
      ))}
    </ul>
  )
}

function SummaryMarkdown({ content }: { content: string }) {
  return (
    <article className="mt-10 max-w-[72ch]">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={markdownComponents}
      >
        {content}
      </ReactMarkdown>
    </article>
  )
}

function ProfileSummaryHeader({
  title,
  description,
}: {
  title: string
  description: string
}) {
  return (
    <>
      <h2 className="text-3xl font-semibold tracking-tight text-balance">
        {title}
      </h2>
      <p className="mt-3 max-w-[72ch] text-sm leading-6 text-muted-foreground">
        {description}
      </p>
    </>
  )
}

function confidenceLabel(
  confidence: string,
  t: ReturnType<typeof useI18n>["t"]
) {
  const labels = {
    high: t("summaryConfidenceHigh"),
    medium: t("summaryConfidenceMedium"),
    low: t("summaryConfidenceLow"),
    undetermined: t("summaryConfidenceUndetermined"),
  }
  return labels[confidence as keyof typeof labels] ?? labels.undetermined
}

function MbtiSummary({
  analysis,
  metadataStatus,
}: {
  analysis: MbtiAnalysis
  metadataStatus: PublicDocumentDetail["metadata_status"]
}) {
  const { t } = useI18n()
  return (
    <>
      <ProfileSummaryHeader
        title={t("mbtiTitle")}
        description={t("mbtiDescription")}
      />
      {analysis.type_code ? (
        <>
          <div className="mt-8 flex flex-wrap items-center gap-3">
            <span className="text-sm font-medium text-muted-foreground">
              {t("mbtiClassification")}
            </span>
            <Badge variant="secondary" className="font-mono text-base">
              {analysis.type_code}
            </Badge>
            <span className="text-sm text-muted-foreground">
              {t("summaryConfidence")}: {confidenceLabel(analysis.confidence, t)}
            </span>
          </div>
          <div className="mt-8 overflow-x-auto rounded-md border">
            <table className="w-full min-w-[44rem] text-left text-sm">
              <thead className="bg-muted/60 text-muted-foreground">
                <tr>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("mbtiDimension")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("summaryResult")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("summaryConfidence")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("summaryEvidence")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {analysis.dimensions.map((dimension) => (
                  <tr key={dimension.axis} className="align-top">
                    <th scope="row" className="px-4 py-4 font-medium">
                      {dimension.axis}
                    </th>
                    <td className="px-4 py-4 font-mono">
                      {dimension.preference}
                    </td>
                    <td className="px-4 py-4">
                      {confidenceLabel(dimension.confidence, t)}
                    </td>
                    <td className="px-4 py-4">
                      <SummaryEvidenceList evidence={dimension.evidence} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      ) : metadataStatus ? (
        <Skeleton className="mt-8 h-72 w-full" />
      ) : (
        <p className="mt-8 text-sm leading-7 text-muted-foreground">
          {t("mbtiEmpty")}
        </p>
      )}
    </>
  )
}

function SchwartzRadar({ values }: { values: SchwartzValue[] }) {
  const { t } = useI18n()
  const valuesByKey = new Map(values.map((value) => [value.key, value]))
  const orderedValues = schwartzValueOrder.map(
    (key) => valuesByKey.get(key) ?? { key, score: 0, rank: 0, evidence: [] }
  )
  const center = 180
  const radius = 112
  const point = (index: number, score: number) => {
    const angle = (Math.PI * 2 * index) / orderedValues.length - Math.PI / 2
    const distance = (radius * score) / 100
    return [center + Math.cos(angle) * distance, center + Math.sin(angle) * distance]
  }
  const polygonPoints = (score: number) =>
    orderedValues
      .map((_, index) => point(index, score).map((value) => value.toFixed(1)).join(","))
      .join(" ")
  const dataPoints = orderedValues
    .map((value, index) =>
      point(index, value.score)
        .map((item) => item.toFixed(1))
        .join(",")
    )
    .join(" ")

  return (
    <figure className="mx-auto mt-8 max-w-xl" aria-labelledby="schwartz-radar-caption">
      <svg viewBox="0 0 360 360" className="h-auto w-full" role="img">
        <title>{t("schwartzRadarTitle")}</title>
        {[25, 50, 75, 100].map((score) => (
          <polygon
            key={score}
            points={polygonPoints(score)}
            fill="none"
            stroke="currentColor"
            strokeOpacity="0.16"
            strokeWidth="1"
          />
        ))}
        {orderedValues.map((value, index) => {
          const [x, y] = point(index, 100)
          const [labelX, labelY] = point(index, 118)
          return (
            <g key={value.key}>
              <line
                x1={center}
                y1={center}
                x2={x}
                y2={y}
                stroke="currentColor"
                strokeOpacity="0.16"
              />
              <text
                x={labelX}
                y={labelY}
                textAnchor={
                  labelX > center + 8
                    ? "start"
                    : labelX < center - 8
                      ? "end"
                      : "middle"
                }
                dominantBaseline={labelY < center - 8 ? "auto" : "hanging"}
                className="fill-muted-foreground text-[9px]"
              >
                {schwartzValueLabel(value.key, t)}
              </text>
            </g>
          )
        })}
        <polygon
          points={dataPoints}
          fill="var(--primary)"
          fillOpacity="0.16"
          stroke="var(--primary)"
          strokeWidth="2"
        />
        {orderedValues.map((value, index) => {
          const [x, y] = point(index, value.score)
          return <circle key={value.key} cx={x} cy={y} r="3" fill="var(--primary)" />
        })}
      </svg>
      <figcaption
        id="schwartz-radar-caption"
        className="mt-2 text-center text-sm text-muted-foreground"
      >
        {t("schwartzRadarCaption")}
      </figcaption>
    </figure>
  )
}

function SchwartzSummary({
  values,
  metadataStatus,
}: {
  values: SchwartzValue[]
  metadataStatus: PublicDocumentDetail["metadata_status"]
}) {
  const { t } = useI18n()
  const rankedValues = values.slice().sort((left, right) => left.rank - right.rank)
  return (
    <>
      <ProfileSummaryHeader
        title={t("schwartzTitle")}
        description={t("schwartzDescription")}
      />
      {values.length ? (
        <>
          <SchwartzRadar values={values} />
          <div className="mt-10 overflow-x-auto rounded-md border">
            <table className="w-full min-w-[46rem] text-left text-sm">
              <thead className="bg-muted/60 text-muted-foreground">
                <tr>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("schwartzRank")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("schwartzValue")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("schwartzScore")}
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    {t("summaryEvidence")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {rankedValues.map((value) => (
                  <tr key={value.key} className="align-top">
                    <td className="px-4 py-4 font-mono text-muted-foreground">
                      {value.rank}
                    </td>
                    <th scope="row" className="px-4 py-4 font-medium">
                      {schwartzValueLabel(value.key, t)}
                    </th>
                    <td className="px-4 py-4 font-mono">{value.score}</td>
                    <td className="px-4 py-4">
                      <SummaryEvidenceList evidence={value.evidence} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      ) : metadataStatus ? (
        <Skeleton className="mt-8 h-96 w-full" />
      ) : (
        <p className="mt-8 text-sm leading-7 text-muted-foreground">
          {t("schwartzEmpty")}
        </p>
      )}
    </>
  )
}

function DailyTimeline({
  entries,
  metadataStatus,
}: {
  entries: DailyTimelineEntry[]
  metadataStatus: PublicDocumentDetail["metadata_status"]
}) {
  const { t } = useI18n()
  const timeline = entries.slice().sort((left, right) => left.date.localeCompare(right.date))
  return (
    <>
      <ProfileSummaryHeader
        title={t("timelineTitle")}
        description={t("timelineDescription")}
      />
      {timeline.length ? (
        <ol className="mt-10 divide-y border-t">
          {timeline.map((entry) => (
            <li
              key={entry.date}
              className="grid gap-3 py-6 sm:grid-cols-[9rem_minmax(0,1fr)] sm:gap-8"
            >
              <time
                dateTime={entry.date}
                className="font-mono text-sm font-medium text-muted-foreground"
              >
                {entry.date}
              </time>
              <div>
                <p className="text-base leading-7 text-foreground/90">
                  {entry.summary}
                </p>
                <div className="mt-3">
                  <SummaryEvidenceList evidence={entry.evidence} />
                </div>
              </div>
            </li>
          ))}
        </ol>
      ) : metadataStatus ? (
        <Skeleton className="mt-8 h-72 w-full" />
      ) : (
        <p className="mt-8 text-sm leading-7 text-muted-foreground">
          {t("timelineEmpty")}
        </p>
      )}
    </>
  )
}

function schwartzValueLabel(
  key: string,
  t: ReturnType<typeof useI18n>["t"]
) {
  const labels = {
    self_direction: t("schwartzValueSelfDirection"),
    stimulation: t("schwartzValueStimulation"),
    hedonism: t("schwartzValueHedonism"),
    achievement: t("schwartzValueAchievement"),
    power: t("schwartzValuePower"),
    security: t("schwartzValueSecurity"),
    conformity: t("schwartzValueConformity"),
    tradition: t("schwartzValueTradition"),
    benevolence: t("schwartzValueBenevolence"),
    universalism: t("schwartzValueUniversalism"),
  }
  return labels[key as keyof typeof labels] ?? key
}

function PublicUserPage() {
  const { ownerId = "" } = useParams()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [searchParams, setSearchParams] = useSearchParams()
  const { locale, t } = useI18n()
  const requestedTab = searchParams.get("tab")
  const activeTab: ProfileTab = isProfileTab(requestedTab)
    ? requestedTab
    : "resume"
  const activeTabTitles: Record<ProfileTab, string> = {
    resume: t("experienceSummary"),
    personality: t("personalityAnalysis"),
    mbti: t("mbtiTitle"),
    schwartz: t("schwartzTitle"),
    motivations: t("motivationsTitle"),
    philosophy: t("philosophyTitle"),
    timeline: t("timelineTitle"),
    article: t("profileArticle"),
  }
  useBrowserTitle(`${activeTabTitles[activeTab]} — ${t("pageTitlePersonalPage")}`)
  const { isReady, isAuthenticated, session } = useAuthMini()
  const profile = useQuery({
    queryKey: ["public-user-profile", ownerId, locale],
    queryFn: () =>
      request<PublicUserProfile>(
        `/api/public/users/${encodeURIComponent(ownerId)}/profile?language=${encodeURIComponent(locale)}`
      ),
    enabled: Boolean(ownerId),
    refetchInterval: (query) => {
      const document = query.state.data?.profile
      return document?.is_translation_fallback || document?.metadata_status
        ? 2_000
        : false
    },
  })
  const me = useQuery({
    queryKey: ["me", session?.accessToken],
    queryFn: () => request<Me>("/api/v1/me", session?.accessToken ?? undefined),
    enabled: isReady && isAuthenticated && Boolean(session?.accessToken),
  })
  const profileDocument = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(
        "/api/v1/profile-document",
        session?.accessToken ?? undefined,
        {
          method: "POST",
        }
      ),
    onSuccess: (detail) => navigate(`/documents/${detail.document.id}`),
    onError: showError,
  })
  const profileSummaries = useMutation({
    mutationFn: () =>
      request<ProfileSummaryTaskResponse>(
        `/api/v1/documents/${profile.data?.profile?.id ?? ""}/profile-summaries`,
        session?.accessToken ?? undefined,
        {
          method: "POST",
          body: JSON.stringify({ task: "all" }),
        }
      ),
    onSuccess: () => {
      toast.success(t("profileSummariesQueued"))
      void queryClient.invalidateQueries({
        queryKey: ["public-user-profile", ownerId, locale],
      })
    },
    onError: showError,
  })
  useEffect(() => {
    if (searchParams.get("tab") === activeTab) return
    const nextSearchParams = new URLSearchParams(searchParams)
    nextSearchParams.set("tab", activeTab)
    setSearchParams(nextSearchParams, { replace: true })
  }, [activeTab, searchParams, setSearchParams])

  if (profile.isPending) return <LoadingPage />
  if (profile.error || !profile.data)
    return (
      <PageError
        error={profile.error ?? new Error(t("profileNotFound"))}
        action={
          <Button variant="outline" onClick={() => navigate("/square")}>
            <ArrowLeftIcon data-icon="inline-start" />
            {t("backToSquare")}
          </Button>
        }
      />
    )

  const document = profile.data.profile
  const isOwner = me.data?.user_id === profile.data.owner_id
  return (
    <main className="mx-auto w-full max-w-6xl p-6 md:py-14">
      <Button variant="ghost" size="sm" onClick={() => navigate("/square")}>
        <ArrowLeftIcon data-icon="inline-start" />
        {t("backToSquare")}
      </Button>
      <header className="mt-10 flex flex-wrap items-start justify-between gap-5 border-b pb-10">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">
            {t("personalPage")}
          </h1>
          <div className="mt-3">
            <LinkitUserInfo userId={profile.data.owner_id} />
          </div>
        </div>
        {isOwner ? (
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              disabled={profileDocument.isPending}
              onClick={() => profileDocument.mutate()}
            >
              {profileDocument.isPending ? (
                <LoaderCircleIcon
                  className="animate-spin"
                  data-icon="inline-start"
                />
              ) : (
                <PencilIcon data-icon="inline-start" />
              )}
              {t("editPersonalPage")}
            </Button>
            {document ? (
              <Button
                variant="ghost"
                disabled={profileSummaries.isPending}
                onClick={() => profileSummaries.mutate()}
              >
                {profileSummaries.isPending ? (
                  <LoaderCircleIcon
                    className="animate-spin"
                    data-icon="inline-start"
                  />
                ) : (
                  <RefreshCwIcon data-icon="inline-start" />
                )}
                {t("refreshProfileSummaries")}
              </Button>
            ) : null}
          </div>
        ) : null}
      </header>
      <section className="py-10" aria-labelledby="profile-activity-title">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div>
            <h2
              id="profile-activity-title"
              className="text-xl font-semibold tracking-tight"
            >
              {t("profileActivityTitle")}
            </h2>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">
              {t("profileActivityDescription")}
            </p>
          </div>
          <span className="text-sm text-muted-foreground">
            {t("profilePublishedCount").replace(
              "{count}",
              String(profile.data.published_article_dates.length)
            )}
          </span>
        </div>
        <PublicationHeatmap dates={profile.data.published_article_dates} />
      </section>
      {document ? (
        <Tabs
          value={activeTab}
          onValueChange={(tab) => {
            const nextSearchParams = new URLSearchParams(searchParams)
            nextSearchParams.set("tab", tab)
            setSearchParams(nextSearchParams)
          }}
          className="border-t pt-10"
        >
          {document.is_translation_fallback ? (
            <Alert>
              <LanguagesIcon data-icon="inline-start" />
              <AlertTitle>{t("translationInProgressTitle")}</AlertTitle>
              <AlertDescription>{t("translationInProgress")}</AlertDescription>
            </Alert>
          ) : null}
          <TabsList
            aria-label={t("personalPage")}
            className="mt-6 h-auto w-full max-w-full justify-start overflow-x-auto p-1"
          >
            <TabsTrigger value="resume" className="shrink-0 px-3">
              {t("profileTabResume")}
            </TabsTrigger>
            <TabsTrigger value="personality" className="shrink-0 px-3">
              {t("profileTabPersonality")}
            </TabsTrigger>
            <TabsTrigger value="mbti" className="shrink-0 px-3">
              {t("profileTabMbti")}
            </TabsTrigger>
            <TabsTrigger value="schwartz" className="shrink-0 px-3">
              {t("profileTabSchwartz")}
            </TabsTrigger>
            <TabsTrigger value="motivations" className="shrink-0 px-3">
              {t("profileTabMotivations")}
            </TabsTrigger>
            <TabsTrigger value="philosophy" className="shrink-0 px-3">
              {t("profileTabPhilosophy")}
            </TabsTrigger>
            <TabsTrigger value="timeline" className="shrink-0 px-3">
              {t("profileTabTimeline")}
            </TabsTrigger>
            <TabsTrigger value="article" className="shrink-0 px-3">
              {t("profileTabArticle")}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="resume" className="mt-8 max-w-[72ch]">
            <h2 className="text-3xl font-semibold tracking-tight text-balance">
              {t("experienceSummary")}
            </h2>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              {t("experienceSummaryDescription")}
            </p>
            {document.metadata.experience_summary ? (
              <article className="mt-10">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm, remarkMath]}
                  rehypePlugins={[rehypeKatex]}
                  components={markdownComponents}
                >
                  {document.metadata.experience_summary}
                </ReactMarkdown>
              </article>
            ) : document.metadata_status ? (
              <Skeleton className="mt-8 h-48 w-full" />
            ) : (
              <p className="mt-8 text-sm leading-7 text-muted-foreground">
                {t("experienceSummaryEmpty")}
              </p>
            )}
          </TabsContent>
          <TabsContent value="personality" className="mt-8 max-w-[72ch]">
            <h2
              id="personality-analysis-title"
              className="text-3xl font-semibold tracking-tight text-balance"
            >
              {t("personalityAnalysis")}
            </h2>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              {t("personalityAnalysisDescription")}
            </p>
            {document.metadata.personality_analysis ? (
              <article className="mt-10">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm, remarkMath]}
                  rehypePlugins={[rehypeKatex]}
                  components={markdownComponents}
                >
                  {document.metadata.personality_analysis}
                </ReactMarkdown>
              </article>
            ) : document.metadata_status ? (
              <Skeleton className="mt-8 h-64 w-full" />
            ) : (
              <p className="mt-8 text-sm leading-7 text-muted-foreground">
                {t("personalityAnalysisEmpty")}
              </p>
            )}
          </TabsContent>
          <TabsContent value="mbti" className="mt-8 max-w-5xl">
            <MbtiSummary
              analysis={document.metadata.mbti_analysis}
              metadataStatus={document.metadata_status}
            />
          </TabsContent>
          <TabsContent value="schwartz" className="mt-8 max-w-5xl">
            <SchwartzSummary
              values={document.metadata.schwartz_values}
              metadataStatus={document.metadata_status}
            />
          </TabsContent>
          <TabsContent value="motivations" className="mt-8 max-w-[72ch]">
            <ProfileSummaryHeader
              title={t("motivationsTitle")}
              description={t("motivationsDescription")}
            />
            {document.metadata.unconscious_motivations ? (
              <SummaryMarkdown content={document.metadata.unconscious_motivations} />
            ) : document.metadata_status ? (
              <Skeleton className="mt-8 h-64 w-full" />
            ) : (
              <p className="mt-8 text-sm leading-7 text-muted-foreground">
                {t("motivationsEmpty")}
              </p>
            )}
          </TabsContent>
          <TabsContent value="philosophy" className="mt-8 max-w-[72ch]">
            <ProfileSummaryHeader
              title={t("philosophyTitle")}
              description={t("philosophyDescription")}
            />
            {document.metadata.philosophical_references ? (
              <SummaryMarkdown content={document.metadata.philosophical_references} />
            ) : document.metadata_status ? (
              <Skeleton className="mt-8 h-64 w-full" />
            ) : (
              <p className="mt-8 text-sm leading-7 text-muted-foreground">
                {t("philosophyEmpty")}
              </p>
            )}
          </TabsContent>
          <TabsContent value="timeline" className="mt-8 max-w-5xl">
            <DailyTimeline
              entries={document.metadata.daily_timeline}
              metadataStatus={document.metadata_status}
            />
          </TabsContent>
          <TabsContent value="article" className="mt-8 max-w-[72ch]">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium text-muted-foreground">
                  {t("profileArticle")}
                </p>
                <h2
                  id="profile-article-title"
                  className="mt-2 text-3xl font-semibold tracking-tight text-balance"
                >
                  {document.title}
                </h2>
              </div>
              <Badge variant="outline">{document.language}</Badge>
            </div>
            <p className="mt-3 text-sm text-muted-foreground">
              {t("publishedOn").replace(
                "{date}",
                formatDate(document.published_at, locale)
              )}
            </p>
            <article className="mt-10 max-w-[72ch]">
              <ReactMarkdown
                remarkPlugins={[remarkGfm, remarkMath]}
                rehypePlugins={[rehypeKatex]}
                components={markdownComponents}
              >
                {document.content}
              </ReactMarkdown>
            </article>
          </TabsContent>
        </Tabs>
      ) : (
        <Empty className="min-h-72 border-t">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <UserRoundIcon />
            </EmptyMedia>
            <EmptyTitle>{t("profileArticleUnavailableTitle")}</EmptyTitle>
            <EmptyDescription>
              {t("profileArticleUnavailableDescription")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </main>
  )
}

function PublicationHeatmap({ dates }: { dates: number[] }) {
  const { locale, t } = useI18n()
  const today = startOfDay(new Date())
  const firstDay = new Date(today)
  firstDay.setDate(today.getDate() - 363)
  const counts = new Map<string, number>()
  for (const timestamp of dates) {
    const key = calendarDayKey(new Date(timestamp * 1_000))
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  const days = Array.from({ length: 364 }, (_, index) => {
    const day = new Date(firstDay)
    day.setDate(firstDay.getDate() + index)
    return day
  })

  return (
    <div className="mt-5 overflow-x-auto pb-1">
      <div
        className="grid w-max grid-flow-col grid-rows-7 gap-1"
        role="img"
        aria-label={t("profileActivityTitle")}
      >
        {days.map((day) => {
          const count = counts.get(calendarDayKey(day)) ?? 0
          const label = t("profileActivityDay")
            .replace("{date}", formatDate(day.getTime() / 1_000, locale))
            .replace("{count}", String(count))
          return (
            <span
              key={calendarDayKey(day)}
              title={label}
              aria-label={label}
              className={publicationActivityClass(count)}
            />
          )
        })}
      </div>
      <div className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
        <span>{t("profileActivityLess")}</span>
        {[0, 1, 2, 3].map((level) => (
          <span key={level} className={publicationActivityClass(level)} />
        ))}
        <span>{t("profileActivityMore")}</span>
      </div>
    </div>
  )
}

function startOfDay(value: Date): Date {
  const date = new Date(value)
  date.setHours(0, 0, 0, 0)
  return date
}

function calendarDayKey(value: Date): string {
  const year = value.getFullYear()
  const month = String(value.getMonth() + 1).padStart(2, "0")
  const day = String(value.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

function publicationActivityClass(count: number): string {
  if (count === 0) return "size-3 rounded-sm bg-muted"
  if (count === 1) return "size-3 rounded-sm bg-primary/25"
  if (count === 2) return "size-3 rounded-sm bg-primary/55"
  return "size-3 rounded-sm bg-primary"
}

function SetupPage({ token, onDone }: { token: string; onDone: () => void }) {
  const { t } = useI18n()
  const setup = useMutation({
    mutationFn: () => request<Me>("/api/v1/setup", token, { method: "POST" }),
    onSuccess: () => {
      toast.success(t("rootConfigured"))
      onDone()
    },
    onError: showError,
  })
  return (
    <main className="grid min-h-svh place-items-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>{t("initializeTitle")}</CardTitle>
          <CardDescription>{t("initializeDescription")}</CardDescription>
        </CardHeader>
        <CardContent>
          <Button disabled={setup.isPending} onClick={() => setup.mutate()}>
            {setup.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <ShieldCheckIcon data-icon="inline-start" />
            )}
            {t("becomeRoot")}
          </Button>
        </CardContent>
      </Card>
    </main>
  )
}

function SystemResourcesPage({ token }: { token: string }) {
  const { locale, t } = useI18n()
  const resources = useQuery({
    queryKey: ["admin-system-resources", token],
    queryFn: () =>
      request<SystemResources>("/api/v1/admin/system-resources", token),
    refetchInterval: 5_000,
  })

  if (resources.isPending) return <SystemResourcesSkeleton />
  if (resources.error || !resources.data)
    return (
      <PageError error={resources.error ?? new Error(t("requestFailed"))} />
    )

  const data = resources.data
  const metrics: Array<{
    icon: LucideIcon
    label: string
    value: string
    detail: string
    secondaryDetail?: string
    percentage?: number
  }> = [
    {
      icon: CpuIcon,
      label: t("systemResourceCpu"),
      value: formatPercentage(data.cpu.usage_percent, locale),
      detail: `${t("systemResourceLoad1m")}: ${data.cpu.load_1m.toFixed(2)} · ${t("systemResourceLogicalCpus")}: ${data.cpu.logical_cpus}`,
      percentage: data.cpu.usage_percent,
    },
    {
      icon: MemoryStickIcon,
      label: t("systemResourceMemory"),
      value: `${formatBytes(data.memory.used_bytes, locale)} / ${formatBytes(data.memory.total_bytes, locale)}`,
      detail: `${t("systemResourceProcessMemory")}: ${formatBytes(data.memory.process_used_bytes, locale)} · ${t("systemResourceOtherMemory")}: ${formatBytes(data.memory.other_used_bytes, locale)}`,
      secondaryDetail: `${t("systemResourceAvailableMemory")}: ${formatBytes(data.memory.available_bytes, locale)} · ${t("systemResourceSwap")}: ${formatBytes(data.memory.swap_used_bytes, locale)} / ${formatBytes(data.memory.swap_total_bytes, locale)}`,
      percentage: data.memory.usage_percent,
    },
    {
      icon: NetworkIcon,
      label: t("systemResourceNetwork"),
      value: `${t("systemResourceReceived")}: ${formatRate(data.network.receive_bytes_per_second, locale)} · ${t("systemResourceTransmitted")}: ${formatRate(data.network.transmit_bytes_per_second, locale)}`,
      detail: `${t("systemResourceNetworkInterfaces")}: ${data.network.interfaces}`,
    },
    {
      icon: HardDriveIcon,
      label: t("systemResourceDisk"),
      value: data.disk
        ? `${formatBytes(data.disk.used_bytes, locale)} / ${formatBytes(data.disk.total_bytes, locale)}`
        : "—",
      detail: data.disk
        ? `${t("systemResourceAvailableDisk")}: ${formatBytes(data.disk.available_bytes, locale)} · ${t("systemResourceMountPoint")}: ${data.disk.mount_point}`
        : t("systemResourceUnavailable"),
      percentage: data.disk?.usage_percent,
    },
    {
      icon: DatabaseIcon,
      label: t("systemResourceSqlite"),
      value: formatBytes(data.sqlite.total_bytes, locale),
      detail: `${t("systemResourceMainFile")}: ${formatBytes(data.sqlite.main_bytes, locale)} · ${t("systemResourceWalFile")}: ${formatBytes(data.sqlite.wal_bytes, locale)} · ${t("systemResourceShmFile")}: ${formatBytes(data.sqlite.shm_bytes, locale)}`,
      secondaryDetail: `${t("systemResourceReclaimable")}: ${formatBytes(data.sqlite.freelist_bytes, locale)} · ${formatPercentage(data.sqlite.freelist_percent, locale)}`,
    },
  ]

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-8 p-4 md:p-6">
      <section className="max-w-2xl">
        <h1 className="text-2xl font-semibold tracking-tight text-balance">
          {t("pageTitleSystemResources")}
        </h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("systemResourcesDescription")}
        </p>
        <p className="mt-3 text-xs text-muted-foreground">
          {t("systemResourcesRefreshes").replace("{seconds}", "5")} ·{" "}
          {t("systemResourcesSampledAt").replace(
            "{date}",
            formatDateTime(data.sampled_at, locale)
          )}
        </p>
      </section>
      <section
        aria-label={t("pageTitleSystemResources")}
        className="grid gap-3 md:grid-cols-2 xl:grid-cols-3"
      >
        {metrics.map((metric) => (
          <Card key={metric.label} className="shadow-none">
            <CardHeader className="pb-3">
              <CardTitle className="flex items-center gap-2 text-sm font-medium">
                <metric.icon className="size-4 text-muted-foreground" />
                {metric.label}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-2xl font-semibold tracking-tight tabular-nums">
                {metric.value}
              </p>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">
                {metric.detail}
              </p>
              {metric.secondaryDetail ? (
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  {metric.secondaryDetail}
                </p>
              ) : null}
              {metric.percentage === undefined ? null : (
                <div
                  className="mt-4 h-1.5 overflow-hidden rounded-full bg-muted"
                  role="progressbar"
                  aria-label={metric.label}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(metric.percentage)}
                >
                  <div
                    className="h-full rounded-full bg-primary transition-[width] duration-200"
                    style={{
                      width: `${Math.min(Math.max(metric.percentage, 0), 100)}%`,
                    }}
                  />
                </div>
              )}
            </CardContent>
          </Card>
        ))}
      </section>
    </main>
  )
}

function SystemResourcesSkeleton() {
  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-8 p-4 md:p-6">
      <section className="max-w-2xl space-y-3">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-5 w-full" />
      </section>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {Array.from({ length: 5 }, (_, index) => (
          <Skeleton key={index} className="h-52 rounded-lg" />
        ))}
      </div>
    </main>
  )
}

function AdministrationPage({ token }: { token: string }) {
  const { t } = useI18n()
  const aiConfiguration = useQuery({
    queryKey: ["admin-ai", token],
    queryFn: () => request<AiConfiguration>("/api/v1/admin/ai", token),
  })
  const [baseUrl, setBaseUrl] = useState<string>()
  const [model, setModel] = useState<string>()
  const [apiKey, setApiKey] = useState("")
  const update = useMutation({
    mutationFn: () =>
      request<AiConfiguration>("/api/v1/admin/ai", token, {
        method: "PUT",
        body: JSON.stringify({
          base_url: baseUrl ?? aiConfiguration.data?.base_url ?? "",
          model: model ?? aiConfiguration.data?.model ?? "",
          api_key: apiKey || undefined,
        }),
      }),
    onSuccess: () => toast.success(t("aiConfigurationSaved")),
    onError: showError,
  })

  const configuration = aiConfiguration.data

  if (aiConfiguration.isPending) return <PageSkeleton />
  if (aiConfiguration.error) return <PageError error={aiConfiguration.error} />

  return (
    <main className="mx-auto flex w-full max-w-3xl flex-col gap-8 p-4 md:p-6">
      <section className="max-w-2xl">
        <h1 className="text-2xl font-semibold tracking-tight text-balance">
          {t("pageTitleAdministration")}
        </h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("administrationDescription")}
        </p>
      </section>
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-3">
            <div>
              <CardTitle>{t("openAiRouting")}</CardTitle>
              <CardDescription className="mt-1">
                {t("openAiRoutingDescription")}
              </CardDescription>
            </div>
            <Badge
              variant={configuration?.configured ? "secondary" : "outline"}
            >
              {configuration?.configured ? t("configured") : t("needsKey")}
            </Badge>
          </div>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="ai-base-url">{t("baseUrl")}</FieldLabel>
              <Input
                id="ai-base-url"
                value={baseUrl ?? configuration?.base_url ?? ""}
                onChange={(event) => setBaseUrl(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-model">{t("model")}</FieldLabel>
              <Input
                id="ai-model"
                value={model ?? configuration?.model ?? ""}
                onChange={(event) => setModel(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-api-key">{t("apiKey")}</FieldLabel>
              <Input
                id="ai-api-key"
                type="password"
                value={apiKey}
                placeholder={t("apiKeyPlaceholder")}
                onChange={(event) => setApiKey(event.target.value)}
              />
              <FieldDescription>{t("apiKeyDescription")}</FieldDescription>
            </Field>
            <Field>
              <Button
                disabled={
                  update.isPending ||
                  !(baseUrl ?? configuration?.base_url) ||
                  !(model ?? configuration?.model)
                }
                onClick={() => update.mutate()}
              >
                {update.isPending ? (
                  <LoaderCircleIcon
                    className="animate-spin"
                    data-icon="inline-start"
                  />
                ) : (
                  <SaveIcon data-icon="inline-start" />
                )}
                {t("saveAiConfiguration")}
              </Button>
            </Field>
          </FieldGroup>
        </CardContent>
      </Card>
    </main>
  )
}

function AiRequestAuditPage({ token }: { token: string }) {
  const { locale, t } = useI18n()
  const requests = useQuery({
    queryKey: ["admin-ai-requests", token],
    queryFn: () => request<AiRequest[]>("/api/v1/admin/ai/requests", token),
  })

  if (requests.isPending) return <PageSkeleton />
  if (requests.error) return <PageError error={requests.error} />
  const items = requests.data ?? []

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-8 p-4 md:p-6">
      <section className="max-w-2xl">
        <h1 className="text-2xl font-semibold tracking-tight text-balance">
          {t("aiRequestsTitle")}
        </h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("aiRequestsDescription")}
        </p>
      </section>
      {items.length === 0 ? (
        <Empty className="min-h-72 border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <SparklesIcon />
            </EmptyMedia>
            <EmptyTitle>{t("aiRequestsEmpty")}</EmptyTitle>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <table className="w-full min-w-[920px] text-left text-sm">
            <thead className="bg-muted/50 text-xs text-muted-foreground">
              <tr>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestDocument")}
                </th>
                <th className="px-4 py-3 font-medium">{t("aiRequestTask")}</th>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestTargetLanguage")}
                </th>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestStatus")}
                </th>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestCreated")}
                </th>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestCompleted")}
                </th>
                <th className="px-4 py-3 font-medium">
                  {t("aiRequestResult")}
                </th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.id} className="border-t align-top">
                  <td className="max-w-64 px-4 py-3">
                    <p
                      className="truncate font-medium"
                      title={item.document_title}
                    >
                      {item.document_title}
                    </p>
                    <p className="mt-1 font-mono text-xs text-muted-foreground">
                      {item.source_revision_id.slice(0, 8)}
                    </p>
                  </td>
                  <td className="px-4 py-3">{aiTaskLabel(item, t)}</td>
                  <td className="px-4 py-3 font-mono text-xs">
                    {item.target_language || "—"}
                  </td>
                  <td className="px-4 py-3">
                    <Badge
                      variant={
                        item.status === "failed" ? "destructive" : "outline"
                      }
                    >
                      {aiStatusLabel(item, t)}
                    </Badge>
                  </td>
                  <td className="px-4 py-3 whitespace-nowrap text-muted-foreground">
                    {formatDate(item.created_at, locale)}
                  </td>
                  <td className="px-4 py-3 whitespace-nowrap text-muted-foreground">
                    {item.completed_at
                      ? formatDate(item.completed_at, locale)
                      : "—"}
                  </td>
                  <td className="max-w-96 px-4 py-3 text-muted-foreground">
                    <p
                      className="line-clamp-2"
                      title={item.error ?? item.result_summary ?? ""}
                    >
                      {item.error ?? item.result_summary ?? "—"}
                    </p>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </main>
  )
}

function MermaidDiagram({ chart }: { chart: string }) {
  const id = useId().replaceAll(":", "")
  const [svg, setSvg] = useState<string>()
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let cancelled = false
    void import("mermaid")
      .then(({ default: mermaid }) => {
        if (cancelled) return null
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: document.documentElement.classList.contains("dark")
            ? "dark"
            : "default",
        })
        return mermaid.render(`ctx-mermaid-${id}`, chart)
      })
      .then((result) => {
        if (!cancelled && result) setSvg(result.svg)
      })
      .catch(() => {
        if (!cancelled) setFailed(true)
      })
    return () => {
      cancelled = true
    }
  }, [chart, id])

  if (failed)
    return (
      <pre className="mt-5 overflow-x-auto rounded-md bg-muted p-4 text-sm leading-6">
        <code className="font-mono">{chart}</code>
      </pre>
    )
  if (!svg)
    return (
      <div
        aria-busy="true"
        aria-label="Loading Mermaid diagram"
        className="mt-5"
      >
        <Skeleton className="h-52 w-full" />
      </div>
    )
  return (
    <div
      role="img"
      aria-label="Mermaid diagram"
      className="mt-5 overflow-x-auto rounded-md border bg-muted/30 p-4 [&_svg]:mx-auto [&_svg]:min-w-max"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  )
}

function MarkdownPre({ children }: { children?: ReactNode }) {
  const code = Children.toArray(children).find((child) =>
    isValidElement<{ className?: string; children?: ReactNode }>(child)
  )
  if (
    isValidElement<{ className?: string; children?: ReactNode }>(code) &&
    code.props.className?.includes("language-mermaid")
  )
    return (
      <MermaidDiagram
        chart={markdownText(code.props.children).replace(/\n$/, "")}
      />
    )

  return (
    <pre className="mt-5 overflow-x-auto rounded-md bg-muted p-4 text-sm leading-6">
      {children}
    </pre>
  )
}

function markdownText(children: ReactNode): string {
  return Children.toArray(children)
    .map((child) => {
      if (typeof child === "string" || typeof child === "number")
        return String(child)
      if (isValidElement<{ children?: ReactNode }>(child))
        return markdownText(child.props.children)
      return ""
    })
    .join("")
}

const markdownComponents = {
  h1: ({ children }: { children?: ReactNode }) => (
    <h1 className="mt-10 text-3xl font-semibold tracking-tight text-balance first:mt-0">
      {children}
    </h1>
  ),
  h2: ({ children }: { children?: ReactNode }) => (
    <h2 className="mt-10 text-xl font-semibold tracking-tight text-balance">
      {children}
    </h2>
  ),
  h3: ({ children }: { children?: ReactNode }) => (
    <h3 className="mt-7 text-base font-semibold">{children}</h3>
  ),
  p: ({ children }: { children?: ReactNode }) => (
    <p className="mt-5 text-base leading-7 text-foreground/90">{children}</p>
  ),
  blockquote: ({ children }: { children?: ReactNode }) => (
    <blockquote className="mt-5 border-l-2 border-muted-foreground/35 pl-4 text-base leading-7 text-muted-foreground">
      {children}
    </blockquote>
  ),
  ul: ({
    children,
    className,
  }: {
    children?: ReactNode
    className?: string
  }) => (
    <ul
      className={
        className?.includes("contains-task-list")
          ? "mt-5 list-none pl-0 text-base leading-7"
          : "mt-5 list-disc pl-6 text-base leading-7"
      }
    >
      {children}
    </ul>
  ),
  ol: ({ children }: { children?: ReactNode }) => (
    <ol className="mt-5 list-decimal pl-6 text-base leading-7">{children}</ol>
  ),
  li: ({
    children,
    className,
  }: {
    children?: ReactNode
    className?: string
  }) => (
    <li
      className={
        className?.includes("task-list-item") ? "mt-2 flex gap-2" : "mt-1"
      }
    >
      {children}
    </li>
  ),
  code: ({
    children,
    className,
  }: {
    children?: ReactNode
    className?: string
  }) => (
    <code
      className={`rounded-sm bg-muted px-1 py-0.5 font-mono text-sm ${className ?? ""}`}
    >
      {children}
    </code>
  ),
  pre: MarkdownPre,
  a: ({ children, href }: { children?: ReactNode; href?: string }) => (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-1 text-primary underline underline-offset-4"
    >
      {children}
      <ExternalLinkIcon className="size-3 shrink-0" aria-hidden="true" />
    </a>
  ),
  img: ({ alt, src }: { alt?: string; src?: string }) => (
    <img src={src} alt={alt ?? ""} className="mt-5 max-w-full rounded-md" />
  ),
  hr: () => <Separator className="my-8" />,
  del: ({ children }: { children?: ReactNode }) => (
    <del className="text-muted-foreground">{children}</del>
  ),
  table: ({ children }: { children?: ReactNode }) => (
    <div className="mt-5 overflow-x-auto">
      <table className="w-full border-collapse text-left text-sm">
        {children}
      </table>
    </div>
  ),
  th: ({ children }: { children?: ReactNode }) => (
    <th className="border-b px-3 py-2 font-medium">{children}</th>
  ),
  td: ({ children }: { children?: ReactNode }) => (
    <td className="border-b px-3 py-2 align-top">{children}</td>
  ),
}

function NavItem({
  active,
  icon: Icon,
  onClick,
  children,
}: {
  active: boolean
  icon: LucideIcon
  onClick: () => void
  children: ReactNode
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton isActive={active} onClick={onClick}>
        <Icon />
        <span>{children}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

function PublicListSkeleton() {
  return (
    <div className="flex flex-col gap-4">
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
    </div>
  )
}

function PageSkeleton() {
  return (
    <main className="p-6">
      <Skeleton className="h-8 w-56" />
      <Skeleton className="mt-6 h-56 w-full" />
    </main>
  )
}

function EditorSkeleton() {
  return (
    <main className="p-6">
      <Skeleton className="h-8 w-64" />
      <Skeleton className="mt-5 h-96 w-full" />
    </main>
  )
}

function LoadingPage() {
  return (
    <main className="grid min-h-svh place-items-center">
      <Skeleton className="h-8 w-48" />
    </main>
  )
}

function PageError({ error, action }: { error: Error; action?: ReactNode }) {
  const { t } = useI18n()
  return (
    <main className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6">
      <Alert variant="destructive">
        <AlertTitle>{t("requestFailed")}</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
      {action}
    </main>
  )
}

function showError(error: Error) {
  toast.error(error.message)
}

function aiTaskLabel(
  request: AiRequest,
  t: (key:
    | "aiTaskMetadata"
    | "aiTaskTranslate"
    | "aiTaskProfileExperience"
    | "aiTaskProfilePersonality"
    | "aiTaskProfileMbti"
    | "aiTaskProfileSchwartz"
    | "aiTaskProfileMotivations"
    | "aiTaskProfilePhilosophy"
    | "aiTaskProfileTimeline") => string
) {
  const labels = {
    metadata: "aiTaskMetadata",
    translate: "aiTaskTranslate",
    profile_experience: "aiTaskProfileExperience",
    profile_personality: "aiTaskProfilePersonality",
    profile_mbti: "aiTaskProfileMbti",
    profile_schwartz: "aiTaskProfileSchwartz",
    profile_motivations: "aiTaskProfileMotivations",
    profile_philosophy: "aiTaskProfilePhilosophy",
    profile_timeline: "aiTaskProfileTimeline",
  } as const
  return t(labels[request.task])
}

function aiStatusLabel(
  request: AiRequest,
  t: (
    key:
      | "aiStatusQueued"
      | "aiStatusRunning"
      | "aiStatusSucceeded"
      | "aiStatusFailed"
  ) => string
) {
  if (request.status === "queued") return t("aiStatusQueued")
  if (request.status === "running") return t("aiStatusRunning")
  if (request.status === "succeeded") return t("aiStatusSucceeded")
  return t("aiStatusFailed")
}

function formatDate(timestamp: number, locale: Locale) {
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(timestamp * 1000))
}

function formatDateTime(timestamp: number, locale: Locale) {
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    year: "numeric",
  }).format(new Date(timestamp * 1000))
}

function datetimeLocalValue(timestamp: number) {
  const date = new Date(timestamp * 1_000)
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}

function useBrowserTitle(title?: string) {
  useEffect(() => {
    if (!title) return
    window.document.title = `${title} · CTX`
  }, [title])
}

function formatBytes(bytes: number, locale: Locale) {
  const units = ["B", "KB", "MB", "GB", "TB"]
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value)} ${units[unit]}`
}

function formatRate(bytes: number, locale: Locale) {
  return `${formatBytes(bytes, locale)}/s`
}

function formatPercentage(value: number, locale: Locale) {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits: 1,
    style: "percent",
  }).format(value / 100)
}

function pageTitle(
  pathname: string,
  t: (
    key:
      | "pageTitleAdministration"
      | "pageTitleDocuments"
      | "pageTitleEditor"
      | "pageTitleAiRequests"
      | "pageTitleSystemResources"
  ) => string
) {
  if (pathname === "/admin/system-resources")
    return t("pageTitleSystemResources")
  if (pathname === "/admin/ai-requests") return t("pageTitleAiRequests")
  if (pathname === "/admin") return t("pageTitleAdministration")
  if (pathname.startsWith("/documents/") && pathname !== "/documents")
    return t("pageTitleEditor")
  return t("pageTitleDocuments")
}

function publicPageTitle(
  pathname: string,
  t: (key: "document" | "pageTitlePersonalPage" | "pageTitleSquare") => string
) {
  if (pathname.startsWith("/u/")) return t("pageTitlePersonalPage")
  if (pathname.startsWith("/p/")) return t("document")
  return t("pageTitleSquare")
}
