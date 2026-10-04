import type { AuthMiniContextValue } from "auth-mini-react-components"

export type AuthSdk = NonNullable<AuthMiniContextValue["sdk"]>

export class ApiRequestError extends Error {
  readonly status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = "ApiRequestError"
    this.status = status
  }
}

// RECOVERY: the Browser SDK rotates access tokens without re-rendering
// consumers, so tokens are resolved when a request is sent. A stale snapshot
// gets a single retry after a session refresh; a failed refresh falls back to
// the original 401 response so callers surface the real authentication error.
async function fetchWithAuthRetry(
  sdk: AuthSdk | undefined,
  send: (accessToken?: string) => Promise<Response>
): Promise<Response> {
  const response = await send(sdk?.session.getState().accessToken ?? undefined)
  if (response.status !== 401 || !sdk) return response
  const refreshed = await sdk.session.refresh().catch(() => null)
  if (!refreshed?.accessToken) return response
  return send(refreshed.accessToken)
}

export async function request<T>(
  path: string,
  sdk?: AuthSdk,
  init?: RequestInit
): Promise<T> {
  const response = await fetchWithAuthRetry(sdk, (accessToken) => {
    const headers = new Headers(init?.headers)
    headers.set("Content-Type", "application/json")
    if (accessToken) headers.set("Authorization", `Bearer ${accessToken}`)
    return fetch(path, { ...init, headers })
  })
  if (response.status === 204) return undefined as T
  const body = (await response.json()) as T & { error?: string }
  if (!response.ok)
    throw new ApiRequestError(body.error ?? "Request failed", response.status)
  return body
}

export async function upload<T>(
  path: string,
  content: Blob,
  sdk: AuthSdk
): Promise<T> {
  const response = await fetchWithAuthRetry(sdk, (accessToken) => {
    const headers = new Headers({
      "Content-Type": content.type || "application/octet-stream",
    })
    if (accessToken) headers.set("Authorization", `Bearer ${accessToken}`)
    return fetch(path, { method: "POST", headers, body: content })
  })
  const body = (await response.json().catch(() => ({}))) as T & {
    error?: string
  }
  if (!response.ok) throw new Error(body.error ?? "Image upload failed")
  return body
}
