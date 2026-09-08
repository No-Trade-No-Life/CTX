import { useState, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { AuthMiniButton, useAuthMini } from "auth-mini-react-components"
import {
  BookOpenIcon,
  FileTextIcon,
  FolderPlusIcon,
  Globe2Icon,
  LayoutDashboardIcon,
  LoaderCircleIcon,
  PanelLeftIcon,
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
  SidebarFooter,
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
        <Route
          path="/p/:contextSlug/:documentSlug"
          element={<PublicDocumentPage />}
        />
        <Route path="*" element={<PrivateApp />} />
      </Routes>
      <Toaster />
    </>
  )
}

function PrivateApp() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  if (!isReady) return <LoadingPage />
  if (!isAuthenticated || !session?.accessToken) return <SignInPage />
  return <CtxShell token={session.accessToken} />
}

function SignInPage() {
  return (
    <main className="grid min-h-svh place-items-center p-6">
      <Card className="w-full max-w-md">
        <CardHeader className="gap-3">
          <div className="grid size-10 place-items-center rounded-md bg-primary text-primary-foreground">
            <PanelLeftIcon />
          </div>
          <div className="flex flex-col gap-1">
            <CardTitle>CTX</CardTitle>
            <CardDescription>
              Markdown-first Context for people and AI.
            </CardDescription>
          </div>
        </CardHeader>
        <CardContent>
          <AuthMiniButton lang="en" variant="default" />
        </CardContent>
      </Card>
    </main>
  )
}

