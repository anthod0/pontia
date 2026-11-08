import { onTestFinished, vi } from "vitest";

// jsdom has no layout; make composer textboxes report two lines and let tests trigger the resize.
export function stubWrappedComposer(): () => void {
  const callbacks: ResizeObserverCallback[] = [];
  const original = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    constructor(callback: ResizeObserverCallback) {
      callbacks.push(callback);
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  const clientHeight = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(40);
  const getComputedStyle = window.getComputedStyle.bind(window);
  const computedStyle = vi
    .spyOn(window, "getComputedStyle")
    .mockImplementation((element, pseudo) => {
      const style = getComputedStyle(element, pseudo);
      if (element.getAttribute("role") !== "textbox") return style;
      const overrides: Record<string, string> = {
        paddingTop: "0px",
        paddingBottom: "0px",
        lineHeight: "20px",
      };
      return new Proxy(style, {
        get: (target, key) =>
          typeof key === "string" && key in overrides
            ? overrides[key]
            : Reflect.get(target, key, target),
      });
    });
  onTestFinished(() => {
    globalThis.ResizeObserver = original;
    clientHeight.mockRestore();
    computedStyle.mockRestore();
  });
  return () => callbacks.forEach((callback) => callback([], {} as ResizeObserver));
}
