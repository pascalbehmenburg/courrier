import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronLeft,
  Forward,
  Inbox,
  Loader2,
  Mail,
  RefreshCw,
  Search,
} from "lucide-react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSyncStatus } from "@/hooks/useAccounts";
import { formatBytes, formatRelative } from "@/lib/utils";

const PIE_COLORS = [
  "hsl(217 91% 60%)",
  "hsl(280 70% 60%)",
  "hsl(160 60% 45%)",
  "hsl(36 85% 55%)",
  "hsl(340 80% 60%)",
  "hsl(200 80% 50%)",
];

export default function AccountDetail() {
  const { id } = useParams();
  const accountId = Number(id);
  const account = useQuery({
    queryKey: ["account", accountId],
    queryFn: () => api.getAccount(accountId),
    enabled: !!accountId,
  });
  const status = useSyncStatus();
  const overview = useQuery({
    queryKey: ["analytics", "overview", accountId],
    queryFn: () => api.overview(accountId),
  });
  const qc = useQueryClient();
  const sync = useMutation({
    mutationFn: () => api.syncOne(accountId),
    onSuccess: (data) => {
      data.started ? toast.success("Sync started") : toast.message("Already syncing");
      qc.invalidateQueries({ queryKey: ["sync"] });
    },
  });

  const st = status.data?.find((s) => s.account_id === accountId);

  if (account.isLoading || !account.data) {
    return <p className="text-sm text-muted-foreground">Loading account…</p>;
  }

  const a = account.data;
  return (
    <div className="space-y-6">
      <div>
        <Button asChild variant="ghost" size="sm" className="mb-2 gap-1">
          <Link to="/accounts">
            <ChevronLeft className="h-4 w-4" /> Back to accounts
          </Link>
        </Button>
        <div className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">{a.label}</h1>
            <p className="text-sm text-muted-foreground">
              {a.email} · {a.host}:{a.port}
            </p>
          </div>
          <Button
            onClick={() => sync.mutate()}
            disabled={st?.running || sync.isPending}
            className="gap-2"
          >
            <RefreshCw className={`h-4 w-4 ${st?.running ? "animate-spin" : ""}`} />
            {st?.running ? "Syncing…" : "Sync now"}
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard icon={<Mail />} label="Messages" value={overview.data?.total_messages.toLocaleString() ?? "—"} />
        <StatCard icon={<Forward />} label="Forwarded" value={overview.data?.forwarded_messages.toLocaleString() ?? "—"} />
        <StatCard icon={<Inbox />} label="Mailboxes" value={overview.data?.mailbox_count.toLocaleString() ?? "—"} />
        <StatCard label="Storage" value={formatBytes(overview.data?.total_storage_bytes ?? 0)} />
      </div>

      <Tabs defaultValue="messages">
        <TabsList>
          <TabsTrigger value="messages">Messages</TabsTrigger>
          <TabsTrigger value="search">Search</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
          <TabsTrigger value="forwarding">Forwarding</TabsTrigger>
        </TabsList>
        <TabsContent value="messages">
          <MessageList accountId={accountId} />
        </TabsContent>
        <TabsContent value="search">
          <AccountSearch accountId={accountId} />
        </TabsContent>
        <TabsContent value="analytics">
          <AccountAnalytics accountId={accountId} />
        </TabsContent>
        <TabsContent value="forwarding">
          <AccountForwarding accountId={accountId} />
        </TabsContent>
      </Tabs>

      {st?.latest_run && (
        <p className="text-xs text-muted-foreground">
          Last run: {st.latest_run.status} ·{" "}
          {formatRelative(st.latest_run.completed_at ?? st.latest_run.started_at)} ·{" "}
          {st.latest_run.messages_fetched.toLocaleString()} fetched
        </p>
      )}
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
}: {
  icon?: React.ReactNode;
  label: string;
  value: React.ReactNode;
}) {
  return (
    <Card>
      <CardContent className="p-5">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {icon && <span className="[&_svg]:h-4 [&_svg]:w-4">{icon}</span>}
          {label}
        </div>
        <div className="mt-2 text-2xl font-semibold">{value}</div>
      </CardContent>
    </Card>
  );
}

