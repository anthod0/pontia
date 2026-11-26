import { expect, test } from "vitest";
import { canPreviewText } from "../src/components/chat/fileKind";

test.each([
  "notes.md",
  "article.mdx",
  "article.svx",
  "build.log",
  "data.json",
  "config.YAML",
  "table.csv",
  "logo.svg",
  "main.rs",
  "Makefile",
  "LICENSE",
  "archive.unknown",
])("%s opens in the preview panel", (name) => expect(canPreviewText(name)).toBe(true));

test.each([
  "demo.mp4",
  "voice.mp3",
  "paper.pdf",
  "bundle.zip",
  "report.docx",
  "sheet.xlsx",
  "slides.pptx",
  "photo.png",
])("%s does not open in the preview panel", (name) => expect(canPreviewText(name)).toBe(false));
