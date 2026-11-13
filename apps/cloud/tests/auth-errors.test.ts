import { expect, test } from "bun:test";
import { authError } from "../src/lib/auth-errors";

test("unknown authentication error codes always produce a displayable error", () => {
  for (const code of ["unrecognized", "__proto__", "constructor", "hasOwnProperty", "toString"]) {
    expect(typeof authError(code)).toBe("string");
  }
});

test("an absent authentication error has no display message", () => {
  expect(authError(null)).toBeNull();
  expect(authError("")).toBeNull();
});
