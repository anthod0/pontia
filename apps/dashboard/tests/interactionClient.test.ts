import { afterEach, expect, test, vi } from "vitest";
import { listPendingInteractions, respondInteraction } from "../src/api/client";

function response(data: unknown) {
  return new Response(JSON.stringify({ data, meta: {}, error: null }), { status: 200 });
}
afterEach(() => vi.unstubAllGlobals());

test("reads every pending page using the opaque cursor", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      response({ interactions: [{ interaction_id: "first" }], next_cursor: "opaque+/cursor" }),
    )
    .mockResolvedValueOnce(
      response({ interactions: [{ interaction_id: "last" }], next_cursor: null }),
    );
  vi.stubGlobal("fetch", fetch);
  const interactions = await listPendingInteractions("session-1");
  expect(interactions.map((i) => i.interaction_id)).toEqual(["first", "last"]);
  const first = new URL(fetch.mock.calls[0][0], "http://localhost");
  const second = new URL(fetch.mock.calls[1][0], "http://localhost");
  expect(first.searchParams.get("state")).toBe("pending");
  expect(first.searchParams.has("cursor")).toBe(false);
  expect(second.searchParams.get("cursor")).toBe("opaque+/cursor");
});

test("sends only option_key with no idempotency key or automatic network retry", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      response({ interaction: { interaction_id: "choice-1", state: "resolved" } }),
    )
    .mockRejectedValueOnce(new TypeError("Failed to fetch"));
  vi.stubGlobal("fetch", fetch);
  await respondInteraction("session-1", "choice-1", "custom-key");
  const init = fetch.mock.calls[0][1] as RequestInit;
  expect(init.method).toBe("PUT");
  expect(JSON.parse(init.body as string)).toEqual({ option_key: "custom-key" });
  expect(new Headers(init.headers).has("Idempotency-Key")).toBe(false);
  await expect(respondInteraction("session-1", "choice-1", "custom-key")).rejects.toBeInstanceOf(
    TypeError,
  );
  expect(fetch).toHaveBeenCalledTimes(2);
});
