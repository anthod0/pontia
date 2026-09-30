import { describe, expect, test } from "bun:test";
import {
  deviceHandleCandidates,
  isValidDeviceHandle,
  reservedDeviceHandles,
} from "../src/lib/server/remote-access/device-handle";

describe("device handle", () => {
  test("accepts canonical handles", () => {
    expect(isValidDeviceHandle("office-mac")).toBe(true);
    expect(isValidDeviceHandle("a_b-")).toBe(true);
    expect(isValidDeviceHandle("a".repeat(48))).toBe(true);
  });

  test("rejects invalid formats", () => {
    expect(isValidDeviceHandle("abc")).toBe(false);
    expect(isValidDeviceHandle("a".repeat(49))).toBe(false);
    expect(isValidDeviceHandle("Office-Mac")).toBe(false);
    expect(isValidDeviceHandle("2office-mac")).toBe(false);
    expect(isValidDeviceHandle("-office-mac")).toBe(false);
    expect(isValidDeviceHandle("office.mac")).toBe(false);
  });

  test("rejects every Website-reserved handle", () => {
    for (const handle of reservedDeviceHandles) {
      expect(isValidDeviceHandle(handle)).toBe(false);
    }
  });

  test("normalizes names and adds a stable UUID suffix when the base is unusable", () => {
    const deviceId = "0195e7c1-1b22-7c33-9d44-123456789abc";
    expect(deviceHandleCandidates("Office Mac", deviceId)[0]).toBe("office-mac");
    expect(deviceHandleCandidates("Devices", deviceId)[0]).toBe("devices-0195e7c1");
    expect(deviceHandleCandidates("東京", deviceId)[0]).toBe("device-0195e7c1");
    expect(deviceHandleCandidates("Office Mac", deviceId).every(isValidDeviceHandle)).toBe(true);
  });
});
