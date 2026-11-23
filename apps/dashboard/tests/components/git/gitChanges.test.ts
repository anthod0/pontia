import { expect, test } from "vitest";
import { changeDirLabel } from "../../../src/components/chat/git/gitChanges";

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
  expect(changeDirLabel({ path: "docs/renamed.md", orig_path: "docs/new.md" })).toBe(
    "docs/new.md → docs",
  );
});
