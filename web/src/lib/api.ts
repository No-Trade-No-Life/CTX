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
  if (!response.ok) throw new Error(body.error ?? "Request failed")
  return body
}
