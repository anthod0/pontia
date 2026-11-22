import { fromStore } from "svelte/store";
import { translate } from "$lib/i18n";
import { downloads, downloadKey } from "$lib/downloads/manager";
import { canPreviewImage } from "$lib/downloads/preview";
import { isImagePath } from "$lib/downloads/localFilePath";
import { ApiError } from "../../../api/errors";
import type { FileInfo } from "../../../api/sessionFiles";
import type { AttachedFile } from "../../../api/types";
import type { DeviceRuntime } from "../../../device/runtime";

const tasks = fromStore(downloads);

/** Download and preview state of one attached file, shared by every view of it in a card. */
export class SharedFileState {
  readonly key: string;
  readonly stem: string;
  readonly extension: string;
  /** Size and type read when the file first scrolls into view. */
  info = $state<FileInfo | null>(null);
  /** Object URL of the image preview; null when the content is not a previewable image. */
  imageUrl = $state<string | null | undefined>(undefined);
  // Reading the file's details or image also tells whether the file is gone.
  private loadFailure = $state<Error | null>(null);
  private requested = false;

  constructor(
    readonly device: DeviceRuntime,
    readonly sessionId: string,
    readonly file: AttachedFile,
  ) {
    this.key = downloadKey(device.id, sessionId, { path: file.path });
    const dot = file.name.lastIndexOf(".");
    this.stem = dot > 0 ? file.name.slice(0, dot) : file.name;
    this.extension = dot > 0 ? file.name.slice(dot + 1) : "";
  }

  readonly task = $derived(tasks.current.find((task) => task.key === this.key));
  readonly receiving = $derived(this.task?.status === "receiving");
  readonly progress = $derived(this.receiving ? this.task!.progress : null);
  readonly percent = $derived(
    this.progress?.total ? (this.progress.received / this.progress.total) * 100 : null,
  );
  private readonly loadMissing = $derived(
    this.loadFailure instanceof ApiError && this.loadFailure.code === "not_found",
  );
  // A download started after a failed load supersedes the load's error.
  readonly failure = $derived(
    this.task
      ? this.task.status === "failed"
        ? this.task.error
        : null
      : this.loadFailure
        ? this.loadMissing
          ? translate("files.missing")
          : this.loadFailure.message
        : null,
  );
  readonly missing = $derived(
    this.task
      ? this.task.status === "failed" && this.task.errorCode === "not_found"
      : this.loadMissing,
  );
  /** Whether the file can show as an image; judged by its name until its details load. */
  get image(): boolean {
    return this.info ? canPreviewImage(this.info) : isImagePath(this.file.path);
  }

  /** Whether the image preview can show, or is still loading. */
  get showImage(): boolean {
    return this.image && this.imageUrl !== null && !this.loadFailure && !this.missing;
  }

  /** Reads the file's details, then its image preview when it is an image. */
  load() {
    if (this.requested) return;
    this.requested = true;
    const fail = (cause: unknown) => {
      this.loadFailure = cause instanceof Error ? cause : new Error(String(cause));
    };
    void this.device.fileApi
      .pathFileInfo(this.sessionId, this.file.path)
      .then((info) => {
        this.info = info;
        if (!canPreviewImage(info)) return;
        return this.device.previews.imageUrl(this.sessionId, this.file.path).then((url) => {
          this.imageUrl = url;
        });
      })
      .catch(fail);
  }

  download() {
    downloads.start(this.device.id, this.sessionId, { path: this.file.path }, this.file.name);
  }

  cancel() {
    if (this.task) downloads.cancel(this.task.id);
  }
}

/** Loads the file's details and preview once the element scrolls into view. */
export function observeFile(node: HTMLElement, state: SharedFileState) {
  const observer = new IntersectionObserver((entries) => {
    if (!entries.some((entry) => entry.isIntersecting)) return;
    observer.disconnect();
    state.load();
  });
  observer.observe(node);
  return { destroy: () => observer.disconnect() };
}

export function formatFileSize(
  size: number,
  format: (value: number, options?: Intl.NumberFormatOptions) => string,
): string {
  if (size < 1024) return `${format(size)} B`;
  if (size < 1024 * 1024) return `${format(size / 1024, { maximumFractionDigits: 1 })} KiB`;
  return `${format(size / 1024 / 1024, { maximumFractionDigits: 1 })} MiB`;
}
