import { expect, test } from "vitest";
import {
  changeDirLabel,
  groupChanges,
  wordHighlights,
} from "../../../src/components/chat/git/gitChanges";

test("shows the directory, or old and new directories for a move", () => {
  expect(changeDirLabel({ path: "apps/dashboard/src/queries/workspaces.ts" })).toBe(
    "apps/dashboard/src/queries",
  );
  expect(changeDirLabel({ path: "README.md" })).toBe("");
  expect(
    changeDirLabel({
      path: "crates/pontia-application/src/git/status.rs",
      orig_path: "crates/pontia-application/src/status.rs",
    }),
  ).toBe("crates/pontia-application/src → crates/pontia-application/src/git");
  expect(changeDirLabel({ path: "src/status.rs", orig_path: "status.rs" })).toBe(". → src");
});

test("keeps the old name visible when a rename changes the file name", () => {
  expect(
    changeDirLabel({
      path: "crates/pontia-application/src/git/status.rs",
      orig_path: "crates/pontia-application/src/git_status.rs",
    }),
  ).toBe("crates/pontia-application/src/git_status.rs → crates/pontia-application/src/git");
  expect(changeDirLabel({ path: "docs/renamed.md", orig_path: "docs/new.md" })).toBe("← new.md");
});

test("shows untracked files with unstaged changes", () => {
  const files = [
    {
      path: "tracked.ts",
      group: "unstaged" as const,
      status: "M",
      additions: 1,
      deletions: 0,
      binary: false,
    },
    {
      path: "new.ts",
      group: "untracked" as const,
      status: "?",
      additions: 2,
      deletions: 0,
      binary: false,
    },
  ];

  expect(groupChanges(files)).toEqual([{ group: "unstaged", label: "git.unstaged", files }]);
});

test("highlights the changed middle of a related deleted and added line", () => {
  const deleted = { kind: "delete" as const, old: 2, new: null, text: "const open = false;" };
  const added = { kind: "add" as const, old: null, new: 2, text: "const open = true;" };

  const highlights = wordHighlights([{ header: "@@ -2 +2 @@", lines: [deleted, added] }]);

  expect(deleted.text.slice(highlights.get(deleted)!.start, highlights.get(deleted)!.end)).toBe(
    "fals",
  );
  expect(added.text.slice(highlights.get(added)!.start, highlights.get(added)!.end)).toBe("tru");
});

test("does not highlight when most of a line changed", () => {
  const deleted = { kind: "delete" as const, old: 2, new: null, text: "old value" };
  const added = { kind: "add" as const, old: null, new: 2, text: "new result" };

  expect(wordHighlights([{ header: "@@ -2 +2 @@", lines: [deleted, added] }]).size).toBe(0);
});
