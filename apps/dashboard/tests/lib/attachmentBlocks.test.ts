import { expect, test } from "vitest";
import { attachedPaths, splitAttachments } from "../../src/lib/attachmentBlocks";

const block = (...lines: string[]) => ["```attachments", ...lines, "```"].join("\n");
const text = (value: string) => ({ kind: "text", text: value });
const files = (...paths: string[]) => ({ kind: "files", paths });

test("keeps a block at the end of the message after its text", () => {
  expect(
    splitAttachments(
      `Here are the results.\n\n${block("/work/report.pdf", "", "  /work/chart.png")}`,
    ),
  ).toEqual([text("Here are the results."), files("/work/report.pdf", "/work/chart.png")]);
});

test("keeps a block at the start of the message before its text", () => {
  expect(splitAttachments(`${block("/a")}\n\nDone.`)).toEqual([files("/a"), text("Done.")]);
});

test("splits the text around a block in the middle", () => {
  expect(splitAttachments(`Before.\n\n${block("/a")}\n\nAfter.`)).toEqual([
    text("Before."),
    files("/a"),
    text("After."),
  ]);
});

test("keeps each block at its place and drops paths repeated within one block", () => {
  expect(splitAttachments(`${block("/a", "/b", "/a")}\n\nMore:\n\n${block("/b", "/c")}`)).toEqual([
    files("/a", "/b"),
    text("More:"),
    files("/b", "/c"),
  ]);
  expect(splitAttachments(`${block("/a")}\n\n\n${block("/b")}`)).toEqual([
    files("/a"),
    files("/b"),
  ]);
});

test("lists every attached path of a message once", () => {
  expect(attachedPaths(`${block("/a", "/b")}\n\nMore:\n\n${block("/b", "/c")}`)).toEqual([
    "/a",
    "/b",
    "/c",
  ]);
  expect(attachedPaths("No files.")).toEqual([]);
});

test("takes paths literally, without escaping", () => {
  expect(splitAttachments(block('/work/a & b "<1>".txt'))).toEqual([
    files('/work/a & b "<1>".txt'),
  ]);
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
    expect(splitAttachments(content)).toEqual([text(content)]);
  expect(splitAttachments("")).toEqual([]);
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
    expect(splitAttachments(content, true)).toEqual([text("Sending.")]);
    expect(splitAttachments(content)).toEqual([text(content)]);
  }
  expect(splitAttachments("```attachments\n/a", true)).toEqual([]);
  expect(splitAttachments("```sh\nls", true)).toEqual([text("```sh\nls")]);
  expect(splitAttachments(`${block("/a")}\n\nmore`, true)).toEqual([files("/a"), text("more")]);
  expect(splitAttachments(`A\n\n${block("/a")}\n\nB\n\n\`\`\`attach`, true)).toEqual([
    text("A"),
    files("/a"),
    text("B"),
  ]);
});

test("reads Windows line endings", () => {
  expect(splitAttachments("Done.\r\n```attachments\r\n/a\r\n```\r\n")).toEqual([
    text("Done."),
    files("/a"),
  ]);
});

test("keeps blank lines inside other code blocks", () => {
  expect(splitAttachments(`\`\`\`txt\na\n\n\n\nb\n\`\`\`\n\n${block("/a")}`)[0]).toEqual(
    text("```txt\na\n\n\n\nb\n```"),
  );
});

test("keeps the markdown blocks around a block intact", () => {
  expect(splitAttachments(`Text\n${block("/a")}\nAfter`)).toEqual([
    text("Text"),
    files("/a"),
    text("After"),
  ]);
  expect(splitAttachments(`- item\n${block("/a")}\n- next`)).toEqual([
    text("- item"),
    files("/a"),
    text("- next"),
  ]);
  expect(splitAttachments(`    indented code\n\n${block("/a")}\n\n    more code`)).toEqual([
    text("    indented code"),
    files("/a"),
    text("    more code"),
  ]);
});
