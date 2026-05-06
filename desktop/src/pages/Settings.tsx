import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, ExternalLink, ServerOff } from "lucide-react";
import { toast } from "sonner";
import { api, getBackendUrl, setBackendUrl } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export default function SettingsPage() {
  const [url, setUrl] = useState(getBackendUrl());

  const health = useQuery({
    queryKey: ["health"],
    queryFn: api.health,
    retry: false,
    refetchInterval: 30000,
  });

  function save() {
    setBackendUrl(url);
    toast.success("Backend URL saved");
    window.location.reload();
  }

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Configure where the app talks to your courrier-server backend.
        </p>
      </header>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Backend</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="backend">Server URL</Label>
            <div className="flex gap-2">
              <Input
                id="backend"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://courrier.your-domain.com"
              />
              <Button onClick={save}>Save</Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Leave blank when running the desktop app against a local
              same-origin server. Set this to a remote URL when connecting to
              a Docker deployment.
            </p>
          </div>

          <div className="flex items-center gap-3 rounded-md border bg-muted/30 px-3 py-2 text-sm">
            {health.isError ? (
              <>
                <ServerOff className="h-4 w-4 text-destructive" />
                <span className="text-destructive">
                  Cannot reach backend: {(health.error as Error).message}
                </span>
              </>
            ) : health.data ? (
              <>
                <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                <span>Backend reachable.</span>
              </>
            ) : (
              <span>Checking…</span>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Encryption</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <p>
            IMAP passwords are stored AES-GCM-256 encrypted. The key lives in
            the backend's <code className="rounded bg-muted px-1">COURRIER_ENCRYPTION_KEY</code>{" "}
            environment variable (32 random bytes, base64-encoded).
          </p>
          <p className="text-muted-foreground">
            Generate a key on a Unix machine:
          </p>
          <pre className="rounded-md bg-muted p-3 text-xs">
            head -c 32 /dev/urandom | base64
          </pre>
          <p className="text-xs text-muted-foreground">
            Losing the key means losing access to stored passwords — accounts
            will need to be re-entered.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">About</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 text-sm">
          <p>
            Courrier · v0.2 ·{" "}
            <a
              href="https://github.com/pascalbehmenburg/courrier"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-foreground underline-offset-2 hover:underline"
            >
              GitHub <ExternalLink className="h-3 w-3" />
            </a>
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
