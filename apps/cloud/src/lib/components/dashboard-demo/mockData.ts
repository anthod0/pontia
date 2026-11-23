// Static content for the homepage Dashboard demo. Nothing here talks to a device.

export type DemoSessionState = "busy" | "idle" | "waiting_for_interaction" | "exited";

export type DemoStep =
  | { kind: "read" | "edit" | "write"; files: number }
  | { kind: "command"; title: string }
  | { kind: "thinking"; text: string };

export type DemoClient = "codex" | "claude" | "pi";

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
  client: DemoClient;
  model: string;
  reasoningEffort?: string;
  /** Context window usage, as the composer shows it. */
  context: string;
  handle: string;
  prompt: string;
  /** Seconds the agent has worked on the latest turn. */
  duration: number;
  steps: DemoStep[];
  reply: DemoBlock[];
}

export type DemoChangeGroup = "staged" | "unstaged" | "untracked";

export interface DemoChange {
  path: string;
  group: DemoChangeGroup;
  status: "M" | "A" | "?";
  /** Unified diff body: lines start with "@@", "+", "-" or " ". */
  diff: string;
}

export interface DemoGit {
  branch: string;
  upstream: string;
  ahead: number;
  behind: number;
  changes: DemoChange[];
}

export interface DemoWorkspace {
  id: string;
  name: string;
  path: string;
  git: DemoGit;
}

export interface DemoBot {
  name: string;
  shape: "mochi" | "drop" | "blob" | "onigiri" | "egg" | "pudding";
  color: string;
}

export interface DemoDevice {
  handle: string;
  name: string;
  bots: DemoBot[];
  workspaces: DemoWorkspace[];
  sessions: DemoSession[];
}

