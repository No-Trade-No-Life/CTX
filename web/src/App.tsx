import { useState, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAuthMini } from "auth-mini-react-components"
import { LinkitMyInfo } from "linkit-react-components"
import {
  BookOpenIcon,
  FileTextIcon,
  FolderPlusIcon,
  Globe2Icon,
  LayoutDashboardIcon,
  LanguagesIcon,
  LoaderCircleIcon,
  PlusIcon,
  SaveIcon,
  Settings2Icon,
  ShieldCheckIcon,
  SparklesIcon,
  UploadIcon,
  type LucideIcon,
} from "lucide-react"
import ReactMarkdown from "react-markdown"
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
  AiRun,
  Context,
  Document,
  DocumentDetail,
  Me,
} from "./lib/types"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
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
        <Route
          path="/p/:contextSlug/:documentSlug"
          element={<PublicDocumentPage />}
        />
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
          <SelectItem value="zh">{t("languageChinese")}</SelectItem>
          <SelectItem value="en">{t("languageEnglish")}</SelectItem>
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

function PrivateApp() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  if (!isReady) return <LoadingPage />
  if (!isAuthenticated || !session?.accessToken) return <LoadingPage />
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
            <div className="flex items-center gap-2 font-semibold">
              <div className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">
                C
              </div>
              <span className="group-data-[collapsible=icon]:hidden">CTX</span>
            </div>
          </SidebarHeader>
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupLabel>{t("navigationWorkspace")}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <NavItem
                    active={location.pathname === "/"}
                    icon={LayoutDashboardIcon}
                    onClick={() => navigate("/")}
                  >
                    {t("navigationOverview")}
                  </NavItem>
                  <NavItem
                    active={location.pathname.startsWith("/contexts")}
                    icon={BookOpenIcon}
                    onClick={() => navigate("/")}
                  >
                    {t("navigationContexts")}
                  </NavItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            <SidebarGroup>
              <SidebarGroupLabel>{t("navigationPublishing")}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <NavItem
                    active={false}
                    icon={FileTextIcon}
                    onClick={() => navigate("/")}
                  >
                    {t("navigationDocsBlog")}
                  </NavItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            {me.data.is_root ? (
              <SidebarGroup>
                <SidebarGroupLabel>{t("navigationSystem")}</SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <NavItem
                      active={location.pathname === "/admin"}
                      icon={Settings2Icon}
                      onClick={() => navigate("/admin")}
                    >
                      {t("navigationAdministration")}
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
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">
                {pageTitle(location.pathname, t)}
              </p>
            </div>
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
                    onClick={refresh}
                    aria-label={t("refreshWorkspace")}
                  />
                }
              >
                <UploadIcon />
              </TooltipTrigger>
              <TooltipContent>{t("refreshWorkspace")}</TooltipContent>
            </Tooltip>
          </header>
          <Routes>
            <Route path="/" element={<OverviewPage token={token} />} />
            <Route
              path="/contexts/:contextId"
              element={<ContextEditor token={token} />}
            />
            <Route
              path="/admin"
              element={
                me.data.is_root ? (
                  <AdministrationPage token={token} />
                ) : (
                  <Navigate to="/" replace />
                )
              }
            />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  )
}

