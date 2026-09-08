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
  appName: "CTX",
  language: "Language",
  languageChinese: "Chinese",
  languageEnglish: "English",
  navigationDocuments: "My documents",
  navigationSquare: "Square",
  navigationAdministration: "Administration",
  root: "Root",
  refreshDocuments: "Refresh documents",
  pageTitleDocuments: "My documents",
  pageTitleEditor: "Document editor",
  pageTitleAdministration: "Administration",
  documentsTitle: "Your documents",
  documentsDescription:
    "Write in Markdown, keep every revision, and choose what to share.",
  newDocument: "New document",
  emptyDocumentsTitle: "Write the first one",
  emptyDocumentsDescription:
    "Start with a Markdown document. Save creates a revision; publishing makes it readable in the square.",
  createDocument: "Create document",
  openDocument: "Open document",
  backToDocuments: "Back to documents",
  updatedOn: "Updated {date}",
  publishedOn: "Published {date}",
  document: "Document",
  title: "Title",
  titlePlaceholder: "Give this document a title",
  sourceLanguage: "Original language",
  sourceLanguagePlaceholder: "For example, zh-CN",
  sourceLanguageDescription:
    "Use a BCP 47 tag. Leave it as und to detect the source language when publishing.",
  sourceLanguageAuto: "Detect on publish",
  markdown: "Markdown",
  markdownDescription:
    "Markdown is the source. Each save writes an immutable revision.",
  draft: "Draft",
  published: "Published",
  save: "Save",
  publish: "Publish",
  savedNewRevision: "Saved as a new revision",
  publishedCurrentRevision: "Published current revision",
  documentCreated: "Document created",
  revision: "Revision",
  viewPublicArticle: "View public article",
  documentNotFound: "Document not found",
  documentAi: "AI for this document",
  documentAiDescription:
    "AI works from the current revision and always leaves a reviewable result.",
  detectSourceLanguage: "Detect original language",
  applyDetectedLanguage: "Use detected language",
  detectedSourceLanguage: "Detected original language: {language}",
  saveBeforeAiTitle: "Save before using AI",
  saveBeforeAi:
    "AI uses the latest saved revision. Save your changes before starting an AI task.",
  aiActions: "Actions",
  aiOutput: "Output",
  extractMetadata: "Extract metadata",
  summarizeDocument: "Summarize document",
  aiWorkRecorded: "AI work recorded with this revision",
  appliedAiMetadata: "Applied AI metadata",
  applyMetadata: "Apply metadata",
  metadataSaved: "Metadata saved as a new revision",
  aiOutputEmpty:
    "Run a task to inspect an auditable AI result without changing your Markdown.",
  languageMatrix: "Publication languages",
  languageMatrixDescription:
    "Published documents are automatically translated into these languages. The original is always retained.",
  languageMatrixPlaceholder: "zh-CN, en-US, ja-JP, es-ES",
  saveLanguagePreferences: "Save publication languages",
  languagePreferencesSaved: "Publication languages saved",
  articleLanguage: "Article language",
  squareTitle: "The square",
  squareDescription: "Published Markdown from everyone writing with CTX.",
  squareEmptyTitle: "Nothing has been published yet",
  squareEmptyDescription:
    "When someone publishes a document, it will appear here for everyone to read.",
  startWriting: "Start writing",
  readArticle: "Read article",
  backToSquare: "Back to the square",
  publishedDocumentNotFound: "Published document not found",
  rootConfigured: "Root user configured",
  initializeTitle: "Initialize CTX",
  initializeDescription:
    "The first authenticated Auth Mini user becomes CTX's root administrator. Sign-in stays owned by Auth Mini.",
  becomeRoot: "Become root administrator",
  aiConfigurationSaved: "AI configuration saved",
  administrationDescription:
    "Instance-only controls stay separate from writing and publishing.",
  openAiRouting: "OpenAI routing",
  openAiRoutingDescription:
    "CTX calls an OpenAI-compatible Responses API. The API key is encrypted on this host and never returned to the browser.",
  configured: "configured",
  needsKey: "needs key",
  baseUrl: "Base URL",
  model: "Model",
  apiKey: "API key",
  apiKeyPlaceholder: "Leave blank to keep the encrypted key",
  apiKeyDescription:
    "Only root can update this secret. CTX will never echo it back.",
  saveAiConfiguration: "Save AI configuration",
  requestFailed: "CTX could not complete this request",
})

type TranslationKey = keyof typeof englishCopy
type Copy = Record<TranslationKey, string>

