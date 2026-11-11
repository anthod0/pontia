import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const cloudRoot = fileURLToPath(new URL("../", import.meta.url));
const origin = "https://pontia.dev";

function decodeEntities(text: string): string {
  const entities: Record<string, string> = {
    amp: "&",
    lt: "<",
    gt: ">",
    quot: '"',
    apos: "'",
    nbsp: " ",
  };
  return text.replace(/&(#x[\da-f]+|#\d+|\w+);/gi, (entity, name: string) => {
    if (name.startsWith("#")) {
      return String.fromCodePoint(
        name[1].toLowerCase() === "x" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10),
      );
    }
    return entities[name] ?? entity;
  });
}

function headingSlug(text: string): string {
  return decodeEntities(text)
    .toLowerCase()
    .replace(/[^\p{L}\p{N}_\s-]/gu, "")
    .replace(/\s/g, "-");
}

function protectCode(source: string) {
  const blocks: string[] = [];
  const save = (code: string) => `\0${blocks.push(code) - 1}\0`;
  const lines = source.split("\n");
  const protectedLines: string[] = [];
  for (let index = 0; index < lines.length; index++) {
    const fence = lines[index].match(/^ {0,3}(`{3,}|~{3,})/)?.[1];
    if (!fence) {
      protectedLines.push(lines[index]);
      continue;
    }
    const start = index;
    const closing = new RegExp(`^ {0,3}${fence[0]}{${fence.length},}[ \\t\\r]*$`);
    while (++index < lines.length && !closing.test(lines[index])) {}
    protectedLines.push(save(lines.slice(start, index + 1).join("\n")));
  }
  const text = protectedLines.join("\n").replace(/(`+)[\s\S]*?\1(?!`)/g, save);
  return {
    text,
    restore: (markdown: string) =>
      markdown.replace(/\0(\d+)\0/g, (_, index) => blocks[Number(index)]),
  };
}

async function findPages(directory: string): Promise<string[]> {
  const files: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await findPages(path)));
    else if (entry.name === "+page.svx") files.push(path);
  }
  return files.sort();
}

function routeOf(base: string, sourceRoot: string, path: string): string {
  return `${base}${relative(sourceRoot, dirname(path))
    .split(sep)
    .filter(Boolean)
    .map((part) => `/${part}`)
    .join("")}`;
}

function collectAnchors(text: string, anchors: Map<string, string>): string {
  return text.replace(
    /<h([1-6])\b[^>]*\bid="([^"]+)"[^>]*>(.*?)<\/h\1>/g,
    (_, level, id, heading) => {
      anchors.set(id, headingSlug(heading));
      return `${"#".repeat(Number(level))} ${heading}`;
    },
  );
}

async function readDocs(root: string) {
  const docsRoot = join(root, "src/routes/docs");
  return Promise.all(
    (await findPages(docsRoot)).map(async (path) => {
      const code = protectCode(await readFile(path, "utf8"));
      const title = code.text.match(/^# (.+)$/m)?.[1];
      const description = code.text.match(
        /<meta\s+name="description"\s+content="([^"]*)"\s*\/?>/,
      )?.[1];
      if (!title || !description)
        throw new Error(`Missing documentation title or description: ${path}`);
      const anchors = new Map<string, string>();
      return {
        route: routeOf("/docs", docsRoot, path),
        title,
        description: decodeEntities(description),
        text: collectAnchors(code.text, anchors),
        anchors,
        restore: code.restore,
      };
    }),
  );
}

/** Reads the flat `key: value` frontmatter that blog posts use. */
function frontmatter(source: string, path: string) {
  const block = source.match(/^---\n([\s\S]*?)\n---\n/);
  if (!block) throw new Error(`Missing blog frontmatter: ${path}`);
  const fields: Record<string, string> = {};
  for (const line of block[1].split("\n")) {
    const field = line.match(/^(\w+):\s*(.*)$/);
    if (!field) continue;
    const value = field[2].trim();
    fields[field[1]] = value.startsWith('"') ? JSON.parse(value) : value;
  }
  const { title, description, published } = fields;
  if (!title || !description || !published)
    throw new Error(`Missing blog title, description, or published date: ${path}`);
  return { title, description, published, body: source.slice(block[0].length) };
}

async function readBlog(root: string) {
  const blogRoot = join(root, "src/routes/blog");
  const posts = await Promise.all(
    (await findPages(blogRoot)).map(async (path) => {
      const post = frontmatter(await readFile(path, "utf8"), path);
      const code = protectCode(`# ${post.title}\n\n${post.body}`);
      const anchors = new Map<string, string>();
      return {
        route: routeOf("/blog", blogRoot, path),
        title: post.title,
        description: post.description,
        published: post.published,
        text: collectAnchors(code.text, anchors),
        anchors,
        restore: code.restore,
      };
    }),
  );
  return posts.sort(
    (a, b) => b.published.localeCompare(a.published) || a.route.localeCompare(b.route),
  );
}

