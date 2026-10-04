import { useMemo, useState } from "react"
import {
  DndContext,
  DragOverlay,
  MouseSensor,
  TouchSensor,
  closestCenter,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragMoveEvent,
  type DragStartEvent,
} from "@dnd-kit/core"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import {
  ChevronRightIcon,
  ExternalLinkIcon,
  FileTextIcon,
  PlusIcon,
} from "lucide-react"
import { useNavigate } from "react-router-dom"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { request, type AuthSdk } from "@/lib/api"
import { useI18n } from "@/lib/i18n"
import type { Document, DocumentDetail } from "@/lib/types"
import { cn } from "@/lib/utils"

type DropZone = "before" | "inside" | "after"

type TreeItem = {
  document: Document
  depth: number
  childCount: number
}

type Placement = { parentId: string | null; afterId: string | null }

export function DocumentTree({
  documents,
  auth,
}: {
  documents: Document[]
  auth: AuthSdk
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [activeId, setActiveId] = useState<string | null>(null)
  const [dropTarget, setDropTarget] = useState<{
    id: string
    zone: DropZone
  } | null>(null)

  const index = useMemo(() => childrenIndex(documents), [documents])
  const items = useMemo(() => flattenTree(index, collapsed), [index, collapsed])
  const invalidTargets = useMemo(
    () =>
      new Set(
        activeId ? [activeId, ...collectSubtreeIds(index, activeId)] : []
      ),
    [index, activeId]
  )
  const activeDocument =
    documents.find((document) => document.id === activeId) ?? null

  const move = useMutation({
    mutationFn: (input: {
      id: string
      parentId: string | null
      afterId: string | null
    }) =>
      request<DocumentDetail>(`/api/v1/documents/${input.id}/move`, auth, {
        method: "POST",
        body: JSON.stringify({
          parent_id: input.parentId,
          after_id: input.afterId,
        }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["documents"] })
    },
    onError: (error) => {
      toast.error(error.message)
      void queryClient.invalidateQueries({ queryKey: ["documents"] })
    },
  })

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    useSensor(TouchSensor, {
      activationConstraint: { delay: 220, tolerance: 6 },
    })
  )

  const toggle = (id: string) =>
    setCollapsed((previous) => {
      const next = new Set(previous)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const resetDrag = () => {
    setActiveId(null)
    setDropTarget(null)
  }

  const handleDragStart = (event: DragStartEvent) => {
    setActiveId(String(event.active.id))
    setDropTarget(null)
  }

  const handleDragMove = (event: DragMoveEvent) => {
    const over = event.over
    const translated = event.active.rect.current.translated
    if (!over || !translated || invalidTargets.has(String(over.id))) {
      setDropTarget(null)
      return
    }
    const center = translated.top + translated.height / 2
    const ratio = (center - over.rect.top) / over.rect.height
    const zone: DropZone =
      ratio < 0.3 ? "before" : ratio > 0.7 ? "after" : "inside"
    const id = String(over.id)
    setDropTarget((previous) =>
      previous && previous.id === id && previous.zone === zone
        ? previous
        : { id, zone }
    )
  }

  const handleDragEnd = (event: DragEndEvent) => {
    const target = dropTarget
    const id = String(event.active.id)
    resetDrag()
    if (!target) return
    const moving = documents.find((document) => document.id === id)
    const over = documents.find((document) => document.id === target.id)
    if (!moving || !over) return
    const placement = placementFor(index, moving, over, target.zone)
    if (!placement) return
    if (
      placement.parentId === moving.parent_id &&
      placement.afterId === previousSiblingId(index, moving)
    ) {
      return
    }
    move.mutate({ id, ...placement })
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragMove={handleDragMove}
      onDragEnd={handleDragEnd}
      onDragCancel={resetDrag}
    >
      <div className="flex flex-col">
        {items.map((item) => (
          <TreeRow
            key={item.document.id}
            item={item}
            disabled={move.isPending}
            isDragging={item.document.id === activeId}
            isCollapsed={collapsed.has(item.document.id)}
            dropZone={
              dropTarget && dropTarget.id === item.document.id
                ? dropTarget.zone
                : null
            }
            onToggle={toggle}
            onOpen={(documentId) => navigate(`/documents/${documentId}`)}
            onViewPublic={(documentId) => navigate(`/p/${documentId}`)}
            onNewChild={(documentId) =>
              navigate(`/documents/new?parent=${documentId}`)
            }
          />
        ))}
      </div>
      <DragOverlay dropAnimation={null}>
        {activeDocument ? (
          <div className="flex w-64 items-center gap-2 rounded-md border bg-background px-3 py-1.5 shadow-lg">
            <FileTextIcon className="size-4 shrink-0 text-muted-foreground" />
            <span className="truncate text-sm">{activeDocument.title}</span>
          </div>
        ) : null}
      </DragOverlay>
    </DndContext>
  )
}

function TreeRow({
  item,
  disabled,
  isDragging,
  isCollapsed,
  dropZone,
  onToggle,
  onOpen,
  onViewPublic,
  onNewChild,
}: {
  item: TreeItem
  disabled: boolean
  isDragging: boolean
  isCollapsed: boolean
  dropZone: DropZone | null
  onToggle: (id: string) => void
  onOpen: (id: string) => void
  onViewPublic: (id: string) => void
  onNewChild: (id: string) => void
}) {
  const { t } = useI18n()
  const { listeners, setNodeRef: setDraggableNode } = useDraggable({
    id: item.document.id,
    disabled,
  })
  const { setNodeRef: setDroppableNode } = useDroppable({
    id: item.document.id,
    disabled,
  })
  const setNodeRefs = (node: HTMLDivElement | null) => {
    setDraggableNode(node)
    setDroppableNode(node)
  }
  const isPublished = item.document.status === "published"
  return (
    <div ref={setNodeRefs} className="relative">
      {dropZone === "before" ? (
        <div className="absolute inset-x-2 top-0 z-10 h-0.5 rounded-full bg-primary" />
      ) : null}
      <div
        {...listeners}
        className={cn(
          "group flex cursor-grab items-center gap-1 rounded-md py-1 pr-1 select-none",
          isDragging && "opacity-40",
          dropZone === "inside" && "bg-primary/10 ring-1 ring-primary/30"
        )}
        style={{ paddingLeft: `${0.5 + item.depth * 1.25}rem` }}
      >
        {item.childCount > 0 ? (
          <Button
            variant="ghost"
            size="icon-sm"
            className="size-6 shrink-0 text-muted-foreground"
            aria-expanded={!isCollapsed}
            aria-label={t("toggleSubDocuments")}
            onClick={() => onToggle(item.document.id)}
          >
            <ChevronRightIcon
              className={cn(
                "size-4 transition-transform",
                !isCollapsed && "rotate-90"
              )}
            />
          </Button>
        ) : (
          <span className="size-6 shrink-0" />
        )}
        <FileTextIcon className="size-4 shrink-0 text-muted-foreground" />
        <Button
          variant="ghost"
          className="min-w-0 flex-1 justify-start px-1 text-sm font-normal"
          onClick={() => onOpen(item.document.id)}
        >
          <span className="truncate">{item.document.title}</span>
        </Button>
        {isPublished ? (
          <Badge variant="secondary" className="shrink-0">
            {t("published")}
          </Badge>
        ) : null}
        {isPublished ? (
          <Button
            variant="ghost"
            size="icon-sm"
            className="shrink-0 text-muted-foreground"
            aria-label={t("viewPublicArticle")}
            onClick={() => onViewPublic(item.document.id)}
          >
            <ExternalLinkIcon />
          </Button>
        ) : null}
        <Button
          variant="ghost"
          size="icon-sm"
          className="shrink-0 text-muted-foreground"
          aria-label={t("newSubDocument")}
          onClick={() => onNewChild(item.document.id)}
        >
          <PlusIcon />
        </Button>
      </div>
      {dropZone === "after" ? (
        <div className="absolute inset-x-2 bottom-0 z-10 h-0.5 rounded-full bg-primary" />
      ) : null}
    </div>
  )
}

function sortKeyOrder(left: Document, right: Document): number {
  if (left.sort_key !== right.sort_key) {
    return left.sort_key < right.sort_key ? -1 : 1
  }
  if (left.id === right.id) return 0
  return left.id < right.id ? -1 : 1
}

function childrenIndex(documents: Document[]): Map<string | null, Document[]> {
  const index = new Map<string | null, Document[]>()
  for (const document of documents) {
    const siblings = index.get(document.parent_id)
    if (siblings) siblings.push(document)
    else index.set(document.parent_id, [document])
  }
  for (const siblings of index.values()) siblings.sort(sortKeyOrder)
  return index
}

function flattenTree(
  index: Map<string | null, Document[]>,
  collapsed: ReadonlySet<string>
): TreeItem[] {
  const items: TreeItem[] = []
  const visit = (parentId: string | null, depth: number) => {
    for (const document of index.get(parentId) ?? []) {
      items.push({
        document,
        depth,
        childCount: (index.get(document.id) ?? []).length,
      })
      if (!collapsed.has(document.id)) visit(document.id, depth + 1)
    }
  }
  visit(null, 0)
  return items
}

function collectSubtreeIds(
  index: Map<string | null, Document[]>,
  id: string
): string[] {
  return (index.get(id) ?? []).flatMap((child) => [
    child.id,
    ...collectSubtreeIds(index, child.id),
  ])
}

function previousSiblingId(
  index: Map<string | null, Document[]>,
  document: Document
): string | null {
  const siblings = index.get(document.parent_id) ?? []
  const position = siblings.findIndex((sibling) => sibling.id === document.id)
  const previous = siblings[position - 1]
  return previous ? previous.id : null
}

function placementFor(
  index: Map<string | null, Document[]>,
  moving: Document,
  over: Document,
  zone: DropZone
): Placement | null {
  if (zone === "inside") {
    const children = (index.get(over.id) ?? []).filter(
      (child) => child.id !== moving.id
    )
    const last = children[children.length - 1]
    return { parentId: over.id, afterId: last ? last.id : null }
  }
  if (zone === "after") {
    if (over.id === moving.id) return null
    return { parentId: over.parent_id, afterId: over.id }
  }
  const siblings = index.get(over.parent_id) ?? []
  const position = siblings.findIndex((sibling) => sibling.id === over.id)
  const previous = siblings
    .slice(0, position)
    .filter((sibling) => sibling.id !== moving.id)
    .pop()
  return { parentId: over.parent_id, afterId: previous ? previous.id : null }
}