const chineseCopy: Copy = {
  appName: "CTX",
  language: "语言",
  languageChinese: "中文",
  languageEnglish: "英文",
  navigationDocuments: "我的文档",
  navigationSquare: "广场",
  navigationAdministration: "管理",
  root: "根管理员",
  refreshDocuments: "刷新文档",
  pageTitleDocuments: "我的文档",
  pageTitleEditor: "文档编辑器",
  pageTitleAdministration: "管理",
  documentsTitle: "你的文档",
  documentsDescription: "用 Markdown 写作，保留每次修订，并选择哪些内容公开。",
  newDocument: "新建文档",
  emptyDocumentsTitle: "写下第一篇",
  emptyDocumentsDescription:
    "从一篇 Markdown 文档开始。保存会创建修订版本；发布后，所有人都能在广场阅读。",
  createDocument: "创建文档",
  openDocument: "打开文档",
  backToDocuments: "返回文档",
  updatedOn: "更新于 {date}",
  publishedOn: "发布于 {date}",
  document: "文档",
  title: "标题",
  titlePlaceholder: "给这篇文档取个标题",
  sourceLanguage: "原文语言",
  sourceLanguagePlaceholder: "例如 zh-CN",
  sourceLanguageDescription:
    "使用 BCP 47 语言标签。保留 und 会在发布时自动识别原文语言。",
  sourceLanguageAuto: "发布时自动识别",
  markdown: "Markdown",
  markdownDescription:
    "Markdown 是事实来源。每次保存都会写入一个不可变修订版本。",
  draft: "草稿",
  published: "已发布",
  save: "保存",
  publish: "发布",
  savedNewRevision: "已保存为新的修订版本",
  publishedCurrentRevision: "已发布当前修订版本",
  documentCreated: "文档已创建",
  revision: "修订版本",
  viewPublicArticle: "查看公开文章",
  documentNotFound: "未找到文档",
  documentAi: "文档 AI",
  documentAiDescription: "AI 只基于当前修订版本工作，结果始终可供审查。",
  detectSourceLanguage: "识别原文语言",
  applyDetectedLanguage: "采用识别结果",
  detectedSourceLanguage: "识别出的原文语言：{language}",
  saveBeforeAiTitle: "先保存再使用 AI",
  saveBeforeAi:
    "AI 只会使用最近保存的修订版本。请先保存当前修改，再运行 AI 任务。",
  aiActions: "操作",
  aiOutput: "输出",
  extractMetadata: "提取元数据",
  summarizeDocument: "总结文档",
  aiWorkRecorded: "AI 工作已记录到此修订版本",
  appliedAiMetadata: "已应用 AI 元数据",
  applyMetadata: "应用元数据",
  metadataSaved: "元数据已保存为新的修订版本",
  aiOutputEmpty: "运行任务以查看可审查的 AI 结果，Markdown 不会被自动改写。",
  languageMatrix: "发布语言矩阵",
  languageMatrixDescription: "发布时会自动派生这些语言的译文，原文始终保留。",
  languageMatrixPlaceholder: "zh-CN, en-US, ja-JP, es-ES",
  saveLanguagePreferences: "保存发布语言",
  languagePreferencesSaved: "发布语言已保存",
  articleLanguage: "文章语言",
  squareTitle: "广场",
  squareDescription: "所有使用 CTX 写作者已经发布的 Markdown 文章。",
  squareEmptyTitle: "还没有文章发布",
  squareEmptyDescription: "任何人发布文档后，它都会出现在这里，供所有人阅读。",
  startWriting: "开始写作",
  readArticle: "阅读文章",
  backToSquare: "返回广场",
  publishedDocumentNotFound: "未找到已发布的文档",
  rootConfigured: "根管理员已配置",
  initializeTitle: "初始化 CTX",
  initializeDescription:
    "第一位完成认证的 Auth Mini 用户将成为 CTX 根管理员。登录仍由 Auth Mini 负责。",
  becomeRoot: "成为根管理员",
  aiConfigurationSaved: "AI 配置已保存",
  administrationDescription: "实例级控制与写作和发布保持分离。",
  openAiRouting: "OpenAI 路由",
  openAiRoutingDescription:
    "CTX 调用兼容 OpenAI 的 Responses API。API 密钥在此主机上加密，绝不会返回浏览器。",
  configured: "已配置",
  needsKey: "需要密钥",
  baseUrl: "基础 URL",
  model: "模型",
  apiKey: "API 密钥",
  apiKeyPlaceholder: "留空即可保留已加密的密钥",
  apiKeyDescription: "只有根管理员可以更新此密钥。CTX 不会将其返回。",
  saveAiConfiguration: "保存 AI 配置",
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
