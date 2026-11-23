import { beforeEach, expect, test } from "vitest";
import type { RecentFile } from "../../src/lib/botRecentFiles";
import {
  createSessionRecentFiles,
  sessionRecentFiles,
  withSessionRecentFile,
  withoutSessionRecentFiles,
} from "../../src/lib/sessionRecentFiles";

const file = (sessionId: string, n: number): RecentFile => ({ sessionId, path: `/work/${n}.md` });

beforeEach(() => localStorage.clear());

test("keeps the same path once per Session, newest first", () => {
  const files = [file("a", 1), file("b", 1), file("a", 2), file("a", 1)].reduce(
    withSessionRecentFile,
    [],
  );
  expect(files).toEqual([file("a", 1), file("a", 2), file("b", 1)]);
  expect(sessionRecentFiles(files, "a")).toEqual([file("a", 1), file("a", 2)]);
  expect(withoutSessionRecentFiles(files, "a")).toEqual([file("b", 1)]);
});

test("shows 10 files per Session and keeps 50 per device", () => {
  const files = Array.from({ length: 60 }, (_, n) => file(n % 2 ? "a" : "b", n)).reduce(
    withSessionRecentFile,
    [],
  );
  expect(files).toHaveLength(50);
  expect(sessionRecentFiles(files, "a")).toHaveLength(10);
  expect(sessionRecentFiles(files, "a")[0]).toEqual(file("a", 59));
});

test("saves one list per device and removes the key when empty", () => {
  createSessionRecentFiles("local").current = [file("a", 1)];
  expect(localStorage.getItem("pontia.sessionRecentFiles:local")).toBe(
    JSON.stringify([file("a", 1)]),
  );
  expect(createSessionRecentFiles("office").current).toEqual([]);
  createSessionRecentFiles("local").current = [];
  expect(localStorage.getItem("pontia.sessionRecentFiles:local")).toBeNull();
});
