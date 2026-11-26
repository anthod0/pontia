import type { Handle } from "@sveltejs/kit";
import { dev } from "$app/environment";
import { deLocalizeUrl } from "$lib/paraglide/runtime";
import { paraglideMiddleware } from "$lib/paraglide/server";
import { localeNavigation } from "$lib/server/locale-navigation";
import { secureResponse } from "$lib/server/security-headers";

const privatePages = ["/login", "/settings", "/device", "/auth/account-conflict"];

const handlePage: Handle = ({ event, resolve }) =>
  paraglideMiddleware(event.request, async ({ request, locale }) => {
    event.request = request;
    event.locals.locale = locale;
    const response = await resolve(event, {
      transformPageChunk: ({ html }) => html.replace("%lang%", locale),
    });
    const { pathname } = deLocalizeUrl(event.url);
    if (
      pathname.startsWith("/api/") ||
      privatePages.some((page) => pathname === page || pathname.startsWith(`${page}/`))
    ) {
      response.headers.set("cache-control", "private, no-store");
    }
    return response;
  });

export const handle: Handle = async (input) =>
  dev
    ? secureResponse(
        input.event.request,
        await localeNavigation(input.event.request, async () => handlePage(input)),
      )
    : handlePage(input);
