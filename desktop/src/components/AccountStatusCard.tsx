import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, Loader2, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { Account, FetchStatus } from "@/types/api";
import { formatRelative } from "@/lib/utils";

interface Props {
  account: Account;
  status?: FetchStatus;
}

export function AccountStatusCard({ account, status }: Props) {
  const qc = useQueryClient();
  const sync = useMutation({
    mutationFn: () => api.syncOne(account.id),
    onSuccess: (data) => {
      if (data.started) {
        toast.success(`Sync started for ${account.label}`);
      } else {
        toast.message("Already syncing", { description: account.label });
      }
      qc.invalidateQueries({ queryKey: ["sync"] });
    },
    onError: (e: Error) => toast.error(`Sync failed: ${e.message}`),
  });

  const running = status?.running ?? false;
  const lastRun = status?.latest_run ?? null;
  const failed = lastRun?.status === "failed";

  return (
    <Card className="overflow-hidden">
      <CardHeader className="flex-row items-start justify-between space-y-0 pb-3">
        <div className="space-y-1">
          <CardTitle className="text-base">{account.label}</CardTitle>
          <p className="text-xs text-muted-foreground">{account.email}</p>
        </div>
        <div className="flex items-center gap-2">
          {!account.enabled && <Badge variant="outline">disabled</Badge>}
          {running && (
            <Badge variant="success" className="gap-1">
              <Loader2 className="h-3 w-3 animate-spin" /> syncing
            </Badge>
          )}
          {!running && failed && <Badge variant="destructive">failed</Badge>}
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid grid-cols-3 gap-3 text-sm">
          <div>
            <div className="text-xs text-muted-foreground">Last sync</div>
            <div className="font-medium">
              {formatRelative(lastRun?.completed_at ?? lastRun?.started_at)}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted-foreground">Last fetched</div>
            <div className="font-medium">
              {lastRun ? lastRun.messages_fetched.toLocaleString() : "—"}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted-foreground">Auto-sync</div>
            <div className="font-medium">
              {account.sync_interval_seconds
                ? `${Math.round(account.sync_interval_seconds / 60)} min`
                : "off"}
            </div>
          </div>
        </div>
        {failed && lastRun?.error && (
          <div className="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive-foreground/80">
            <span className="font-mono">{lastRun.error.slice(0, 220)}</span>
          </div>
        )}
        <div className="flex items-center justify-between gap-2 pt-1">
          <Button
            size="sm"
            variant="outline"
            disabled={running || sync.isPending || !account.enabled}
            onClick={() => sync.mutate()}
            className="gap-1"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${running ? "animate-spin" : ""}`} />
            Sync now
          </Button>
          <Button asChild size="sm" variant="ghost" className="gap-1">
            <Link to={`/accounts/${account.id}`}>
              Details <ChevronRight className="h-3.5 w-3.5" />
            </Link>
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
