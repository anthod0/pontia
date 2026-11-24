import { expect, test, vi } from "vitest";

test.each([
  [767.5, 800, false, true, false],
  [768, 800, true, false, false],
  [900, 499, true, true, true],
  [900, 500, true, false, false],
  [900, 499, false, false, false],
] as const)(
  "classifies %ipx by %ipx with coarse pointer %s",
  async (width, height, coarse, mobile, landscape) => {
    const matchMedia = window.matchMedia;
    const mock = vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
      ...matchMedia(query),
      get matches() {
        if (query === "(width < 768px)") return width < 768;
        if (query === "(max-height: 499px) and (pointer: coarse)") return height <= 499 && coarse;
        throw new Error(`Unexpected media query: ${query}`);
      },
    }));
    try {
      vi.resetModules();
      const { isMobileShell, isLandscapePhone } =
        await import("../../src/lib/hooks/shellMode.svelte");
      expect(isMobileShell()).toBe(mobile);
      expect(isLandscapePhone()).toBe(landscape);
    } finally {
      mock.mockRestore();
    }
  },
);
