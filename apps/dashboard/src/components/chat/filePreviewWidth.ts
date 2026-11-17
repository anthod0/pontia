import { writable } from "svelte/store";

const storageKey = "pontia.filePreviewWidth";
export const FILE_PREVIEW_MIN_WIDTH = 320;
// Room kept for the sidebar and the conversation next to the preview.
export const FILE_PREVIEW_RESERVED_WIDTH = 480;

function readWidth(): number | null {
  if (typeof localStorage === "undefined") return null;
  const saved = Number(localStorage.getItem(storageKey));
  return Number.isFinite(saved) && saved > 0 ? saved : null;
}

const width = writable<number | null>(readWidth());

export function clampFilePreviewWidth(value: number, viewport: number): number {
  return Math.round(
    Math.max(FILE_PREVIEW_MIN_WIDTH, Math.min(value, viewport - FILE_PREVIEW_RESERVED_WIDTH)),
  );
}

export const filePreviewWidth = {
  subscribe: width.subscribe,
  /** Updates the width while dragging without persisting it. */
  preview(value: number): void {
    width.set(value);
  },
  save(value: number | null): void {
    width.set(value);
    if (value === null) localStorage.removeItem(storageKey);
    else localStorage.setItem(storageKey, String(value));
  },
};
