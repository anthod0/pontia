import type { Shape } from "$lib/bot-stories";
// Static content for the homepage Dashboard demo. Nothing here talks to a device.

export type DemoSessionState = "busy" | "idle" | "waiting_for_interaction" | "exited";

export type DemoStep =
  | { kind: "read" | "edit" | "write"; files: number }
  | { kind: "command"; title: string }
  | { kind: "thinking"; text: string };

export const demoClients = {
  codex: { name: "Codex", logo: "codex", models: ["gpt-6.2-astra", "gpt-6.2-sol"] },
  claude: { name: "Claude Code", logo: "claude-code", models: ["opus"] },
  pi: { name: "Pi", logo: "pi", models: ["gpt-6.2-astra"] },
  kiro: { name: "Kiro", logo: "kiro", models: [] },
  grok: { name: "Grok Build", logo: "grok-build", models: ["grok-4.7"] },
  antigravity: {
    name: "Google Antigravity",
    logo: "google-antigravity",
    models: ["gemini-4-neon"],
  },
} as const;

export type DemoClient = keyof typeof demoClients;

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
  files?: Record<string, string>;
  recentFiles?: string[];
}

export type DemoChangeGroup = "staged" | "unstaged" | "untracked";

export interface DemoChange {
  path: string;
  group: DemoChangeGroup;
  status: "M" | "A" | "D" | "?";
  /** Unified diff body: lines start with "@@", "+", "-" or " ". */
  diff: string;
}

export interface DemoCommit {
  hash: string;
  subject: string;
  body?: string;
  author: string;
  /** Day heading in History: "today", "yesterday" or a date. */
  day: string;
  time: string;
  pushed: boolean;
  parents: string[];
  files: Omit<DemoChange, "group">[];
}

export interface DemoGit {
  branch: string;
  upstream: string;
  ahead: number;
  behind: number;
  /** Newest first, as the History view lists them. */
  commits: DemoCommit[];
  changes: DemoChange[];
}

export interface DemoWorkspace {
  id: string;
  name: string;
  path: string;
  git: DemoGit;
}

/** A Bot reply, kept to the Markdown the demo renders. */
export type DemoBotBlock = Exclude<DemoBlock, { kind: "code" }>;

export type DemoTodo = "open" | { by: "user" | "bot"; at: string };

export type DemoBotEntry =
  | {
      id: string;
      kind: "user";
      text: string;
      time: string;
      /** The id of the quoted message. */
      quote?: string;
      failed?: boolean;
    }
  | { id: string; kind: "bot"; blocks: DemoBotBlock[]; time: string; todo?: DemoTodo }
  | { id: string; kind: "notice"; text: string; time: string };

export interface DemoBot {
  id: string;
  name: string;
  shape: Shape;
  color: string;
  entries: DemoBotEntry[];
  /** What the Bot is doing now, shown above the composer. */
  working?: string;
  /** Recently opened files, newest first. */
  recentFiles: string[];
  /** File contents for the Files tab, by path. Bot replies link to these paths. */
  files: Record<string, string>;
}

export interface DemoDevice {
  handle: string;
  name: string;
  clients: DemoClient[];
  bots: DemoBot[];
  workspaces: DemoWorkspace[];
  sessions: DemoSession[];
}

