import { m } from "$lib/paraglide/messages";

/** Adds a copy button to each code block under `root` that does not have one yet. */
export function addCodeCopyButtons(root: Element): void {
  for (const block of root.querySelectorAll<HTMLElement>("pre")) {
    if (block.dataset.copyable === "true") continue;
    const code = block.querySelector("code");
    if (!code) continue;

    block.dataset.copyable = "true";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "code-copy-button";
    button.textContent = m.code_copy();
    button.setAttribute("aria-label", m.code_copy_label());
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent ?? "");
        button.textContent = m.code_copied();
        button.setAttribute("aria-label", m.code_copy_again());
        setTimeout(() => {
          button.textContent = m.code_copy();
          button.setAttribute("aria-label", m.code_copy_label());
        }, 2000);
      } catch {
        button.textContent = m.code_copy_failed();
        setTimeout(() => {
          button.textContent = m.code_copy();
        }, 2000);
      }
    });
    block.appendChild(button);
  }
}
