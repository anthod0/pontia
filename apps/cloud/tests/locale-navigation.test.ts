import { expect, test } from "bun:test";
import { localeNavigation } from "../src/lib/server/locale-navigation";

function navigate(path: string, headers: HeadersInit = {}, method = "GET") {
  return localeNavigation(
    new Request(`https://pontia.example${path}`, {
      method,
      headers: { accept: "text/html", ...headers },
    }),
    async () =>
      new Response("prerendered page", {
        headers: { "content-type": "text/html; charset=utf-8" },
      }),
  );
}

test.each([
  ["zh-CN,zh;q=0.9,en;q=0.8", "/zh/docs"],
  ["zh", "/zh/docs"],
  ["ja-JP,en;q=0.8", "/ja/docs"],
  ["pt-BR,pt;q=0.9", "/pt-br/docs"],
  ["pt", "/pt-br/docs"],
  ["zh;q=0.2,ja;q=0.9", "/ja/docs"],
  ["fr,ja;q=0.8", "/ja/docs"],
])("redirects a first visit using %s", async (language, target) => {
  const response = await navigate("/docs", { "accept-language": language });
  expect(response.status).toBe(302);
  expect(response.headers.get("location")).toBe(`https://pontia.example${target}`);
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(response.headers.get("vary")).toContain("Accept-Language");
  expect(response.headers.get("vary")).toContain("Cookie");
});

test.each(["en-US,zh;q=0.9", "fr-FR", "zh;q=0", ""])(
  "serves unprefixed English for %s",
  async (language) => {
    const response = await navigate("/docs", { "accept-language": language });
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("prerendered page");
    expect(response.headers.get("set-cookie")).toContain("lang=en;");
  },
);

test("a saved language takes precedence over the browser and preserves the destination", async () => {
  const response = await navigate("/docs/platforms/linux?from=home", {
    cookie: "other=value; lang=ja",
    "accept-language": "zh-CN",
  });
  expect(response.status).toBe(302);
  expect(response.headers.get("location")).toBe(
    "https://pontia.example/ja/docs/platforms/linux?from=home",
  );
});

test("a saved English choice stays on the unprefixed page", async () => {
  const response = await navigate("/docs", {
    cookie: "lang=en",
    "accept-language": "zh-CN",
  });
  expect(response.status).toBe(200);
  expect(response.headers.get("location")).toBeNull();
  expect(response.headers.get("set-cookie")).toBeNull();
});

test("an invalid cookie falls back to the browser language", async () => {
  const response = await navigate("/docs", {
    cookie: "lang=unknown",
    "accept-language": "zh-CN",
  });
  expect(response.headers.get("location")).toBe("https://pontia.example/zh/docs");
});

test.each([
  ["/zh/docs", "zh-Hans"],
  ["/ja", "ja"],
  ["/ja/", "ja"],
  ["/pt-br/blog", "pt-BR"],
])("remembers %s even when cookie and browser disagree", async (path, locale) => {
  const response = await navigate(path, {
    cookie: "lang=en",
    "accept-language": "en-US",
  });
  expect(response.status).toBe(200);
  expect(await response.text()).toBe("prerendered page");
  expect(response.headers.get("set-cookie")).toContain(`lang=${locale};`);
  expect(response.headers.get("set-cookie")).toContain("Path=/;");
  expect(response.headers.get("set-cookie")).toContain("SameSite=Lax; Secure");
  expect(response.headers.get("cache-control")).toBe("private, no-store");
});

test.each([
  "/api/auth/github/callback?code=123",
  "/api",
  "/privacy",
  "/terms",
  "/sitemap.xml",
  "/logo.svg",
  "/docs/configuration.md",
  "/zh/docs/__data.json",
  "/settings/edges/health",
  "/missing",
])("does not negotiate or remember a language for %s", async (path) => {
  const response = await navigate(path, { "accept-language": "zh-CN" });
  expect(response.status).toBe(200);
  expect(response.headers.get("set-cookie")).toBeNull();
});

test("does not redirect form submissions", async () => {
  const response = await navigate("/device", { "accept-language": "zh-CN" }, "POST");
  expect(response.status).toBe(200);
  expect(response.headers.get("set-cookie")).toBeNull();
});

test.each([
  ["accept", "application/json"],
  ["purpose", "prefetch"],
  ["sec-purpose", "prefetch"],
])("does not change preferences on background requests (%s: %s)", async (name, value) => {
  const response = await navigate("/zh/docs", { [name]: value });
  expect(response.headers.get("set-cookie")).toBeNull();
});

test("an unsuccessful page does not replace the remembered language", async () => {
  const response = await localeNavigation(
    new Request("https://pontia.example/ja/docs/missing", { headers: { accept: "text/html" } }),
    async () =>
      new Response("not found", { status: 404, headers: { "content-type": "text/html" } }),
  );
  expect(response.status).toBe(404);
  expect(response.headers.get("set-cookie")).toBeNull();
});
