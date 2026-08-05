import { invoke } from "@tauri-apps/api/core";

// Cached per-launch, but resettable once the user (re)configures the server —
// see `resetConnectionCache`, called after `setServerConfig` succeeds.
let tokenPromise: Promise<string | null> | null = null;
let urlPromise: Promise<string | null> | null = null;

function getToken(): Promise<string | null> {
  if (!tokenPromise) tokenPromise = invoke<string | null>("get_server_token");
  return tokenPromise;
}

function getBaseUrl(): Promise<string | null> {
  if (!urlPromise) {
    urlPromise = invoke<{ url: string | null; has_token: boolean }>("get_server_config").then(
      (cfg) => cfg.url,
    );
  }
  return urlPromise;
}

export function resetConnectionCache(): void {
  tokenPromise = null;
  urlPromise = null;
}

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "ApiError";
  }
}

/** Thrown when no server URL/token has been configured yet — distinct from a
 *  real network/API failure so the UI can show the connection settings
 *  screen instead of a generic error. */
export class NotConfiguredError extends Error {
  constructor() {
    super("No server configured yet");
    this.name = "NotConfiguredError";
  }
}

export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const [token, baseUrl] = await Promise.all([getToken(), getBaseUrl()]);
  if (!token || !baseUrl) {
    throw new NotConfiguredError();
  }
  const headers: Record<string, string> = {
    Authorization: `Bearer ${token}`,
    ...(init.body ? { "Content-Type": "application/json" } : {}),
    ...((init.headers as Record<string, string>) ?? {}),
  };
  const res = await fetch(`${baseUrl}${path}`, { ...init, headers });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const body = await res.json();
      if (body && typeof body.error === "string") message = body.error;
    } catch {
      // non-JSON error body — fall back to statusText
    }
    throw new ApiError(res.status, message);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
