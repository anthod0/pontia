import { getContext, setContext } from "svelte";
import type { DeviceRuntime } from "../../device/runtime";

export type FilePreviewTarget = {
  device: DeviceRuntime;
  sessionId: string;
  path: string;
  anchor?: string | null;
};

/** Opens local file links. The preview panel provides its own navigator for links inside the previewed file. */
export type FilePreviewNavigator = { open(target: FilePreviewTarget): void };

const key = Symbol("file-preview");

export function provideFilePreviewNavigator(navigator: FilePreviewNavigator) {
  setContext(key, navigator);
}

export function useFilePreview() {
  return getContext<FilePreviewNavigator>(key);
}
