import { afterEach, expect, mock, spyOn, test } from "bun:test";
import { base64url, generateKeyPair, jwtVerify } from "jose";
import {
  CloudflareDnsProvider,
  edgeApiOrigin,
  edgeHost,
  isGlobalUnicastIp,
  isGlobalUnicastIpv4,
  isLegacyEdgeDnsOwner,
  isValidEdgeHost,
  managedEdgeHostname,
  ownershipMessage,
  probeEdgeHealth,
  resolvesToPublicAddresses,
  verifyEdgeOwnership,
  verifyOwnershipProof,
} from "../src/lib/server/edge-network";

afterEach(() => mock.restore());

function mockGlobalFetch(
  implementation: (...args: Parameters<typeof fetch>) => ReturnType<typeof fetch>,
) {
  return spyOn(globalThis, "fetch").mockImplementation(implementation as typeof fetch);
}

const identity = {
  id: "0199791c-6600-7000-8000-000000000001",
  tunnelUrl: "wss://brave-atlas.edge.pontia.dev/tunnel",
};

test("accepts any public DNS name, including Pontia domains", () => {
  for (const hostname of [
    "edge.example.com",
    "a.b.c.example.co.uk",
    "xn--bcher-kva.example",
    "edge.xn--p1ai",
    "1.example.com",
    "pontia.dev",
    "app.pontia.dev",
    "brave-atlas.edge.pontia.dev",
    "two.parts.edge.pontia.dev",
    "pontia.run",
    "relay.pontia.run",
    "edge.example.test",
  ]) {
    expect(isValidEdgeHost(hostname)).toBeTrue();
  }
});

test("accepts public IP literals and rejects private or special-use addresses", () => {
  for (const address of ["8.8.8.8", "192.0.0.9", "2606:4700:4700::1111"]) {
    expect(isValidEdgeHost(address)).toBeTrue();
  }
  for (const address of ["10.0.0.1", "127.0.0.1", "192.0.2.1", "::1", "2001:db8::1"]) {
    expect(isValidEdgeHost(address)).toBeFalse();
  }
});

test("rejects single labels and local or special-use names", () => {
  for (const hostname of [
    "",
    "example",
    "localhost",
    "edge.localhost",
    "printer.local",
    "service.internal",
    "1.0.0.127.in-addr.arpa",
    "router.home.arpa",
    "host.localdomain",
    "site.invalid",
    "hidden.onion",
    "name.alt",
    "127.0.0.1",
    "0x7f.1",
    "::1",
    "[2001:db8::1]",
    "example.123",
    "Edge.Example.com",
    "edge.example.com.",
    " edge.example.com",
    "-edge.example.com",
    "edge-.example.com",
    "edge..example.com",
    "edge_1.example.com",
    "edge.example.c",
    "edge.xn--a",
    `${"a".repeat(64)}.example`,
    `${"a.".repeat(124)}example`,
    42,
    null,
  ]) {
    expect(isValidEdgeHost(hostname)).toBeFalse();
  }
});

test("extracts only canonical tunnel URLs for public hosts", () => {
  expect(edgeHost(identity.tunnelUrl)).toBe("brave-atlas.edge.pontia.dev");
  expect(edgeHost("wss://edge.example.com:8443/tunnel")).toBe("edge.example.com");
  expect(edgeHost("wss://8.8.8.8/tunnel")).toBe("8.8.8.8");
  expect(edgeHost("wss://[2606:4700:4700::1111]:8443/tunnel")).toBe("2606:4700:4700::1111");
  expect(edgeApiOrigin("wss://[2606:4700:4700::1111]:8443/tunnel")).toBe(
    "https://[2606:4700:4700::1111]:8443",
  );
  expect(edgeApiOrigin("wss://edge.example.com:8443/tunnel")).toBe("https://edge.example.com:8443");
  expect(edgeApiOrigin("wss://two.parts.edge.pontia.dev/tunnel")).toBe(
    "https://two.parts.edge.pontia.dev",
  );
  for (const invalid of [
    "ws://edge.example.com/tunnel",
    "wss://edge.example.com:25/tunnel",
    "wss://edge.example.com/other",
    "wss://edge.example.com/tunnel?q=1",
    "wss://127.0.0.1/tunnel",
    "wss://[2001:db8::1]/tunnel",
    "wss://localhost/tunnel",
    "wss://nas.local/tunnel",
    "wss://intranet/tunnel",
  ]) {
    expect(edgeHost(invalid)).toBeNull();
    expect(edgeApiOrigin(invalid)).toBeNull();
  }
  expect(edgeApiOrigin("wss://edge.example.com:443/tunnel")).toBeNull();
});