export const demoDevices: DemoDevice[] = [
  {
    handle: "studio",
    name: "Studio MacBook",
    clients: ["codex", "claude", "pi", "grok", "antigravity", "kiro"],
    bots: [
      {
        id: "atlas",
        name: "Atlas",
        shape: "mochi",
        color: "#458588",
        working: "Updating docs/release-notes.md",
        recentFiles: ["docs/release-notes.md", "reports/release-2.3.md"],
        files: {
          "docs/release-notes.md":
            "# Atlas 2.4\n\nRelease notes drafted from this week's merged pull requests.\n\n## Highlights\n\n- **Command menu**: open it anywhere with `⌘K` / `Ctrl+K`.\n- **Cursor pagination** for project lists.\n- A new **Settings** page with profile, notifications and danger zone.\n\n## Breaking changes\n\n> `GET /projects` now returns `items` instead of `projects`.\n\n```ts\nconst { items, cursor } = await atlas.projects.list({ limit: 50 });\n```\n\n| Area | Change | PR |\n| --- | --- | --- |\n| API | `items` + `cursor` | #412 |\n| UI | Command menu | #398 |\n",
          "reports/release-2.3.md":
            "# Release 2.3 check\n\nDeploy: finished at 21:02\nMigrations: 3 applied on staging\nSmoke tests: 42 passed\n\nStaging certificate expires on Nov 28.\n",
        },
        entries: [
          {
            id: "a1",
            kind: "user",
            text: "Check whether last night's release went out cleanly.",
            time: "Nov 20, 9:14 PM",
          },
          {
            id: "a2",
            kind: "bot",
            time: "Nov 20, 9:16 PM",
            todo: "open",
            blocks: [
              {
                kind: "p",
                text: "The release finished and the migrations ran on staging. Details are in [release-2.3.md](reports/release-2.3.md).",
              },
              {
                kind: "p",
                text: "The staging certificate expires this week. **Should I renew it?**",
              },
            ],
          },
          {
            id: "a3",
            kind: "user",
            quote: "a2",
            text: "Hold off until I confirm the budget.",
            time: "Nov 20, 9:20 PM",
            failed: true,
          },
          {
            id: "a4",
            kind: "bot",
            time: "Nov 21, 2:05 PM",
            todo: { by: "user", at: "Nov 22, 8:20 AM" },
            blocks: [
              {
                kind: "p",
                text: "The CI build cache is using 38 GB. Should I clear the entries older than a month?",
              },
            ],
          },
          { id: "a5", kind: "notice", text: "Bot Session exited", time: "Nov 22, 6:02 PM" },
          {
            id: "a6",
            kind: "user",
            text: "Summarize what changed in atlas this week and write it up as release notes.",
            time: "9:12 AM",
          },
          {
            id: "a7",
            kind: "bot",
            time: "9:15 AM",
            blocks: [
              {
                kind: "p",
                text: "The draft is in [release-notes.md](docs/release-notes.md). Highlights:",
              },
              {
                kind: "ul",
                items: [
                  "**⌘K** opens the command menu.",
                  "Project lists page with a cursor.",
                  "The new settings page has three sections.",
                ],
              },
              {
                kind: "p",
                text: "Clients must act on one change: `GET /projects` now returns `items`. The preview build is at [localhost:5173/projects](http://localhost:5173/projects).",
              },
            ],
          },
          {
            id: "a8",
            kind: "user",
            quote: "a7",
            text: "Add a migration note for SDK users.",
            time: "10:40 AM",
          },
        ],
      },
      {
        id: "release-desk",
        name: "Release Desk",
        shape: "drop",
        color: "#d79921",
        recentFiles: [],
        files: {},
        entries: [
          {
            id: "r1",
            kind: "user",
            text: "What is still blocking the 2.4 release?",
            time: "8:30 AM",
          },
          {
            id: "r2",
            kind: "bot",
            time: "8:31 AM",
            blocks: [
              { kind: "p", text: "Two things:" },
              {
                kind: "ul",
                items: [
                  "The checkout tests fail on tax rounding.",
                  "The changelog needs your approval.",
                ],
              },
            ],
          },
          {
            id: "r3",
            kind: "bot",
            time: "8:52 AM",
            todo: "open",
            blocks: [
              { kind: "p", text: "The checkout tests pass now. **Can I publish the changelog?**" },
            ],
          },
        ],
      },
    ],
    workspaces: [
      {
        id: "atlas",
        name: "atlas",
        path: "~/code/atlas",
        git: {
          branch: "feat/command-menu",
          upstream: "origin/feat/command-menu",
          commits: [
            {
              hash: "4c2a9d8",
              subject: "Open the command menu with ⌘K",
              body: "Escape closes the menu and returns focus to the trigger.",
              author: "Ada Lovelace",
              day: "today",
              time: "10:26 AM",
              pushed: false,
              parents: ["9b1e04f"],
              files: [
                {
                  path: "src/lib/CommandMenu.svelte",
                  status: "M",
                  diff: '@@ -12,6 +12,13 @@\n   let open = $state(false);\n+\n+  function onKeydown(event: KeyboardEvent) {\n+    if ((event.metaKey || event.ctrlKey) && event.key === "k") {\n+      event.preventDefault();\n+      open = !open;\n+    }\n+  }\n </script>',
                },
                {
                  path: "src/lib/shortcuts.ts",
                  status: "A",
                  diff: '@@ -0,0 +1,3 @@\n+export const shortcuts = {\n+  commandMenu: "mod+k",\n+};',
                },
              ],
            },
            {
              hash: "9b1e04f",
              subject: "Add the command menu skeleton",
              author: "Ada Lovelace",
              day: "today",
              time: "9:58 AM",
              pushed: false,
              parents: ["e71c2aa"],
              files: [
                {
                  path: "src/lib/CommandMenu.svelte",
                  status: "A",
                  diff: '@@ -0,0 +1,4 @@\n+<script lang="ts">\n+  let open = $state(false);\n+</script>\n+<dialog {open}></dialog>',
                },
              ],
            },
            {
              hash: "e71c2aa",
              subject: "Merge branch 'main' into feat/command-menu",
              author: "Grace Hopper",
              day: "today",
              time: "9:40 AM",
              pushed: true,
              parents: ["3f8d6b1", "51ac0e9"],
              files: [
                {
                  path: "src/api/projects.ts",
                  status: "M",
                  diff: "@@ -40,4 +40,4 @@\n   return {\n-    projects: rows,\n+    items: rows,\n   };",
                },
              ],
            },
            {
              hash: "3f8d6b1",
              subject: "Paginate projects with a cursor",
              author: "Grace Hopper",
              day: "yesterday",
              time: "6:12 PM",
              pushed: true,
              parents: ["51ac0e9"],
              files: [
                {
                  path: "src/api/projects.ts",
                  status: "M",
                  diff: "@@ -40,5 +40,5 @@\n   const rows = await db.projects.list(params);\n   return {\n-    next: null,\n+    cursor: rows.at(-1)?.id ?? null,\n   };",
                },
              ],
            },
            {
              hash: "51ac0e9",
              subject: "Add the settings page",
              author: "Ada Lovelace",
              day: "yesterday",
              time: "2:30 PM",
              pushed: true,
              parents: ["0d94f27"],
              files: [
                {
                  path: "src/routes/settings/+page.svelte",
                  status: "A",
                  diff: "@@ -0,0 +1,3 @@\n+<h1>Settings</h1>\n+<ProfileForm />\n+<DangerZone />",
                },
              ],
            },
            {
              hash: "0d94f27",
              subject: "Release 2.3.0",
              author: "Grace Hopper",
              day: "Nov 21, 2026",
              time: "9:02 PM",
              pushed: true,
              parents: ["8a1b3c2"],
              files: [
                {
                  path: "package.json",
                  status: "M",
                  diff: '@@ -2,3 +2,3 @@\n   "name": "atlas",\n-  "version": "2.2.4",\n+  "version": "2.3.0",',
                },
              ],
            },
          ],
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
          commits: [
            {
              hash: "9ef80c4",
              subject: "Document the projects endpoint",
              author: "Grace Hopper",
              day: "today",
              time: "8:15 AM",
              pushed: true,
              parents: ["2b7d0e1"],
              files: [
                {
                  path: "api.md",
                  status: "M",
                  diff: "@@ -1 +1,2 @@\n Projects\n+Use GET /projects to list projects.",
                },
              ],
            },
            {
              hash: "2b7d0e1",
              subject: "Add the search page",
              author: "Ada Lovelace",
              day: "Nov 20, 2026",
              time: "4:40 PM",
              pushed: true,
              parents: ["77c0f3a"],
              files: [
                {
                  path: "src/routes/search/+page.ts",
                  status: "A",
                  diff: "@@ -0,0 +1 @@\n+export const load = () => ({ results: [] });",
                },
              ],
            },
          ],
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
        model: "gpt-6.2-astra",
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
            text: "All **12 tests pass**. The dev server is running at [localhost:5173/projects](http://localhost:5173/projects), press **⌘K** there to try it.",
          },
        ],
      },
      {
        id: "review-api",
        title: "Review API changes",
        workspaceId: "atlas",
        state: "busy",
        client: "claude",
        model: "opus",
        reasoningEffort: "high",
        context: "52% · 104k / 200k",
        handle: "atlas-review",
        recentFiles: ["notes/api-review.md", "src/api/projects.ts"],
        files: {
          "notes/api-review.md":
            "# API review: v2.3.0..HEAD\n\nChecked **14 files** in `src/api/` against the last release.\n\n## Breaking changes\n\n- `GET /projects` returns `items` instead of `projects`.\n- Older SDKs read `projects` and will see an empty list.\n\n> Ship a migration note with 2.4 and keep `projects` as an alias for one release.\n\n## Compatible changes\n\n| Change | Clients |\n| --- | --- |\n| Optional `cursor` parameter | Unaffected |\n| New `next` link header | Unaffected |\n\n```ts\nconst { items, cursor } = await atlas.projects.list({ limit: 50 });\n```\n",
          "src/api/projects.ts":
            "export async function listProjects(params: ListParams) {\n  const rows = await db.projects.list(params);\n  return {\n    items: rows,\n    cursor: rows.at(-1)?.id ?? null,\n  };\n}",
        },
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
            text: "Compared `v2.3.0..HEAD` across 14 changed files. So far I found one breaking change in [projects.ts](src/api/projects.ts):",
          },
          {
            kind: "ul",
            items: [
              "`GET /projects` now returns `items` instead of `projects`. Older SDKs read the old key and will see an empty list.",
              "The new `cursor` parameter is optional, so pagination stays compatible.",
            ],
          },
          {
            kind: "p",
            text: "Notes so far are in [api-review.md](notes/api-review.md). Checking the webhook payloads next.",
          },
        ],
      },
      {
        id: "integration-tests",
        title: "Add integration tests",
        workspaceId: "atlas",
        state: "busy",
        client: "pi",
        model: "gpt-6.2-astra",
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
        client: "grok",
        model: "grok-4.7",
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
        model: "gpt-6.2-sol",
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
        client: "antigravity",
        model: "gemini-4-neon",
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
    clients: ["claude", "codex", "pi", "grok"],
    bots: [
      {
        id: "ops",
        name: "Ops",
        shape: "egg",
        color: "#98971a",
        recentFiles: ["reports/staging-overnight.md"],
        files: {
          "reports/staging-overnight.md":
            "# Staging overnight\n\nUptime: 100%\np95 latency: 182 ms\nErrors: 3 (all retried)\n\nbuild-02 disk: 91% used\n",
        },
        entries: [
          {
            id: "o1",
            kind: "user",
            text: "Watch staging overnight and tell me if anything breaks.",
            time: "Nov 22, 11:40 PM",
          },
          {
            id: "o2",
            kind: "bot",
            time: "7:05 AM",
            blocks: [
              {
                kind: "p",
                text: "Staging stayed up all night. The report is in [staging-overnight.md](reports/staging-overnight.md).",
              },
            ],
          },
          {
            id: "o3",
            kind: "bot",
            time: "7:06 AM",
            todo: "open",
            blocks: [
              {
                kind: "p",
                text: "The disk on build-02 is 91% full. **Should I prune old Docker images?**",
              },
            ],
          },
        ],
      },
    ],
    workspaces: [
      {
        id: "infra",
        name: "infra",
        path: "/srv/infra",
        git: {
          branch: "ops/staging",
          upstream: "origin/ops/staging",
          commits: [
            {
              hash: "d41e7b0",
              subject: "Rotate staging certificates",
              author: "Ada Lovelace",
              day: "today",
              time: "7:48 AM",
              pushed: false,
              parents: ["6bca583"],
              files: [
                {
                  path: "staging/lb/certs.tf",
                  status: "M",
                  diff: '@@ -5,2 +5,2 @@\n-  not_after = "2025-11-30"\n+  not_after = "2026-02-28"',
                },
              ],
            },
            {
              hash: "6bca583",
              subject: "Configure staging replicas",
              author: "Grace Hopper",
              day: "yesterday",
              time: "11:20 PM",
              pushed: true,
              parents: ["1f0a9c4"],
              files: [
                {
                  path: "staging.yaml",
                  status: "M",
                  diff: "@@ -1 +1 @@\n-replicas: 1\n+replicas: 2",
                },
              ],
            },
          ],
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
          commits: [
            {
              hash: "a850d1e",
              subject: "Add the invoice export route",
              author: "Ada Lovelace",
              day: "yesterday",
              time: "3:05 PM",
              pushed: true,
              parents: ["c3e2d19"],
              files: [
                {
                  path: "routes.ts",
                  status: "M",
                  diff: '@@ -8,0 +9 @@\n+router.get("/invoices/export", exportInvoices);',
                },
              ],
            },
          ],
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
        model: "opus",
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
        model: "gpt-6.2-astra",
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
        model: "gpt-6.2-astra",
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
        client: "grok",
        model: "grok-4.7",
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
