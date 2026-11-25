import { onMount } from "svelte";
import { dashboardRelativePath } from "$lib/dashboardRoutes";
import { activeOnPath, eventCombo, ignoredEvent, matches, type ShortcutId } from "./registry";
import { shortcuts } from "./store.svelte";

type Handler = (event: KeyboardEvent) => void;
const handlers = new Map<ShortcutId, Handler>();

export function registerShortcut(id: ShortcutId, handler: Handler) {
  onMount(() => {
    handlers.set(id, handler);
    return () => {
      if (handlers.get(id) === handler) handlers.delete(id);
    };
  });
}

export function dispatchShortcut(event: KeyboardEvent) {
  if (ignoredEvent(event)) return;
  const pressed = eventCombo(event);
  const path = dashboardRelativePath();
  for (const [id, handler] of handlers) {
    const binding = shortcuts.binding(id);
    if (binding && activeOnPath(id, path) && matches(id, binding, pressed)) {
      event.preventDefault();
      handler(event);
      return;
    }
  }
  // Option+letter can insert a symbol in the composer while its chat control is unavailable.
  if (!pressed.alt || !activeOnPath("newChat", path)) return;
  for (const id of ["toggleGit", "switchAgentClient", "switchWorkspace"] as const) {
    const binding = shortcuts.binding(id);
    if (binding && matches(id, binding, pressed)) {
      event.preventDefault();
      return;
    }
  }
}
