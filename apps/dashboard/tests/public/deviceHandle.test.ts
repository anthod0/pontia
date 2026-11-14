import { describe, expect, test } from "vitest";
import { match as matchHandle } from "../../src/params/handle";
import { isValidDeviceHandle } from "../../src/lib/deviceHandle";

const handle = "office-mac";

describe("device handle", () => {
  test("accepts 4-48 character handles that start with a lowercase letter", () => {
    expect(isValidDeviceHandle(handle)).toBe(true);
    expect(isValidDeviceHandle("a_b-")).toBe(true);
    expect(isValidDeviceHandle("a".repeat(48))).toBe(true);
    expect(isValidDeviceHandle("abc")).toBe(false);
    expect(isValidDeviceHandle("a".repeat(49))).toBe(false);
    expect(isValidDeviceHandle("Office-Mac")).toBe(false);
    expect(isValidDeviceHandle("2office-mac")).toBe(false);
    expect(isValidDeviceHandle("-office-mac")).toBe(false);
    expect(isValidDeviceHandle("office.mac")).toBe(false);
    expect(isValidDeviceHandle("workflows")).toBe(false);
  });

  test("matches device handles without taking reserved routes", () => {
    expect(matchHandle(handle)).toBe(true);
    expect(matchHandle("workflows")).toBe(false);
  });
});
