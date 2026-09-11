import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { AuthMiniProvider } from "auth-mini-react-components"
import { LinkitProvider } from "linkit-react-components"
import { HashRouter, useLocation } from "react-router-dom"

import "./index.css"
import App, { PublicApp } from "./App.tsx"
import { ThemeProvider } from "@/components/theme-provider.tsx"
import { I18nProvider, useI18n } from "@/lib/i18n.tsx"

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 5_000 } },
})

function RouteBoundary() {
  const location = useLocation()
  const { locale } = useI18n()

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
        <LinkitProvider lang={locale} linkitBaseUrl="https://linkit.ntnl.io">
          <PublicApp />
        </LinkitProvider>
      </AuthMiniProvider>
    )

  return (
    <AuthMiniProvider
      authMiniBaseUrl="https://auth.ntnl.io"
      audiences={["ctx.ntnl.io", "linkit.ntnl.io"]}
      autoRedirectToLogin
    >
      <LinkitProvider lang={locale} linkitBaseUrl="https://linkit.ntnl.io">
        <App />
      </LinkitProvider>
    </AuthMiniProvider>
  )
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <I18nProvider>
          <HashRouter>
            <RouteBoundary />
          </HashRouter>
        </I18nProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>
)
