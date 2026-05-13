// Thin REST client for the courrier-server backend. The base URL is
// configurable so the Tauri app can point at a remote Docker deployment.

import type {
  Account,
  AccountPayload,
  AnalyticsOverview,
  CountedString,
  DateBucket,
  FetchStatus,
  ForwarderTree,
  ForwardingBreakdown,
  Message,
  MessageSummary,
  Provider,
  SearchHit,
  TestResult,
} from "@/types/api";

const STORAGE_KEY = "courrier.backendUrl";

export function getBackendUrl(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem(STORAGE_KEY) ?? "";
}

export function setBackendUrl(url: string): void {
  window.localStorage.setItem(STORAGE_KEY, url.trim().replace(/\/+$/, ""));
}

function url(path: string): string {
  const base = getBackendUrl();
  // Leave path-only when empty so Vite's dev proxy / same-origin browser
  // serving (axum embedded SPA) just works.
  return `${base}${path}`;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    Accept: "application/json",
    ...((init.headers as Record<string, string>) ?? {}),
  };
  if (init.method && init.method !== "GET" && init.method !== "HEAD") {
    headers["X-Requested-With"] = "XMLHttpRequest";
    if (init.body && !headers["Content-Type"]) {
      headers["Content-Type"] = "application/json";
    }
  }
  const response = await fetch(url(path), { ...init, headers });
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export const api = {
  health: () => request<{ status: string }>("/api/health"),

  // Providers
  providers: () => request<Provider[]>("/api/providers"),

  // Accounts
  listAccounts: () => request<Account[]>("/api/accounts"),
  getAccount: (id: number) => request<Account>(`/api/accounts/${id}`),
  createAccount: (payload: AccountPayload) =>
    request<Account>("/api/accounts", {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  updateAccount: (id: number, payload: AccountPayload) =>
    request<Account>(`/api/accounts/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload),
    }),
  deleteAccount: (id: number) =>
    request<void>(`/api/accounts/${id}`, { method: "DELETE" }),
  testAccount: (id: number) =>
    request<TestResult>(`/api/accounts/${id}/test`, { method: "POST" }),

  // Sync
  syncAll: () =>
    request<{ started: number[] }>("/api/sync", { method: "POST" }),
  syncOne: (id: number) =>
    request<{ started: boolean; reason?: string; account_id: number }>(
      `/api/sync/${id}`,
      { method: "POST" },
    ),
  syncStatus: () => request<FetchStatus[]>("/api/sync/status"),

  // Messages
  listMessages: (params: {
    account_id?: number;
    mailbox?: string;
    limit?: number;
    offset?: number;
  }) => {
    const q = new URLSearchParams();
    if (params.account_id != null) q.set("account_id", String(params.account_id));
    if (params.mailbox) q.set("mailbox", params.mailbox);
    if (params.limit != null) q.set("limit", String(params.limit));
    if (params.offset != null) q.set("offset", String(params.offset));
    const qs = q.toString();
    return request<MessageSummary[]>(`/api/messages${qs ? `?${qs}` : ""}`);
  },
  getMessage: (id: number) => request<Message>(`/api/messages/${id}`),
  rawMessageUrl: (id: number) => url(`/api/messages/${id}/raw`),

  // Search
  search: (q: string, account_id?: number, limit = 50) => {
    const sp = new URLSearchParams({ q, limit: String(limit) });
    if (account_id != null) sp.set("account_id", String(account_id));
    return request<SearchHit[]>(`/api/search?${sp}`);
  },

  // Analytics
  overview: (account_id?: number) =>
    request<AnalyticsOverview>(
      `/api/analytics/overview${account_id ? `?account_id=${account_id}` : ""}`,
    ),
  topSenders: (account_id?: number, limit = 20) => {
    const sp = new URLSearchParams({ limit: String(limit) });
    if (account_id != null) sp.set("account_id", String(account_id));
    return request<CountedString[]>(`/api/analytics/top-senders?${sp}`);
  },
  topSenderDomains: (account_id?: number, limit = 20) => {
    const sp = new URLSearchParams({ limit: String(limit) });
    if (account_id != null) sp.set("account_id", String(account_id));
    return request<CountedString[]>(`/api/analytics/top-sender-domains?${sp}`);
  },
  forwarding: (account_id?: number, limit = 20) => {
    const sp = new URLSearchParams({ limit: String(limit) });
    if (account_id != null) sp.set("account_id", String(account_id));
    return request<ForwardingBreakdown>(`/api/analytics/forwarding?${sp}`);
  },
  forwardingTree: (account_id?: number) => {
    const sp = new URLSearchParams();
    if (account_id != null) sp.set("account_id", String(account_id));
    const qs = sp.toString();
    return request<ForwarderTree>(
      `/api/analytics/forwarding-tree${qs ? `?${qs}` : ""}`,
    );
  },
  timeline: (account_id?: number, days = 30) => {
    const sp = new URLSearchParams({ days: String(days) });
    if (account_id != null) sp.set("account_id", String(account_id));
    return request<DateBucket[]>(`/api/analytics/timeline?${sp}`);
  },
  mailboxes: (account_id?: number) =>
    request<CountedString[]>(
      `/api/analytics/mailboxes${account_id ? `?account_id=${account_id}` : ""}`,
    ),
};
