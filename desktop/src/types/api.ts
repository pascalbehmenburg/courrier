// Mirrors the Rust types serialized by courrier-server. Kept narrow on
// purpose — when the server gains a field, add it here so TS picks up
// the new shape.

export type UsernameStyle = "full_email" | "local_part" | "manual";

export interface Provider {
  id: string;
  label: string;
  host: string;
  port: number;
  username_style: UsernameStyle;
  app_password_url: string | null;
  notes: string;
}

export interface Account {
  id: number;
  label: string;
  email: string;
  username: string;
  host: string;
  port: number;
  provider_id: string;
  sync_interval_seconds: number | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface AccountPayload {
  label: string;
  email: string;
  username: string;
  /** Empty string means "leave existing untouched" on update. */
  password: string;
  host: string;
  port: number;
  provider_id: string;
  sync_interval_seconds: number | null;
  enabled: boolean;
}

export interface TestResult {
  ok: boolean;
  message: string;
}

export type FetchRunStatus = "running" | "completed" | "failed";

export interface FetchRun {
  id: number;
  account_id: number | null;
  started_at: string;
  completed_at: string | null;
  messages_fetched: number;
  status: FetchRunStatus;
  error: string | null;
}

export interface FetchStatus {
  account_id: number;
  running: boolean;
  latest_run: FetchRun | null;
}

export interface MessageSummary {
  id: number;
  mailbox: string;
  subject: string | null;
  from_addr: string | null;
  from_name: string | null;
  date_utc: string | null;
  is_forwarded: boolean;
  forwarded_from: string | null;
  size_bytes: number;
}

export interface Message extends MessageSummary {
  fetched_email_id: number;
  account_id: number;
  message_id: string | null;
  to_addrs: string | null;
  cc_addrs: string | null;
  body_text: string | null;
  forwarded_from_domain: string | null;
  original_sender_domain: string | null;
  original_sender_addr: string | null;
}

export interface SearchHit {
  id: number;
  account_id: number;
  mailbox: string;
  subject: string | null;
  from_addr: string | null;
  from_name: string | null;
  date_utc: string | null;
  snippet: string;
  rank: number;
}

export interface AnalyticsOverview {
  account_id: number | null;
  total_messages: number;
  total_storage_bytes: number;
  forwarded_messages: number;
  mailbox_count: number;
  last_message_date: string | null;
  first_message_date: string | null;
}

export interface CountedString {
  key: string;
  count: number;
}

export interface DateBucket {
  day: string;
  count: number;
}

export interface ForwarderOriginRow {
  forwarded_from: string;
  origin_domain: string;
  count: number;
}

export interface ForwardingBreakdown {
  by_forwarder: CountedString[];
  by_forwarder_then_origin: ForwarderOriginRow[];
}

export interface OriginDomainNode {
  domain: string;
  count: number;
  addresses: CountedString[];
}

export interface ForwarderNode {
  forwarder: string;
  total: number;
  domains: OriginDomainNode[];
}

export interface ForwarderTree {
  forwarders: ForwarderNode[];
}
