import { fromStore } from "svelte/store";
import { translate } from "$lib/i18n";
import { downloads, downloadKey } from "$lib/downloads/manager";
import { canPreviewImage } from "$lib/downloads/preview";
import { ApiError } from "../../../api/errors";
import type { SharedFile } from "../../../api/types";
import type { DeviceRuntime } from "../../../device/runtime";

const tasks = fromStore(downloads);

/** Download and preview state of one shared file, shared by every view of it in a card. */
export class SharedFileState {
  readonly device: DeviceRuntime;
  readonly sessionId: string;
  readonly file: SharedFile;
  readonly image: boolean;
  readonly key: string;
  readonly stem: string;
  readonly extension: string;
  /** Object URL of the image preview; null when the content is not a previewable image. */
  imageUrl = $state<string | null | undefined>(undefined);
  // Loading an image preview reads the same file, so its failure also tells whether the file is gone.
  private previewFailure = $state<Error | null>(null);
  private previewRequested = false;

  constructor(device: DeviceRuntime, sessionId: string, file: SharedFile) {
    this.device = device;
    this.sessionId = sessionId;
    this.file = file;
    this.image = canPreviewImage(file);
    this.key = downloadKey(device.id, sessionId, { kind: "shared", fileId: file.file_id });
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
  private readonly previewMissing = $derived(
    this.previewFailure instanceof ApiError && this.previewFailure.code === "not_found",
  );
  // A download started after a failed preview supersedes the preview's error.
  readonly failure = $derived(
    this.task
      ? this.task.status === "failed"
        ? this.task.error
        : null
      : this.previewFailure
        ? this.previewMissing
          ? translate("files.missing")
          : this.previewFailure.message
        : null,
  );
  readonly missing = $derived(
    this.task
      ? this.task.status === "failed" && this.task.errorCode === "not_found"
      : this.previewMissing,
  );
  /** Whether the image preview can show, or is still loading. */
  get showImage(): boolean {
    return this.image && this.imageUrl !== null && !this.previewFailure && !this.missing;
  }

  loadPreview() {
    if (!this.image || this.previewRequested) return;
    this.previewRequested = true;
    void this.device.previews
      .sharedImageUrl(this.sessionId, this.file.file_id)
      .then((url) => {
        this.imageUrl = url;
      })
      .catch((cause) => {
        this.previewFailure = cause instanceof Error ? cause : new Error(String(cause));
      });
  }

  download() {
    downloads.start(
      this.device.id,
      this.sessionId,
      { kind: "shared", fileId: this.file.file_id },
      this.file.name,
    );
  }

  cancel() {
    if (this.task) downloads.cancel(this.task.id);
  }
}

/** Loads the image preview once the element scrolls into view. */
export function observePreview(node: HTMLElement, state: SharedFileState) {
  if (!state.image) return;
  const observer = new IntersectionObserver((entries) => {
    if (!entries.some((entry) => entry.isIntersecting)) return;
    observer.disconnect();
    state.loadPreview();
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
