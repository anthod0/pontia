import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { onTestFinished } from "vitest";

const mkdtempDisposable = fs.mkdtempDisposable;

export async function tempDir(prefix) {
    const dir = await mkdtempDisposable(join(tmpdir(), prefix));
    onTestFinished(() => dir.remove());
    return dir.path;
}
