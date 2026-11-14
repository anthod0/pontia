import { expect, test } from "vitest";
import { createApiAccess } from "../../src/modes/public/apiAccess";

import { createRemoteConnection } from "../../src/modes/public/remote-access";

test("public API access uses browser credentials without forwarding bearer authentication", () => {
  const { apiCredentials, applyApiAuthentication } = createApiAccess(
    createRemoteConnection(
      {
        handle: "office-mac",
        deviceId: "01234567-89ab-cdef-0123-456789abcdef",
        edgeId: "01234567-89ab-cdef-0123-456789abcdef",
      },
      "01234567-89ab-cdef-0123-456789abcdef",
    ),
  );
  const headers = new Headers({ Authorization: "Bearer local-token" });

  expect(applyApiAuthentication(headers)).toBe(true);
  expect(headers.has("Authorization")).toBe(false);
  expect(apiCredentials).toBe("include");
});
