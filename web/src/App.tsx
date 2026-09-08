import {
  Children,
  isValidElement,
  useEffect,
  useId,
  useState,
  type ReactNode,
} from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAuthMini } from "auth-mini-react-components"
import { LinkitMyInfo, LinkitUserInfo } from "linkit-react-components"
import {
  ArrowLeftIcon,
  BookOpenIcon,
  ExternalLinkIcon,
  FileTextIcon,
  Globe2Icon,
  LanguagesIcon,
  LoaderCircleIcon,
  PlusIcon,
  RefreshCwIcon,
  SaveIcon,
  Settings2Icon,
  ShieldCheckIcon,
  SparklesIcon,
  type LucideIcon,
} from "lucide-react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from "react-router-dom"
import { toast } from "sonner"

import { request } from "./lib/api"
import { useI18n, type Locale } from "./lib/i18n"
import type {
  AiConfiguration,
  AiRequest,
  AiRun,
  Document,
  DocumentDetail,
  Me,
  PublishedMetadata,
  PublicDocument,
  PublicDocumentDetail,
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
      <Routes>
        <Route path="/" element={<Navigate to="/square" replace />} />
        <Route path="/square" element={<SquarePage />} />
        <Route path="/p/:documentId" element={<PublicDocumentPage />} />
        <Route path="*" element={<Navigate to="/square" replace />} />
      </Routes>
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

function PrivateApp() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  if (!isReady || !isAuthenticated || !session?.accessToken)
    return <LoadingPage />
  return <CtxShell token={session.accessToken} />
}

function CtxShell({ token }: { token: string }) {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
  const { t } = useI18n()
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
                    active={false}
                    icon={Globe2Icon}
                    onClick={() => navigate("/square")}
                  >
                    {t("navigationSquare")}
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
            <Route path="*" element={<Navigate to="/documents" replace />} />
          </Routes>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  )
}