function MessageList({ accountId }: { accountId: number }) {
  const messages = useQuery({
    queryKey: ["messages", accountId],
    queryFn: () => api.listMessages({ account_id: accountId, limit: 100 }),
  });
  if (messages.isLoading)
    return <p className="text-sm text-muted-foreground">Loading…</p>;
  if (!messages.data?.length)
    return (
      <Card>
        <CardContent className="py-10 text-center text-sm text-muted-foreground">
          No mail yet for this account.
        </CardContent>
      </Card>
    );
  return (
    <div className="overflow-hidden rounded-lg border">
      <table className="w-full text-sm">
        <thead className="bg-muted/40 text-left text-xs uppercase tracking-wide text-muted-foreground">
          <tr>
            <th className="px-4 py-3">From</th>
            <th className="px-4 py-3">Subject</th>
            <th className="px-4 py-3">Mailbox</th>
            <th className="px-4 py-3">Date</th>
          </tr>
        </thead>
        <tbody className="divide-y">
          {messages.data.map((m) => (
            <tr key={m.id} className="hover:bg-muted/30">
              <td className="px-4 py-3">
                <Link to={`/messages/${m.id}`} className="block font-medium hover:underline">
                  {m.from_name || m.from_addr || "(no sender)"}
                </Link>
                {m.from_name && m.from_addr && (
                  <div className="text-xs text-muted-foreground">{m.from_addr}</div>
                )}
              </td>
              <td className="px-4 py-3">
                <Link to={`/messages/${m.id}`} className="line-clamp-1 hover:underline">
                  {m.subject || "(no subject)"}
                </Link>
                {m.is_forwarded && (
                  <Badge variant="warning" className="ml-2 text-[10px]">
                    forwarded
                  </Badge>
                )}
              </td>
              <td className="px-4 py-3 text-xs">{m.mailbox}</td>
              <td className="px-4 py-3 text-xs text-muted-foreground">
                {formatRelative(m.date_utc)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AccountSearch({ accountId }: { accountId: number }) {
  const [q, setQ] = useState("");
  const [submitted, setSubmitted] = useState("");
  const hits = useQuery({
    queryKey: ["search", accountId, submitted],
    queryFn: () => api.search(submitted, accountId),
    enabled: submitted.length > 0,
  });
  return (
    <div className="space-y-4">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setSubmitted(q);
        }}
        className="flex items-center gap-2"
      >
        <Search className="h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search this account (FTS5: phrase, AND/OR, subject:foo …)"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <Button type="submit">Search</Button>
      </form>

      {submitted && hits.isLoading && (
        <p className="text-sm text-muted-foreground">Searching…</p>
      )}
      {submitted && hits.data && hits.data.length === 0 && (
        <p className="text-sm text-muted-foreground">No matches.</p>
      )}
      <div className="space-y-2">
        {hits.data?.map((h) => (
          <Link
            key={h.id}
            to={`/messages/${h.id}`}
            className="block rounded-md border p-4 hover:bg-muted/30"
          >
            <div className="flex items-center justify-between">
              <div className="font-medium">{h.subject || "(no subject)"}</div>
              <div className="text-xs text-muted-foreground">
                {formatRelative(h.date_utc)}
              </div>
            </div>
            <div className="text-xs text-muted-foreground">
              {h.from_name || h.from_addr} · {h.mailbox}
            </div>
            <p
              className="mt-2 text-sm text-muted-foreground"
              dangerouslySetInnerHTML={{ __html: h.snippet }}
            />
          </Link>
        ))}
      </div>
    </div>
  );
}

function AccountAnalytics({ accountId }: { accountId: number }) {
  const senders = useQuery({
    queryKey: ["top-senders", accountId],
    queryFn: () => api.topSenders(accountId, 12),
  });
  const domains = useQuery({
    queryKey: ["top-domains", accountId],
    queryFn: () => api.topSenderDomains(accountId, 8),
  });
  const timeline = useQuery({
    queryKey: ["timeline", accountId],
    queryFn: () => api.timeline(accountId, 60),
  });

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Top senders</CardTitle>
        </CardHeader>
        <CardContent className="h-72">
          <ResponsiveContainer>
            <BarChart layout="vertical" data={senders.data ?? []}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis type="number" tick={{ fontSize: 11 }} />
              <YAxis
                type="category"
                dataKey="key"
                width={150}
                tick={{ fontSize: 11 }}
              />
              <ChartTooltip />
              <Bar dataKey="count" fill="hsl(var(--primary))" radius={[0, 3, 3, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Sender domains</CardTitle>
        </CardHeader>
        <CardContent className="h-72">
          <ResponsiveContainer>
            <PieChart>
              <Pie
                data={domains.data ?? []}
                dataKey="count"
                nameKey="key"
                outerRadius={90}
                label={({ key, percent }) =>
                  `${key} ${((percent ?? 0) * 100).toFixed(0)}%`
                }
                labelLine={false}
              >
                {(domains.data ?? []).map((_, i) => (
                  <Cell key={i} fill={PIE_COLORS[i % PIE_COLORS.length]} />
                ))}
              </Pie>
              <ChartTooltip />
            </PieChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      <Card className="lg:col-span-2">
        <CardHeader>
          <CardTitle className="text-sm">Last 60 days</CardTitle>
        </CardHeader>
        <CardContent className="h-64">
          <ResponsiveContainer>
            <BarChart data={timeline.data ?? []}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis
                dataKey="day"
                tick={{ fontSize: 11 }}
                tickFormatter={(d: string) => d.slice(5)}
              />
              <YAxis tick={{ fontSize: 11 }} />
              <ChartTooltip />
              <Bar dataKey="count" fill="hsl(var(--primary))" radius={[3, 3, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>
    </div>
  );
}

function AccountForwarding({ accountId }: { accountId: number }) {
  const forwarding = useQuery({
    queryKey: ["forwarding", accountId],
    queryFn: () => api.forwarding(accountId, 50),
  });

  if (forwarding.isLoading)
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-3 w-3 animate-spin" /> Computing…
      </p>
    );

  const breakdown = forwarding.data;
  if (!breakdown || breakdown.by_forwarder.length === 0) {
    return (
      <Card>
        <CardContent className="py-10 text-center text-sm text-muted-foreground">
          No forwarded mail detected for this account yet.
        </CardContent>
      </Card>
    );
  }

  // Group origin rows by forwarder for display.
  const originsByForwarder = new Map<string, { domain: string; count: number }[]>();
  for (const r of breakdown.by_forwarder_then_origin) {
    const list = originsByForwarder.get(r.forwarded_from) ?? [];
    list.push({ domain: r.origin_domain, count: r.count });
    originsByForwarder.set(r.forwarded_from, list);
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Detection uses Resent-* / X-Forwarded-* headers and body sentinels
        (Gmail / Apple Mail / Outlook / German Outlook variants). The original
        sender is extracted from the inner From: line where present.
      </p>
      <div className="space-y-3">
        {breakdown.by_forwarder.map((row) => (
          <Card key={row.key}>
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">{row.key}</CardTitle>
                <Badge variant="secondary">{row.count.toLocaleString()} forwarded</Badge>
              </div>
            </CardHeader>
            <CardContent>
              {originsByForwarder.has(row.key) ? (
                <div className="space-y-1">
                  {originsByForwarder.get(row.key)!.map((o) => (
                    <div
                      key={o.domain}
                      className="flex items-center justify-between rounded-md px-2 py-1 text-sm hover:bg-muted/40"
                    >
                      <span className="font-mono text-xs">{o.domain}</span>
                      <span className="text-xs text-muted-foreground">
                        {o.count.toLocaleString()}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Origin sender domain not parseable for these messages.
                </p>
              )}
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
