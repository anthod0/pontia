import { expect, test } from "bun:test";
import unsafePorts from "../../../shared/edge-unsafe-ports.json";
import { isEdgePort } from "../../../shared/edge-port";
import {
  configureEdgeNetwork,
  edgeApiOrigin,
  verifyEdgeHealth,
} from "../src/lib/server/edge-network";

const hostname = "brave-atlas.edge.pontia.dev";
const identity = { tunnelUrl: `wss://${hostname}/tunnel` };
const value = "x".repeat(43);
const nonce = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

function socket(body: string) {
  return {
    opened: Promise.resolve(),
    readable: new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(
          new TextEncoder().encode(
            `HTTP/1.1 200 OK\r\nContent-Length: ${body.length}\r\n\r\n${body}`,
          ),
        );
        controller.close();
      },
    }),
    writable: new WritableStream<Uint8Array>(),
    async close() {},
  };
}

test("shared port policy rejects unsafe ports, not an application whitelist", () => {
  for (const port of unsafePorts) expect(isEdgePort(port)).toBeFalse();
  for (const port of [80, 443, 444, 8443, 65535]) expect(isEdgePort(port)).toBeTrue();
  for (const port of [0, 65536, 1.5, "8443", null]) expect(isEdgePort(port)).toBeFalse();
});

for (const customPort of [false, true]) {
  for (const dns01 of [false, true]) {
    test(`${customPort ? "custom" : "default"} port with ${dns01 ? "DNS-01" : "HTTP-01"}`, async () => {
      const events: string[] = [];
      const probePort = customPort ? 8443 : 80;
      const servicePort = customPort ? 8443 : 443;
      const result = await configureEdgeNetwork(
        identity,
        "8.8.8.8",
        {
          randomBytes: () => new Uint8Array(32).fill(7),
          connect: (address) => {
            events.push(`probe:${address.port}`);
            return socket(nonce);
          },
          dns: {
            async ensureA(name, address) {
              events.push(`A:${name}:${address}`);
            },
            async publishTxt(name, challenge) {
              events.push(`TXT:${name}:${challenge}`);
            },
          },
        },
        probePort,
        dns01 ? value : undefined,
      );
      expect(result.status).toBe("configured");
      expect(events).toEqual([
        `probe:${probePort}`,
        `A:${hostname}:8.8.8.8`,
        ...(dns01 ? [`TXT:${hostname}:${value}`] : []),
      ]);
      const authority = servicePort === 443 ? hostname : `${hostname}:${servicePort}`;
      const endpoint = { tunnelUrl: `wss://${authority}/tunnel` };
      expect(edgeApiOrigin(endpoint.tunnelUrl)).toBe(`https://${authority}`);
      expect(
        await verifyEdgeHealth(endpoint, async (url) => {
          expect(url).toBe(`https://${authority}/healthz`);
          return new Response("ok");
        }),
      ).toBeTrue();
    });
  }
}

test("DNS-01 still requires precise IP challenge and never publishes before verification", async () => {
  for (const port of [8443, 25]) {
    const events: string[] = [];
    const result = await configureEdgeNetwork(
      identity,
      "8.8.8.8",
      {
        randomBytes: () => new Uint8Array(32).fill(7),
        connect: () => socket("wrong nonce"),
        dns: {
          async ensureA() {
            events.push("A");
          },
          async publishTxt() {
            events.push("TXT");
          },
        },
      },
      port,
      value,
    );
    expect(result.status).toBe(port === 25 ? "invalid" : "unreachable");
    expect(events).toEqual([]);
  }
});
