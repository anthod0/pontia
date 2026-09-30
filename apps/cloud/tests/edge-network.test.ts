import { expect, test } from "bun:test";
import {
  CloudflareDnsProvider,
  configureEdgeNetwork,
  edgeHostname,
  isGlobalUnicastIpv4,
  verifyEdgeHealth,
  type DnsProvider,
} from "../src/lib/server/edge-network";

const identity = {
  edgeId: "0199791c-6600-7000-8000-000000000001",
  tunnelUrl: "wss://brave-silver-atlas.edge.pontia.dev/tunnel",
};

function httpResponse(body: string, status = "200 OK") {
  return `HTTP/1.1 ${status}\r\nContent-Length: ${new TextEncoder().encode(body).byteLength}\r\nConnection: close\r\n\r\n${body}`;
}

function socketResponse(response: string, request?: (value: string) => void) {
  let written = "";
  return {
    opened: Promise.resolve({}),
    readable: new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(response));
        controller.close();
      },
    }),
    writable: new WritableStream<Uint8Array>({
      write(chunk) {
        written += new TextDecoder().decode(chunk);
        request?.(written);
      },
      close() {
        throw new Error("closing the writer discards the response");
      },
    }),
    async close() {},
  };
}

test("extracts only a canonical assigned edge hostname", () => {
  expect(edgeHostname(identity.tunnelUrl)).toBe("brave-silver-atlas.edge.pontia.dev");
  for (const invalid of [
    "ws://brave-silver-atlas.edge.pontia.dev/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev:444/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev/other",
    "wss://two.parts.edge.pontia.dev/tunnel",
    "wss://brave-silver-atlas.edge.pontia.dev/tunnel?q=1",
    "wss://attacker.example/tunnel",
  ]) {
    expect(edgeHostname(invalid)).toBeNull();
  }
});

test("accepts only global-unicast IPv4 candidates", () => {
  expect(isGlobalUnicastIpv4("8.8.8.8")).toBeTrue();
  expect(isGlobalUnicastIpv4("192.0.0.9")).toBeTrue();
  expect(isGlobalUnicastIpv4("192.0.0.10")).toBeTrue();
  for (const invalid of [
    "127.0.0.1",
    "10.0.0.1",
    "100.64.0.1",
    "169.254.1.1",
    "172.16.0.1",
    "192.0.2.1",
    "192.168.1.1",
    "198.51.100.1",
    "203.0.113.1",
    "224.0.0.1",
    "1.2.3.999",
    "1.2.3",
  ]) {
    expect(isGlobalUnicastIpv4(invalid)).toBeFalse();
  }
});

test("verifies address control before creating an idempotent A record", async () => {
  const calls: string[] = [];
  const dns: DnsProvider = {
    async ensureA(hostname, address) {
      calls.push(`${hostname}=${address}`);
    },
  };
  const result = await configureEdgeNetwork(identity, "8.8.8.8", {
    randomBytes: () => new Uint8Array(32).fill(7),
    connect: (address) => {
      calls.push(`${address.hostname}:${address.port}`);
      const challenge = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";
      return socketResponse(httpResponse(challenge), (request) => calls.push(request));
    },
    dns,
  });
  expect(result).toEqual({ status: "configured", hostname: "brave-silver-atlas.edge.pontia.dev" });
  expect(calls[0]).toBe("8.8.8.8:80");
  expect(calls[1]).toStartWith(
    "GET /.well-known/pontia-edge-address/BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc HTTP/1.1\r\n",
  );
  expect(calls[1]).toContain("\r\nHost: 8.8.8.8\r\n");
  expect(calls[2]).toBe("brave-silver-atlas.edge.pontia.dev=8.8.8.8");
});

test("does not touch DNS when the TCP probe has a bad status, mismatches, or is too large", async () => {
  for (const response of [
    httpResponse("wrong"),
    httpResponse("redirect", "302 Found"),
    httpResponse("x".repeat(4_097)),
  ]) {
    let dnsCalls = 0;
    const result = await configureEdgeNetwork(identity, "8.8.8.8", {
      randomBytes: () => new Uint8Array(32).fill(9),
      connect: () => socketResponse(response),
      dns: {
        async ensureA() {
          dnsCalls += 1;
        },
      },
    });
    expect(result.status).toBe("unreachable");
    expect(dnsCalls).toBe(0);
  }
});

test("health verification requires trusted HTTPS and the fixed response", async () => {
  expect(
    await verifyEdgeHealth(identity, async (input, init) => {
      expect(input).toBe("https://brave-silver-atlas.edge.pontia.dev/healthz");
      expect(init.redirect).toBe("manual");
      return new Response("ok");
    }),
  ).toBeTrue();
  expect(await verifyEdgeHealth(identity, async () => new Response("almost ok"))).toBeFalse();
});

test("Cloudflare DNS adapter rejects a public IPv4 address change", async () => {
  let requests = 0;
  const provider = new CloudflareDnsProvider("secret", "zone", async () => {
    requests += 1;
    return Response.json({
      success: true,
      result: [
        {
          id: "record",
          name: "brave-silver-atlas.edge.pontia.dev",
          type: "A",
          content: "9.9.9.9",
          proxied: false,
        },
      ],
    });
  });

  await expect(provider.ensureA("brave-silver-atlas.edge.pontia.dev", "8.8.8.8")).rejects.toThrow(
    "unsupported",
  );
  expect(requests).toBe(1);
});

test("Cloudflare DNS adapter updates only the exact A record", async () => {
  const requests: Array<[string, RequestInit]> = [];
  const provider = new CloudflareDnsProvider("secret", "zone", async (input, init) => {
    requests.push([String(input), init ?? {}]);
    if (requests.length === 1) {
      return Response.json({
        success: true,
        result: [
          {
            id: "record",
            name: "brave-silver-atlas.edge.pontia.dev",
            type: "A",
            content: "8.8.8.8",
            proxied: true,
          },
        ],
      });
    }
    return Response.json({ success: true });
  });
  await provider.ensureA("brave-silver-atlas.edge.pontia.dev", "8.8.8.8");
  expect(requests[1][0]).toEndWith("/dns_records/record");
  expect(requests[1][1].method).toBe("PUT");
  expect(requests[1][1].body).toContain('"proxied":false');
});
