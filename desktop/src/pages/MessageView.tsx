import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, Download } from "lucide-react";
import { api } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { formatBytes, formatDate } from "@/lib/utils";

export default function MessageView() {
  const { id } = useParams();
  const messageId = Number(id);
  const { data, isLoading } = useQuery({
    queryKey: ["message", messageId],
    queryFn: () => api.getMessage(messageId),
    enabled: !!messageId,
  });

  if (isLoading || !data) {
    return <p className="text-sm text-muted-foreground">Loading message…</p>;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <Button asChild variant="ghost" size="sm" className="gap-1">
          <Link to={`/accounts/${data.account_id}`}>
            <ChevronLeft className="h-4 w-4" /> Back
          </Link>
        </Button>
        <Button asChild variant="outline" size="sm" className="gap-2">
          <a href={api.rawMessageUrl(data.id)} target="_blank" rel="noreferrer">
            <Download className="h-4 w-4" /> Raw .eml
          </a>
        </Button>
      </div>

      <Card>
        <CardContent className="space-y-4 p-6">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              {data.subject || "(no subject)"}
            </h1>
            {data.is_forwarded && (
              <div className="mt-2 flex flex-wrap gap-2">
                <Badge variant="warning">forwarded</Badge>
                {data.forwarded_from && (
                  <Badge variant="outline">via {data.forwarded_from}</Badge>
                )}
                {data.original_sender_domain && (
                  <Badge variant="outline">
                    original sender: {data.original_sender_domain}
                  </Badge>
                )}
              </div>
            )}
          </div>

          <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1 text-sm">
            <Field label="From">
              {data.from_name ? `${data.from_name} ` : ""}
              {data.from_addr ? <span className="text-muted-foreground">&lt;{data.from_addr}&gt;</span> : "—"}
            </Field>
            <Field label="To">{data.to_addrs ?? "—"}</Field>
            {data.cc_addrs && <Field label="Cc">{data.cc_addrs}</Field>}
            <Field label="Date">{formatDate(data.date_utc)}</Field>
            <Field label="Mailbox">{data.mailbox}</Field>
            <Field label="Size">{formatBytes(data.size_bytes)}</Field>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="p-6">
          <pre className="whitespace-pre-wrap break-words text-sm leading-relaxed">
            {data.body_text || "(no plain-text body extracted)"}
          </pre>
        </CardContent>
      </Card>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-medium">{children}</dd>
    </>
  );
}
