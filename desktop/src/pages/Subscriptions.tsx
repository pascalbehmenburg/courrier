import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Loader2, MailX, RotateCcw } from "lucide-react";
import { toast } from "sonner";

import { api } from "@/lib/api";
import type { Sender, SubscriptionKind, UnsubscribeOutcome } from "@/types/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const WINDOW_DAYS = 180;

export default function Subscriptions() {
  const qc = useQueryClient();
  const [tab, setTab] = useState<SubscriptionKind>("one_click");

  const senders = useQuery({
    queryKey: ["subscriptions", tab, WINDOW_DAYS],
    queryFn: () => api.listSubscriptions(tab, WINDOW_DAYS),
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["subscriptions"] });
  };

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-3xl font-semibold tracking-tight">Subscriptions</h1>
        <p className="text-sm text-muted-foreground">
          Senders seen in the past {WINDOW_DAYS} days. A new mail from an
          unsubscribed sender will surface them here again automatically.
        </p>
      </header>

      <Tabs value={tab} onValueChange={(v) => setTab(v as SubscriptionKind)}>
        <TabsList>
          <TabsTrigger value="one_click">One-click</TabsTrigger>
          <TabsTrigger value="manual">Manual link</TabsTrigger>
          <TabsTrigger value="other">Other</TabsTrigger>
          <TabsTrigger value="unsubscribed">Done</TabsTrigger>
        </TabsList>

        <TabsContent value="one_click">
          <OneClickPanel
            senders={senders.data ?? []}
            loading={senders.isLoading}
            onChange={invalidate}
          />
        </TabsContent>
        <TabsContent value="manual">
          <ManualPanel
            senders={senders.data ?? []}
            loading={senders.isLoading}
            onChange={invalidate}
          />
        </TabsContent>
        <TabsContent value="other">
          <OtherPanel
            senders={senders.data ?? []}
            loading={senders.isLoading}
            onChange={invalidate}
          />
        </TabsContent>
        <TabsContent value="unsubscribed">
          <DonePanel
            senders={senders.data ?? []}
            loading={senders.isLoading}
            onChange={invalidate}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}

interface PanelProps {
  senders: Sender[];
  loading: boolean;
  onChange: () => void;
}

