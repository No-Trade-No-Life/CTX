import { StrictMode, useEffect } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { AuthMiniProvider } from "auth-mini-react-components"
import { LinkitProvider, useLinkit } from "linkit-react-components"
import { HashRouter, useLocation } from "react-router-dom"

import "./index.css"
import App, { PublicApp } from "./App.tsx"
import { applyFavicon } from "@/lib/favicon"
import { I18nProvider, localeFor } from "@/lib/i18n.tsx"

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 5_000 } },
})

// The Linkit profile owns the language preference. Until one is stored, or
// while the viewer is signed out, Linkit falls back to the browser language.
const fallbackLanguage = localeFor(window.navigator.language)

function RouteBoundary() {
  const location = useLocation()

  if (
    location.pathname === "/" ||
    location.pathname.startsWith("/square") ||
    location.pathname.startsWith("/p/") ||
    location.pathname.startsWith("/u/")
  )
    return (
      <AuthMiniProvider
        authMiniBaseUrl="https://auth.ntnl.io"
        audiences={["ctx.ntnl.io", "linkit.ntnl.io"]}
        autoRedirectToLogin={false}
      >
        <LinkitProvider
          lang={fallbackLanguage}
          linkitBaseUrl="https://linkit.ntnl.io"
        >
          <I18nProvider>
            <FaviconSync />
            <PublicApp />
          </I18nProvider>
        </LinkitProvider>
      </AuthMiniProvider>
    )

  return (
    <AuthMiniProvider
      authMiniBaseUrl="https://auth.ntnl.io"
      audiences={["ctx.ntnl.io", "linkit.ntnl.io"]}
      autoRedirectToLogin
    >
      <LinkitProvider
        lang={fallbackLanguage}
        linkitBaseUrl="https://linkit.ntnl.io"
      >
        <I18nProvider>
          <FaviconSync />
          <App />
        </I18nProvider>
      </LinkitProvider>
    </AuthMiniProvider>
  )
}

// Keep the favicon in step with the resolved Linkit theme without a reload.
function FaviconSync() {
  const { resolvedTheme } = useLinkit()
  useEffect(() => {
    applyFavicon(resolvedTheme)
  }, [resolvedTheme])
  return null
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <RouteBoundary />
      </HashRouter>
    </QueryClientProvider>
  </StrictMode>
)
