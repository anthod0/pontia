import { expect, test } from "vitest";
import { selectionToolbarPosition } from "../src/lib/visualViewport";

const viewport = { x: 20, y: 80, width: 300, height: 200 };
test("keeps the touch toolbar above the keyboard when the selection extends below it", () => {
  expect(
    selectionToolbarPosition(
      { left: 290, width: 100, top: 180, bottom: 350 },
      true,
      viewport,
      100,
      44,
    ),
  ).toEqual({ left: 266, top: 232 });
});
test("keeps the mouse toolbar visible when the visual viewport is panned", () => {
  expect(
    selectionToolbarPosition({ left: 0, width: 10, top: 30, bottom: 40 }, false, viewport, 100, 28),
  ).toEqual({ left: 74, top: 112 });
});