function OneClickPanel({ senders, loading, onChange }: PanelProps) {
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const unsubscribe = useMutation({
    mutationFn: (ids: number[]) => api.bulkUnsubscribe(ids),
    onSuccess: (outcomes: UnsubscribeOutcome[]) => {
      const ok = outcomes.filter((o) => o.ok).length;
      const fail = outcomes.length - ok;
      toast.success(`Unsubscribed ${ok} sender(s)${fail ? `, ${fail} failed` : ""}`);
      setSelected(new Set());
      onChange();
    },
    onError: (err: Error) => toast.error(`Unsubscribe failed: ${err.message}`),
  });

  const toggle = (id: number) =>
    setSelected((s) => {
      const next = new Set(s);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const toggleAll = () => {
    if (selected.size === senders.length) setSelected(new Set());
    else setSelected(new Set(senders.map((s) => s.id)));
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <div>
          <CardTitle className="text-base">
            One-click unsubscribe ({senders.length})
          </CardTitle>
          <p className="text-xs text-muted-foreground mt-1">
            RFC 8058 — we POST to the sender's unsubscribe endpoint server-side.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={toggleAll} disabled={!senders.length}>
            {selected.size === senders.length && senders.length > 0
              ? "Deselect all"
              : "Select all"}
          </Button>
          <Button
            onClick={() => unsubscribe.mutate(Array.from(selected))}
            disabled={selected.size === 0 || unsubscribe.isPending}
          >
            {unsubscribe.isPending ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <MailX className="mr-2 h-4 w-4" />
            )}
            Unsubscribe selected ({selected.size})
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <SenderList
          senders={senders}
          loading={loading}
          empty="No one-click subscriptions in the window."
          selected={selected}
          onToggle={toggle}
        />
      </CardContent>
    </Card>
  );
}

function ManualPanel({ senders, loading, onChange }: PanelProps) {
  const markDone = useMutation({
    mutationFn: ({ id, method }: { id: number; method: string }) =>
      api.markUnsubscribed(id, method),
    onSuccess: () => {
      toast.success("Marked as unsubscribed.");
      onChange();
    },
    onError: (err: Error) => toast.error(`Failed: ${err.message}`),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">
          Manual unsubscribe ({senders.length})
        </CardTitle>
        <p className="text-xs text-muted-foreground mt-1">
          Open the link, complete the flow on the sender's page, then mark it done.
        </p>
      </CardHeader>
      <CardContent>
        {loading ? (
          <SkeletonRows />
        ) : senders.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No manual subscriptions in the window.
          </p>
        ) : (
          <ul className="divide-y divide-border">
            {senders.map((s) => {
              const link = s.unsub_web_url ?? s.unsub_mailto;
              return (
                <li
                  key={s.id}
                  className="flex items-center justify-between gap-3 py-2"
                >
                  <SenderRowLabel s={s} />
                  <div className="flex items-center gap-2">
                    {link && (
                      <Button asChild variant="outline" size="sm">
                        <a
                          href={link}
                          target="_blank"
                          rel="noopener noreferrer"
                          onClick={(e) => {
                            // For https://, this opens a tab. For mailto:, the
                            // OS hands it to a mail client. Either way we
                            // don't auto-mark — user confirms after.
                            e.stopPropagation();
                          }}
                        >
                          <ExternalLink className="mr-1 h-3 w-3" />
                          {s.unsub_web_url ? "Open page" : "Open mailto"}
                        </a>
                      </Button>
                    )}
                    <Button
                      variant="default"
                      size="sm"
                      onClick={() =>
                        markDone.mutate({
                          id: s.id,
                          method: s.unsub_web_url ? "manual_link" : "mailto",
                        })
                      }
                    >
                      Mark done
                    </Button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function OtherPanel({ senders, loading, onChange }: PanelProps) {
  const markDone = useMutation({
    mutationFn: (id: number) => api.markUnsubscribed(id, "skip"),
    onSuccess: () => {
      toast.success("Marked as unsubscribed.");
      onChange();
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">
          No unsubscribe header ({senders.length})
        </CardTitle>
        <p className="text-xs text-muted-foreground mt-1">
          These senders don't advertise an unsubscribe method. Default rule:
          they're still considered subscribed if you saw mail in the window.
          Mark them done manually once you've sorted it out.
        </p>
      </CardHeader>
      <CardContent>
        {loading ? (
          <SkeletonRows />
        ) : senders.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No senders in this bucket.
          </p>
        ) : (
          <ul className="divide-y divide-border">
            {senders.map((s) => (
              <li
                key={s.id}
                className="flex items-center justify-between gap-3 py-2"
              >
                <SenderRowLabel s={s} />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => markDone.mutate(s.id)}
                >
                  Mark done
                </Button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function DonePanel({ senders, loading, onChange }: PanelProps) {
  const resub = useMutation({
    mutationFn: (id: number) => api.resubscribe(id),
    onSuccess: () => {
      toast.success("Re-flagged as subscribed.");
      onChange();
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">
          Unsubscribed ({senders.length})
        </CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <SkeletonRows />
        ) : senders.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nothing here yet.</p>
        ) : (
          <ul className="divide-y divide-border">
            {senders.map((s) => (
              <li
                key={s.id}
                className="flex items-center justify-between gap-3 py-2"
              >
                <SenderRowLabel s={s} showResult />
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => resub.mutate(s.id)}
                >
                  <RotateCcw className="mr-1 h-3 w-3" />
                  Re-flag
                </Button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function SenderList({
  senders,
  loading,
  empty,
  selected,
  onToggle,
}: {
  senders: Sender[];
  loading: boolean;
  empty: string;
  selected: Set<number>;
  onToggle: (id: number) => void;
}) {
  if (loading) return <SkeletonRows />;
  if (!senders.length) {
    return <p className="text-sm text-muted-foreground">{empty}</p>;
  }
  return (
    <ul className="divide-y divide-border">
      {senders.map((s) => (
        <li
          key={s.id}
          className="flex items-center gap-3 py-2 cursor-pointer hover:bg-muted/30 rounded-sm px-2 -mx-2"
          onClick={() => onToggle(s.id)}
        >
          <input
            type="checkbox"
            checked={selected.has(s.id)}
            onChange={() => onToggle(s.id)}
            onClick={(e) => e.stopPropagation()}
            className="h-4 w-4 shrink-0"
          />
          <SenderRowLabel s={s} />
        </li>
      ))}
    </ul>
  );
}

function SenderRowLabel({ s, showResult }: { s: Sender; showResult?: boolean }) {
  return (
    <div className="min-w-0 flex-1">
      <div className="flex items-center gap-2">
        {s.display_name && (
          <span className="truncate text-sm font-medium">{s.display_name}</span>
        )}
        <span className="truncate font-mono text-xs text-muted-foreground">
          {s.address}
        </span>
      </div>
      <div className="text-[11px] text-muted-foreground">
        {s.message_count.toLocaleString()} mail
        {s.message_count === 1 ? "" : "s"} · last{" "}
        {new Date(s.last_seen_at).toLocaleDateString()}
        {showResult && s.unsubscribe_result && (
          <> · {s.unsubscribed_method}: {s.unsubscribe_result}</>
        )}
      </div>
    </div>
  );
}

function SkeletonRows() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 5 }).map((_, i) => (
        <div key={i} className="h-10 animate-pulse rounded-md bg-muted/40" />
      ))}
    </div>
  );
}
