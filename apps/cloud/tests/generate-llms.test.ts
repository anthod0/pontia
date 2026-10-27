import { expect, test } from "bun:test";
import { cp, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { generateLlms } from "../scripts/generate-llms";

async function withCloud(run: (root: string) => Promise<void>) {
  const testRoot = await mkdtemp(join(resolve(tmpdir()), "pontia-llms-"));
  try {
    await putPage(testRoot, "", "Overview", "Overview summary", "");
    await run(testRoot);
  } finally {
    if (
      !testRoot ||
      dirname(testRoot) !== resolve(tmpdir()) ||
      testRoot === "/" ||
      testRoot === homedir() ||
      homedir().startsWith(`${testRoot}${sep}`)
    )
      throw new Error("Unsafe test cleanup path");
    await rm(testRoot, { recursive: true, force: true });
  }
}

async function putPage(root: string, route: string, title: string, summary: string, body: string) {
  const path = join(root, "src/routes/docs", route, "+page.svx");
  await mkdir(dirname(path), { recursive: true });
  await writeFile(
    path,
    `<svelte:head>\n<title>${title}</title>\n<meta name="description" content="${summary}" />\n</svelte:head>\n\n# ${title}\n\n${body}\n`,
  );
}

const output = (root: string, path: string) => readFile(join(root, "static", path), "utf8");

test("indexes every nested subpage without generating docs.md", async () => {
  await withCloud(async (root) => {
    await putPage(root, "getting-started", "Setup", "Install &amp; configure", "Start here.");
    await putPage(root, "agent-clients/pi", "Client", "Client setup", "Client instructions.");
    await writeFile(join(root, "src/routes/docs/+layout.svelte"), "not documentation");

    await generateLlms(root);

    const index = await output(root, "llms.txt");
    const links = [...index.matchAll(/\]\(([^)]+)\)/g)].map((match) => match[1]);
    expect(links).toEqual([
      "https://pontia.dev/docs/agent-clients/pi.md",
      "https://pontia.dev/docs/getting-started.md",
    ]);
    expect(index).toContain("Install & configure");
    for (const link of links) {
      expect(await output(root, new URL(link).pathname)).toStartWith("# ");
    }
    expect(await readdir(join(root, "static"))).not.toContain("docs.md");
  });
});

test("preserves code examples and Markdown tables verbatim", async () => {
  await withCloud(async (root) => {
    const example =
      "```xml\n<plist>\n  <key>PATH</key>\n\n\n  <string>${HOME}/bin</string>\n</plist>\n<!-- [link](/docs/getting-started) -->\n```";
    const nested = '````md\n```sh\necho "<style>keep</style>"\n```\n````';
    const inline = "`<CLIENT_ID>` and `` `<style>example</style>` ``";
    const table = "| Key | Value |\n| --- | --- |\n| command | `pontia up` |";
    const longerClosingFence = "~~~html\n<style>keep</style>\n[link](/docs/getting-started)\n~~~~";
    const body = `${example}\n\n${nested}\n\n${longerClosingFence}\n\n${inline}\n\n${table}\n\n<http://127.0.0.1:8080/dashboard>`;
    await putPage(root, "getting-started", "Setup", "Summary", body);

    await generateLlms(root);

    expect(await output(root, "docs/getting-started.md")).toContain(body);
  });
});