function CtxShell({ token }: { token: string }) {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
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
              <SidebarGroupLabel>Workspace</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <NavItem
                    active={location.pathname === "/"}
                    icon={LayoutDashboardIcon}
                    onClick={() => navigate("/")}
                  >
                    Overview
                  </NavItem>
                  <NavItem
                    active={location.pathname.startsWith("/contexts")}
                    icon={BookOpenIcon}
                    onClick={() => navigate("/")}
                  >
                    Contexts
                  </NavItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            <SidebarGroup>
              <SidebarGroupLabel>Publishing</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <NavItem
                    active={false}
                    icon={FileTextIcon}
                    onClick={() => navigate("/")}
                  >
                    Docs &amp; blog
                  </NavItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            {me.data.is_root ? (
              <SidebarGroup>
                <SidebarGroupLabel>System</SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <NavItem
                      active={location.pathname === "/admin"}
                      icon={Settings2Icon}
                      onClick={() => navigate("/admin")}
                    >
                      Administration
                    </NavItem>
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            ) : null}
          </SidebarContent>
          <SidebarFooter className="p-3">
            <AuthMiniButton lang="en" variant="ghost" size="sm" />
          </SidebarFooter>
        </Sidebar>
        <SidebarInset>
          <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background px-4">
            <SidebarTrigger />
            <Separator orientation="vertical" className="h-5" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">
                {pageTitle(location.pathname)}
              </p>
            </div>
            {me.data.is_root ? (
              <Badge variant="outline">
                <ShieldCheckIcon data-icon="inline-start" />
                Root
              </Badge>
            ) : null}
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={refresh}
                    aria-label="Refresh workspace"
                  />
                }
              >
                <UploadIcon />
              </TooltipTrigger>
              <TooltipContent>Refresh workspace</TooltipContent>
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
            Your Contexts
          </h1>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            A Context gives your Markdown a deliberate boundary: its documents,
            editorial rules, AI work, and published surface.
          </p>
        </div>
        <Button onClick={() => setDialogOpen(true)}>
          <FolderPlusIcon data-icon="inline-start" />
          New Context
        </Button>
      </section>
      {items.length === 0 ? (
        <Empty className="min-h-80">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <BookOpenIcon />
            </EmptyMedia>
            <EmptyTitle>Start with one bounded idea</EmptyTitle>
            <EmptyDescription>
              Create a Context for a product, research area, personal wiki, or
              publication. Markdown stays the source of truth.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => setDialogOpen(true)}>
              <PlusIcon data-icon="inline-start" />
              Create Context
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
                    {context.visibility}
                  </Badge>
                </div>
                <CardDescription>
                  {context.description || "No description yet."}
                </CardDescription>
              </CardHeader>
              <CardContent className="flex items-center justify-between text-sm text-muted-foreground">
                <span>{context.document_count} documents</span>
                <span className="font-mono text-xs">/{context.slug}</span>
              </CardContent>
              <CardFooter>
                <Button
                  variant="outline"
                  className="w-full"
                  onClick={() => navigate(`/contexts/${context.id}`)}
                >
                  Open Context
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
            {context.description || "No editorial description"}
          </p>
        </div>
        <Button size="sm" onClick={() => setNewDocumentOpen(true)}>
          <PlusIcon data-icon="inline-start" />
          Document
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
            <SelectValue placeholder="Select document" />
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
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-4 py-3">
        <p className="text-xs font-medium text-muted-foreground">DOCUMENTS</p>
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
      toast.success("Saved as a new revision")
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
      toast.success("Published current revision")
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
            {detail.document.status}
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
            Save
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
            Publish
          </Button>
        </div>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 md:p-6">
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="document-title">Title</FieldLabel>
            <Input
              id="document-title"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="document-slug">Slug</FieldLabel>
            <Input
              id="document-slug"
              value={slug}
              onChange={(event) => setSlug(event.target.value)}
            />
          </Field>
        </FieldGroup>
        <Field className="min-h-0 flex-1">
          <FieldLabel htmlFor="document-markdown">Markdown</FieldLabel>
          <Textarea
            id="document-markdown"
            className="min-h-105 flex-1 font-mono text-sm leading-6"
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />
          <FieldDescription>
            Saving writes a new immutable revision. AI never replaces this
            source without an explicit save.
          </FieldDescription>
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
      toast.success("AI work recorded with this revision")
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
            message: "Applied AI metadata",
            metadata: JSON.parse(run?.output ?? "{}"),
          }),
        }
      ),
    onSuccess: () => {
      toast.success("Metadata saved on a new revision")
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
      toast.success("Translation saved as a new draft")
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
        <p className="text-sm font-medium">Context AI</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          Works from this revision and its Context rules.
        </p>
      </div>
      <Tabs defaultValue="actions" className="min-h-0 flex-1 p-4">
        <TabsList>
          <TabsTrigger value="actions">Actions</TabsTrigger>
          <TabsTrigger value="output">Output</TabsTrigger>
        </TabsList>
        <TabsContent value="actions" className="flex flex-col gap-3">
          {action("metadata", "Extract metadata")}
          {action("summary", "Summarize document")}
          <FieldGroup className="pt-2">
            <Field>
              <FieldLabel htmlFor="translation-language">
                Translation language
              </FieldLabel>
              <Input
                id="translation-language"
                value={targetLanguage}
                onChange={(event) => setTargetLanguage(event.target.value)}
              />
            </Field>
          </FieldGroup>
          {action("translate", "Translate Markdown")}
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
                  Apply metadata
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
                  Save translation as draft
                </Button>
              ) : null}
            </div>
          ) : (
            <p className="pt-3 text-sm leading-6 text-muted-foreground">
              Run a metadata, summary, or translation task to inspect an
              auditable AI output.
            </p>
          )}
        </TabsContent>
      </Tabs>
    </aside>
  )
}

function AiPanelPlaceholder() {
  return (
    <aside className="p-4">
      <p className="text-sm font-medium">Context AI</p>
      <p className="mt-2 text-sm leading-6 text-muted-foreground">
        Select a document to work from its current revision.
      </p>
    </aside>
  )
}

