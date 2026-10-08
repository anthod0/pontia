import type { Handle } from "@sveltejs/kit";

const privatePages = ["/login", "/settings", "/device", "/auth/account-conflict"];

export const handle: Handle = async ({ event, resolve }) => {
  const response = await resolve(event);
  const { pathname } = event.url;
  if (
    pathname.startsWith("/api/") ||
    privatePages.some((page) => pathname === page || pathname.startsWith(`${page}/`))
  ) {
    response.headers.set("cache-control", "private, no-store");
  }
  return response;
};