function OverviewPage({ token }: { token: string }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const contexts = useQuery({
    queryKey: ["contexts"],
    queryFn: () => request<Context[]>("/api/v1/contexts", token),
  })
  const [dialogOpen, setDialogOpen] = useState(false)

  if (contexts.isPending) return <PageSkeleton />
  if (contexts.error) return <PageError error={contexts.error} />
  const items = contexts.data ?? []
  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-8 p-4 md:p-6">
      <section className="flex flex-wrap items-end justify-between gap-4">
        <div className="max-w-2xl">
          <h1 className="text-2xl font-semibold tracking-tight text-balance">
            {t("contextsTitle")}
          </h1>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {t("contextsDescription")}
          </p>
        </div>
        <Button onClick={() => setDialogOpen(true)}>
          <FolderPlusIcon data-icon="inline-start" />
          {t("newContext")}
        </Button>
      </section>
      {items.length === 0 ? (
        <Empty className="min-h-80">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <BookOpenIcon />
            </EmptyMedia>
            <EmptyTitle>{t("emptyContextTitle")}</EmptyTitle>
            <EmptyDescription>{t("emptyContextDescription")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => setDialogOpen(true)}>
              <PlusIcon data-icon="inline-start" />
              {t("createContext")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {items.map((context) => (
            <Card key={context.id}>
              <CardHeader className="gap-3">
                <div className="flex items-start justify-between gap-3">
                  <CardTitle>{context.name}</CardTitle>
                  <Badge
                    variant={
                      context.visibility === "public" ? "secondary" : "outline"
                    }
                  >
                    {context.visibility === "public"
                      ? t("visibilityPublic")
                      : t("visibilityPrivate")}
                  </Badge>
                </div>
                <CardDescription>
                  {context.description || t("noDescription")}
                </CardDescription>
              </CardHeader>
              <CardContent className="flex items-center justify-between text-sm text-muted-foreground">
                <span>
                  {t("documentCount").replace(
                    "{count}",
                    String(context.document_count)
                  )}
                </span>
                <span className="font-mono text-xs">/{context.slug}</span>
              </CardContent>
              <CardFooter>
                <Button
                  variant="outline"
                  className="w-full"
                  onClick={() => navigate(`/contexts/${context.id}`)}
                >
                  {t("openContext")}
                </Button>
              </CardFooter>
            </Card>
          ))}
        </section>
      )}
      <ContextDialog
        token={token}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onCreated={(context) => {
          void queryClient.invalidateQueries({ queryKey: ["contexts"] })
          navigate(`/contexts/${context.id}`)
        }}
      />
    </main>
  )
}

