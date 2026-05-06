import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Plus, Trash2, XCircle } from "lucide-react";
import { Link } from "react-router-dom";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { useAccounts, useSyncStatus } from "@/hooks/useAccounts";
import { AccountFormDialog } from "@/components/AccountFormDialog";
import type { Account } from "@/types/api";
import { formatRelative } from "@/lib/utils";

export default function Accounts() {
  const accounts = useAccounts();
  const status = useSyncStatus();
  const [editing, setEditing] = useState<Account | "new" | null>(null);
  const qc = useQueryClient();

  const del = useMutation({
    mutationFn: (id: number) => api.deleteAccount(id),
    onSuccess: () => {
      toast.success("Account deleted");
      qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const test = useMutation({
    mutationFn: (id: number) => api.testAccount(id),
    onSuccess: (data) =>
      data.ok ? toast.success(data.message) : toast.error(data.message),
    onError: (e: Error) => toast.error(e.message),
  });

  const statusByAccount = new Map(status.data?.map((s) => [s.account_id, s] as const));

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Accounts</h1>
          <p className="text-sm text-muted-foreground">
            IMAP connections. Passwords are encrypted at rest with AES-GCM.
          </p>
        </div>
        <Button onClick={() => setEditing("new")} className="gap-2">
          <Plus className="h-4 w-4" /> Add account
        </Button>
      </header>

      {(accounts.data?.length ?? 0) === 0 ? (
        <Card>
          <CardContent className="py-16 text-center text-sm text-muted-foreground">
            No accounts yet. Click <em>Add account</em> to connect one.
          </CardContent>
        </Card>
      ) : (
        <div className="overflow-hidden rounded-lg border">
          <table className="w-full text-sm">
            <thead className="bg-muted/40 text-left text-xs uppercase tracking-wide text-muted-foreground">
              <tr>
                <th className="px-4 py-3">Account</th>
                <th className="px-4 py-3">Server</th>
                <th className="px-4 py-3">Auto-sync</th>
                <th className="px-4 py-3">Last run</th>
                <th className="px-4 py-3">Status</th>
                <th className="px-4 py-3"></th>
              </tr>
            </thead>
            <tbody className="divide-y">
              {accounts.data!.map((a) => {
                const st = statusByAccount.get(a.id);
                const lastRun = st?.latest_run;
                return (
                  <tr key={a.id} className="hover:bg-muted/30">
                    <td className="px-4 py-3">
                      <Link to={`/accounts/${a.id}`} className="font-medium hover:underline">
                        {a.label}
                      </Link>
                      <div className="text-xs text-muted-foreground">{a.email}</div>
                    </td>
                    <td className="px-4 py-3 font-mono text-xs">
                      {a.host}:{a.port}
                    </td>
                    <td className="px-4 py-3">
                      {a.sync_interval_seconds
                        ? `${Math.round(a.sync_interval_seconds / 60)} min`
                        : "—"}
                    </td>
                    <td className="px-4 py-3">
                      {formatRelative(lastRun?.completed_at ?? lastRun?.started_at)}
                    </td>
                    <td className="px-4 py-3">
                      {st?.running ? (
                        <Badge variant="success">syncing</Badge>
                      ) : lastRun?.status === "failed" ? (
                        <Badge variant="destructive">failed</Badge>
                      ) : !a.enabled ? (
                        <Badge variant="outline">disabled</Badge>
                      ) : (
                        <Badge variant="secondary">idle</Badge>
                      )}
                    </td>
                    <td className="px-4 py-3 text-right">
                      <div className="flex justify-end gap-1">
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => test.mutate(a.id)}
                          disabled={test.isPending}
                        >
                          {test.isPending ? "…" : <><CheckCircle2 className="mr-1 h-3.5 w-3.5" /> Test</>}
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => setEditing(a)}>
                          Edit
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            if (confirm(`Delete "${a.label}"? This will remove all stored mail for this account.`)) {
                              del.mutate(a.id);
                            }
                          }}
                          className="text-destructive"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <AccountFormDialog
        open={editing !== null}
        editing={editing === "new" ? null : editing}
        onClose={() => setEditing(null)}
      />
    </div>
  );
}
