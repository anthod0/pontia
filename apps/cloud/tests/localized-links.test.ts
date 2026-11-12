import { expect, test } from "bun:test";
import { rehypeLocalizeLinks } from "../scripts/rehype-localize-links";

function links(filename: string | undefined, hrefs: string[]) {
  const children = hrefs.map((href) => ({ type: "element", tagName: "a", properties: { href } }));
  rehypeLocalizeLinks()({ type: "root", children: [{ type: "element", children }] }, { filename });
  return children.map((node) => node.properties.href);
}

test.each([
  ["zh-Hans", "/zh"],
  ["ja", "/ja"],
  ["pt-BR", "/pt-br"],
])(
  "prefixes internal links in %s content and preserves query strings and anchors",
  (locale, prefix) => {
    expect(
      links(`/project/src/content/docs/${locale}/platforms/linux.svx`, [
        "/",
        "/docs/setup?a=1#install",
        "/blog/news",
      ]),
    ).toEqual([`${prefix}/`, `${prefix}/docs/setup?a=1#install`, `${prefix}/blog/news`]);
  },
);

test.each(["docs", "blog"])(
  "leaves English-only, API, external and fragment links unchanged in %s",
  (section) => {
    const hrefs = [
      "/privacy",
      "/privacy#cookies",
      "/privacy?from=docs#cookies",
      "/terms",
      "/terms?from=docs",
      "/api",
      "/api/auth/github?next=login",
      "//example.com/path",
      "https://example.com/path",
      "#install",
      "./setup",
      "mailto:hello@example.com",
    ];
    expect(links(`/project/src/content/${section}/ja/index.svx`, hrefs)).toEqual(hrefs);
    expect(
      links(`/project/src/content/${section}/ja/index.svx`, ["/privacy-guide", "/api-guide"]),
    ).toEqual(["/ja/privacy-guide", "/ja/api-guide"]);
  },
);

test.each([undefined, "/src/routes/privacy.svx", "/src/content/docs/en/index.svx"])(
  "leaves links outside translated content unchanged (%s)",
  (filename) => {
    expect(links(filename, ["/docs/setup"])).toEqual(["/docs/setup"]);
  },
);

test("handles Windows content paths", () => {
  expect(links("C:\\project\\src\\content\\blog\\pt-BR\\news.svx", ["/docs"])).toEqual([
    "/pt-br/docs",
  ]);
});

test.each(["fr", "constructor"])(
  "rejects unknown content locale %s instead of emitting broken links",
  (locale) => {
    expect(() => links(`/src/content/docs/${locale}/index.svx`, ["/docs"])).toThrow();
  },
);
