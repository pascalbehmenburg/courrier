import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Mail } from "lucide-react";
import { api } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccounts } from "@/hooks/useAccounts";
import { formatRelative } from "@/lib/utils";

const PAGE_SIZE = 50;

export default function Messages() {
  const accounts = useAccounts();
  const [accountId, setAccountId] = useState<string>("all");
  const [offset, setOffset] = useState(0);

  const messages = useQuery({
    queryKey: ["messages", accountId, offset],
    queryFn: () =>
      api.listMessages({
        account_id: accountId === "all" ? undefined : Number(accountId),
        limit: PAGE_SIZE,
        offset,
      }),
  });

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Messages</h1>
          <p className="text-sm text-muted-foreground">
            Latest mail across your accounts.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Select
            value={accountId}
            onValueChange={(v) => {
              setAccountId(v);
              setOffset(0);
            }}
          >
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
        </div>
      </header>

      {messages.isLoading ? (
        <p className="text-sm text-muted-foreground">Loading…</p>
      ) : !messages.data?.length ? (
        <div className="flex flex-col items-center gap-2 rounded-lg border py-16 text-sm text-muted-foreground">
          <Mail className="h-10 w-10 opacity-30" />
          <span>No messages.</span>
        </div>
      ) : (
        <>
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
          <div className="flex items-center justify-between">
            <p className="text-xs text-muted-foreground">
              Showing {offset + 1}–{offset + messages.data.length}
            </p>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                Previous
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={messages.data.length < PAGE_SIZE}
                onClick={() => setOffset(offset + PAGE_SIZE)}
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
