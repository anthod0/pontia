import { getContext, setContext } from "svelte";

/** The composer that receives files dropped anywhere on the main panel. */
export type FileDropTarget = {
  /** Why the composer cannot take files right now, or null when it can. */
  readonly unavailableReason: string | null;
  /** The input box the drop overlay stops above. */
  readonly element: HTMLElement | null;
  add(files: File[]): void;
};

const key = Symbol("file-drop");

export class FileDrop {
  target = $state.raw<FileDropTarget | null>(null);
  /** True while files are dragged over the main panel. */
  dragging = $state(false);

  /** The target that would take the files if they were dropped now. */
  get activeTarget(): FileDropTarget | null {
    return this.dragging && this.target && !this.target.unavailableReason ? this.target : null;
  }

  /** Makes `target` receive drops until the returned function runs. */
  register(target: FileDropTarget): () => void {
    this.target = target;
    return () => {
      if (this.target === target) this.target = null;
    };
  }
}

export function provideFileDrop(): FileDrop {
  return setContext(key, new FileDrop());
}

/** Undefined outside the dashboard shell, where the composer has no drop zone. */
export function useFileDrop(): FileDrop | undefined {
  return getContext<FileDrop | undefined>(key);
}
