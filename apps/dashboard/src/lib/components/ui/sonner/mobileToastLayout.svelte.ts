import { isMobileShell } from "$lib/hooks/shellMode.svelte";

const anchorSelector = "[data-mobile-toast-anchor]";
const avoidSelector = "[data-mobile-toast-avoid]";

export const mobileToastLayout = new (class {
  visible = $state(false);
  height = $state(0);
})();

function visibleElements(selector: string): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(selector)].filter((element) => {
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  });
}

export function syncMobileToastLayout(root: HTMLElement): () => void {
  const documentRoot = document.documentElement;
  const resizeObserver = new ResizeObserver(schedule);
  let observed = new Set<HTMLElement>();
  let frame = 0;

  function schedule(): void {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(update);
  }

  function observe(elements: HTMLElement[]): void {
    const next = new Set(elements);
    for (const element of observed) if (!next.has(element)) resizeObserver.unobserve(element);
    for (const element of next) if (!observed.has(element)) resizeObserver.observe(element);
    observed = next;
  }

  function hide(): void {
    mobileToastLayout.visible = false;
    mobileToastLayout.height = 0;
    documentRoot.removeAttribute("data-mobile-toast-visible");
    documentRoot.style.removeProperty("--mobile-toast-height");
    for (const element of visibleElements(avoidSelector))
      element.removeAttribute("data-mobile-toast-overlap");
  }

  function reset(): void {
    hide();
    documentRoot.style.removeProperty("--mobile-toast-bottom");
  }

  function update(): void {
    if (!isMobileShell()) {
      reset();
      return;
    }

    const anchors = visibleElements(anchorSelector);
    const avoiders = visibleElements(avoidSelector);
    const toast = root.querySelector<HTMLElement>(
      '[data-sonner-toast][data-front="true"]:not([data-removed="true"])',
    );
    observe([...anchors, ...avoiders, ...(toast ? [toast] : [])]);

    const viewportBottom = window.visualViewport
      ? window.visualViewport.offsetTop + window.visualViewport.height
      : window.innerHeight;
    const anchor = anchors.at(-1);
    if (anchor) {
      const rect = anchor.getBoundingClientRect();
      const fullHeightSheet = anchor.matches(".bottom-sheet") && rect.height > innerHeight * 0.8;
      if (!fullHeightSheet)
        documentRoot.style.setProperty(
          "--mobile-toast-bottom",
          `${Math.max(0, window.innerHeight - Math.min(viewportBottom, rect.top)) + 8}px`,
        );
      else documentRoot.style.removeProperty("--mobile-toast-bottom");
    } else documentRoot.style.removeProperty("--mobile-toast-bottom");

    if (!toast) {
      hide();
      return;
    }

    const content = toast.querySelector<HTMLElement>("[data-content]");
    toast.toggleAttribute("data-mobile-multiline", Boolean(content && content.scrollHeight > 20));
    const toastRect = toast.getBoundingClientRect();
    mobileToastLayout.visible = true;
    mobileToastLayout.height = toastRect.height;
    documentRoot.setAttribute("data-mobile-toast-visible", "");
    documentRoot.style.setProperty("--mobile-toast-height", `${toastRect.height}px`);

    for (const element of avoiders) {
      const rect = element.getBoundingClientRect();
      const overlaps = toastRect.left - 8 < rect.right && toastRect.right + 8 > rect.left;
      element.toggleAttribute("data-mobile-toast-overlap", overlaps);
    }
  }

  const mutations = new MutationObserver(schedule);
  mutations.observe(document.body, { childList: true, subtree: true });
  mutations.observe(root, {
    attributes: true,
    subtree: true,
    attributeFilter: ["data-front", "data-removed", "data-mounted"],
  });
  window.addEventListener("resize", schedule);
  window.visualViewport?.addEventListener("resize", schedule);
  window.visualViewport?.addEventListener("scroll", schedule);
  schedule();

  return () => {
    cancelAnimationFrame(frame);
    mutations.disconnect();
    resizeObserver.disconnect();
    window.removeEventListener("resize", schedule);
    window.visualViewport?.removeEventListener("resize", schedule);
    window.visualViewport?.removeEventListener("scroll", schedule);
    reset();
  };
}
