import { expect, test } from "vitest";
import { parseStructuredPreview } from "../src/features/chat/structuredPreview";

test.each(["json", "yaml"] as const)("retains the digits of a large %s integer", (format) => {
  expect(parseStructuredPreview("9007199254740993", format)).toEqual({
    kind: "number",
    text: "9007199254740993",
  });
});

test("shows each YAML document and keeps a recursive alias as a leaf", () => {
  expect(parseStructuredPreview("&root [*root]\n---\nnull", "yaml")).toEqual({
    kind: "array",
    children: [
      ["0", { kind: "array", children: [["0", { kind: "alias", text: "*root" }]] }],
      ["1", { kind: "null", text: "null" }],
    ],
  });
});
