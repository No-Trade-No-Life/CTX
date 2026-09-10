import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { createPortal } from "react-dom"
import Image from "@tiptap/extension-image"
import Placeholder from "@tiptap/extension-placeholder"
import TableCell from "@tiptap/extension-table-cell"
import TableHeader from "@tiptap/extension-table-header"
import TableRow from "@tiptap/extension-table-row"
import TaskItem from "@tiptap/extension-task-item"
import TaskList from "@tiptap/extension-task-list"
import Typography from "@tiptap/extension-typography"
import { Table } from "@tiptap/extension-table"
import { Markdown } from "@tiptap/markdown"
import { BubbleMenu } from "@tiptap/react/menus"
import {
  EditorContent,
  type Editor,
  useEditor,
  useEditorState,
} from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  BoldIcon,
  Code2Icon,
  FileCode2Icon,
  Heading1Icon,
  Heading2Icon,
  Heading3Icon,
  ImageIcon,
  LinkIcon,
  ListChecksIcon,
  ListIcon,
  ListOrderedIcon,
  MinusIcon,
  PilcrowIcon,
  QuoteIcon,
  Redo2Icon,
  StrikethroughIcon,
  Table2Icon,
  Undo2Icon,
  type LucideIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"

type EditorMode = "write" | "markdown"
type EmbedKind = "image" | "link"

type MarkdownEditorProps = {
  value: string
  disabled?: boolean
  onChange: (value: string) => void
  writeLabel: string
  markdownLabel: string
  placeholder: string
  commandsLabel: string
  insertLinkLabel: string
  insertImageLabel: string
  insertUrlLabel: string
  insertLabel: string
  cancelLabel: string
}

type SlashCommand = {
  id: string
  label: string
  description: string
  icon: LucideIcon
  run: (editor: Editor) => void
}

type SlashMenu = {
  from: number
  query: string
  top: number
  left: number
}

const slashCommands: SlashCommand[] = [
  {
    id: "text",
    label: "Text",
    description: "Start writing with plain text",
    icon: PilcrowIcon,
    run: (editor) => editor.chain().focus().setParagraph().run(),
  },
  {
    id: "heading-1",
    label: "Heading 1",
    description: "Large section heading",
    icon: Heading1Icon,
    run: (editor) => editor.chain().focus().toggleHeading({ level: 1 }).run(),
  },
  {
    id: "heading-2",
    label: "Heading 2",
    description: "Medium section heading",
    icon: Heading2Icon,
    run: (editor) => editor.chain().focus().toggleHeading({ level: 2 }).run(),
  },
  {
    id: "bulleted-list",
    label: "Bulleted list",
    description: "A simple unordered list",
    icon: ListIcon,
    run: (editor) => editor.chain().focus().toggleBulletList().run(),
  },
  {
    id: "task-list",
    label: "To-do list",
    description: "Track a checked or open task",
    icon: ListChecksIcon,
    run: (editor) => editor.chain().focus().toggleTaskList().run(),
  },
  {
    id: "quote",
    label: "Quote",
    description: "Call out a quotation",
    icon: QuoteIcon,
    run: (editor) => editor.chain().focus().toggleBlockquote().run(),
  },
  {
    id: "code",
    label: "Code block",
    description: "Code, Mermaid, or a literal block",
    icon: FileCode2Icon,
    run: (editor) => editor.chain().focus().toggleCodeBlock().run(),
  },
  {
    id: "table",
    label: "Table",
    description: "A three-column Markdown table",
    icon: Table2Icon,
    run: (editor) =>
      editor
        .chain()
        .focus()
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run(),
  },
]

function markdownFromEditor(editor: Editor) {
  return editor.getMarkdown().replace(/\n$/, "")
}

function findSlashMenu(editor: Editor): SlashMenu | null {
  const { from, empty } = editor.state.selection
  if (!empty) return null

  const start = Math.max(1, from - 48)
  const text = editor.state.doc.textBetween(start, from, "\n", "\0")
  const match = /(?:^|\s)\/([\p{L}\p{N}-]*)$/u.exec(text)
  if (!match) return null

  const slashOffset = match[0].lastIndexOf("/")
  const position = start + text.length - match[0].length + slashOffset
  const coordinates = editor.view.coordsAtPos(from)
  const menuWidth = 288
  const menuHeight = 344
  const top =
    coordinates.bottom + menuHeight < window.innerHeight
      ? coordinates.bottom + 8
      : Math.max(12, coordinates.top - menuHeight - 8)

  return {
    from: position,
    query: match[1].toLocaleLowerCase(),
    top,
    left: Math.min(
      Math.max(12, coordinates.left),
      window.innerWidth - menuWidth - 12
    ),
  }
}

