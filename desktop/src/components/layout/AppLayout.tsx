import { NavLink, Outlet } from "react-router-dom";
import {
  Inbox,
  LayoutDashboard,
  Mail,
  MailX,
  Search,
  Settings,
  Sparkles,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeToggle } from "@/components/layout/ThemeToggle";

const NAV = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
  { to: "/accounts", icon: Inbox, label: "Accounts" },
  { to: "/messages", icon: Mail, label: "Messages" },
  { to: "/search", icon: Search, label: "Search" },
  { to: "/analytics", icon: Sparkles, label: "Analytics" },
  { to: "/subscriptions", icon: MailX, label: "Subscriptions" },
  { to: "/settings", icon: Settings, label: "Settings" },
] as const;

export function AppLayout() {
  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full">
        <aside className="hidden w-60 flex-col border-r bg-muted/30 md:flex">
          <div className="flex h-16 items-center gap-2 border-b px-6">
            <span className="text-2xl">📧</span>
            <span className="text-lg font-semibold tracking-tight">Courrier</span>
          </div>
          <nav className="flex-1 space-y-1 px-3 py-4">
            {NAV.map(({ to, icon: Icon, label }) => (
              <NavLink
                key={to}
                to={to}
                end={to === "/"}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                    isActive
                      ? "bg-primary text-primary-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-foreground",
                  )
                }
              >
                <Icon className="h-4 w-4" />
                {label}
              </NavLink>
            ))}
          </nav>
          <div className="border-t p-4">
            <ThemeToggle />
          </div>
        </aside>

        <main className="flex-1 overflow-auto">
          <div className="mx-auto max-w-7xl px-6 py-8">
            <Outlet />
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}