function ContextEditor({ token }: { token: string }) {
  const { contextId = "" } = useParams()
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const contexts = useQuery({
    queryKey: ["contexts"],
    queryFn: () => request<Context[]>("/api/v1/contexts", token),
  })
  const documents = useQuery({
    queryKey: ["documents", contextId],
    queryFn: () =>
      request<Document[]>(`/api/v1/contexts/${contextId}/documents`, token),
    enabled: Boolean(contextId),
  })
  const [selectedDocumentId, setSelectedDocumentId] = useState("")
  const [newDocumentOpen, setNewDocumentOpen] = useState(false)
  const activeDocumentId = selectedDocumentId || documents.data?.[0]?.id || ""
  const selectedDocument = documents.data?.find(
    (document) => document.id === activeDocumentId
  )
  const detail = useQuery({
    queryKey: ["document", activeDocumentId],
    queryFn: () =>
      request<DocumentDetail>(`/api/v1/documents/${activeDocumentId}`, token),
    enabled: Boolean(activeDocumentId),
  })

  if (contexts.isPending || documents.isPending) return <PageSkeleton />
  if (contexts.error) return <PageError error={contexts.error} />
  if (documents.error) return <PageError error={documents.error} />
  const context = contexts.data?.find((item) => item.id === contextId)
  if (!context) return <Navigate to="/" replace />

  return (
    <main className="flex min-h-[calc(100svh-3.5rem)] flex-col">
      <div className="flex items-center justify-between gap-3 border-b px-4 py-3 md:px-6">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{context.name}</p>
          <p className="truncate text-xs text-muted-foreground">
            {context.description || t("noEditorialDescription")}
          </p>
        </div>
        <Button size="sm" onClick={() => setNewDocumentOpen(true)}>
          <PlusIcon data-icon="inline-start" />
          {t("document")}
        </Button>
      </div>
      <div className="border-b p-3 lg:hidden">
        <Select
          value={activeDocumentId}
          onValueChange={(value) => {
            if (value) setSelectedDocumentId(value)
          }}
        >
          <SelectTrigger>
            <SelectValue placeholder={t("selectDocument")} />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {(documents.data ?? []).map((document) => (
                <SelectItem key={document.id} value={document.id}>
                  {document.title}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </div>
      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel
          defaultSize="20%"
          minSize="15%"
          className="hidden border-r lg:block"
        >
          <DocumentTree
            documents={documents.data ?? []}
            selectedId={activeDocumentId}
            onSelect={setSelectedDocumentId}
          />
        </ResizablePanel>
        <ResizableHandle className="hidden lg:flex" />
        <ResizablePanel defaultSize="55%" minSize="35%" className="min-w-0">
          {selectedDocument && detail.isPending ? <EditorSkeleton /> : null}
          {selectedDocument && detail.error ? (
            <PageError error={detail.error} />
          ) : null}
          {selectedDocument && detail.data ? (
            <DocumentEditor
              key={detail.data.revision.id}
              token={token}
              documentId={selectedDocument.id}
              detail={detail.data}
              onSaved={() =>
                void queryClient.invalidateQueries({
                  queryKey: ["documents", contextId],
                })
              }
            />
          ) : selectedDocument ? null : (
            <NoDocument onCreate={() => setNewDocumentOpen(true)} />
          )}
        </ResizablePanel>
        <ResizableHandle className="hidden xl:flex" />
        <ResizablePanel
          defaultSize="25%"
          minSize="20%"
          className="hidden border-l xl:block"
        >
          {selectedDocument && detail.data ? (
            <AiPanel token={token} document={detail.data} />
          ) : (
            <AiPanelPlaceholder />
          )}
        </ResizablePanel>
      </ResizablePanelGroup>
      <DocumentDialog
        token={token}
        contextId={contextId}
        open={newDocumentOpen}
        onOpenChange={setNewDocumentOpen}
        onCreated={(document) => {
          void queryClient.invalidateQueries({
            queryKey: ["documents", contextId],
          })
          setSelectedDocumentId(document.document.id)
        }}
      />
    </main>
  )
}

function DocumentTree({
  documents,
  selectedId,
  onSelect,
}: {
  documents: Document[]
  selectedId: string
  onSelect: (id: string) => void
}) {
  const { t } = useI18n()
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-4 py-3">
        <p className="text-xs font-medium text-muted-foreground">
          {t("documents")}
        </p>
        <Badge variant="outline">{documents.length}</Badge>
      </div>
      <div className="flex flex-col gap-1 px-2 pb-3">
        {documents.map((document) => (
          <Button
            key={document.id}
            variant={selectedId === document.id ? "secondary" : "ghost"}
            className="justify-start"
            onClick={() => onSelect(document.id)}
          >
            <FileTextIcon data-icon="inline-start" />
            <span className="truncate">{document.title}</span>
          </Button>
        ))}
      </div>
    </div>
  )
}

function DocumentEditor({
  token,
  documentId,
  detail,
  onSaved,
}: {
  token: string
  documentId: string
  detail: DocumentDetail
  onSaved: () => void
}) {
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const [title, setTitle] = useState(detail.document.title)
  const [slug, setSlug] = useState(detail.document.slug)
  const [content, setContent] = useState(detail.revision.content)

  const save = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(`/api/v1/documents/${documentId}`, token, {
        method: "PUT",
        body: JSON.stringify({ title, slug, content }),
      }),
    onSuccess: () => {
      toast.success(t("savedNewRevision"))
      onSaved()
      void queryClient.invalidateQueries({ queryKey: ["document", documentId] })
    },
    onError: showError,
  })
  const publish = useMutation({
    mutationFn: () =>
      request<Document>(`/api/v1/documents/${documentId}/publish`, token, {
        method: "POST",
      }),
    onSuccess: () => {
      toast.success(t("publishedCurrentRevision"))
      onSaved()
      void queryClient.invalidateQueries({ queryKey: ["document", documentId] })
    },
    onError: showError,
  })

  return (
    <section className="flex h-full min-w-0 flex-col">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3 md:px-6">
        <div className="flex items-center gap-2">
          <Badge
            variant={
              detail.document.status === "published" ? "secondary" : "outline"
            }
          >
            {detail.document.status === "published"
              ? t("statusPublished")
              : t("statusDraft")}
          </Badge>
          <span className="font-mono text-xs text-muted-foreground">
            r:{detail.revision.id.slice(0, 8)}
          </span>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={save.isPending}
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
            size="sm"
            disabled={publish.isPending}
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
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 md:p-6">
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="document-title">{t("title")}</FieldLabel>
            <Input
              id="document-title"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="document-slug">{t("slug")}</FieldLabel>
            <Input
              id="document-slug"
              value={slug}
              onChange={(event) => setSlug(event.target.value)}
            />
          </Field>
        </FieldGroup>
        <Field className="min-h-0 flex-1">
          <FieldLabel htmlFor="document-markdown">{t("markdown")}</FieldLabel>
          <Textarea
            id="document-markdown"
            className="min-h-105 flex-1 font-mono text-sm leading-6"
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />
          <FieldDescription>{t("markdownDescription")}</FieldDescription>
        </Field>
      </div>
    </section>
  )
}

