import { getContext, setContext, type Snippet } from "svelte";
import { pushState } from "$app/navigation";
import { page } from "$app/state";

type Layer = {
  id: string;
  title?: string;
  subtitle?: string;
  modal?: boolean;
  content?: Snippet;
  dismiss?: () => void;
};
const key = Symbol("mobile overlays");

export class MobileOverlayStack {
  layers = $state.raw<Layer[]>([]);
  #backPending = false;
  #afterBack?: () => void;

  open(layer: Omit<Layer, "id">) {
    const id = crypto.randomUUID();
    this.layers = [...this.layers, { ...layer, id }];
    pushState("", { ...page.state, mobileLayer: id });
    return id;
  }
  back(after?: () => void) {
    if (this.#backPending) return;
    this.#backPending = true;
    this.#afterBack = after;
    history.back();
  }
  reconcile(id?: string) {
    const index = this.layers.findIndex((layer) => layer.id === id);
    const removed = this.layers.slice(index + 1);
    if (!removed.length) return;
    this.#backPending = false;
    this.layers = this.layers.slice(0, index + 1);
    for (const layer of removed.reverse()) layer.dismiss?.();
    const after = this.#afterBack;
    this.#afterBack = undefined;
    after?.();
  }
  release(id: string) {
    if (page.state.mobileLayer === id && this.layers.at(-1)?.id === id) this.back();
  }
}
export function provideMobileStack() {
  return setContext(key, new MobileOverlayStack());
}
export function useMobileStack() {
  return getContext<MobileOverlayStack | undefined>(key);
}
