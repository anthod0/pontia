import { writable } from "svelte/store";
import { readStoredWith, writeStored } from "$lib/storage";

const storageKey = "pontia.filePreviewWidth";
export const FILE_PREVIEW_MIN_WIDTH = 320;
// Room kept for the sidebar and the conversation next to the preview.
export const FILE_PREVIEW_RESERVED_WIDTH = 480;

function parseWidth(saved: string): number | undefined {
  const width = Number(saved);
  return Number.isFinite(width) && width > 0 ? width : undefined;
}

const width = writable<number | null>(readStoredWith(storageKey, parseWidth));

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
    writeStored(storageKey, value === null ? null : String(value));
  },
};