function AiPanel({
  token,
  document,
}: {
  token: string
  document: DocumentDetail
}) {
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const [targetLanguage, setTargetLanguage] = useState("zh-Hans")
  const [run, setRun] = useState<AiRun>()
  const ai = useMutation({
    mutationFn: (task: "metadata" | "summary" | "translate") =>
      request<AiRun>(`/api/v1/documents/${document.document.id}/ai`, token, {
        method: "POST",
        body: JSON.stringify({
          task,
          target_language: task === "translate" ? targetLanguage : undefined,
        }),
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
        `/api/v1/documents/${document.document.id}`,
        token,
        {
          method: "PUT",
          body: JSON.stringify({
            title: document.document.title,
            slug: document.document.slug,
            content: document.revision.content,
            message: t("appliedAiMetadata"),
            metadata: JSON.parse(run?.output ?? "{}"),
          }),
        }
      ),
    onSuccess: () => {
      toast.success(t("metadataSaved"))
      void queryClient.invalidateQueries({
        queryKey: ["document", document.document.id],
      })
    },
    onError: showError,
  })
  const saveTranslation = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(
        `/api/v1/contexts/${document.document.context_id}/documents`,
        token,
        {
          method: "POST",
          body: JSON.stringify({
            title: `${document.document.title} (${targetLanguage})`,
            slug: `${document.document.slug}-${slugify(targetLanguage)}`,
            language: targetLanguage,
            kind: document.document.kind,
            content: run?.proposed_content,
          }),
        }
      ),
    onSuccess: () => {
      toast.success(t("translationSaved"))
      void queryClient.invalidateQueries({
        queryKey: ["documents", document.document.context_id],
      })
    },
    onError: showError,
  })
  const action = (
    task: "metadata" | "summary" | "translate",
    label: string
  ) => (
    <Button
      key={task}
      variant="outline"
      className="w-full justify-start"
      disabled={ai.isPending}
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
    <aside className="flex h-full min-h-0 flex-col">
      <div className="border-b px-4 py-3">
        <p className="text-sm font-medium">{t("contextAi")}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {t("contextAiDescription")}
        </p>
      </div>
      <Tabs defaultValue="actions" className="min-h-0 flex-1 p-4">
        <TabsList>
          <TabsTrigger value="actions">{t("aiActions")}</TabsTrigger>
          <TabsTrigger value="output">{t("aiOutput")}</TabsTrigger>
        </TabsList>
        <TabsContent value="actions" className="flex flex-col gap-3">
          {action("metadata", t("extractMetadata"))}
          {action("summary", t("summarizeDocument"))}
          <FieldGroup className="pt-2">
            <Field>
              <FieldLabel htmlFor="translation-language">
                {t("translationLanguage")}
              </FieldLabel>
              <Input
                id="translation-language"
                value={targetLanguage}
                onChange={(event) => setTargetLanguage(event.target.value)}
              />
            </Field>
          </FieldGroup>
          {action("translate", t("translateMarkdown"))}
        </TabsContent>
        <TabsContent value="output" className="min-h-0">
          {run ? (
            <div className="flex flex-col gap-3">
              <pre className="max-h-125 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap">
                {run.output}
              </pre>
              {run.task === "metadata" ? (
                <Button
                  variant="outline"
                  disabled={applyMetadata.isPending}
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
              {run.task === "translate" && run.proposed_content ? (
                <Button
                  variant="outline"
                  disabled={saveTranslation.isPending}
                  onClick={() => saveTranslation.mutate()}
                >
                  {saveTranslation.isPending ? (
                    <LoaderCircleIcon
                      className="animate-spin"
                      data-icon="inline-start"
                    />
                  ) : (
                    <FileTextIcon data-icon="inline-start" />
                  )}
                  {t("saveTranslationDraft")}
                </Button>
              ) : null}
            </div>
          ) : (
            <p className="pt-3 text-sm leading-6 text-muted-foreground">
              {t("aiOutputEmpty")}
            </p>
          )}
        </TabsContent>
      </Tabs>
    </aside>
  )
}

function AiPanelPlaceholder() {
  const { t } = useI18n()
  return (
    <aside className="p-4">
      <p className="text-sm font-medium">{t("contextAi")}</p>
      <p className="mt-2 text-sm leading-6 text-muted-foreground">
        {t("aiPlaceholder")}
      </p>
    </aside>
  )
}

function NoDocument({ onCreate }: { onCreate: () => void }) {
  const { t } = useI18n()
  return (
    <Empty className="m-4 min-h-96">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <FileTextIcon />
        </EmptyMedia>
        <EmptyTitle>{t("noDocumentTitle")}</EmptyTitle>
        <EmptyDescription>{t("noDocumentDescription")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button onClick={onCreate}>
          <PlusIcon data-icon="inline-start" />
          {t("newDocument")}
        </Button>
      </EmptyContent>
    </Empty>
  )
}

function ContextDialog({
  token,
  open,
  onOpenChange,
  onCreated,
}: {
  token: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (context: Context) => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState("")
  const [slug, setSlug] = useState("")
  const [description, setDescription] = useState("")
  const [instructions, setInstructions] = useState("")
  const [visibility, setVisibility] = useState<"private" | "public">("private")
  const create = useMutation({
    mutationFn: () =>
      request<Context>("/api/v1/contexts", token, {
        method: "POST",
        body: JSON.stringify({
          name,
          slug,
          description,
          instructions,
          visibility,
        }),
      }),
    onSuccess: (context) => {
      toast.success(t("contextCreated"))
      onOpenChange(false)
      onCreated(context)
    },
    onError: showError,
  })
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("contextDialogTitle")}</DialogTitle>
          <DialogDescription>{t("contextDialogDescription")}</DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="context-name">{t("name")}</FieldLabel>
            <Input
              id="context-name"
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                if (!slug) setSlug(slugify(event.target.value))
              }}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="context-slug">{t("publicSlug")}</FieldLabel>
            <Input
              id="context-slug"
              value={slug}
              onChange={(event) => setSlug(slugify(event.target.value))}
            />
            <FieldDescription>{t("slugDescription")}</FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor="context-description">
              {t("description")}
            </FieldLabel>
            <Textarea
              id="context-description"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="context-instructions">
              {t("aiInstructions")}
            </FieldLabel>
            <Textarea
              id="context-instructions"
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
            />
            <FieldDescription>
              {t("aiInstructionsDescription")}
            </FieldDescription>
          </Field>
          <Field>
            <FieldLabel>{t("visibility")}</FieldLabel>
            <Select
              value={visibility}
              onValueChange={(value) =>
                setVisibility(value as "private" | "public")
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="private">
                    {t("visibilityPrivate")}
                  </SelectItem>
                  <SelectItem value="public">
                    {t("visibilityPublic")}
                  </SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
        </FieldGroup>
        <DialogFooter>
          <Button
            disabled={create.isPending || !name || !slug}
            onClick={() => create.mutate()}
          >
            {create.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <FolderPlusIcon data-icon="inline-start" />
            )}
            {t("createContext")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function DocumentDialog({
  token,
  contextId,
  open,
  onOpenChange,
  onCreated,
}: {
  token: string
  contextId: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (document: DocumentDetail) => void
}) {
  const { locale, t } = useI18n()
  const [title, setTitle] = useState("")
  const [slug, setSlug] = useState("")
  const [language, setLanguage] = useState(locale === "zh" ? "zh-Hans" : "en")
  const [kind, setKind] = useState<"docs" | "blog">("docs")
  const create = useMutation({
    mutationFn: () =>
      request<DocumentDetail>(
        `/api/v1/contexts/${contextId}/documents`,
        token,
        {
          method: "POST",
          body: JSON.stringify({
            title,
            slug,
            language,
            kind,
            content: `# ${title}\n`,
          }),
        }
      ),
    onSuccess: (document) => {
      toast.success(t("documentCreated"))
      onOpenChange(false)
      onCreated(document)
    },
    onError: showError,
  })
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("documentDialogTitle")}</DialogTitle>
          <DialogDescription>
            {t("documentDialogDescription")}
          </DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="new-document-title">{t("title")}</FieldLabel>
            <Input
              id="new-document-title"
              value={title}
              onChange={(event) => {
                setTitle(event.target.value)
                if (!slug) setSlug(slugify(event.target.value))
              }}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="new-document-slug">{t("slug")}</FieldLabel>
            <Input
              id="new-document-slug"
              value={slug}
              onChange={(event) => setSlug(slugify(event.target.value))}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="new-document-language">
              {t("sourceLanguage")}
            </FieldLabel>
            <Input
              id="new-document-language"
              value={language}
              onChange={(event) => setLanguage(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel>{t("surface")}</FieldLabel>
            <Select
              value={kind}
              onValueChange={(value) => setKind(value as "docs" | "blog")}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="docs">{t("surfaceDocs")}</SelectItem>
                  <SelectItem value="blog">{t("surfaceBlog")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
        </FieldGroup>
        <DialogFooter>
          <Button
            disabled={create.isPending || !title || !slug}
            onClick={() => create.mutate()}
          >
            {create.isPending ? (
              <LoaderCircleIcon
                className="animate-spin"
                data-icon="inline-start"
              />
            ) : (
              <PlusIcon data-icon="inline-start" />
            )}
            {t("createDocument")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
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
    <main className="p-6">
      <Card className="mx-auto mt-16 max-w-xl">
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
  const configuration = useQuery({
    queryKey: ["ai-configuration"],
    queryFn: () => request<AiConfiguration>("/api/v1/admin/ai", token),
  })
  if (configuration.isPending) return <PageSkeleton />
  if (configuration.error) return <PageError error={configuration.error} />
  if (!configuration.data) return <PageSkeleton />
  return (
    <AdministrationForm
      key={`${configuration.data.base_url}:${configuration.data.model}`}
      token={token}
      configuration={configuration.data}
    />
  )
}

function AdministrationForm({
  token,
  configuration,
}: {
  token: string
  configuration: AiConfiguration
}) {
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const [baseUrl, setBaseUrl] = useState(configuration.base_url)
  const [model, setModel] = useState(configuration.model)
  const [apiKey, setApiKey] = useState("")
  const update = useMutation({
    mutationFn: () =>
      request<AiConfiguration>("/api/v1/admin/ai", token, {
        method: "PUT",
        body: JSON.stringify({
          base_url: baseUrl,
          model,
          api_key: apiKey || undefined,
        }),
      }),
    onSuccess: () => {
      toast.success(t("aiConfigurationSaved"))
      setApiKey("")
      void queryClient.invalidateQueries({ queryKey: ["ai-configuration"] })
    },
    onError: showError,
  })
  return (
    <main className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">
          {t("navigationAdministration")}
        </h1>
        <p className="mt-2 text-sm text-muted-foreground">
          {t("administrationDescription")}
        </p>
      </div>
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-4">
            <div>
              <CardTitle>{t("openAiRouting")}</CardTitle>
              <CardDescription>{t("openAiRoutingDescription")}</CardDescription>
            </div>
            <Badge variant={configuration.configured ? "secondary" : "outline"}>
              {configuration.configured ? t("configured") : t("needsKey")}
            </Badge>
          </div>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="ai-base-url">{t("baseUrl")}</FieldLabel>
              <Input
                id="ai-base-url"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-model">{t("model")}</FieldLabel>
              <Input
                id="ai-model"
                value={model}
                onChange={(event) => setModel(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-api-key">{t("apiKey")}</FieldLabel>
              <Input
                id="ai-api-key"
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder={t("apiKeyPlaceholder")}
              />
              <FieldDescription>{t("apiKeyDescription")}</FieldDescription>
            </Field>
            <Field>
              <Button
                disabled={update.isPending || !baseUrl || !model}
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

function PublicDocumentPage() {
  const { contextSlug = "", documentSlug = "" } = useParams()
  const { t } = useI18n()
  const document = useQuery({
    queryKey: ["public-document", contextSlug, documentSlug],
    queryFn: () =>
      request<DocumentDetail>(
        `/api/public/contexts/${contextSlug}/documents/${documentSlug}`
      ),
  })
  if (document.isPending) return <LoadingPage />
  if (document.error || !document.data)
    return (
      <PageError
        error={document.error ?? new Error(t("publishedDocumentNotFound"))}
      />
    )
  return (
    <main className="mx-auto w-full max-w-3xl p-6 md:py-16">
      <div className="mb-10 flex items-center justify-between gap-3">
        <a href="#/" className="text-sm font-medium text-primary">
          CTX
        </a>
        <div className="flex items-center gap-2">
          <Badge variant="outline">
            {document.data.document.kind === "docs"
              ? t("surfaceDocs")
              : t("surfaceBlog")}
          </Badge>
          <LanguageSelect />
        </div>
      </div>
      <article className="max-w-[72ch]">
        <ReactMarkdown components={markdownComponents}>
          {document.data.revision.content}
        </ReactMarkdown>
      </article>
    </main>
  )
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
  ul: ({ children }: { children?: ReactNode }) => (
    <ul className="mt-5 list-disc pl-6 text-base leading-7">{children}</ul>
  ),
  ol: ({ children }: { children?: ReactNode }) => (
    <ol className="mt-5 list-decimal pl-6 text-base leading-7">{children}</ol>
  ),
  code: ({ children }: { children?: ReactNode }) => (
    <code className="rounded-sm bg-muted px-1 py-0.5 font-mono text-sm">
      {children}
    </code>
  ),
  pre: ({ children }: { children?: ReactNode }) => (
    <pre className="mt-5 overflow-x-auto rounded-md bg-muted p-4 text-sm leading-6">
      {children}
    </pre>
  ),
  a: ({ children, href }: { children?: ReactNode; href?: string }) => (
    <a href={href} className="text-primary underline underline-offset-4">
      {children}
    </a>
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
    <div className="p-6">
      <Skeleton className="h-8 w-64" />
      <Skeleton className="mt-5 h-96 w-full" />
    </div>
  )
}
function LoadingPage() {
  return (
    <main className="grid min-h-svh place-items-center">
      <Skeleton className="h-8 w-48" />
    </main>
  )
}
function PageError({ error }: { error: Error }) {
  const { t } = useI18n()
  return (
    <main className="p-6">
      <Alert variant="destructive">
        <AlertTitle>{t("requestFailed")}</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </main>
  )
}
function showError(error: Error) {
  toast.error(error.message)
}
function slugify(value: string) {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "")
}
function pageTitle(
  pathname: string,
  t: (
    key:
      | "pageTitleAdministration"
      | "pageTitleContextEditor"
      | "pageTitleWorkspace"
  ) => string
) {
  if (pathname === "/admin") return t("pageTitleAdministration")
  if (pathname.startsWith("/contexts")) return t("pageTitleContextEditor")
  return t("pageTitleWorkspace")
}
