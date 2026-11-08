import { expect, test } from "vitest";
import { createApiAccess } from "../../src/modes/public/apiAccess";

import { createRemoteConnection } from "../../src/modes/public/remote-access";

test("public API access uses browser credentials without forwarding bearer authentication", () => {
  const { apiCredentials, applyApiAuthentication } = createApiAccess(
    createRemoteConnection("office-mac"),
  );
  const headers = new Headers({ Authorization: "Bearer local-token" });

  expect(applyApiAuthentication(headers)).toBe(true);
  expect(headers.has("Authorization")).toBe(false);
  expect(apiCredentials).toBe("include");
});
