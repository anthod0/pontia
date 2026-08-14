import * as fs from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { onTestFinished } from "vitest";

interface TempDir {
  path: string;
  remove(): Promise<void>;
}

const mkdtempDisposable = (fs as typeof fs & {
  mkdtempDisposable(prefix: string): Promise<TempDir>;
}).mkdtempDisposable;

export async function tempDir(prefix: string): Promise<string> {
  const dir = await mkdtempDisposable(join(tmpdir(), prefix));
  onTestFinished(() => dir.remove());
  return dir.path;
}
