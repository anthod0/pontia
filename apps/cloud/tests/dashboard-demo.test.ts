import { expect, test } from "bun:test";
import { inlineTokens, markdownBlocks } from "../src/lib/components/dashboard-demo/text";

test("chat links distinguish a workspace file from a localhost preview", () => {
  expect(
    inlineTokens("[notes](notes/check.md) or [Atlas](http://localhost:5173/projects)"),
  ).toEqual([
    { kind: "link", value: "notes", path: "notes/check.md" },
    { kind: "text", value: " or " },
    { kind: "preview", value: "Atlas", url: "http://localhost:5173/projects" },
  ]);
});

test("Markdown keeps fenced text literal and resumes parsing after the closing fence", () => {
  expect(
    markdownBlocks(
      [
        "## Results",
        "- one",
        "- two",
        "",
        "```sh",
        "# comment",
        "- literal",
        "```",
        "|A|B|",
        "|---|---|",
        "|1|2|",
        "> Done",
        "Next",
      ].join("\n"),
    ),
  ).toEqual([
    { kind: "h2", text: "Results" },
    { kind: "ul", items: ["one", "two"] },
    { kind: "code", language: "sh", code: "# comment\n- literal" },
    { kind: "table", head: ["A", "B"], rows: [["1", "2"]] },
    { kind: "quote", text: "Done" },
    { kind: "p", text: "Next" },
  ]);
});
