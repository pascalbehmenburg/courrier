import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
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
import { api } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccounts } from "@/hooks/useAccounts";

const PIE_COLORS = [
  "hsl(217 91% 60%)",
  "hsl(280 70% 60%)",
  "hsl(160 60% 45%)",
  "hsl(36 85% 55%)",
  "hsl(340 80% 60%)",
  "hsl(200 80% 50%)",
  "hsl(120 60% 45%)",
  "hsl(20 80% 55%)",
];

export default function Analytics() {
  const accounts = useAccounts();
  const [scope, setScope] = useState<string>("all");
  const accountId = scope === "all" ? undefined : Number(scope);

  const senders = useQuery({
    queryKey: ["a", "top-senders", scope],
    queryFn: () => api.topSenders(accountId, 15),
  });
  const domains = useQuery({
    queryKey: ["a", "top-domains", scope],
    queryFn: () => api.topSenderDomains(accountId, 8),
  });
  const timeline = useQuery({
    queryKey: ["a", "timeline", scope],
    queryFn: () => api.timeline(accountId, 90),
  });
  const mailboxes = useQuery({
    queryKey: ["a", "mailboxes", scope],
    queryFn: () => api.mailboxes(accountId),
  });
  const forwarding = useQuery({
    queryKey: ["a", "forwarding", scope],
    queryFn: () => api.forwarding(accountId, 50),
  });

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Analytics</h1>
          <p className="text-sm text-muted-foreground">
            Aggregate views over your fetched mail.
          </p>
        </div>
        <Select value={scope} onValueChange={setScope}>
          <SelectTrigger className="w-56">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All accounts</SelectItem>
            {accounts.data?.map((a) => (
              <SelectItem key={a.id} value={String(a.id)}>
                {a.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </header>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Top senders</CardTitle>
          </CardHeader>
          <CardContent className="h-80">
            <ResponsiveContainer>
              <BarChart layout="vertical" data={senders.data ?? []}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis type="number" tick={{ fontSize: 11 }} />
                <YAxis type="category" dataKey="key" width={180} tick={{ fontSize: 11 }} />
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
          <CardContent className="h-80">
            <ResponsiveContainer>
              <PieChart>
                <Pie
                  data={domains.data ?? []}
                  dataKey="count"
                  nameKey="key"
                  outerRadius={110}
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
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Last 90 days</CardTitle>
        </CardHeader>
        <CardContent className="h-72">
          <ResponsiveContainer>
            <BarChart data={timeline.data ?? []}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis
                dataKey="day"
                tick={{ fontSize: 10 }}
                tickFormatter={(d: string) => d.slice(5)}
              />
              <YAxis tick={{ fontSize: 11 }} />
              <ChartTooltip />
              <Bar dataKey="count" fill="hsl(var(--primary))" radius={[3, 3, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Mailbox distribution</CardTitle>
          </CardHeader>
          <CardContent>
            {(mailboxes.data?.length ?? 0) === 0 ? (
              <p className="text-sm text-muted-foreground">No data.</p>
            ) : (
              <ul className="space-y-1 text-sm">
                {mailboxes.data!.map((m) => (
                  <li
                    key={m.key}
                    className="flex items-center justify-between rounded-md px-2 py-1 hover:bg-muted/40"
                  >
                    <span>{m.key}</span>
                    <span className="font-mono text-xs text-muted-foreground">
                      {m.count.toLocaleString()}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Forwarded — top forwarders</CardTitle>
          </CardHeader>
          <CardContent>
            {(forwarding.data?.by_forwarder.length ?? 0) === 0 ? (
              <p className="text-sm text-muted-foreground">
                No forwarded mail detected yet.
              </p>
            ) : (
              <ul className="space-y-1 text-sm">
                {forwarding.data!.by_forwarder.slice(0, 12).map((row) => (
                  <li
                    key={row.key}
                    className="flex items-center justify-between rounded-md px-2 py-1 hover:bg-muted/40"
                  >
                    <span className="font-mono text-xs">{row.key}</span>
                    <span className="font-mono text-xs text-muted-foreground">
                      {row.count.toLocaleString()}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