test("converts Svelte presentation to text and rewrites document links and custom heading anchors", async () => {
  await withCloud(async (root) => {
    await putPage(
      root,
      "codex",
      "Codex",
      "Summary",
      '<h2 id="custom-id">Match Codex home</h2>\n\n[This section](#custom-id)',
    );
    await putPage(
      root,
      "getting-started",
      "Setup",
      "Summary",
      [
        '<script>const hidden = "script-only";</script>',
        "<style>.hidden { color: red; }</style>",
        '<figure role="img" aria-label="Browser &amp; device"><div>layout-only</div><figcaption>Encrypted connection.</figcaption></figure>',
        "[Next](/docs/codex#custom-id)",
        "[Relative](codex#custom-id)",
        "[Absolute](https://pontia.dev/docs/codex?view=text#custom-id)",
        "[Overview](/docs)",
        "[External](https://example.com/docs/codex)",
      ].join("\n\n"),
    );

    await generateLlms(root);

    const markdown = await output(root, "docs/getting-started.md");
    expect(markdown).not.toMatch(/<\/?(?:svelte:head|script|style|figure|div|figcaption)\b/);
    expect(markdown).not.toMatch(/script-only|layout-only|color: red/);
    expect(markdown).toContain("Browser & device\n\nEncrypted connection.");
    expect(markdown).toContain("[Next](https://pontia.dev/docs/codex.md#match-codex-home)");
    expect(markdown).toContain("[Relative](https://pontia.dev/docs/codex.md#match-codex-home)");
    expect(markdown).toContain(
      "[Absolute](https://pontia.dev/docs/codex.md?view=text#match-codex-home)",
    );
    expect(markdown).toContain("[Overview](https://pontia.dev/llms.txt)");
    expect(markdown).toContain("[External](https://example.com/docs/codex)");
    const codex = await output(root, "docs/codex.md");
    expect(codex).toContain("## Match Codex home");
    expect(codex).toContain("[This section](https://pontia.dev/docs/codex.md#match-codex-home)");
  });
});

test("regeneration reflects edited source and produces deterministic output", async () => {
  await withCloud(async (root) => {
    await putPage(root, "getting-started", "Setup", "Summary", "Before.");
    await generateLlms(root);
    await putPage(root, "getting-started", "Setup", "Summary", "After.");
    await generateLlms(root);
    const first = await output(root, "docs/getting-started.md");
    expect(first).toBe("# Setup\n\nAfter.\n");
    const index = await output(root, "llms.txt");
    await generateLlms(root);
    expect(await output(root, "docs/getting-started.md")).toBe(first);
    expect(await output(root, "llms.txt")).toBe(index);
  });
});

test("retires renamed and removed generated pages without touching unrelated static assets", async () => {
  await withCloud(async (root) => {
    await putPage(root, "old", "Page", "Summary", "Content.");
    await generateLlms(root);
    await writeFile(join(root, "static/docs/manual.md"), "Hand-maintained asset");
    await rename(join(root, "src/routes/docs/old"), join(root, "src/routes/docs/new"));

    await generateLlms(root);

    expect((await readdir(join(root, "static/docs"))).sort()).toEqual(["manual.md", "new.md"]);
    expect(await output(root, "llms.txt")).toContain("https://pontia.dev/docs/new.md");
    await rename(
      join(root, "src/routes/docs/new/+page.svx"),
      join(root, "src/routes/docs/new/page.backup"),
    );
    await generateLlms(root);
    expect(await readdir(join(root, "static/docs"))).toEqual(["manual.md"]);
    expect(await output(root, "docs/manual.md")).toBe("Hand-maintained asset");
    expect(await output(root, "llms.txt")).not.toContain("/docs/new.md");
  });
});

test("exports the actual documentation with intact fenced examples and resolvable Markdown links", async () => {
  await withCloud(async (root) => {
    const docs = fileURLToPath(new URL("../src/routes/docs/", import.meta.url));
    await cp(docs, join(root, "src/routes/docs"), { recursive: true });
    await generateLlms(root);
    const index = await output(root, "llms.txt");
    const pages = [...index.matchAll(/\]\(https:\/\/pontia.dev(\/docs\/[^)]+\.md)\)/g)].map(
      (match) => match[1],
    );
    const sources = (await readdir(docs, { recursive: true })).filter((path) =>
      path.endsWith(`${sep}+page.svx`),
    );
    expect(pages).toHaveLength(sources.length);
    for (const page of pages) {
      const markdown = await output(root, page);
      const source = await readFile(
        join(docs, page.slice("/docs/".length, -3), "+page.svx"),
        "utf8",
      );
      for (const block of source.matchAll(/^```[^\n]*\n[\s\S]*?^```/gm)) {
        expect(markdown).toContain(block[0]);
      }
      expect(markdown).not.toMatch(/<\/?(?:svelte:head|style|figure|h[1-6])\b/);
      for (const link of markdown.matchAll(
        /\]\(https:\/\/pontia.dev(\/docs\/[^)#?]+\.md)(?:[?#][^)]*)?\)/g,
      )) {
        expect(pages).toContain(link[1]);
      }
    }
  });
});
