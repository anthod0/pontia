import type { RequestEvent } from "@sveltejs/kit";

export type AuthSessionRequest = {
  ip: string | null;
  userAgent: string | null;
  country: string | null;
};

export function authSessionRequest(
  event: Pick<RequestEvent, "getClientAddress" | "platform" | "request">,
): AuthSessionRequest {
  let ip: string | null = null;
  try {
    ip = event.getClientAddress();
  } catch {
    // Client addresses are unavailable outside a configured server adapter.
  }
  return {
    ip,
    userAgent: event.request.headers.get("user-agent"),
    country: event.platform?.cf?.country ?? null,
  };
}
