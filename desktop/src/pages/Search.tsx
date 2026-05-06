import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Search } from "lucide-react";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccounts } from "@/hooks/useAccounts";
import { formatRelative } from "@/lib/utils";

export default function SearchPage() {
  const accounts = useAccounts();
  const [q, setQ] = useState("");
  const [accountId, setAccountId] = useState<string>("all");
  const [submitted, setSubmitted] = useState("");

  const hits = useQuery({
    queryKey: ["search", accountId, submitted],
    queryFn: () =>
      api.search(submitted, accountId === "all" ? undefined : Number(accountId), 100),
    enabled: submitted.length > 0,
  });

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-3xl font-semibold tracking-tight">Search</h1>
        <p className="text-sm text-muted-foreground">
          Full-text across subject, from, to, body. Uses SQLite FTS5 syntax —
          phrase queries with quotes, AND/OR, column:value (e.g.{" "}
          <code className="rounded bg-muted px-1">from_addr:amazon.de</code>).
        </p>
      </header>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          setSubmitted(q);
        }}
        className="flex gap-2"
      >
        <Search className="mt-2 h-5 w-5 text-muted-foreground" />
        <Input
          placeholder="Search…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          autoFocus
        />
        <Select value={accountId} onValueChange={setAccountId}>
          <SelectTrigger className="w-48">
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
