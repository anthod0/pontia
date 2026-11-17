import { getContext, setContext } from "svelte";
import { writable, type Writable } from "svelte/store";
import type { DeviceRuntime } from "../../device/runtime";

export type FilePreviewTarget = { device: DeviceRuntime; sessionId: string; path: string };
const key = Symbol("file-preview");

export function provideFilePreview() {
  return setContext(key, writable<FilePreviewTarget | null>(null));
}

export function useFilePreview() {
  return getContext<Writable<FilePreviewTarget | null>>(key);
}
