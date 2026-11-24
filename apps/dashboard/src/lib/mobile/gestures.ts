export function edgeSwipeBack(node: HTMLElement, back?: () => void) {
  let start: PointerEvent | null = null;
  function reset() {
    start = null;
    node.style.removeProperty("translate");
  }
  function down(event: PointerEvent) {
    if (back && event.pointerType === "touch" && event.isPrimary && event.clientX <= 24)
      start = event;
  }
  function move(event: PointerEvent) {
    if (!start || start.pointerId !== event.pointerId) return;
    const dx = event.clientX - start.clientX;
    if (Math.abs(event.clientY - start.clientY) > Math.max(dx, 12)) reset();
    else if (dx > 10) node.style.translate = Math.min(node.offsetWidth, dx) + "px 0";
  }
  function up(event: PointerEvent) {
    const completed =
      start && start.pointerId === event.pointerId && event.clientX - start.clientX >= 72;
    reset();
    if (completed) back?.();
  }
  node.addEventListener("pointerdown", down);
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", up);
  node.addEventListener("pointercancel", reset);
  window.addEventListener("popstate", reset);
  return {
    update(next?: () => void) {
      reset();
      back = next;
    },
    destroy() {
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", up);
      node.removeEventListener("pointercancel", reset);
      window.removeEventListener("popstate", reset);
    },
  };
}
