import { expect, test } from "bun:test";
import { isRedirect } from "@sveltejs/kit";
import { paraglideMiddleware } from "../src/lib/paraglide/server";
import { GET } from "../src/routes/docs/codex/+server";

async function requestRedirect(path: string): Promise<Response> {
  return paraglideMiddleware(
    new Request(`https://pontia.dev${path}`),
    async ({ request, locale }) => {
      try {
        await GET({ locals: { locale }, url: new URL(request.url) } as Parameters<typeof GET>[0]);
      } catch (cause) {
        if (!isRedirect(cause)) throw cause;
        return new Response(null, {
          status: cause.status,
          headers: { location: cause.location },
        });
      }
      throw new Error("Expected a redirect");
    },
  );
}

test.each([
  ["/docs/codex", "/docs/agent-clients/codex"],
  ["/zh/docs/codex", "/zh/docs/agent-clients/codex"],
  ["/ja/docs/codex", "/ja/docs/agent-clients/codex"],
  ["/pt-br/docs/codex", "/pt-br/docs/agent-clients/codex"],
])("redirects the former Codex route %s permanently", async (source, destination) => {
  const response = await requestRedirect(source);
  expect(response.status).toBe(301);
  expect(response.headers.get("location")).toBe(destination);
});
