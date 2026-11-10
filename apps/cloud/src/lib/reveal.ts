import type { Attachment } from "svelte/attachments";

/**
 * Reveals an element when it scrolls into view. Descendants with the `reveal`
 * class animate in, staggered by their `--i` custom property.
 * Content stays visible when JavaScript is unavailable or motion is reduced.
 */
export function reveal(onVisible?: () => void): Attachment<HTMLElement> {
  return (node) => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      onVisible?.();
      return;
    }
    node.dataset.reveal = "";
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        node.dataset.reveal = "visible";
        onVisible?.();
        observer.disconnect();
      },
      { rootMargin: "0px 0px -12% 0px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  };
}
