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

export async function generateLlms(root = cloudRoot): Promise<void> {
  const docsRoot = join(root, "src/routes/docs");
  const pages = await Promise.all(
    (await findPages(docsRoot)).map(async (path) => {
      const source = await readFile(path, "utf8");
      const code = protectCode(source);
      const route = `/docs${relative(docsRoot, dirname(path))
        .split(sep)
        .filter(Boolean)
        .map((part) => `/${part}`)
        .join("")}`;
      const title = code.text.match(/^# (.+)$/m)?.[1];
      const description = code.text.match(
        /<meta\s+name="description"\s+content="([^"]*)"\s*\/?>/,
      )?.[1];
      if (!title || !description)
        throw new Error(`Missing documentation title or description: ${path}`);
      const anchors = new Map<string, string>();
      const text = code.text.replace(
        /<h([1-6])\b[^>]*\bid="([^"]+)"[^>]*>(.*?)<\/h\1>/g,
        (_, level, id, heading) => {
          anchors.set(id, headingSlug(heading));
          return `${"#".repeat(Number(level))} ${heading}`;
        },
      );
      return {
        route,
        title,
        description: decodeEntities(description),
        text,
        anchors,
        restore: code.restore,
      };
    }),
  );
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

  const entries: string[] = [];
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
    entries.push(`- [${page.title}](${origin}${page.route}.md): ${page.description}`);
  }
  // Only retire files named in our previous index, never unrelated static assets.
  for (const match of previousIndex.matchAll(
    /\]\(https:\/\/pontia\.dev\/docs\/([a-zA-Z0-9_/-]+)\.md\)/g,
  )) {
    if (!byRoute.has(`/docs/${match[1]}`)) {
      await rm(join(root, "static/docs", `${match[1]}.md`), { force: true });
    }
  }
  await writeFile(
    indexPath,
    `# Pontia\n\n> ${overview.description}\n\n## Documentation\n\n${entries.join("\n")}\n`,
  );
}

if (import.meta.main) {
  await generateLlms();
}
