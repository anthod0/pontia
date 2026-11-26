import { expect, test } from "bun:test";
import { generateKeyPair } from "jose";
import { isEdgePort } from "../src/lib/edge-port";
import { edgeApiOrigin, probeEdgeHealth } from "../src/lib/server/edge-network";

test("port policy rejects unsafe ports, not an application whitelist", () => {
  for (const port of [1, 25, 6000, 10080]) expect(isEdgePort(port)).toBeFalse();
  for (const port of [80, 443, 444, 8443, 65535]) expect(isEdgePort(port)).toBeTrue();
  for (const port of [0, 65536, 1.5, "8443", null]) expect(isEdgePort(port)).toBeFalse();
});

const { privateKey: signingKey } = await generateKeyPair("EdDSA");

for (const hostname of ["edge.example.com", "brave-atlas.edge.pontia.dev"]) {
  for (const port of [443, 8443]) {
    test(`${hostname} on port ${port} has a canonical API origin and version URL`, async () => {
      const authority = port === 443 ? hostname : `${hostname}:${port}`;
      const endpoint = {
        id: "0199791c-6600-7000-8000-000000000001",
        tunnelUrl: `wss://${authority}/tunnel`,
      };
      expect(edgeApiOrigin(endpoint.tunnelUrl)).toBe(`https://${authority}`);
      expect(
        await probeEdgeHealth(endpoint, signingKey, async (url, init) => {
          expect(url).toBe(`https://${authority}/__pontia/version`);
          expect(init.method).toBe("POST");
          return new Response("ok");
        }),
      ).toEqual({ status: "healthy", version: null });
    });
  }
}
