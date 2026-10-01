import { expect, test } from "bun:test";
import { DnsProviderError, readDnsProviderResponse } from "../src/lib/server/cloudflare-dns-errors";
import {
  logDeploymentEvent,
  type DeploymentLogger,
} from "../src/lib/server/deployment-observability";

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

test("DNS provider failures retain only bounded safe metadata", async () => {
  const failure = readDnsProviderResponse(
    response(
      {
        success: false,
        errors: [
          { code: 10000, message: "Authentication error" },
          { code: "x".repeat(65), message: "discarded" },
          { code: 1, message: "x".repeat(257) },
        ],
        secret: "must-not-escape",
      },
      403,
    ),
    "lookup",
  );

  try {
    await failure;
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
});

test("DNS provider parsing distinguishes malformed and explicit rejection responses", async () => {
  await expect(
    readDnsProviderResponse(new Response("not json", { status: 502 }), "create"),
  ).rejects.toMatchObject({ category: "invalid_provider_response", status: 502 });
  await expect(
    readDnsProviderResponse(response({ success: false, errors: [] }), "update"),
  ).rejects.toMatchObject({ category: "provider_rejected", status: 200 });
});

test("DNS provider parsing stops an undeclared oversized response stream", async () => {
  let cancelled = false;
  const response = new Response(
    new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.enqueue(new Uint8Array(8_193));
      },
      cancel() {
        cancelled = true;
      },
    }),
  );

  await expect(readDnsProviderResponse(response, "lookup")).rejects.toMatchObject({
    category: "invalid_provider_response",
  });
  expect(cancelled).toBeTrue();
});

test("deployment logging cannot replace the operation result", () => {
  const logger = {
    info() {
      throw new Error("logger unavailable");
    },
    warn() {
      throw new Error("logger unavailable");
    },
    error() {
      throw new Error("logger unavailable");
    },
  } satisfies DeploymentLogger;

  expect(() =>
    logDeploymentEvent("error", { event: "edge_enrollment_failed" }, logger),
  ).not.toThrow();
});
