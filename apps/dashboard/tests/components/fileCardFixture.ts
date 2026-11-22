import { vi } from "vitest";
import { ApiError } from "../../src/api/errors";
import type { FileInfo } from "../../src/api/sessionFiles";
import { testDevice } from "../deviceFixture";

/** Reports every element as visible, so cards load their files' details right away. */
export class VisibleObserver {
  constructor(private readonly callback: IntersectionObserverCallback) {}
  observe(target: Element) {
    this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
  }
  disconnect() {}
}

export const missing = () => new ApiError("File no longer exists", "not_found", 404);

/**
 * Answers file details by path: images by extension, 1 KiB each, unless `files` says
 * otherwise; `null` marks a file that is gone.
 */
export function mockFileInfo(files: Record<string, Partial<FileInfo> | null> = {}) {
  return vi.spyOn(testDevice.fileApi, "pathFileInfo").mockImplementation(async (_session, path) => {
    const info = files[path];
    if (info === null) throw missing();
    return {
      size: 1024,
      media_type: path.endsWith(".png") ? "image/png" : "application/octet-stream",
      ...info,
    };
  });
}
