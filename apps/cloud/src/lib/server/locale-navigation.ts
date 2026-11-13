import { languageCookie } from "../locales";
import {
  baseLocale,
  deLocalizeUrl,
  extractLocaleFromHeader,
  extractLocaleFromUrl,
  isLocale,
  localizeUrl,
} from "../paraglide/runtime";

const pagePaths =
  /^\/(?:$|docs(?:\/|$)|blog(?:\/|$)|login\/?$|device\/?$|settings(?:\/|$)|auth\/account-conflict\/?$)/;

export async function localeNavigation(
  request: Request,
  resolve: () => Promise<Response>,
): Promise<Response> {
  const url = new URL(request.url);
  const path = deLocalizeUrl(url).pathname;
  if (
    !["GET", "HEAD"].includes(request.method) ||
    !request.headers.get("accept")?.includes("text/html") ||
    request.headers.get("purpose") === "prefetch" ||
    request.headers.get("sec-purpose")?.includes("prefetch") ||
    !pagePaths.test(path) ||
    path.includes(".") ||
    path === "/settings/edges/health"
  ) {
    return resolve();
  }

  const saved = request.headers
    .get("cookie")
    ?.split(";")
    .map((cookie) => cookie.trim())
    .find((cookie) => cookie.startsWith(`${languageCookie.name}=`))
    ?.slice(languageCookie.name.length + 1);
  const urlLocale = extractLocaleFromUrl(url)!;
  let locale = urlLocale;
  if (urlLocale === baseLocale) {
    // The site's Chinese and Portuguese translations use script/region locale IDs.
    const languages = (request.headers.get("accept-language") ?? "").replace(
      /(^|,)\s*(zh|pt)(?:-[a-z0-9]+)*(?=\s*(?:;|,|$))/gi,
      (_, separator: string, language: string) =>
        `${separator}${language.toLowerCase() === "zh" ? "zh-Hans" : "pt-BR"}`,
    );
    locale = isLocale(saved)
      ? saved
      : (extractLocaleFromHeader(new Request(url, { headers: { "accept-language": languages } })) ??
        baseLocale);
  }

  const target = localizeUrl(url, { locale });
  if (target.href !== url.href) {
    return new Response(null, {
      status: 302,
      headers: {
        location: target.href,
        "cache-control": "private, no-store",
        vary: "Cookie, Accept-Language, Accept",
      },
    });
  }

  const response = await resolve();
  if (!response.ok || !response.headers.get("content-type")?.includes("text/html")) {
    return response;
  }
  const result = new Response(response.body, response);
  result.headers.set("cache-control", "private, no-store");
  if (saved !== locale) {
    result.headers.append(
      "set-cookie",
      `${languageCookie.name}=${locale}; Path=/; Max-Age=${languageCookie.maxAge}; SameSite=Lax${url.protocol === "https:" ? "; Secure" : ""}`,
    );
  }
  return result;
}
