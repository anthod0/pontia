import { expect, test } from "bun:test";
import { secureResponse } from "../src/lib/server/security-headers";

test.each([200, 302, 404])("secures HTTPS responses including status %i", (status) => {
  const response = secureResponse(
    new Request("https://pontia.dev/device"),
    new Response(null, { status }),
  );
  expect(response.status).toBe(status);
  expect(response.headers.get("content-security-policy")).toContain("frame-ancestors 'none'");
  expect(response.headers.get("x-frame-options")).toBe("DENY");
  expect(response.headers.get("x-content-type-options")).toBe("nosniff");
  expect(response.headers.get("referrer-policy")).toBe("strict-origin");
  expect(response.headers.get("strict-transport-security")).toBe(
    "max-age=31536000; includeSubDomains",
  );
  expect(response.headers.get("permissions-policy")).toContain("camera=()");
});

test("preserves the SvelteKit script policy and skips HSTS on HTTP", () => {
  const headers = { "content-security-policy": "script-src 'self' 'nonce-page'" };
  const response = secureResponse(new Request("http://localhost"), new Response(null, { headers }));
  expect(response.headers.get("content-security-policy")).toContain(
    "script-src 'self' 'nonce-page'",
  );
  expect(response.headers.get("strict-transport-security")).toBeNull();
});