test("only a labeled edge on its single-label managed hostname has Cloud-managed DNS", () => {
  expect(
    managedEdgeHostname({ dnsLabel: "sg", tunnelUrl: "wss://sg.edge.pontia.dev/tunnel" }),
  ).toBe("sg.edge.pontia.dev");
  expect(
    managedEdgeHostname({ dnsLabel: "sg", tunnelUrl: "wss://sg.edge.pontia.dev:8443/tunnel" }),
  ).toBe("sg.edge.pontia.dev");
  for (const edge of [
    { dnsLabel: null, tunnelUrl: "wss://sg.edge.pontia.dev/tunnel" },
    { dnsLabel: "sg", tunnelUrl: "wss://other.edge.pontia.dev/tunnel" },
    { dnsLabel: "sg", tunnelUrl: "wss://sg.example.com/tunnel" },
    { dnsLabel: "a.sg", tunnelUrl: "wss://a.sg.edge.pontia.dev/tunnel" },
    { dnsLabel: null, tunnelUrl: "wss://edge.example.com/tunnel" },
  ]) {
    expect(managedEdgeHostname(edge)).toBeNull();
  }
});

test("legacy DNS ownership is an explicit comma-separated allowlist", () => {
  const env = (value?: string) =>
    ({ LEGACY_EDGE_DNS_OWNER_IDS: value }) as unknown as App.Platform["env"];
  expect(isLegacyEdgeDnsOwner(env(undefined), "owner")).toBeFalse();
  expect(isLegacyEdgeDnsOwner(env(""), "owner")).toBeFalse();
  expect(isLegacyEdgeDnsOwner(env(" , "), "")).toBeFalse();
  expect(isLegacyEdgeDnsOwner(env("other"), "owner")).toBeFalse();
  expect(isLegacyEdgeDnsOwner(env("owner-2"), "owner")).toBeFalse();
  expect(isLegacyEdgeDnsOwner(env("owner"), "owner")).toBeTrue();
  expect(isLegacyEdgeDnsOwner(env(" other , owner "), "owner")).toBeTrue();
});

