import { expect, test } from "vitest";
import { apiCredentials, applyApiAuthentication } from "../src/modes/public/apiAccess";

test("public API access uses browser credentials without forwarding bearer authentication", () => {
  const headers = new Headers({ Authorization: "Bearer local-token" });

  expect(applyApiAuthentication(headers)).toBe(true);
  expect(headers.has("Authorization")).toBe(false);
  expect(apiCredentials).toBe("include");
});
