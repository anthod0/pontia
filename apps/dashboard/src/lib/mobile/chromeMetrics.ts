export function chromeMetrics(node: HTMLElement, variable: string) {
  if (!node.closest(".mobile-shell")) return;
  const root = document.documentElement;
  const update = () => root.style.setProperty(variable, node.offsetHeight + "px");
  update();
  const observer = new ResizeObserver(update);
  observer.observe(node);
  return {
    destroy() {
      observer.disconnect();
      root.style.removeProperty(variable);
    },
  };
}
