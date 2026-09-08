import {
  createContext,
  useContext,
  useEffect,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react"

export type Locale = "zh" | "en"

const storageKey = "ctx.locale"

const defineCopy = <T extends Record<string, string>>(translations: T) =>
  translations

const englishCopy = defineCopy({
  language: "Language",
  languageChinese: "Chinese",
  languageEnglish: "English",
  navigationWorkspace: "Workspace",
  navigationOverview: "Overview",
  navigationContexts: "Contexts",
  navigationPublishing: "Publishing",
  navigationDocsBlog: "Docs & blog",
  navigationSystem: "System",
  navigationAdministration: "Administration",
  root: "Root",
  refreshWorkspace: "Refresh workspace",
  pageTitleWorkspace: "Workspace",
  pageTitleContextEditor: "Context editor",
  pageTitleAdministration: "Administration",
  contextsTitle: "Your Contexts",
  contextsDescription:
    "A Context gives your Markdown a deliberate boundary: its documents, editorial rules, AI work, and published surface.",
  newContext: "New Context",
  emptyContextTitle: "Start with one bounded idea",
  emptyContextDescription:
    "Create a Context for a product, research area, personal wiki, or publication. Markdown stays the source of truth.",
  createContext: "Create Context",
  noDescription: "No description yet.",
  documentCount: "{count} documents",
  visibilityPrivate: "Private",
  visibilityPublic: "Public",
  openContext: "Open Context",
  noEditorialDescription: "No editorial description",
  document: "Document",
  selectDocument: "Select document",
  documents: "Documents",
  statusDraft: "Draft",
  statusPublished: "Published",
  save: "Save",
  publish: "Publish",
  title: "Title",
  slug: "Slug",
  markdown: "Markdown",
  markdownDescription:
    "Saving writes a new immutable revision. AI never replaces this source without an explicit save.",
  savedNewRevision: "Saved as a new revision",
  publishedCurrentRevision: "Published current revision",
  contextAi: "Context AI",
  contextAiDescription: "Works from this revision and its Context rules.",
  aiActions: "Actions",
  aiOutput: "Output",
  extractMetadata: "Extract metadata",
  summarizeDocument: "Summarize document",
  translationLanguage: "Translation language",
  translateMarkdown: "Translate Markdown",
  aiWorkRecorded: "AI work recorded with this revision",
  appliedAiMetadata: "Applied AI metadata",
  applyMetadata: "Apply metadata",
  saveTranslationDraft: "Save translation as draft",
  metadataSaved: "Metadata saved on a new revision",
  translationSaved: "Translation saved as a new draft",
  aiOutputEmpty:
    "Run a metadata, summary, or translation task to inspect an auditable AI output.",
  aiPlaceholder: "Select a document to work from its current revision.",
  noDocumentTitle: "Create the first document",
  noDocumentDescription:
    "Use Docs for stable navigation or Blog for chronological writing. Both remain Markdown revisions in this Context.",
  newDocument: "New document",
  contextCreated: "Context created",
  contextDialogTitle: "New Context",
  contextDialogDescription:
    "A Context defines the documents and editorial instructions that AI may use together.",
  name: "Name",
  publicSlug: "Public slug",
  slugDescription: "Lowercase letters, numbers, and hyphens only.",
  description: "Description",
  aiInstructions: "AI instructions",
  aiInstructionsDescription:
    "Voice, terminology, intended audience, and evidence rules for this Context.",
  visibility: "Visibility",
  documentCreated: "Document created",
  documentDialogTitle: "New document",
  documentDialogDescription:
    "Create a Markdown source document. Publishing is a separate action.",
  sourceLanguage: "Source language",
  surface: "Surface",
  surfaceDocs: "Docs",
  surfaceBlog: "Blog",
  createDocument: "Create document",
  rootConfigured: "Root user configured",
  initializeTitle: "Initialize CTX",
  initializeDescription:
    "The first authenticated Auth Mini user becomes CTX's root administrator. Sign-in stays owned by Auth Mini.",
  becomeRoot: "Become root administrator",
  aiConfigurationSaved: "AI configuration saved",
  administrationDescription:
    "Instance-only controls stay separate from Context authoring.",
  openAiRouting: "OpenAI routing",
  openAiRoutingDescription:
    "CTX calls OpenAI-compatible chat completions through openai.ntnl.io. The API key is encrypted on this host and never returned to the browser.",
  configured: "configured",
  needsKey: "needs key",
  baseUrl: "Base URL",
  model: "Model",
  apiKey: "API key",
  apiKeyPlaceholder: "Leave blank to keep the encrypted key",
  apiKeyDescription:
    "Only root can update this secret. CTX will never echo it back.",
  saveAiConfiguration: "Save AI configuration",
  publishedDocumentNotFound: "Published document not found",
  requestFailed: "CTX could not complete this request",
})

type TranslationKey = keyof typeof englishCopy
type Copy = Record<TranslationKey, string>

const chineseCopy: Copy = {
  language: "语言",
  languageChinese: "中文",
  languageEnglish: "英文",
  navigationWorkspace: "工作区",
  navigationOverview: "概览",
  navigationContexts: "Contexts",
  navigationPublishing: "发布",
  navigationDocsBlog: "文档与博客",
  navigationSystem: "系统",
  navigationAdministration: "管理",
  root: "根管理员",
  refreshWorkspace: "刷新工作区",
  pageTitleWorkspace: "工作区",
  pageTitleContextEditor: "Context 编辑器",
  pageTitleAdministration: "管理",
  contextsTitle: "你的 Contexts",
  contextsDescription:
    "Context 为 Markdown 划定清晰边界：其中包含文档、编辑规则、AI 工作和发布面。",
  newContext: "新建 Context",
  emptyContextTitle: "从一个有边界的想法开始",
  emptyContextDescription:
    "为产品、研究领域、个人知识库或出版物创建 Context。Markdown 始终是事实来源。",
  createContext: "创建 Context",
  noDescription: "暂无说明。",
  documentCount: "{count} 篇文档",
  visibilityPrivate: "私有",
  visibilityPublic: "公开",
  openContext: "打开 Context",
  noEditorialDescription: "暂无编辑说明",
  document: "文档",
  selectDocument: "选择文档",
  documents: "文档",
  statusDraft: "草稿",
  statusPublished: "已发布",
  save: "保存",
  publish: "发布",
  title: "标题",
  slug: "标识符",
  markdown: "Markdown",
  markdownDescription:
    "保存会创建新的不可变修订版本。除非明确保存，AI 不会替换此源内容。",
  savedNewRevision: "已保存为新的修订版本",
  publishedCurrentRevision: "已发布当前修订版本",
  contextAi: "Context AI",
  contextAiDescription: "基于此修订版本和它所属 Context 的规则工作。",
  aiActions: "操作",
  aiOutput: "输出",
  extractMetadata: "提取元数据",
  summarizeDocument: "总结文档",
  translationLanguage: "目标语言",
  translateMarkdown: "翻译 Markdown",
  aiWorkRecorded: "AI 工作已记录到此修订版本",
  appliedAiMetadata: "已应用 AI 元数据",
  applyMetadata: "应用元数据",
  saveTranslationDraft: "将翻译另存为草稿",
  metadataSaved: "元数据已保存为新的修订版本",
  translationSaved: "翻译已另存为草稿",
  aiOutputEmpty: "运行元数据、总结或翻译任务，以查看可审查的 AI 输出。",
  aiPlaceholder: "选择文档后，可基于其当前修订版本开展工作。",
  noDocumentTitle: "创建第一篇文档",
  noDocumentDescription:
    "Docs 用于稳定导航，Blog 用于按时间写作。两者都作为此 Context 中的 Markdown 修订版本保存。",
  newDocument: "新建文档",
  contextCreated: "Context 已创建",
  contextDialogTitle: "新建 Context",
  contextDialogDescription: "Context 定义了 AI 可以共同使用的文档和编辑指令。",
  name: "名称",
  publicSlug: "公开标识符",
  slugDescription: "仅限小写字母、数字和连字符。",
  description: "说明",
  aiInstructions: "AI 指令",
  aiInstructionsDescription: "此 Context 的语调、术语、目标读者和证据规则。",
  visibility: "可见性",
  documentCreated: "文档已创建",
  documentDialogTitle: "新建文档",
  documentDialogDescription: "创建一篇 Markdown 源文档。发布是单独的操作。",
  sourceLanguage: "源语言",
  surface: "发布面",
  surfaceDocs: "文档",
  surfaceBlog: "博客",
  createDocument: "创建文档",
  rootConfigured: "根管理员已配置",
  initializeTitle: "初始化 CTX",
  initializeDescription:
    "第一位完成认证的 Auth Mini 用户将成为 CTX 根管理员。登录仍由 Auth Mini 负责。",
  becomeRoot: "成为根管理员",
  aiConfigurationSaved: "AI 配置已保存",
  administrationDescription: "实例级控制与 Context 编写保持分离。",
  openAiRouting: "OpenAI 路由",
  openAiRoutingDescription:
    "CTX 通过 openai.ntnl.io 调用兼容 OpenAI 的聊天补全。API 密钥在此主机上加密，绝不会返回浏览器。",
  configured: "已配置",
  needsKey: "需要密钥",
  baseUrl: "基础 URL",
  model: "模型",
  apiKey: "API 密钥",
  apiKeyPlaceholder: "留空即可保留已加密的密钥",
  apiKeyDescription: "只有根管理员可以更新此密钥。CTX 不会将其返回。",
  saveAiConfiguration: "保存 AI 配置",
  publishedDocumentNotFound: "未找到已发布的文档",
  requestFailed: "CTX 无法完成此请求",
}

export const copy: Record<Locale, Copy> = {
  zh: chineseCopy,
  en: englishCopy,
}

export function initialLocale(): Locale {
  const savedLocale = window.localStorage.getItem(storageKey)
  if (savedLocale === "zh" || savedLocale === "en") return savedLocale
  return window.navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en"
}

type I18n = {
  locale: Locale
  setLocale: Dispatch<SetStateAction<Locale>>
  t: (key: TranslationKey) => string
}

const I18nContext = createContext<I18n | null>(null)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<Locale>(initialLocale)

  useEffect(() => {
    window.localStorage.setItem(storageKey, locale)
    document.documentElement.lang = locale
  }, [locale])

  return (
    <I18nContext.Provider
      value={{ locale, setLocale, t: (key) => copy[locale][key] }}
    >
      {children}
    </I18nContext.Provider>
  )
}

export function useI18n() {
  const i18n = useContext(I18nContext)
  if (!i18n) throw new Error("useI18n must be used within I18nProvider")
  return i18n
}
