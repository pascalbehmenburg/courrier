import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Inbox, Mail, RefreshCw, Sparkles, TrendingUp } from "lucide-react";
import { Link } from "react-router-dom";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AccountStatusCard } from "@/components/AccountStatusCard";
import { useAccounts, useSyncStatus } from "@/hooks/useAccounts";

export default function Dashboard() {
  const accounts = useAccounts();
  const status = useSyncStatus();
  const overview = useQuery({
    queryKey: ["analytics", "overview"],
    queryFn: () => api.overview(),
    refetchInterval: 15000,
  });
  const timeline = useQuery({
    queryKey: ["analytics", "timeline", 30],
    queryFn: () => api.timeline(undefined, 30),
    refetchInterval: 60000,
  });

  const qc = useQueryClient();
  const syncAll = useMutation({
    mutationFn: api.syncAll,
    onSuccess: (data) => {
      const n = data.started.length;
      if (n === 0) toast.message("Nothing to sync", { description: "All accounts are up to date or already syncing." });
      else toast.success(`Started sync for ${n} account${n === 1 ? "" : "s"}`);
      qc.invalidateQueries({ queryKey: ["sync"] });
    },
    onError: (e: Error) => toast.error(`Sync failed: ${e.message}`),
  });

  const statusByAccount = new Map(status.data?.map((s) => [s.account_id, s] as const));
  const stats = overview.data;

  return (
    <div className="space-y-8">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Dashboard</h1>
          <p className="text-sm text-muted-foreground">
            All your IMAP accounts, mail, and analytics in one place.
          </p>
        </div>
        <Button
          onClick={() => syncAll.mutate()}
          disabled={syncAll.isPending}
          className="gap-2"
        >
          <RefreshCw className={`h-4 w-4 ${syncAll.isPending ? "animate-spin" : ""}`} />
          Sync all
        </Button>
      </header>

      <section className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          icon={<Inbox className="h-4 w-4" />}
          label="Accounts"
          value={accounts.data?.length ?? 0}
        />
        <StatCard
          icon={<Mail className="h-4 w-4" />}
          label="Total messages"
          value={(stats?.total_messages ?? 0).toLocaleString()}
        />
        <StatCard
          icon={<Sparkles className="h-4 w-4" />}
          label="Forwarded"
          value={(stats?.forwarded_messages ?? 0).toLocaleString()}
          hint={
            stats && stats.total_messages > 0
              ? `${((stats.forwarded_messages / stats.total_messages) * 100).toFixed(1)}%`
              : undefined
          }
        />
        <StatCard
          icon={<TrendingUp className="h-4 w-4" />}
          label="Storage"
          value={formatBytes(stats?.total_storage_bytes ?? 0)}
        />
      </section>

      <section className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Last 30 days</CardTitle>
          </CardHeader>
          <CardContent className="h-64">
            {(timeline.data?.length ?? 0) === 0 ? (
              <EmptyChart />
            ) : (
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={timeline.data}>
                  <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                  <XAxis
                    dataKey="day"
                    tick={{ fontSize: 11 }}
                    tickFormatter={(d: string) => d.slice(5)}
                  />
                  <YAxis tick={{ fontSize: 11 }} />
                  <Tooltip
                    contentStyle={{
                      background: "hsl(var(--popover))",
                      border: "1px solid hsl(var(--border))",
                      borderRadius: "var(--radius)",
                      fontSize: 12,
                    }}
                  />
                  <Bar dataKey="count" fill="hsl(var(--primary))" radius={[3, 3, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Quick actions</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            <ActionRow
              to="/accounts"
              label="Add an account"
              hint="Connect another IMAP mailbox"
            />
            <ActionRow to="/search" label="Search mail" hint="Full-text across every message" />
            <ActionRow to="/analytics" label="View analytics" hint="Senders, forwards, traffic" />
            <ActionRow to="/messages" label="Browse messages" hint="Latest mail across accounts" />
          </CardContent>
        </Card>
      </section>

      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Accounts</h2>
          <Button asChild variant="ghost" size="sm">
            <Link to="/accounts">Manage all</Link>
          </Button>
        </div>
        {accounts.isLoading ? (
          <p className="text-sm text-muted-foreground">Loading accounts…</p>
        ) : (accounts.data?.length ?? 0) === 0 ? (
          <Card>
            <CardContent className="py-10 text-center">
              <p className="text-sm text-muted-foreground">
                No accounts yet. Add one to start syncing mail.
              </p>
              <Button asChild className="mt-4">
                <Link to="/accounts">Add an account</Link>
              </Button>
            </CardContent>
          </Card>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {accounts.data!.map((a) => (
              <AccountStatusCard key={a.id} account={a} status={statusByAccount.get(a.id)} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
  hint,
}: {
  icon: React.ReactNode;
  label: string;
  value: React.ReactNode;
  hint?: string;
}) {
  return (
    <Card>
      <CardContent className="p-5">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {icon}
          <span>{label}</span>
        </div>
        <div className="mt-2 flex items-baseline gap-2">
          <span className="text-2xl font-semibold">{value}</span>
          {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
        </div>
      </CardContent>
    </Card>
  );
}

function ActionRow({ to, label, hint }: { to: string; label: string; hint: string }) {
  return (
    <Link
      to={to}
      className="flex items-center justify-between rounded-md border border-transparent px-3 py-2 hover:border-border hover:bg-accent"
    >
      <div>
        <div className="text-sm font-medium">{label}</div>
        <div className="text-xs text-muted-foreground">{hint}</div>
      </div>
      <span className="text-xs text-muted-foreground">→</span>
    </Link>
  );
}

function EmptyChart() {
  return (
    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
      No mail yet — sync an account to see traffic over time.
    </div>
  );
}
