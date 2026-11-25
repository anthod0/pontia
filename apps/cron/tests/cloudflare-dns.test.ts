import { afterEach, expect, mock, spyOn, test } from "bun:test";
import { CloudflareDnsProvider } from "../src/cloudflare-dns";
import { DnsProviderError } from "../src/dns-errors";

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
  const provider = new CloudflareDnsProvider("secret-token", "zone", "preview-zone");
  const now = new Date("2099-01-01T00:00:00.000Z");
  try {
    await provider.cleanupExpiredTxt(now);
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
  await expect(provider.cleanupExpiredTxt(now)).rejects.toMatchObject({
    operation: "lookup",
    category: "invalid_provider_response",
  });
});
