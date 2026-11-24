import { readonly, writable } from "svelte/store";

const keyboard = writable(false);
export const keyboardOpen = readonly(keyboard);

export function installVisualViewportCssVars(win: Window = window): () => void {
  const root = win.document.documentElement;
  const viewport = win.visualViewport;

  function update(): void {
    const top = viewport?.offsetTop ?? 0;
    const bottom = viewport
      ? Math.max(0, win.innerHeight - viewport.height - viewport.offsetTop)
      : 0;
    keyboard.set(bottom > 100);
    root.style.setProperty("--visual-viewport-top", `${Math.round(top)}px`);
    root.style.setProperty("--visual-viewport-bottom", `${Math.round(bottom)}px`);
  }

  update();

  if (!viewport) {
    return () => {
      root.style.setProperty("--visual-viewport-top", "0px");
      root.style.setProperty("--visual-viewport-bottom", "0px");
    };
  }

  viewport.addEventListener("resize", update);
  viewport.addEventListener("scroll", update);

  return () => {
    keyboard.set(false);
    viewport.removeEventListener("resize", update);
    viewport.removeEventListener("scroll", update);
    root.style.setProperty("--visual-viewport-top", "0px");
    root.style.setProperty("--visual-viewport-bottom", "0px");
  };
}

/** Fixed toolbar center and attachment edge in layout viewport coordinates. */
export function selectionToolbarPosition(
  selection: Pick<DOMRect, "left" | "width" | "top" | "bottom">,
  touch: boolean,
  viewport: Pick<DOMRect, "x" | "y" | "width" | "height">,
  width: number,
  height: number,
) {
  const margin = 4;
  const half = width / 2;
  const left = Math.max(
    viewport.x + half + margin,
    Math.min(selection.left + selection.width / 2, viewport.x + viewport.width - half - margin),
  );
  const min = viewport.y + margin;
  const max = viewport.y + viewport.height - margin;
  const top = touch
    ? Math.max(min, Math.min(selection.bottom + 48, max - height))
    : Math.max(min + height, Math.min(selection.top - 8, max));
  return { left, top };
}
