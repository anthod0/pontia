import { getContext, setContext } from "svelte";
import type { LocalhostPreviewTarget } from "$lib/localhostPreview";

type OpenPreview = (target: LocalhostPreviewTarget) => void;
const key = Symbol("localhost-preview");

export function provideLocalhostPreview(open: OpenPreview) {
  setContext(key, open);
}

export function useLocalhostPreview() {
  return getContext<OpenPreview | undefined>(key);
}
