import { describe, expect, test, vi } from "vitest";
import { installVisualViewportCssVars } from "../../src/lib/visualViewport";

function createVisualViewport(offsetTop: number, height: number, windowHeight: number) {
  const listeners = new Map<string, Set<() => void>>();
  return {
    innerHeight: windowHeight,
    document: {
      documentElement: document.createElement("html"),
    },
    visualViewport: {
      offsetTop,
      height,
      addEventListener: vi.fn((type: string, listener: () => void) => {
        if (!listeners.has(type)) listeners.set(type, new Set());
        listeners.get(type)?.add(listener);
      }),
      removeEventListener: vi.fn((type: string, listener: () => void) => {
        listeners.get(type)?.delete(listener);
      }),
      dispatch(type: string) {
        for (const listener of listeners.get(type) ?? []) listener();
      },
    },
  };
}

describe("installVisualViewportCssVars", () => {
  test("updates both insets when the keyboard pans a focused field and then closes", () => {
    const win = createVisualViewport(0, 367, 667);
    const uninstall = installVisualViewportCssVars(win as unknown as Window);
    const style = win.document.documentElement.style;

    win.visualViewport.offsetTop = 80;
    win.visualViewport.dispatch("scroll");
    expect(style.getPropertyValue("--visual-viewport-top")).toBe("80px");
    expect(style.getPropertyValue("--visual-viewport-bottom")).toBe("220px");

    win.visualViewport.offsetTop = 0;
    win.visualViewport.height = 667;
    win.visualViewport.dispatch("resize");
    expect(style.getPropertyValue("--visual-viewport-top")).toBe("0px");
    expect(style.getPropertyValue("--visual-viewport-bottom")).toBe("0px");
    uninstall();
  });

  test("tracks visual viewport top and bottom offsets as css variables", () => {
    const win = createVisualViewport(24, 500, 800);

    const uninstall = installVisualViewportCssVars(win as unknown as Window);

    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-top")).toBe(
      "24px",
    );
    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-bottom")).toBe(
      "276px",
    );

    win.visualViewport.offsetTop = 64;
    win.visualViewport.height = 620;
    win.visualViewport.dispatch("resize");

    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-top")).toBe(
      "64px",
    );
    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-bottom")).toBe(
      "116px",
    );

    uninstall();

    expect(win.visualViewport.removeEventListener).toHaveBeenCalledWith(
      "resize",
      expect.any(Function),
    );
    expect(win.visualViewport.removeEventListener).toHaveBeenCalledWith(
      "scroll",
      expect.any(Function),
    );
    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-top")).toBe(
      "0px",
    );
    expect(win.document.documentElement.style.getPropertyValue("--visual-viewport-bottom")).toBe(
      "0px",
    );
  });
});
