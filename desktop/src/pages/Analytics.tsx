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
  const forwardingTree = useQuery({
    queryKey: ["a", "forwarding-tree", scope],
    queryFn: () => api.forwardingTree(accountId),
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
            <CardTitle className="text-sm">Forwarded mail</CardTitle>
          </CardHeader>
          <CardContent>
            <ForwardingTreeView tree={forwardingTree.data} />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

import { ChevronRight } from "lucide-react";
import type { ForwarderTree, ForwarderNode, OriginDomainNode } from "@/types/api";

function ForwardingTreeView({ tree }: { tree: ForwarderTree | undefined }) {
  const [openForwarders, setOpenForwarders] = useState<Set<string>>(new Set());
  const [openDomains, setOpenDomains] = useState<Set<string>>(new Set());

  if (!tree || tree.forwarders.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No forwarded mail detected yet.
      </p>
    );
  }

  const toggle = (set: Set<string>, key: string) => {
    const next = new Set(set);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    return next;
  };

  return (
    <ul className="space-y-1 text-sm">
      {tree.forwarders.map((fwd: ForwarderNode) => {
        const fwdKey = fwd.forwarder;
        const isOpen = openForwarders.has(fwdKey);
        return (
          <li key={fwdKey} className="rounded-md hover:bg-muted/30">
            <button
              type="button"
              onClick={() =>
                setOpenForwarders((s) => toggle(s, fwdKey))
              }
              className="flex w-full items-center justify-between gap-2 px-2 py-1 text-left"
            >
              <span className="flex min-w-0 items-center gap-1">
                <ChevronRight
                  className={`h-3 w-3 shrink-0 transition-transform ${
                    isOpen ? "rotate-90" : ""
                  }`}
                />
                <span className="truncate font-mono text-xs">{fwdKey}</span>
              </span>
              <span className="font-mono text-xs text-muted-foreground">
                {fwd.total.toLocaleString()}
              </span>
            </button>
            {isOpen && (
              <ul className="ml-5 space-y-1 border-l border-border/40 pl-2">
                {fwd.domains.map((dom: OriginDomainNode) => {
                  const domKey = `${fwdKey}|${dom.domain}`;
                  const domOpen = openDomains.has(domKey);
                  const hasAddrs = dom.addresses.length > 0;
                  return (
                    <li key={domKey} className="rounded-md hover:bg-muted/40">
                      <button
                        type="button"
                        onClick={() =>
                          hasAddrs && setOpenDomains((s) => toggle(s, domKey))
                        }
                        className="flex w-full items-center justify-between gap-2 px-2 py-1 text-left"
                        disabled={!hasAddrs}
                      >
                        <span className="flex min-w-0 items-center gap-1">
                          {hasAddrs ? (
                            <ChevronRight
                              className={`h-3 w-3 shrink-0 transition-transform ${
                                domOpen ? "rotate-90" : ""
                              }`}
                            />
                          ) : (
                            <span className="w-3 shrink-0" />
                          )}
                          <span className="truncate text-xs">
                            {dom.domain}
                          </span>
                        </span>
                        <span className="font-mono text-xs text-muted-foreground">
                          {dom.count.toLocaleString()}
                        </span>
                      </button>
                      {domOpen && hasAddrs && (
                        <ul className="ml-5 space-y-0.5 border-l border-border/40 pl-2">
                          {dom.addresses.map((a) => (
                            <li
                              key={a.key}
                              className="flex items-center justify-between gap-2 px-2 py-0.5"
                            >
                              <span className="truncate font-mono text-[11px] text-muted-foreground">
                                {a.key}
                              </span>
                              <span className="font-mono text-[11px] text-muted-foreground">
                                {a.count.toLocaleString()}
                              </span>
                            </li>
                          ))}
                        </ul>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </li>
        );
      })}
    </ul>
  );
}
