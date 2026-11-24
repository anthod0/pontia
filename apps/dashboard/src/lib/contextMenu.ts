export type MenuAnchor = { getBoundingClientRect: () => DOMRect };

/**
 * Takes over a `contextmenu` event and returns where its menu opens: at the pointer, or below the
 * element when the keyboard opened it. Shift and touch long presses keep the browser's own menu, so
 * links can still open in a new tab; this returns null for them.
 */
export function openContextMenu(event: MouseEvent): MenuAnchor | null {
  if (event.shiftKey || ("pointerType" in event && event.pointerType === "touch")) return null;
  event.preventDefault();
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const point =
    event.clientX || event.clientY
      ? { x: event.clientX, y: event.clientY }
      : { x: rect.left, y: rect.bottom };
  return { getBoundingClientRect: () => DOMRect.fromRect({ ...point, width: 0, height: 0 }) };
}
