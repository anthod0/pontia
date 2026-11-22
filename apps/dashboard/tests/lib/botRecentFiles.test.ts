import { beforeEach, expect, test } from "vitest";
import { createRecentFiles, withRecentFile, type RecentFile } from "../../src/lib/botRecentFiles";

const file = (n: number): RecentFile => ({ sessionId: "main", path: `/work/${n}.md` });

beforeEach(() => localStorage.clear());

test("puts the newest file first and moves a reopened file up without repeating it", () => {
  const files = [file(1), file(2), file(3)].reduce(withRecentFile, []);
  expect(files).toEqual([file(3), file(2), file(1)]);
  const reopened = { sessionId: "later", path: file(1).path };
  expect(withRecentFile(files, reopened)).toEqual([reopened, file(3), file(2)]);
});

test("keeps the 10 most recent files", () => {
  const files = Array.from({ length: 12 }, (_, n) => file(n)).reduce(withRecentFile, []);
  expect(files).toHaveLength(10);
  expect(files[0]).toEqual(file(11));
  expect(files.at(-1)).toEqual(file(2));
});

test("keeps a separate list per device and Bot that survives a reload", () => {
  createRecentFiles("local", "bot-1").current = [file(1)];
  createRecentFiles("local", "bot-2").current = [file(2)];
  createRecentFiles("office", "bot-1").current = [file(3)];
  expect(createRecentFiles("local", "bot-1").current).toEqual([file(1)]);
  expect(createRecentFiles("local", "bot-2").current).toEqual([file(2)]);
  expect(createRecentFiles("office", "bot-1").current).toEqual([file(3)]);
  expect(createRecentFiles("office", "bot-2").current).toEqual([]);
});
