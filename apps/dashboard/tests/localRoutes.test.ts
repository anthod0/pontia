import { expect, test } from "vitest";
import { match as matchHandle } from "../src/params/handle";

test("local routing does not consume a device handle prefix", () => {
  expect(matchHandle("office-mac")).toBe(false);
});
