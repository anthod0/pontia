import { expect, test, vi } from "vitest";
import { readTextPreview } from "../src/lib/downloads/textPreview";

const FILE_PREVIEW_LIMIT = 5 * 1024 * 1024;

test("decodes UTF-8 characters split across network chunks and preserves whitespace", async () => {
  const content = "\t你好\nconst x = '🌍';\n";
  const bytes = new TextEncoder().encode(content);
  const response = new Response(
    new ReadableStream({
      start(controller) {
        for (const byte of bytes) controller.enqueue(new Uint8Array([byte]));
        controller.close();
      },
    }),
  );
  expect(await readTextPreview(response)).toBe(content);
});

test.each([true, false])(
  "rejects oversized text and cancels the response (known size: %s)",
  async (known) => {
    const cancel = vi.fn();
    const response = new Response(
      new ReadableStream({
        start(controller) {
          controller.enqueue(new Uint8Array(FILE_PREVIEW_LIMIT + 1));
        },
        cancel,
      }),
      { headers: known ? { "Content-Length": String(FILE_PREVIEW_LIMIT + 1) } : {} },
    );
    await expect(readTextPreview(response)).rejects.toThrow();
    expect(cancel).toHaveBeenCalled();
  },
);

test("accepts text at the five MiB preview limit", async () => {
  const content = "x".repeat(FILE_PREVIEW_LIMIT);
  expect(await readTextPreview(new Response(content))).toBe(content);
});

test.each([new Uint8Array([0, 1]), new Uint8Array([0xff]), new Uint8Array([0xe4, 0xbd])])(
  "rejects binary or invalid UTF-8 instead of displaying corrupted text",
  async (bytes) => {
    await expect(readTextPreview(new Response(bytes))).rejects.toThrow();
  },
);

test("accepts an empty file", async () => {
  expect(await readTextPreview(new Response(""))).toBe("");
});