function NoDocument({ onCreate }: { onCreate: () => void }) {
  return (
    <Empty className="m-4 min-h-96">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <FileTextIcon />
        </EmptyMedia>
        <EmptyTitle>Create the first document</EmptyTitle>
        <EmptyDescription>
          Use Docs for stable navigation or Blog for chronological writing. Both
          remain Markdown revisions in this Context.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button onClick={onCreate}>
          <PlusIcon data-icon="inline-start" />
          New document
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
      toast.success("Context created")
      onOpenChange(false)
      onCreated(context)
    },
    onError: showError,
  })
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>New Context</DialogTitle>
          <DialogDescription>
            A Context defines the documents and editorial instructions that AI
            may use together.
          </DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="context-name">Name</FieldLabel>
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
            <FieldLabel htmlFor="context-slug">Public slug</FieldLabel>
            <Input
              id="context-slug"
              value={slug}
              onChange={(event) => setSlug(slugify(event.target.value))}
            />
            <FieldDescription>
              Lowercase letters, numbers, and hyphens only.
            </FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor="context-description">Description</FieldLabel>
            <Textarea
              id="context-description"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="context-instructions">
              AI instructions
            </FieldLabel>
            <Textarea
              id="context-instructions"
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
            />
            <FieldDescription>
              Voice, terminology, intended audience, and evidence rules for this
              Context.
            </FieldDescription>
          </Field>
          <Field>
            <FieldLabel>Visibility</FieldLabel>
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
                  <SelectItem value="private">Private</SelectItem>
                  <SelectItem value="public">Public</SelectItem>
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
            Create Context
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
  const [title, setTitle] = useState("")
  const [slug, setSlug] = useState("")
  const [language, setLanguage] = useState("en")
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
      toast.success("Document created")
      onOpenChange(false)
      onCreated(document)
    },
    onError: showError,
  })
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New document</DialogTitle>
          <DialogDescription>
            Create a Markdown source document. Publishing is a separate action.
          </DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="new-document-title">Title</FieldLabel>
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
            <FieldLabel htmlFor="new-document-slug">Slug</FieldLabel>
            <Input
              id="new-document-slug"
              value={slug}
              onChange={(event) => setSlug(slugify(event.target.value))}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="new-document-language">
              Source language
            </FieldLabel>
            <Input
              id="new-document-language"
              value={language}
              onChange={(event) => setLanguage(event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel>Surface</FieldLabel>
            <Select
              value={kind}
              onValueChange={(value) => setKind(value as "docs" | "blog")}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="docs">Docs</SelectItem>
                  <SelectItem value="blog">Blog</SelectItem>
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
            Create document
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function SetupPage({ token, onDone }: { token: string; onDone: () => void }) {
  const setup = useMutation({
    mutationFn: () => request<Me>("/api/v1/setup", token, { method: "POST" }),
    onSuccess: () => {
      toast.success("Root user configured")
      onDone()
    },
    onError: showError,
  })
  return (
    <main className="p-6">
      <Card className="mx-auto mt-16 max-w-xl">
        <CardHeader>
          <CardTitle>Initialize CTX</CardTitle>
          <CardDescription>
            The first authenticated Auth Mini user becomes CTX's root
            administrator. Sign-in stays owned by Auth Mini.
          </CardDescription>
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
            Become root administrator
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
      toast.success("AI configuration saved")
      setApiKey("")
      void queryClient.invalidateQueries({ queryKey: ["ai-configuration"] })
    },
    onError: showError,
  })
  return (
    <main className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">
          Administration
        </h1>
        <p className="mt-2 text-sm text-muted-foreground">
          Instance-only controls stay separate from Context authoring.
        </p>
      </div>
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-4">
            <div>
              <CardTitle>OpenAI routing</CardTitle>
              <CardDescription>
                CTX calls OpenAI-compatible chat completions through
                openai.ntnl.io. The API key is encrypted on this host and never
                returned to the browser.
              </CardDescription>
            </div>
            <Badge variant={configuration.configured ? "secondary" : "outline"}>
              {configuration.configured ? "configured" : "needs key"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="ai-base-url">Base URL</FieldLabel>
              <Input
                id="ai-base-url"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-model">Model</FieldLabel>
              <Input
                id="ai-model"
                value={model}
                onChange={(event) => setModel(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ai-api-key">API key</FieldLabel>
              <Input
                id="ai-api-key"
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder="Leave blank to keep the encrypted key"
              />
              <FieldDescription>
                Only root can update this secret. CTX will never echo it back.
              </FieldDescription>
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
                Save AI configuration
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
        error={document.error ?? new Error("Published document not found")}
      />
    )
  return (
    <main className="mx-auto w-full max-w-3xl p-6 md:py-16">
      <div className="mb-10 flex items-center justify-between gap-3">
        <a href="#/" className="text-sm font-medium text-primary">
          CTX
        </a>
        <Badge variant="outline">{document.data.document.kind}</Badge>
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
  return (
    <main className="p-6">
      <Alert variant="destructive">
        <AlertTitle>CTX could not complete this request</AlertTitle>
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
function pageTitle(pathname: string) {
  if (pathname === "/admin") return "Administration"
  if (pathname.startsWith("/contexts")) return "Context editor"
  return "Workspace"
}