export const demoDevices: DemoDevice[] = [
  {
    handle: "studio",
    name: "Studio MacBook",
    bots: [
      { name: "Atlas", shape: "mochi", color: "#458588" },
      { name: "Release Desk", shape: "drop", color: "#d79921" },
    ],
    workspaces: [
      {
        id: "atlas",
        name: "atlas",
        path: "~/code/atlas",
        git: {
          branch: "feat/command-menu",
          upstream: "origin/feat/command-menu",
          ahead: 2,
          behind: 0,
          changes: [
            {
              path: "src/lib/CommandMenu.svelte",
              group: "staged",
              status: "M",
              diff: '@@ -18,5 +18,12 @@\n   let open = $state(false);\n-  let query = "";\n+  let query = $state("");\n+\n+  function onKeydown(event: KeyboardEvent) {\n+    if ((event.metaKey || event.ctrlKey) && event.key === "k") {\n+      event.preventDefault();\n+      open = !open;\n+    }\n+  }\n </script>\n \n-<svelte:window />\n+<svelte:window onkeydown={onKeydown} />',
            },
            {
              path: "tests/command-menu.test.ts",
              group: "staged",
              status: "A",
              diff: '@@ -0,0 +1,8 @@\n+import { render } from "@testing-library/svelte";\n+import CommandMenu from "../src/lib/CommandMenu.svelte";\n+\n+test("opens with Ctrl+K", async () => {\n+  const view = render(CommandMenu);\n+  await view.user.keyboard("{Control>}k{/Control}");\n+  expect(view.getByRole("dialog")).toBeVisible();\n+});',
            },
            {
              path: "tests/checkout.test.ts",
              group: "untracked",
              status: "?",
              diff: '@@ -0,0 +1,5 @@\n+test("places an order", async () => {\n+  const cart = await seedCart({ items: 2 });\n+  const order = await checkout(cart, { card: "4242" });\n+  expect(order.status).toBe("confirmed");\n+});',
            },
            {
              path: "src/api/projects.ts",
              group: "unstaged",
              status: "M",
              diff: "@@ -40,7 +40,7 @@ export async function listProjects(params: ListParams) {\n   const rows = await db.projects.list(params);\n   return {\n-    projects: rows,\n-    next: null,\n+    items: rows,\n+    cursor: rows.at(-1)?.id ?? null,\n   };\n }",
            },
            {
              path: "tests/fixtures/payments.ts",
              group: "untracked",
              status: "?",
              diff: '@@ -0,0 +1,5 @@\n+export const fakePayments = {\n+  async charge(card: string, amount: number) {\n+    return { id: `ch_${card}`, amount, status: "succeeded" };\n+  },\n+};',
            },
          ],
        },
      },
      {
        id: "atlas-docs",
        name: "atlas-docs",
        path: "~/code/atlas-docs",
        git: {
          branch: "main",
          upstream: "origin/main",
          ahead: 0,
          behind: 0,
          changes: [
            {
              path: "src/routes/search/+page.ts",
              group: "unstaged",
              status: "M",
              diff: "@@ -6,5 +6,5 @@ export async function load({ url }) {\n   const params = Object.fromEntries(url.searchParams);\n-  const offset = (Number(params.page) - 1) * PAGE_SIZE;\n-  const page = Math.max(1, Number(params.page) || 1);\n+  const page = Math.max(1, Number(params.page) || 1);\n+  const offset = (page - 1) * PAGE_SIZE;\n   return { results: await search(params.q, { offset, limit: PAGE_SIZE }) };\n }",
            },
          ],
        },
      },
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
        reasoningEffort: "high",
        context: "38% · 76k / 200k",
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
        reasoningEffort: "high",
        context: "52% · 104k / 200k",
        handle: "atlas-review",
        prompt:
          "Review the changes in `src/api/` since the last release and flag anything that breaks existing clients.",
        duration: 136,
        steps: [
          {
            kind: "thinking",
            text: "I'll diff the API folder against the last release tag first.",
          },
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
        reasoningEffort: "medium",
        context: "21% · 42k / 200k",
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
        client: "pi",
        model: "kimi-k2",
        context: "64% · 82k / 128k",
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
        reasoningEffort: "low",
        context: "9% · 18k / 200k",
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
        reasoningEffort: "medium",
        context: "17% · 34k / 200k",
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
    bots: [{ name: "Ops", shape: "egg", color: "#98971a" }],
    workspaces: [
      {
        id: "infra",
        name: "infra",
        path: "/srv/infra",
        git: {
          branch: "ops/staging",
          upstream: "origin/ops/staging",
          ahead: 1,
          behind: 1,
          changes: [
            {
              path: "staging/lb/certs.tf",
              group: "staged",
              status: "M",
              diff: '@@ -4,5 +4,5 @@ resource "tls_certificate" "staging" {\n   domain = "staging.example.com"\n-  serial = "4f:a1:09"\n-  not_after = "2025-11-30"\n-  fingerprint = "sha256:9c1e"\n+  serial = "7b:22:e4"\n+  not_after = "2026-02-28"\n+  fingerprint = "sha256:41d0"\n }',
            },
            {
              path: "ci/e2e.yml",
              group: "unstaged",
              status: "M",
              diff: '@@ -18,3 +18,4 @@ jobs:\n       - run: pnpm build\n-      - run: pnpm test:e2e --port 3000\n+      - run: pnpm test:e2e --port "$PORT"\n+        env: { PORT: 0 }\n       - uses: actions/upload-artifact@v4',
            },
          ],
        },
      },
      {
        id: "billing-api",
        name: "billing-api",
        path: "/srv/billing-api",
        git: {
          branch: "feat/invoice-export",
          upstream: "origin/feat/invoice-export",
          ahead: 0,
          behind: 0,
          changes: [],
        },
      },
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
        reasoningEffort: "high",
        context: "47% · 94k / 200k",
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
        reasoningEffort: "medium",
        context: "29% · 58k / 200k",
        handle: "infra-ci",
        prompt: "The `e2e` job fails about one run in five. Find the cause.",
        duration: 175,
        steps: [
          {
            kind: "thinking",
            text: "The failures look timing related. I'll compare the failing runs first.",
          },
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
        reasoningEffort: "medium",
        context: "12% · 24k / 200k",
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
        client: "pi",
        model: "kimi-k2",
        context: "31% · 40k / 128k",
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