function EditorButton({
  active = false,
  label,
  onClick,
  children,
}: {
  active?: boolean
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <Button
      type="button"
      variant={active ? "secondary" : "ghost"}
      size="icon-sm"
      aria-label={label}
      aria-pressed={active}
      title={label}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
    >
      {children}
    </Button>
  )
}

function ToolbarSeparator() {
  return <span aria-hidden="true" className="mx-0.5 h-5 w-px bg-border" />
}

export function MarkdownEditor({
  value,
  disabled = false,
  onChange,
  writeLabel,
  markdownLabel,
  placeholder,
  commandsLabel,
  insertLinkLabel,
  insertImageLabel,
  insertUrlLabel,
  insertLabel,
  cancelLabel,
}: MarkdownEditorProps) {
  const [mode, setMode] = useState<EditorMode>("write")
  const [embedKind, setEmbedKind] = useState<EmbedKind | null>(null)
  const [embedUrl, setEmbedUrl] = useState("")
  const [slashMenu, setSlashMenu] = useState<SlashMenu | null>(null)
  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
          link: {
            autolink: true,
            defaultProtocol: "https",
            openOnClick: false,
            HTMLAttributes: { rel: "noreferrer" },
          },
          underline: false,
        }),
        Markdown.configure({ indentation: { style: "space", size: 2 } }),
        Image.configure({ allowBase64: false, inline: false }),
        TaskList,
        TaskItem.configure({ nested: true }),
        Table.configure({ resizable: false }),
        TableRow,
        TableHeader,
        TableCell,
        Placeholder.configure({ placeholder }),
        Typography,
      ],
      content: value,
      contentType: "markdown",
      editable: !disabled,
      immediatelyRender: false,
      onUpdate: ({ editor: currentEditor }) => {
        onChange(markdownFromEditor(currentEditor))
        setSlashMenu(findSlashMenu(currentEditor))
      },
      onSelectionUpdate: ({ editor: currentEditor }) => {
        setSlashMenu(findSlashMenu(currentEditor))
      },
    },
    [placeholder]
  )

  const editorState = useEditorState({
    editor,
    selector: ({ editor: currentEditor }) => ({
      bold: currentEditor?.isActive("bold") ?? false,
      italic: currentEditor?.isActive("italic") ?? false,
      strike: currentEditor?.isActive("strike") ?? false,
      code: currentEditor?.isActive("code") ?? false,
      heading1: currentEditor?.isActive("heading", { level: 1 }) ?? false,
      heading2: currentEditor?.isActive("heading", { level: 2 }) ?? false,
      heading3: currentEditor?.isActive("heading", { level: 3 }) ?? false,
      bulletList: currentEditor?.isActive("bulletList") ?? false,
      orderedList: currentEditor?.isActive("orderedList") ?? false,
      taskList: currentEditor?.isActive("taskList") ?? false,
      blockquote: currentEditor?.isActive("blockquote") ?? false,
      codeBlock: currentEditor?.isActive("codeBlock") ?? false,
      canUndo: currentEditor?.can().undo() ?? false,
      canRedo: currentEditor?.can().redo() ?? false,
    }),
  })

  useEffect(() => {
    if (!editor || mode !== "write") return
    if (markdownFromEditor(editor) === value) return
    editor.commands.setContent(value, {
      contentType: "markdown",
      emitUpdate: false,
    })
  }, [editor, mode, value])

  useEffect(() => {
    editor?.setEditable(!disabled)
  }, [disabled, editor])

  useEffect(() => {
    const clearSlashMenu = () => setSlashMenu(null)
    window.addEventListener("resize", clearSlashMenu)
    window.addEventListener("scroll", clearSlashMenu, true)
    return () => {
      window.removeEventListener("resize", clearSlashMenu)
      window.removeEventListener("scroll", clearSlashMenu, true)
    }
  }, [])

  const filteredCommands = useMemo(() => {
    const query = slashMenu?.query ?? ""
    return slashCommands.filter((command) =>
      `${command.label} ${command.description}`
        .toLocaleLowerCase()
        .includes(query)
    )
  }, [slashMenu?.query])

  const runSlashCommand = useCallback(
    (command: SlashCommand) => {
      if (!editor || !slashMenu) return
      editor
        .chain()
        .focus()
        .deleteRange({ from: slashMenu.from, to: editor.state.selection.from })
        .run()
      command.run(editor)
      setSlashMenu(null)
    },
    [editor, slashMenu]
  )

  const insertEmbed = () => {
    const url = embedUrl.trim()
    if (!editor || !embedKind || !url) return
    const chain = editor.chain().focus()
    if (embedKind === "image") chain.setImage({ src: url, alt: "" })
    else if (editor.state.selection.empty) {
      chain.insertContent({
        type: "text",
        text: url,
        marks: [
          {
            type: "link",
            attrs: { href: url, target: null, rel: "noreferrer" },
          },
        ],
      })
    } else chain.setLink({ href: url, target: null, rel: "noreferrer" })
    chain.run()
    setEmbedUrl("")
    setEmbedKind(null)
  }

  if (!editor || !editorState) return null

  return (
    <div className="markdown-editor min-w-0" data-mode={mode}>
      <div
        role="toolbar"
        aria-label={commandsLabel}
        aria-orientation="horizontal"
        className="markdown-editor__toolbar"
      >
        <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
          <EditorButton
            label="Undo"
            onClick={() => editor.chain().focus().undo().run()}
          >
            <Undo2Icon />
          </EditorButton>
          <EditorButton
            label="Redo"
            onClick={() => editor.chain().focus().redo().run()}
          >
            <Redo2Icon />
          </EditorButton>
          <ToolbarSeparator />
          <EditorButton
            active={editorState.bold}
            label="Bold"
            onClick={() => editor.chain().focus().toggleBold().run()}
          >
            <BoldIcon />
          </EditorButton>
          <EditorButton
            active={editorState.italic}
            label="Italic"
            onClick={() => editor.chain().focus().toggleItalic().run()}
          >
            <span className="font-serif text-base italic">I</span>
          </EditorButton>
          <EditorButton
            active={editorState.strike}
            label="Strikethrough"
            onClick={() => editor.chain().focus().toggleStrike().run()}
          >
            <StrikethroughIcon />
          </EditorButton>
          <EditorButton
            active={editorState.code}
            label="Inline code"
            onClick={() => editor.chain().focus().toggleCode().run()}
          >
            <Code2Icon />
          </EditorButton>
          <ToolbarSeparator />
          <EditorButton
            active={editorState.heading1}
            label="Heading 1"
            onClick={() =>
              editor.chain().focus().toggleHeading({ level: 1 }).run()
            }
          >
            <Heading1Icon />
          </EditorButton>
          <EditorButton
            active={editorState.heading2}
            label="Heading 2"
            onClick={() =>
              editor.chain().focus().toggleHeading({ level: 2 }).run()
            }
          >
            <Heading2Icon />
          </EditorButton>
          <EditorButton
            active={editorState.heading3}
            label="Heading 3"
            onClick={() =>
              editor.chain().focus().toggleHeading({ level: 3 }).run()
            }
          >
            <Heading3Icon />
          </EditorButton>
          <EditorButton
            active={editorState.bulletList}
            label="Bulleted list"
            onClick={() => editor.chain().focus().toggleBulletList().run()}
          >
            <ListIcon />
          </EditorButton>
          <EditorButton
            active={editorState.orderedList}
            label="Numbered list"
            onClick={() => editor.chain().focus().toggleOrderedList().run()}
          >
            <ListOrderedIcon />
          </EditorButton>
          <EditorButton
            active={editorState.taskList}
            label="To-do list"
            onClick={() => editor.chain().focus().toggleTaskList().run()}
          >
            <ListChecksIcon />
          </EditorButton>
          <EditorButton
            active={editorState.blockquote}
            label="Quote"
            onClick={() => editor.chain().focus().toggleBlockquote().run()}
          >
            <QuoteIcon />
          </EditorButton>
          <EditorButton
            active={editorState.codeBlock}
            label="Code block"
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}
          >
            <FileCode2Icon />
          </EditorButton>
          <EditorButton
            label="Horizontal rule"
            onClick={() => editor.chain().focus().setHorizontalRule().run()}
          >
            <MinusIcon />
          </EditorButton>
          <EditorButton
            label="Table"
            onClick={() =>
              editor
                .chain()
                .focus()
                .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
                .run()
            }
          >
            <Table2Icon />
          </EditorButton>
          <ToolbarSeparator />
          <EditorButton
            label={insertLinkLabel}
            onClick={() => setEmbedKind("link")}
          >
            <LinkIcon />
          </EditorButton>
          <EditorButton
            label={insertImageLabel}
            onClick={() => setEmbedKind("image")}
          >
            <ImageIcon />
          </EditorButton>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-1 border-l pl-2">
          <Button
            type="button"
            size="sm"
            variant={mode === "write" ? "secondary" : "ghost"}
            aria-pressed={mode === "write"}
            onClick={() => setMode("write")}
          >
            {writeLabel}
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "markdown" ? "secondary" : "ghost"}
            aria-pressed={mode === "markdown"}
            onClick={() => setMode("markdown")}
          >
            {markdownLabel}
          </Button>
        </div>
      </div>

      {embedKind ? (
        <div className="markdown-editor__embed-form">
          <Input
            autoFocus
            aria-label={insertUrlLabel}
            placeholder={insertUrlLabel}
            value={embedUrl}
            disabled={disabled}
            onChange={(event) => setEmbedUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") insertEmbed()
              if (event.key === "Escape") setEmbedKind(null)
            }}
          />
          <Button
            type="button"
            size="sm"
            disabled={!embedUrl.trim() || disabled}
            onClick={insertEmbed}
          >
            {insertLabel}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={() => setEmbedKind(null)}
          >
            {cancelLabel}
          </Button>
        </div>
      ) : null}

      {mode === "write" ? (
        <>
          <EditorContent editor={editor} className="markdown-editor__canvas" />
          <BubbleMenu
            editor={editor}
            shouldShow={({ state }) => !state.selection.empty}
            className="markdown-editor__bubble-menu"
          >
            <EditorButton
              active={editorState.bold}
              label="Bold"
              onClick={() => editor.chain().focus().toggleBold().run()}
            >
              <BoldIcon />
            </EditorButton>
            <EditorButton
              active={editorState.italic}
              label="Italic"
              onClick={() => editor.chain().focus().toggleItalic().run()}
            >
              <span className="font-serif text-base italic">I</span>
            </EditorButton>
            <EditorButton
              active={editorState.code}
              label="Inline code"
              onClick={() => editor.chain().focus().toggleCode().run()}
            >
              <Code2Icon />
            </EditorButton>
            <EditorButton
              label={insertLinkLabel}
              onClick={() => setEmbedKind("link")}
            >
              <LinkIcon />
            </EditorButton>
          </BubbleMenu>
          {slashMenu && filteredCommands.length > 0
            ? createPortal(
                <div
                  role="listbox"
                  aria-label={commandsLabel}
                  className="markdown-editor__slash-menu"
                  style={{ top: slashMenu.top, left: slashMenu.left }}
                >
                  <p className="px-3 pt-2.5 pb-1 text-xs font-medium text-muted-foreground">
                    {commandsLabel}
                  </p>
                  <div className="max-h-72 overflow-y-auto px-1.5 pb-1.5">
                    {filteredCommands.map((command) => {
                      const Icon = command.icon
                      return (
                        <Button
                          key={command.id}
                          type="button"
                          variant="ghost"
                          className="h-auto w-full justify-start gap-2 px-2 py-2 text-left"
                          onMouseDown={(event) => event.preventDefault()}
                          onClick={() => runSlashCommand(command)}
                        >
                          <span className="grid size-7 shrink-0 place-items-center rounded-md bg-muted">
                            <Icon className="size-3.5" />
                          </span>
                          <span className="min-w-0">
                            <span className="block text-sm font-medium">
                              {command.label}
                            </span>
                            <span className="block truncate text-xs font-normal text-muted-foreground">
                              {command.description}
                            </span>
                          </span>
                        </Button>
                      )
                    })}
                  </div>
                </div>,
                document.body
              )
            : null}
        </>
      ) : (
        <Textarea
          aria-label={markdownLabel}
          className={cn(
            "markdown-editor__source min-h-[32rem] rounded-none border-x-0 border-b-0 font-mono text-sm leading-6",
            disabled && "cursor-not-allowed"
          )}
          value={value}
          disabled={disabled}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </div>
  )
}
