import { expect, test } from "vitest";
import { splitAttachments } from "../../src/lib/attachmentBlocks";

const block = (...lines: string[]) => ["```attachments", ...lines, "```"].join("\n");

test("moves one block of several files out of the text", () => {
  expect(
    splitAttachments(
      `Here are the results.\n\n${block("/work/report.pdf", "", "  /work/chart.png")}\n\nDone.`,
    ),
  ).toEqual({
    text: "Here are the results.\n\nDone.",
    paths: ["/work/report.pdf", "/work/chart.png"],
  });
});

test("merges blocks in order without duplicates", () => {
  expect(splitAttachments(`${block("/a", "/b")}\n\nMore:\n\n${block("/b", "/c")}`)).toEqual({
    text: "More:",
    paths: ["/a", "/b", "/c"],
  });
});

test("takes paths literally, without escaping", () => {
  expect(splitAttachments(block('/work/a & b "<1>".txt')).paths).toEqual(['/work/a & b "<1>".txt']);
});

test("keeps other code blocks, and blocks with a line that is not an absolute path", () => {
  for (const content of [
    "```sh\n/usr/bin/env\n```",
    block("/work/report.pdf", "relative.txt"),
    block("/work/report.pdf", "~/notes.md"),
    block(),
    "    ```attachments\n    /a\n    ```",
    "~~~attachments\n/a\n~~~",
    "Inline `attachments /a` text",
  ])
    expect(splitAttachments(content)).toEqual({ text: content, paths: [] });
});

test("hides an unclosed block while streaming and shows it as text once finished", () => {
  for (const tail of [
    "```",
    "```attach",
    "```attachments",
    "```attachments\n/work/a",
    "```attachments\n/work/a\n``",
  ]) {
    const content = `Sending.\n\n${tail}`;
    expect(splitAttachments(content, true)).toEqual({ text: "Sending.", paths: [] });
    expect(splitAttachments(content)).toEqual({ text: content, paths: [] });
  }
  expect(splitAttachments("```sh\nls", true)).toEqual({ text: "```sh\nls", paths: [] });
  expect(splitAttachments(`${block("/a")}\n\nmore`, true)).toEqual({ text: "more", paths: ["/a"] });
});

test("reads Windows line endings", () => {
  expect(splitAttachments("Done.\r\n```attachments\r\n/a\r\n```\r\n")).toEqual({
    text: "Done.",
    paths: ["/a"],
  });
});

test("keeps blank lines inside other code blocks", () => {
  expect(splitAttachments(`\`\`\`txt\na\n\n\n\nb\n\`\`\`\n\n${block("/a")}`).text).toBe(
    "```txt\na\n\n\n\nb\n```",
  );
});

test("keeps the blocks around a removed block apart", () => {
  expect(splitAttachments(`Text\n${block("/a")}\nAfter`).text).toBe("Text\n\nAfter");
  expect(splitAttachments(`- item\n${block("/a")}\nAfter`).text).toBe("- item\n\nAfter");
  expect(splitAttachments(`    indented code\n\n${block("/a")}`).text).toBe("    indented code");
});