test("accepts only global-unicast IPv4 addresses", () => {
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

test("accepts only allocated global-unicast IPv6 addresses", () => {
  for (const address of ["2606:4700:4700::1111", "2a00:1450:4001:80b::200e", "2400:cb00::1"]) {
    expect(isGlobalUnicastIp(address)).toBeTrue();
  }
  for (const address of [
    "::",
    "::1",
    "::ffff:8.8.8.8",
    "64:ff9b::808:808",
    "100::1",
    "2001::1",
    "2001:db8::1",
    "2002:c000:201::1",
    "3fff::1",
    "fc00::1",
    "fd12:3456::1",
    "fe80::1",
    "fec0::1",
    "ff02::1",
    "2606:4700::1::1",
    "2606:4700:4700:0:0:0:0:1111:1",
    "2606:4700:4700::11111",
  ]) {
    expect(isGlobalUnicastIp(address)).toBeFalse();
  }
});

type DnsAnswers = Partial<Record<"A" | "AAAA", { Status?: number; Answer?: unknown[] }>>;

function dnsAnswers(records: DnsAnswers) {
  const requests: Array<[string, RequestInit]> = [];
  const fetcher = async (input: string, init: RequestInit) => {
    requests.push([input, init]);
    const url = new URL(input);
    const answer = records[url.searchParams.get("type") as "A" | "AAAA"] ?? { Status: 0 };
    return Response.json({ Status: 0, ...answer });
  };
  return { requests, fetcher };
}

test("DNS-over-HTTPS preflight requires public A or AAAA answers only", async () => {
  const publicV4 = dnsAnswers({
    A: {
      Answer: [
        { type: 5, data: "edge.cdn.example." },
        { type: 1, data: "8.8.8.8" },
      ],
    },
  });
  expect(await resolvesToPublicAddresses("edge.example.com", publicV4.fetcher)).toBe("public");
  expect(publicV4.requests.map(([url]) => url).sort()).toEqual([
    "https://cloudflare-dns.com/dns-query?name=edge.example.com&type=A",
    "https://cloudflare-dns.com/dns-query?name=edge.example.com&type=AAAA",
  ]);
  expect(publicV4.requests.every(([, init]) => init.redirect === "error")).toBeTrue();
  expect(
    await resolvesToPublicAddresses(
      "edge.example.com",
      dnsAnswers({ AAAA: { Answer: [{ type: 28, data: "2606:4700:4700::1111" }] } }).fetcher,
    ),
  ).toBe("public");
  for (const records of [
    {},
    { A: { Status: 3 }, AAAA: { Status: 3 } },
    { A: { Answer: [{ type: 1, data: "10.0.0.8" }] } },
    {
      A: { Answer: [{ type: 1, data: "8.8.8.8" }] },
      AAAA: { Answer: [{ type: 28, data: "::1" }] },
    },
    { AAAA: { Answer: [{ type: 28, data: "fd00::1" }] } },
  ] as DnsAnswers[]) {
    expect(await resolvesToPublicAddresses("edge.example.com", dnsAnswers(records).fetcher)).toBe(
      "non_public",
    );
  }
  await expect(
    resolvesToPublicAddresses("edge.example.com", dnsAnswers({ A: { Status: 2 } }).fetcher),
  ).rejects.toThrow();
  await expect(
    resolvesToPublicAddresses("edge.example.com", async () => new Response("", { status: 502 })),
  ).rejects.toThrow();
});

test("public IP deployments bypass DNS-over-HTTPS", async () => {
  expect(
    await resolvesToPublicAddresses("8.8.8.8", async () => {
      throw new Error("must not query DNS");
    }),
  ).toBe("public");
  expect(
    await resolvesToPublicAddresses("2606:4700:4700::1111", async () => {
      throw new Error("must not query DNS");
    }),
  ).toBe("public");
});

// crates/pontia-edge/src/ownership.rs computes the same vector.
const credentialSecret = base64url.encode(new Uint8Array(32).fill(42));
const vector = {
  edgeId: "0199791c-6600-7000-8000-000000000001",
  hostname: "edge.example.com",
  port: 443,
  nonce: "A".repeat(43),
  proof: "G8PT0z2_FUc1KfPw8pFPg35CeSoVicv3je5l1O_UAyo",
};

test("ownership proof matches the edge implementation and binds every value", async () => {
  const message = ownershipMessage(vector.edgeId, vector.hostname, vector.port, vector.nonce);
  expect(await verifyOwnershipProof(credentialSecret, message, vector.proof)).toBeTrue();
  for (const [secret, other] of [
    [base64url.encode(new Uint8Array(32).fill(43)), message],
    [credentialSecret, ownershipMessage(vector.edgeId, "other.example.com", 443, vector.nonce)],
    [credentialSecret, ownershipMessage(vector.edgeId, vector.hostname, 8443, vector.nonce)],
    [credentialSecret, ownershipMessage(vector.edgeId, vector.hostname, 443, "B".repeat(43))],
  ] as const) {
    expect(await verifyOwnershipProof(secret, other, vector.proof)).toBeFalse();
  }
  for (const proof of [
    "",
    "ok",
    `${vector.proof}=`,
    `${vector.proof.slice(0, 42)}B`,
    "A".repeat(44),
  ]) {
    expect(await verifyOwnershipProof(credentialSecret, message, proof)).toBeFalse();
  }
});

test("ownership verification signs a bound challenge and accepts only the edge's proof", async () => {
  const { privateKey, publicKey } = await generateKeyPair("EdDSA", { extractable: true });
  const signingKey = base64url.encode(
    new Uint8Array(await crypto.subtle.exportKey("pkcs8", privateKey)),
  );
  const challenge = { ...vector, credentialSecret };
  let request: [string, RequestInit] | undefined;
  const verify = (response: () => Response, port = 443) =>
    verifyEdgeOwnership(
      { ...challenge, port },
      signingKey,
      async (input, init) => {
        request = [input, init];
        return response();
      },
      () => new Uint8Array(32),
    );

  expect(await verify(() => new Response(vector.proof))).toBeTrue();
  const [url, init] = request!;
  expect(url).toBe("https://edge.example.com/__pontia/verify-ownership");
  expect(init.redirect).toBe("manual");
  const token = new Headers(init.headers).get("authorization")!.slice("Bearer ".length);
  const { payload } = await jwtVerify(token, publicKey, {
    audience: vector.edgeId,
    typ: "pontia-edge-ownership+jwt",
    algorithms: ["EdDSA"],
  });
  expect(payload).toMatchObject({ jti: vector.nonce, host: vector.hostname, port: 443 });
  expect(payload.exp! - payload.iat!).toBe(60);

  await verify(() => new Response("wrong"), 8443);
  expect(request![0]).toBe("https://edge.example.com:8443/__pontia/verify-ownership");
  for (const response of [
    () => new Response("ok"),
    () => new Response("wrong"),
    () => new Response(vector.proof, { status: 302, headers: { Location: "http://10.0.0.1/" } }),
    () => new Response(vector.proof, { status: 401 }),
    () => new Response(vector.proof.repeat(4)),
    () => {
      throw new TypeError("certificate verification failed");
    },
  ]) {
    expect(await verify(response)).toBeFalse();
  }
  // The proof is bound to the port the challenge was issued for.
  expect(await verify(() => new Response(vector.proof), 8443)).toBeFalse();
  request = undefined;
  expect(
    await verifyEdgeOwnership({ ...challenge, hostname: "localhost" }, signingKey, async () => {
      throw new Error("must not be called");
    }),
  ).toBeFalse();
});

test("health verification requires trusted HTTPS and the fixed response", async () => {
  const { privateKey, publicKey } = await generateKeyPair("EdDSA");
  let token = "";
  expect(
    await probeEdgeHealth(identity, privateKey, async (input, init) => {
      expect(input).toBe("https://brave-atlas.edge.pontia.dev/__pontia/version");
      expect(init.method).toBe("POST");
      expect(init.redirect).toBe("manual");
      token = new Headers(init.headers).get("authorization")!.slice("Bearer ".length);
      return new Response("ok");
    }),
  ).toEqual({ status: "healthy", version: null });
  const { payload } = await jwtVerify(token, publicKey, {
    audience: identity.id,
    typ: "pontia-edge-version+jwt",
    algorithms: ["EdDSA"],
  });
  expect(payload.exp! - payload.iat!).toBe(60);
  const headers = { "Pontia-Edge-Version": "1.2.3" };
  expect(
    await probeEdgeHealth(identity, privateKey, async () => new Response("almost ok", { headers })),
  ).toEqual({ status: "unreachable" });
});

test("an invalid Edge version does not affect health", async () => {
  const { privateKey } = await generateKeyPair("EdDSA");
  expect(
    await probeEdgeHealth(
      identity,
      privateKey,
      async () =>
        new Response("ok", {
          headers: { "Pontia-Edge-Version": "1.2.3, 2.3.4" },
        }),
    ),
  ).toEqual({ status: "healthy", version: null });
});

test("Cloudflare DNS adapter rejects a public IPv4 address change", async () => {
  const fetchMock = mockGlobalFetch(async () =>
    Response.json({
      success: true,
      result: [
        {
          id: "record",
          name: "brave-atlas.edge.pontia.dev",
          type: "A",
          content: "9.9.9.9",
          proxied: false,
        },
      ],
    }),
  );
  const provider = new CloudflareDnsProvider("secret", "zone");

  await expect(provider.ensureA("brave-atlas.edge.pontia.dev", "8.8.8.8")).rejects.toMatchObject({
    operation: "update",
    category: "local_validation_error",
  });
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

test("Cloudflare DNS adapter identifies lookup transport and provider failures", async () => {
  let behavior = async (): Promise<Response> => {
    throw new Error("token must not escape");
  };
  mockGlobalFetch(() => behavior());
  const provider = new CloudflareDnsProvider("secret", "zone");
  await expect(provider.ensureA("brave-atlas.edge.pontia.dev", "8.8.8.8")).rejects.toMatchObject({
    operation: "lookup",
    category: "network_error",
  });

  behavior = async () =>
    Response.json(
      { success: false, errors: [{ code: 9109, message: "Invalid access token" }] },
      { status: 403 },
    );
  await expect(provider.ensureA("brave-atlas.edge.pontia.dev", "8.8.8.8")).rejects.toMatchObject({
    operation: "lookup",
    category: "provider_http_error",
    status: 403,
    providerErrors: [{ code: 9109, message: "Invalid access token" }],
  });
});

test("Cloudflare DNS adapter removes the managed A and ACME challenge records", async () => {
  const requests: Array<[string, RequestInit]> = [];
  mockGlobalFetch(async (input, init) => {
    const url = String(input);
    requests.push([url, (init as RequestInit | undefined) ?? {}]);
    if (url.includes("?type=A")) {
      return Response.json({
        success: true,
        result: [
          {
            id: "address-record",
            name: "brave-atlas.edge.pontia.dev",
            type: "A",
            content: "8.8.8.8",
          },
        ],
      });
    }
    if (url.includes("?type=TXT")) {
      return Response.json({
        success: true,
        result: [
          {
            id: "challenge-record",
            name: "_acme-challenge.brave-atlas.edge.pontia.dev",
            type: "TXT",
            content: '"challenge"',
          },
        ],
      });
    }
    return Response.json({ success: true, result: {} });
  });

  await new CloudflareDnsProvider("secret", "zone").cleanupHostname("brave-atlas.edge.pontia.dev");

  expect(requests.map(([url]) => url)).toEqual([
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records?type=A&name=brave-atlas.edge.pontia.dev&per_page=20&page=1",
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records?type=TXT&name=_acme-challenge.brave-atlas.edge.pontia.dev&per_page=20&page=1",
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records/address-record",
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records/challenge-record",
  ]);
  expect(requests.slice(2).every(([, init]) => init.method === "DELETE")).toBe(true);
});

test("Cloudflare DNS adapter updates only the exact A record", async () => {
  const requests: Array<[string, RequestInit]> = [];
  mockGlobalFetch(async (input, init) => {
    requests.push([String(input), (init as RequestInit | undefined) ?? {}]);
    if (requests.length === 1) {
      return Response.json({
        success: true,
        result: [
          {
            id: "record",
            name: "brave-atlas.edge.pontia.dev",
            type: "A",
            content: "8.8.8.8",
            proxied: true,
          },
        ],
      });
    }
    return Response.json({ success: true });
  });
  const provider = new CloudflareDnsProvider("secret", "zone");
  await provider.ensureA("brave-atlas.edge.pontia.dev", "8.8.8.8");
  expect(requests[0][0]).toBe(
    "https://api.cloudflare.com./client/v4/zones/zone/dns_records?type=A&name=brave-atlas.edge.pontia.dev",
  );
  expect(requests[1][0]).toEndWith("/dns_records/record");
  expect(requests[1][1].method).toBe("PUT");
  expect(requests[1][1].body).toContain('"proxied":false');
});

test("health checks never contact non-public tunnel hosts", async () => {
  const { privateKey } = await generateKeyPair("EdDSA");
  for (const tunnelUrl of [
    "wss://127.0.0.1/tunnel",
    "wss://[::1]/tunnel",
    "wss://localhost/tunnel",
    "wss://metadata.internal/tunnel",
  ]) {
    expect(
      await probeEdgeHealth({ id: identity.id, tunnelUrl }, privateKey, async () => {
        throw new Error("must not be called");
      }),
    ).toEqual({ status: "unreachable" });
  }
});
