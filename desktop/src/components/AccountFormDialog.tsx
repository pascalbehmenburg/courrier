import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useProviders } from "@/hooks/useAccounts";
import type { Account, AccountPayload, Provider } from "@/types/api";

interface Props {
  open: boolean;
  editing: Account | null;
  onClose: () => void;
}

const INITIAL: AccountPayload = {
  label: "",
  email: "",
  username: "",
  password: "",
  host: "",
  port: 993,
  provider_id: "icloud",
  sync_interval_seconds: 3600,
  enabled: true,
};

export function AccountFormDialog({ open, editing, onClose }: Props) {
  const providers = useProviders();
  const [form, setForm] = useState<AccountPayload>(INITIAL);
  const qc = useQueryClient();

  useEffect(() => {
    if (editing) {
      setForm({
        label: editing.label,
        email: editing.email,
        username: editing.username,
        password: "",
        host: editing.host,
        port: editing.port,
        provider_id: editing.provider_id,
        sync_interval_seconds: editing.sync_interval_seconds,
        enabled: editing.enabled,
      });
    } else if (open) {
      setForm(INITIAL);
    }
  }, [editing, open]);

  const save = useMutation({
    mutationFn: () =>
      editing ? api.updateAccount(editing.id, form) : api.createAccount(form),
    onSuccess: () => {
      toast.success(editing ? "Account updated" : "Account added");
      qc.invalidateQueries({ queryKey: ["accounts"] });
      onClose();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const provider = providers.data?.find((p) => p.id === form.provider_id);

  function applyProvider(p: Provider) {
    setForm((f) => {
      const username =
        p.username_style === "full_email"
          ? f.email
          : p.username_style === "local_part"
            ? f.email.split("@")[0] ?? ""
            : f.username;
      return {
        ...f,
        provider_id: p.id,
        host: p.host || f.host,
        port: p.port || f.port,
        username,
      };
    });
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? "Edit account" : "Add account"}</DialogTitle>
          <DialogDescription>
            Pick a provider preset or enter a custom IMAP host. The password is
            encrypted before it touches the database.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          <Field label="Provider">
            <Select
              value={form.provider_id}
              onValueChange={(v) => {
                const p = providers.data?.find((x) => x.id === v);
                if (p) applyProvider(p);
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Pick a provider" />
              </SelectTrigger>
              <SelectContent>
                {providers.data?.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>

          {provider && (
            <p className="rounded-md border border-dashed bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
              {provider.notes}{" "}
              {provider.app_password_url && (
                <a
                  href={provider.app_password_url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-foreground underline-offset-2 hover:underline"
                >
                  Open app-password page <ExternalLink className="h-3 w-3" />
                </a>
              )}
            </p>
          )}

          <div className="grid grid-cols-2 gap-4">
            <Field label="Label">
              <Input
                value={form.label}
                onChange={(e) => setForm({ ...form, label: e.target.value })}
                placeholder="Personal iCloud"
              />
            </Field>
            <Field label="Email">
              <Input
                type="email"
                value={form.email}
                onChange={(e) => {
                  const email = e.target.value;
                  const username =
                    provider?.username_style === "full_email"
                      ? email
                      : provider?.username_style === "local_part"
                        ? email.split("@")[0] ?? ""
                        : form.username;
                  setForm({ ...form, email, username });
                }}
                placeholder="you@example.com"
              />
            </Field>
          </div>

          <div className="grid grid-cols-3 gap-4">
            <Field label="Host" className="col-span-2">
              <Input
                value={form.host}
                onChange={(e) => setForm({ ...form, host: e.target.value })}
              />
            </Field>
            <Field label="Port">
              <Input
                type="number"
                value={form.port}
                onChange={(e) => setForm({ ...form, port: Number(e.target.value) || 993 })}
              />
            </Field>
          </div>

          <Field label="Username">
            <Input
              value={form.username}
              onChange={(e) => setForm({ ...form, username: e.target.value })}
            />
          </Field>

          <Field
            label={editing ? "Password (leave blank to keep existing)" : "App password"}
          >
            <Input
              type="password"
              autoComplete="new-password"
              value={form.password}
              onChange={(e) => setForm({ ...form, password: e.target.value })}
            />
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label="Auto-sync interval (minutes, blank = off)">
              <Input
                type="number"
                min="1"
                value={
                  form.sync_interval_seconds
                    ? Math.round(form.sync_interval_seconds / 60)
                    : ""
                }
                onChange={(e) => {
                  const val = e.target.value === "" ? null : Number(e.target.value) * 60;
                  setForm({ ...form, sync_interval_seconds: val });
                }}
              />
            </Field>
            <div className="flex items-end gap-3">
              <Switch
                id="enabled"
                checked={form.enabled}
                onCheckedChange={(v) => setForm({ ...form, enabled: !!v })}
              />
              <Label htmlFor="enabled" className="text-sm">
                Enabled
              </Label>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={() => save.mutate()} disabled={save.isPending}>
            {save.isPending ? "Saving…" : editing ? "Save changes" : "Add account"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={`space-y-1.5 ${className ?? ""}`}>
      <Label className="text-xs uppercase tracking-wide text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
}
