import { afterEach, expect, mock, spyOn, test } from "bun:test";
import { CloudflareDnsProvider } from "../src/cloudflare-dns";
import { DnsProviderError } from "../src/dns-errors";

const hostname = "brave-silver-atlas.edge.pontia.dev";

afterEach(() => mock.restore());

function mockGlobalFetch(
  implementation: (...args: Parameters<typeof fetch>) => ReturnType<typeof fetch>,
) {
  return spyOn(globalThis, "fetch").mockImplementation(implementation as typeof fetch);
}

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

test("DNS lookup accepts only the exact canonical A record", async () => {
  const requests: Array<{ url: string; init: RequestInit }> = [];
  mockGlobalFetch(async (input, init) => {
    requests.push({ url: input.toString(), init: (init as RequestInit | undefined) ?? {} });
    return response({ success: true, result: [{ id: "record/id", name: hostname, type: "A" }] });
  });
  const provider = new CloudflareDnsProvider("secret-token", "zone/id");

  await expect(provider.findA(hostname)).resolves.toEqual({ id: "record/id", hostname });
  expect(requests[0]?.url).toBe(
    `https://api.cloudflare.com./client/v4/zones/zone%2Fid/dns_records?type=A&name=${hostname}`,
  );
  expect(requests[0]?.init.headers).toEqual({
    Authorization: "Bearer secret-token",
    "Content-Type": "application/json",
  });
});

test("DNS lookup treats absence as success and rejects ambiguous responses", async () => {
  let body: unknown = { success: true, result: [] };
  mockGlobalFetch(async () => response(body));
  const provider = new CloudflareDnsProvider("token", "zone");
  await expect(provider.findA(hostname)).resolves.toBeNull();

  for (body of [
    { success: false, result: [] },
    { success: true, result: [{ id: "one", name: hostname, type: "A" }, { id: "two" }] },
    { success: true, result: [{ id: "one", name: `other.${hostname}`, type: "A" }] },
    { success: true, result: [{ id: "one", name: hostname, type: "AAAA" }] },
  ]) {
    await expect(provider.findA(hostname)).rejects.toThrow();
  }
});

test("DNS failures expose safe operation, status, and provider metadata", async () => {
  let behavior = async () =>
    response(
      {
        success: false,
        errors: [{ code: 10000, message: "Authentication error" }],
        token: "must-not-escape",
      },
      403,
    );
  mockGlobalFetch(() => behavior());
  const provider = new CloudflareDnsProvider("secret-token", "zone");
  try {
    await provider.findA(hostname);
    throw new Error("expected provider error");
  } catch (error) {
    expect(error).toBeInstanceOf(DnsProviderError);
    expect((error as DnsProviderError).fields()).toEqual({
      operation: "lookup",
      error: "provider_http_error",
      status: 403,
      provider_errors: [{ code: 10000, message: "Authentication error" }],
    });
    expect(JSON.stringify((error as DnsProviderError).fields())).not.toContain("must-not-escape");
  }

  behavior = async () => new Response("not json");
  await expect(provider.findA(hostname)).rejects.toMatchObject({
    operation: "lookup",
    category: "invalid_provider_response",
  });

  behavior = async () => {
    throw new Error("network details");
  };
  await expect(provider.findA(hostname)).rejects.toMatchObject({
    operation: "lookup",
    category: "network_error",
  });
});

test("DNS deletion targets the previously looked-up record ID and is idempotent", async () => {
  const urls: string[] = [];
  let behavior = async (input: RequestInfo | URL) => {
    urls.push(input.toString());
    return response({ success: true });
  };
  mockGlobalFetch((input) => behavior(input));
  const provider = new CloudflareDnsProvider("token", "zone");
  await provider.deleteA({ id: "record/id", hostname });
  expect(urls).toEqual([
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records/record%2Fid",
  ]);

  let requestCount = 0;
  behavior = async () => {
    requestCount += 1;
    return requestCount === 1
      ? response({ success: false }, 404)
      : response({ success: true, result: [] });
  };
  await expect(provider.deleteA({ id: "gone", hostname })).resolves.toBeUndefined();

  behavior = async () => response({ success: false }, 404);
  await expect(provider.deleteA({ id: "gone", hostname })).rejects.toThrow();
});