export async function generateLlms(root = cloudRoot): Promise<void> {
  const docs = await readDocs(root);
  const blog = await readBlog(root);
  const pages = [...docs, ...blog];
  const byRoute = new Map(pages.map((page) => [page.route, page]));
  const overview = byRoute.get("/docs");
  if (!overview) throw new Error("Missing docs overview: src/routes/docs/+page.svx");

  const indexPath = join(root, "static/llms.txt");
  let previousIndex = "";
  try {
    previousIndex = await readFile(indexPath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  await mkdir(dirname(indexPath), { recursive: true });

  const entries = new Map<string, string>();
  for (const page of pages) {
    if (page.route === "/docs") continue;
    const markdown = page.text
      .replace(/<(svelte:head|script|style)\b[^>]*>[\s\S]*?<\/\1>/g, "")
      // The diagram's accessible description conveys its meaning without its layout.
      .replace(/<figure\b([^>]*)>[\s\S]*?<\/figure>/g, (figure, attributes: string) => {
        const label = attributes.match(/\baria-label="([^"]*)"/)?.[1];
        if (!label) throw new Error(`Figure needs an accessible description: ${page.route}`);
        const caption = figure.match(/<figcaption\b[^>]*>([\s\S]*?)<\/figcaption>/)?.[1];
        return [label, caption]
          .filter((text) => text !== undefined)
          .map((text) => decodeEntities(text))
          .join("\n\n");
      })
      .replace(/\]\(([^\s)]+)\)/g, (link, target: string) => {
        const url = new URL(target, `${origin}${page.route}`);
        if (url.origin !== origin) return link;
        const destination = byRoute.get(url.pathname.replace(/\/$/, ""));
        if (!destination) return link;
        if (url.hash) url.hash = destination.anchors.get(url.hash.slice(1)) ?? url.hash;
        url.pathname = destination.route === "/docs" ? "/llms.txt" : `${destination.route}.md`;
        return `](${url.href})`;
      })
      .replace(/\n{3,}/g, "\n\n")
      .trim();
    const output = join(root, "static", `${page.route.slice(1)}.md`);
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, `${page.restore(markdown)}\n`);
    entries.set(page.route, `- [${page.title}](${origin}${page.route}.md): ${page.description}`);
  }
  // Only retire files named in our previous index, never unrelated static assets.
  for (const match of previousIndex.matchAll(
    /\]\(https:\/\/pontia\.dev\/((?:docs|blog)\/[a-zA-Z0-9_/-]+)\.md\)/g,
  )) {
    if (!byRoute.has(`/${match[1]}`)) {
      await rm(join(root, "static", `${match[1]}.md`), { force: true });
    }
  }
  const section = (title: string, list: { route: string }[]) =>
    list.length
      ? `\n## ${title}\n\n${list.flatMap((page) => entries.get(page.route) ?? []).join("\n")}\n`
      : "";
  await writeFile(
    indexPath,
    `# Pontia\n\n> ${overview.description}\n${section("Documentation", docs)}${section("Blog", blog)}`,
  );
}

if (import.meta.main) {
  await generateLlms();
}