function DocumentListPage({ token }: { token: string }) {
  const navigate = useNavigate()
  const { locale, t } = useI18n()
  const documents = useQuery({
    queryKey: ["documents", token],
    queryFn: () => request<Document[]>("/api/v1/documents", token),
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
        <Button onClick={() => navigate("/documents/new")}>
          <PlusIcon data-icon="inline-start" />
          {t("newDocument")}
        </Button>
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
                onViewPublic={() => navigate(`/p/${document.id}`)}
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
  const [title, setTitle] = useState("")
  const [sourceLanguage, setSourceLanguage] = useState("und")
  const [content, setContent] = useState("")
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
        onTitleChange={setTitle}
        onSourceLanguageChange={setSourceLanguage}
        onContentChange={setContent}
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

  return (
    <ExistingDocumentEditor
      key={document.data.revision.id}
      token={token}
      detail={document.data}
    />
  )
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
  const [title, setTitle] = useState(detail.document.title)
  const [sourceLanguage, setSourceLanguage] = useState(
    detail.document.source_language
  )
  const [content, setContent] = useState(detail.revision.content)
  const isDirty =
    title !== detail.document.title ||
    sourceLanguage !== detail.document.source_language ||
    content !== detail.revision.content
  const saveCurrentRevision = () =>
    request<DocumentDetail>(`/api/v1/documents/${detail.document.id}`, token, {
      method: "PUT",
      body: JSON.stringify({
        title: title.trim(),
        source_language: sourceLanguage.trim(),
        content,
      }),
    })
  const save = useMutation({
    mutationFn: saveCurrentRevision,
    onSuccess: () => {
      toast.success(t("savedNewRevision"))
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
    },
    onError: (error) => {
      void queryClient.invalidateQueries({ queryKey: ["documents", token] })
      void queryClient.invalidateQueries({
        queryKey: ["document", token, detail.document.id],
      })
      showError(error)
    },
  })
  const publish = useMutation({
    mutationFn: async () => {
      if (isDirty) await saveCurrentRevision()
      return request<Document>(
        `/api/v1/documents/${detail.document.id}/publish`,
        token,
        {
          method: "POST",
        }
      )
    },
    onSuccess: () => {
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
  const isPublished = detail.document.status === "published"
  const isWriting = save.isPending || publish.isPending

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
              <Button
                variant="outline"
                disabled={isWriting || !title.trim()}
                onClick={() => save.mutate()}
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
                onClick={() => publish.mutate()}
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
            </div>
          }
        />
        <div className="mt-4 flex items-center gap-2 text-xs text-muted-foreground">
          <span>{t("revision")}</span>
          <span className="font-mono">{detail.revision.id.slice(0, 8)}</span>
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
          disabled={isWriting}
          onTitleChange={setTitle}
          onSourceLanguageChange={setSourceLanguage}
          onContentChange={setContent}
        />
      </section>
      <aside className="min-w-0 xl:pt-14">
        <EditorialMetadata
          metadata={publishedMetadata(detail.document.metadata)}
          pending={isPublished && !hasEditorialMetadata(detail.document.metadata)}
          variant="private"
        />
        <AiPanel
          token={token}
          detail={detail}
          title={title}
          sourceLanguage={sourceLanguage}
          content={content}
          onSourceLanguageChange={setSourceLanguage}
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
  onTitleChange,
  onSourceLanguageChange,
  onContentChange,
}: {
  title: string
  sourceLanguage: string
  content: string
  disabled?: boolean
  onTitleChange: (value: string) => void
  onSourceLanguageChange: (value: string) => void
  onContentChange: (value: string) => void
}) {
  const { t } = useI18n()
  return (
    <FieldGroup>
      <Field>
        <FieldLabel htmlFor="document-title">{t("title")}</FieldLabel>
        <Input
          id="document-title"
          value={title}
          placeholder={t("titlePlaceholder")}
          disabled={disabled}
          onChange={(event) => onTitleChange(event.target.value)}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="document-source-language">
          {t("sourceLanguage")}
        </FieldLabel>
        <Input
          id="document-source-language"
          value={sourceLanguage}
          placeholder={t("sourceLanguagePlaceholder")}
          disabled={disabled}
          onChange={(event) => onSourceLanguageChange(event.target.value)}
        />
        <FieldDescription>{t("sourceLanguageDescription")}</FieldDescription>
      </Field>
      <Field>
        <FieldLabel htmlFor="document-markdown">{t("markdown")}</FieldLabel>
        <Textarea
          id="document-markdown"
          className="min-h-[28rem] font-mono text-sm leading-6"
          value={content}
          disabled={disabled}
          onChange={(event) => onContentChange(event.target.value)}
        />
        <FieldDescription>{t("markdownDescription")}</FieldDescription>
      </Field>
    </FieldGroup>
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
  const needsSaveBeforeAi = isDirty || isSaving || isPublishing
  const isAiActionPending = ai.isPending || applyMetadata.isPending
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
  return {
    description: strings("description"),
    summary: strings("summary"),
    short_summary: strings("short_summary"),
    tags: textList("tags"),
    inferred_date: strings("inferred_date"),
    inferred_lang: strings("inferred_lang"),
    key_points: textList("key_points"),
    audience: strings("audience"),
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
      metadata.audience
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
        <LoaderCircleIcon className="size-3.5 animate-spin" aria-hidden="true" />
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
          <div className="flex flex-wrap gap-1.5" aria-label={t("metadataTags")}>
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
        <div className="mt-4 flex flex-wrap gap-1.5" aria-label={t("metadataTags")}>
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
          <span className="font-medium text-foreground">{t("metadataAudience")}: </span>
          {metadata.audience}
        </p>
      ) : null}
    </section>
  )
}

function SquarePage() {
  const navigate = useNavigate()
  const { locale, t } = useI18n()
  const documents = useQuery({
    queryKey: ["public-documents", locale],
    queryFn: () =>
      request<PublicDocument[]>(
        `/api/public/documents?language=${encodeURIComponent(locale)}`
      ),
  })

  return (
    <main className="min-h-svh">
      <PublicHeader onStartWriting={() => navigate("/documents")} />
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-10 p-6 md:py-14">
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
      </div>
    </main>
  )
}

function PublicHeader({ onStartWriting }: { onStartWriting: () => void }) {
  const navigate = useNavigate()
  const { t } = useI18n()
  return (
    <header className="border-b">
      <div className="mx-auto flex h-14 w-full max-w-4xl items-center gap-3 px-6">
        <Button variant="ghost" onClick={() => navigate("/square")}>
          {t("appName")}
        </Button>
        <div className="flex-1" />
        <LanguageSelect />
        <Button size="sm" onClick={onStartWriting}>
          <PlusIcon data-icon="inline-start" />
          {t("startWriting")}
        </Button>
      </div>
    </header>
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
        <div className="mt-2">
          <LinkitUserInfo userId={document.owner_id} compact />
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
  const document = useQuery({
    queryKey: ["public-document", documentId, locale],
    queryFn: () =>
      request<PublicDocumentDetail>(
        `/api/public/documents/${documentId}?language=${encodeURIComponent(locale)}`
      ),
    enabled: Boolean(documentId),
    refetchInterval: (query) =>
      query.state.data?.is_translation_fallback || query.state.data?.metadata_status
        ? 2_000
        : false,
  })

  if (document.isPending) return <LoadingPage />
  if (document.error || !document.data)
    return (
      <main className="min-h-svh">
        <PublicHeader onStartWriting={() => navigate("/documents")} />
        <PageError
          error={document.error ?? new Error(t("publishedDocumentNotFound"))}
          action={
            <Button variant="outline" onClick={() => navigate("/square")}>
              <ArrowLeftIcon data-icon="inline-start" />
              {t("backToSquare")}
            </Button>
          }
        />
      </main>
    )

  return (
    <main className="min-h-svh">
      <PublicHeader onStartWriting={() => navigate("/documents")} />
      <div className="mx-auto w-full max-w-3xl p-6 md:py-14">
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
            <Badge variant="outline">{document.data.language}</Badge>
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
        <article className="mt-10 max-w-[72ch]">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={markdownComponents}
          >
            {document.data.content}
          </ReactMarkdown>
        </article>
      </div>
    </main>
  )
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
  t: (key: "aiTaskMetadata" | "aiTaskTranslate") => string
) {
  return t(request.task === "metadata" ? "aiTaskMetadata" : "aiTaskTranslate")
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

function pageTitle(
  pathname: string,
  t: (
    key: "pageTitleAdministration" | "pageTitleDocuments" | "pageTitleEditor"
  ) => string
) {
  if (pathname === "/admin") return t("pageTitleAdministration")
  if (pathname.startsWith("/documents/") && pathname !== "/documents")
    return t("pageTitleEditor")
  return t("pageTitleDocuments")
}
