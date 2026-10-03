import { expect, test } from "bun:test";
import type { RequestEvent } from "@sveltejs/kit";
import { GET } from "../src/routes/api/edge/network/address/+server";

const discover = GET as unknown as (event: RequestEvent) => Response;

function event(address: string, protocol = "https:") {
  const url = new URL(`${protocol}//pontia.example/api/edge/network/address`);
  return {
    url,
    request: new Request(url, {
      headers: { "X-Forwarded-For": "8.8.8.8", "X-Real-IP": "8.8.4.4" },
    }),
    getClientAddress: () => address,
  } as unknown as RequestEvent;
}

test("returns the platform-observed public IPv4, not forwarding headers, without caching", async () => {
  const response = discover(event("154.8.220.61"));
  expect(response.status).toBe(200);
  expect((await response.json()) as unknown).toEqual({ ipv4: "154.8.220.61" });
  expect(response.headers.get("Cache-Control")).toBe("no-store");
});

test("rejects non-global addresses and IPv6", async () => {
  for (const address of [
    "10.2.0.10",
    "100.64.0.1",
    "127.0.0.1",
    "::1",
    "2606:4700::1111",
    "invalid",
  ]) {
    const response = discover(event(address));
    expect(response.status).toBe(400);
    expect((await response.json()) as unknown).toEqual({ error: "global_ipv4_required" });
    expect(response.headers.get("Cache-Control")).toBe("no-store");
  }
});

test("requires HTTPS", async () => {
  const response = discover(event("154.8.220.61", "http:"));
  expect(response.status).toBe(400);
  expect((await response.json()) as unknown).toEqual({ error: "https_required" });
});
