// Static content for the homepage Dashboard demo. Nothing here talks to a device.

export type DemoSessionState = "busy" | "idle" | "waiting_for_interaction" | "exited";

export type DemoStep =
  | { kind: "read" | "edit" | "write"; files: number }
  | { kind: "command"; title: string };

export type DemoBlock =
  | { kind: "p"; text: string }
  | { kind: "ul"; items: string[] }
  | { kind: "code"; language: string; code: string };

export interface DemoSession {
  id: string;
  title: string;
  workspaceId: string;
  state: DemoSessionState;
  pinned?: boolean;
  client: string;
  model: string;
  branch: string;
  handle: string;
  prompt: string;
  /** Seconds the agent has worked on the latest turn. */
  duration: number;
  steps: DemoStep[];
  reply: DemoBlock[];
}

export interface DemoWorkspace {
  id: string;
  name: string;
  path: string;
}

export interface DemoDevice {
  handle: string;
  name: string;
  workspaces: DemoWorkspace[];
  sessions: DemoSession[];
}

export const demoDevices: DemoDevice[] = [
  {
    handle: "studio",
    name: "Studio MacBook",
    workspaces: [
      { id: "atlas", name: "atlas", path: "~/code/atlas" },
      { id: "atlas-docs", name: "atlas-docs", path: "~/code/atlas-docs" },
    ],
    sessions: [
      {
        id: "keyboard-shortcuts",
        title: "Add keyboard shortcuts",
        workspaceId: "atlas",
        state: "idle",
        pinned: true,
        client: "codex",
        model: "gpt-5.4",
        branch: "feat/keyboard-shortcuts",
        handle: "atlas-ui",
        prompt:
          "Add keyboard shortcuts to the command menu. Use ⌘K to open it and Escape to close it, and cover both with tests.",
        duration: 83,
        steps: [
          { kind: "read", files: 1 },
          { kind: "edit", files: 1 },
          { kind: "command", title: "Run command" },
        ],
        reply: [
          { kind: "p", text: "Added keyboard shortcuts to the command menu." },
          {
            kind: "ul",
            items: [
              "**⌘K / Ctrl+K** opens the menu from anywhere in the app.",
              "**Escape** closes it and returns focus to the trigger.",
              "Shortcuts stay inactive while typing in an input.",
            ],
          },
          {
            kind: "code",
            language: "ts",
            code: 'function onKeydown(event: KeyboardEvent) {\n  if ((event.metaKey || event.ctrlKey) && event.key === "k") {\n    event.preventDefault();\n    open = !open;\n  }\n}',
          },
          {
            kind: "p",
            text: "All **12 tests pass**, including keyboard navigation and focus restoration.",
          },
        ],
      },
      {
        id: "review-api",
        title: "Review API changes",
        workspaceId: "atlas",
        state: "busy",
        client: "claude",
        model: "claude-sonnet-4.5",
        branch: "main",
        handle: "atlas-review",
        prompt:
          "Review the changes in `src/api/` since the last release and flag anything that breaks existing clients.",
        duration: 136,
        steps: [
          { kind: "command", title: "Run command" },
          { kind: "read", files: 6 },
        ],
        reply: [
          {
            kind: "p",
            text: "Compared `v2.3.0..HEAD` across 14 changed files. So far I found one breaking change:",
          },
          {
            kind: "ul",
            items: [
              "`GET /projects` now returns `items` instead of `projects`. Older SDKs read the old key and will see an empty list.",
              "The new `cursor` parameter is optional, so pagination stays compatible.",
            ],
          },
          { kind: "p", text: "Checking the webhook payloads next." },
        ],
      },
      {
        id: "integration-tests",
        title: "Add integration tests",
        workspaceId: "atlas",
        state: "busy",
        client: "pi",
        model: "gpt-5.4",
        branch: "test/checkout-flow",
        handle: "atlas-tests",
        prompt: "Write integration tests for the checkout flow, from cart to order confirmation.",
        duration: 248,
        steps: [
          { kind: "read", files: 4 },
          { kind: "write", files: 2 },
          { kind: "command", title: "Run command" },
        ],
        reply: [
          {
            kind: "p",
            text: "Created `tests/checkout.test.ts` with a seeded cart and a fake payment provider.",
          },
          {
            kind: "code",
            language: "ts",
            code: 'test("places an order", async () => {\n  const cart = await seedCart({ items: 2 });\n  const order = await checkout(cart, { card: "4242" });\n  expect(order.status).toBe("confirmed");\n});',
          },
          { kind: "p", text: "5 of 7 tests pass. Fixing the tax rounding case now." },
        ],
      },
      {
        id: "settings-page",
        title: "Build the settings page",
        workspaceId: "atlas",
        state: "exited",
        client: "opencode",
        model: "kimi-k2",
        branch: "feat/settings",
        handle: "atlas-settings",
        prompt: "Build a settings page with profile, notifications, and danger zone sections.",
        duration: 191,
        steps: [
          { kind: "read", files: 3 },
          { kind: "write", files: 4 },
          { kind: "command", title: "Run command" },
        ],
        reply: [
          { kind: "p", text: "Added `/settings` with three sections:" },
          {
            kind: "ul",
            items: [
              "**Profile** edits name, avatar, and time zone.",
              "**Notifications** toggles email and push alerts per project.",
              "**Danger zone** deletes the account after a typed confirmation.",
            ],
          },
          { kind: "p", text: "The page passes type checks and the existing route tests." },
        ],
      },
      {
        id: "search-pagination",
        title: "Fix search pagination",
        workspaceId: "atlas-docs",
        state: "exited",
        client: "codex",
        model: "gpt-5.4-mini",
        branch: "fix/search-pages",
        handle: "docs-search",
        prompt: "Search results repeat on page 2. Find out why and fix it.",
        duration: 57,
        steps: [
          { kind: "read", files: 2 },
          { kind: "edit", files: 1 },
        ],
        reply: [
          {
            kind: "p",
            text: "The offset was computed from the page number before it was clamped, so page 2 started at 0.",
          },
          {
            kind: "code",
            language: "ts",
            code: "const page = Math.max(1, Number(params.page) || 1);\nconst offset = (page - 1) * PAGE_SIZE;",
          },
          { kind: "p", text: "Each page now returns distinct results." },
        ],
      },
      {
        id: "mobile-navigation",
        title: "Improve mobile navigation",
        workspaceId: "atlas-docs",
        state: "exited",
        client: "claude",
        model: "claude-haiku-4.5",
        branch: "feat/mobile-nav",
        handle: "docs-nav",
        prompt: "The docs sidebar covers the page on phones. Turn it into a drawer.",
        duration: 112,
        steps: [
          { kind: "read", files: 2 },
          { kind: "edit", files: 3 },
        ],
        reply: [
          {
            kind: "p",
            text: "Below 768px the sidebar now opens as a drawer from the menu button.",
          },
          {
            kind: "ul",
            items: [
              "The drawer closes after a link is chosen.",
              "**Escape** and the backdrop also close it.",
              "Body scrolling is locked while it is open.",
            ],
          },
        ],
      },
    ],
  },
  {
    handle: "build-server",
    name: "Build Server",
    workspaces: [
      { id: "infra", name: "infra", path: "/srv/infra" },
      { id: "billing-api", name: "billing-api", path: "/srv/billing-api" },
    ],
    sessions: [
      {
        id: "postgres-upgrade",
        title: "Upgrade Postgres to 17",
        workspaceId: "infra",
        state: "idle",
        pinned: true,
        client: "claude",
        model: "claude-sonnet-4.5",
        branch: "ops/postgres-17",
        handle: "infra-db",
        prompt: "Plan the Postgres 16 → 17 upgrade for staging and run it.",
        duration: 402,
        steps: [
          { kind: "read", files: 3 },
          { kind: "command", title: "Run command" },
          { kind: "edit", files: 2 },
        ],
        reply: [
          { kind: "p", text: "Staging now runs **Postgres 17.2**." },
          {
            kind: "code",
            language: "bash",
            code: "pg_upgrade --link \\\n  -b /usr/lib/postgresql/16/bin \\\n  -B /usr/lib/postgresql/17/bin \\\n  -d /var/lib/postgresql/16/main \\\n  -D /var/lib/postgresql/17/main",
          },
          { kind: "p", text: "Downtime was 48 seconds. All migrations and smoke tests pass." },
        ],
      },
      {
        id: "flaky-ci",
        title: "Investigate flaky CI job",
        workspaceId: "infra",
        state: "busy",
        client: "codex",
        model: "gpt-5.4",
        branch: "main",
        handle: "infra-ci",
        prompt: "The `e2e` job fails about one run in five. Find the cause.",
        duration: 175,
        steps: [
          { kind: "command", title: "Run command" },
          { kind: "read", files: 2 },
        ],
        reply: [
          {
            kind: "p",
            text: "The last 40 failures all time out in `waitForServer`. The test server binds to port 3000, which a cache container also uses on some runners.",
          },
          { kind: "p", text: "Trying a random free port to confirm." },
        ],
      },
      {
        id: "staging-certs",
        title: "Rotate staging certificates",
        workspaceId: "infra",
        state: "waiting_for_interaction",
        client: "pi",
        model: "gpt-5.4",
        branch: "ops/certs",
        handle: "infra-certs",
        prompt: "Rotate the TLS certificates on the staging load balancer.",
        duration: 64,
        steps: [
          { kind: "read", files: 1 },
          { kind: "command", title: "Run command" },
        ],
        reply: [
          { kind: "p", text: "New certificates are issued and staged." },
          {
            kind: "p",
            text: "Reloading the load balancer drops open connections for a moment. **Should I reload it now?**",
          },
        ],
      },
      {
        id: "invoice-export",
        title: "Add invoice export endpoint",
        workspaceId: "billing-api",
        state: "exited",
        client: "opencode",
        model: "kimi-k2",
        branch: "feat/invoice-export",
        handle: "billing-export",
        prompt: "Add an endpoint that exports a month of invoices as CSV.",
        duration: 146,
        steps: [
          { kind: "read", files: 3 },
          { kind: "write", files: 2 },
          { kind: "command", title: "Run command" },
        ],
        reply: [
          { kind: "p", text: "Added `GET /invoices/export?month=2025-10`." },
          {
            kind: "ul",
            items: [
              "Streams rows, so large months stay within memory limits.",
              "Amounts use the invoice currency with two decimals.",
              "Requires the `billing:read` scope.",
            ],
          },
        ],
      },
    ],
  },
];
