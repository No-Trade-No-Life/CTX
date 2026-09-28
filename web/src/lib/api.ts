export class ApiRequestError extends Error {
  readonly status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = "ApiRequestError"
    this.status = status
  }
}

export async function request<T>(
  path: string,
  accessToken?: string,
  init?: RequestInit
): Promise<T> {
  const headers = new Headers(init?.headers)
  headers.set("Content-Type", "application/json")
  if (accessToken) headers.set("Authorization", `Bearer ${accessToken}`)
  const response = await fetch(path, { ...init, headers })
  if (response.status === 204) return undefined as T
  const body = (await response.json()) as T & { error?: string }
  if (!response.ok)
    throw new ApiRequestError(body.error ?? "Request failed", response.status)
  return body
}

export async function upload<T>(
  path: string,
  content: Blob,
  accessToken: string
): Promise<T> {
  const headers = new Headers({
    "Content-Type": content.type || "application/octet-stream",
    Authorization: `Bearer ${accessToken}`,
  })
  const response = await fetch(path, {
    method: "POST",
    headers,
    body: content,
  })
  const body = (await response.json().catch(() => ({}))) as T & {
    error?: string
  }
  if (!response.ok) throw new Error(body.error ?? "Image upload failed")
  return body
}
