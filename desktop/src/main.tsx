import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "sonner";

import "@/index.css";
import { AppLayout } from "@/components/layout/AppLayout";
import Dashboard from "@/pages/Dashboard";
import Accounts from "@/pages/Accounts";
import AccountDetail from "@/pages/AccountDetail";
import Messages from "@/pages/Messages";
import MessageView from "@/pages/MessageView";
import SearchPage from "@/pages/Search";
import Analytics from "@/pages/Analytics";
import Subscriptions from "@/pages/Subscriptions";
import SettingsPage from "@/pages/Settings";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The dashboard polls anyway via interval; default queries shouldn't
      // hammer the backend on every mount.
      staleTime: 10_000,
      refetchOnWindowFocus: false,
    },
  },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<AppLayout />}>
            <Route index element={<Dashboard />} />
            <Route path="accounts" element={<Accounts />} />
            <Route path="accounts/:id" element={<AccountDetail />} />
            <Route path="messages" element={<Messages />} />
            <Route path="messages/:id" element={<MessageView />} />
            <Route path="search" element={<SearchPage />} />
            <Route path="analytics" element={<Analytics />} />
            <Route path="subscriptions" element={<Subscriptions />} />
            <Route path="settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
      <Toaster richColors closeButton position="top-right" />
    </QueryClientProvider>
  </React.StrictMode>,
);
