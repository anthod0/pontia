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
    button.textContent = "Copy";
    button.setAttribute("aria-label", "Copy code");
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent ?? "");
        button.textContent = "Copied";
        button.setAttribute("aria-label", "Copy code again");
        setTimeout(() => {
          button.textContent = "Copy";
          button.setAttribute("aria-label", "Copy code");
        }, 2000);
      } catch {
        button.textContent = "Failed";
        setTimeout(() => {
          button.textContent = "Copy";
        }, 2000);
      }
    });
    block.appendChild(button);
  }
}
