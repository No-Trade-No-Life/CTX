import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { AuthMiniProvider } from "auth-mini-react-components"
import { HashRouter } from "react-router-dom"

import "./index.css"
import App from "./App.tsx"
import { ThemeProvider } from "@/components/theme-provider.tsx"

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 5_000 } },
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <AuthMiniProvider
          authMiniBaseUrl="https://auth.ntnl.io"
          audiences={["ctx.ntnl.io", "linkit.ntnl.io"]}
          autoRedirectToLogin={false}
        >
          <HashRouter>
            <App />
          </HashRouter>
        </AuthMiniProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>
)
