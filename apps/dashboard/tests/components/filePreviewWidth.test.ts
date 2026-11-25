import { expect, test } from "vitest";
import { clampFilePreviewWidth } from "../../src/components/chat/filePreviewWidth";

test.each<[string, number, number, number]>([
  ["expanded sidebar", 1200, 2000, 720],
  ["collapsed sidebar", 1392, 2000, 912],
  ["minimum preview width", 1200, 100, 320],
  ["preferred width within bounds", 1200, 435, 435],
  ["narrow desktop", 784, 320, 304],
])("%s", (_, available, requested, expected) => {
  expect(clampFilePreviewWidth(requested, available)).toBe(expected);
});
