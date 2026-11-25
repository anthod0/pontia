import { expect, test } from "bun:test";
import { isEdgePort } from "../src/lib/edge-port";
import { edgeApiOrigin, probeEdgeHealth } from "../src/lib/server/edge-network";

test("port policy rejects unsafe ports, not an application whitelist", () => {
  for (const port of [1, 25, 6000, 10080]) expect(isEdgePort(port)).toBeFalse();
  for (const port of [80, 443, 444, 8443, 65535]) expect(isEdgePort(port)).toBeTrue();
  for (const port of [0, 65536, 1.5, "8443", null]) expect(isEdgePort(port)).toBeFalse();
});

for (const hostname of ["edge.example.com", "brave-atlas.edge.pontia.dev"]) {
  for (const port of [443, 8443]) {
    test(`${hostname} on port ${port} has a canonical API origin and health URL`, async () => {
      const authority = port === 443 ? hostname : `${hostname}:${port}`;
      const endpoint = { tunnelUrl: `wss://${authority}/tunnel` };
      expect(edgeApiOrigin(endpoint.tunnelUrl)).toBe(`https://${authority}`);
      expect(
        await probeEdgeHealth(endpoint, async (url) => {
          expect(url).toBe(`https://${authority}/healthz`);
          return new Response("ok");
        }),
      ).toEqual({ status: "healthy", version: null });
    });
  }
}
