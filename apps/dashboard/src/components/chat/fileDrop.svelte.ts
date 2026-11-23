import { getContext, setContext } from "svelte";

/** The composer that receives files dropped anywhere on the main panel. */
export type FileDropTarget = {
  readonly unavailable: boolean;
  add(files: File[]): void;
};

const key = Symbol("file-drop");

export class FileDrop {
  target = $state.raw<FileDropTarget | null>(null);

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
