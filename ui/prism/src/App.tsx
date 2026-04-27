/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import {
  createRouter,
  createRoute,
  createRootRoute,
  RouterProvider,
  Link,
  Outlet,
} from "@tanstack/react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import Dashboard from "./pages/Dashboard";
import Discover from "./pages/Discover";
import Network from "./pages/Network";
import Market from "./pages/Market";
import TracesPage from "./pages/Traces";
import Publish from "./pages/Publish";
import Datasets from "./pages/Datasets";
import Wallet from "./pages/Wallet";
import Approvals from "./pages/Approvals";

import "./vendor/ui-components/theme/theme.css";
import "./vendor/ui-index.css";

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, retry: 1 } },
});

// --- Router ---

const NAV_ITEMS = [
  { to: "/", label: "Dashboard" },
  { to: "/discover", label: "Discover" },
  { to: "/datasets", label: "Datasets" },
  { to: "/publish", label: "Publish" },
  { to: "/market", label: "Market" },
  { to: "/traces", label: "Traces" },
  { to: "/network", label: "Network" },
  { to: "/wallet", label: "Wallet" },
  { to: "/approvals", label: "Approvals" },
] as const;

const rootRoute = createRootRoute({
  component: () => (
    <div className="bg-agentprism-background text-agentprism-foreground h-screen flex flex-col">
      <nav className="flex items-center h-[50px] border-b border-agentprism-border px-4 gap-4 shrink-0 overflow-x-auto">
        <h1 className="text-sm font-semibold mr-2 shrink-0">Guixu</h1>
        {NAV_ITEMS.map((n) => (
          <Link
            key={n.to}
            to={n.to}
            className="text-xs py-1 border-b-2 transition-colors shrink-0"
            activeProps={{
              className:
                "border-agentprism-primary text-agentprism-foreground font-medium",
            }}
            inactiveProps={{
              className:
                "border-transparent text-agentprism-muted-foreground hover:text-agentprism-foreground",
            }}
          >
            {n.label}
          </Link>
        ))}
      </nav>
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
    </div>
  ),
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Dashboard,
});

const discoverRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/discover",
  validateSearch: (search: Record<string, unknown>) => ({
    q: (search.q as string) ?? "",
  }),
  component: () => {
    const { q } = discoverRoute.useSearch();
    return <Discover initialQuery={q || undefined} />;
  },
});

const datasetsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/datasets",
  component: Datasets,
});

const publishRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/publish",
  component: Publish,
});

const networkRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/network",
  component: Network,
});

const marketRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/market",
  component: Market,
});

const tracesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces",
  component: TracesPage,
});

const walletRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/wallet",
  component: Wallet,
});

const approvalsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/approvals",
  component: Approvals,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  discoverRoute,
  datasetsRoute,
  publishRoute,
  networkRoute,
  marketRoute,
  tracesRoute,
  walletRoute,
  approvalsRoute,
]);

const router = createRouter({
  routeTree,
  basepath: "/prism",
  defaultPreload: "intent",
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
