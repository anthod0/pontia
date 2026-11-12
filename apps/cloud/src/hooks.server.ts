import type { Handle } from "@sveltejs/kit";
import { deLocalizeUrl } from "$lib/paraglide/runtime";
import { paraglideMiddleware } from "$lib/paraglide/server";

const privatePages = ["/login", "/settings", "/device", "/auth/account-conflict"];

export const handle: Handle = ({ event, resolve }) =>
  paraglideMiddleware(event.request, async ({ request, locale }) => {
    event.request = request;
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
