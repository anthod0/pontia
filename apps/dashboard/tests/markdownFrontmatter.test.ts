import { describe, expect, test } from "vitest";
import { parseMarkdownFrontmatter } from "../src/components/chat/markdownFrontmatter";

describe("parseMarkdownFrontmatter", () => {
  test("extracts frontmatter terminated by a document separator", () => {
    expect(parseMarkdownFrontmatter("---\ntitle: Notes\ntags:\n  - demo\n---\n# Body\n")).toEqual({
      frontmatter: "title: Notes\ntags:\n  - demo",
      body: "# Body\n",
    });
  });

  test("extracts CRLF frontmatter terminated by a document end marker", () => {
    expect(parseMarkdownFrontmatter("---\r\ntitle: Notes\r\n...\r\n# Body\r\n")).toEqual({
      frontmatter: "title: Notes",
      body: "# Body\r\n",
    });
  });

  test("recognizes empty frontmatter and an empty body", () => {
    expect(parseMarkdownFrontmatter("---\n---")).toEqual({ frontmatter: "", body: "" });
  });

  test.each([
    ["an empty file", ""],
    ["a thematic break in the body", "# Heading\n\n---\n\nParagraph"],
    ["an opening delimiter after content", "Intro\n---\ntitle: Notes\n---\nBody"],
    ["an unterminated opening delimiter", "---\ntitle: Notes\n# Body"],
  ])("leaves %s unchanged", (_case, source) => {
    expect(parseMarkdownFrontmatter(source)).toEqual({ frontmatter: null, body: source });
  });
});
