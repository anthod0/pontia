import { expect, test } from "vitest";
import { parseDelimitedTable } from "../../src/features/chat/delimitedTable";

test("reads RFC 4180 records with embedded delimiters, newlines and doubled quotes", () => {
  expect(
    parseDelimitedTable('\uFEFFname,note,extra\r\n"a,b","line 1\r\nline ""2""",\r\n, "",\r\n', ","),
  ).toEqual({
    header: ["name", "note", "extra"],
    rows: [
      ["a,b", 'line 1\r\nline "2"', ""],
      ["", ' ""', ""],
    ],
    rowCount: 2,
    columnCount: 3,
  });
});

test("preserves TSV tabs inside quotes, commas, empty fields and blank records", () => {
  const table = parseDelimitedTable('title\tvalue\n\n"a\tb"\t1,000\t', "\t");
  expect(table.rows).toEqual([[""], ["a\tb", "1,000", ""]]);
  expect(table.columnCount).toBe(3);
});

test("counts records and columns beyond both display limits", () => {
  const table = parseDelimitedTable(
    ["first", ...Array(201).fill("1"), Array(51).fill("last").join(",")].join("\n"),
    ",",
  );
  expect(table.rows).toHaveLength(200);
  expect(table.rowCount).toBe(202);
  expect(table.columnCount).toBe(51);
  const wide = parseDelimitedTable(Array(51).fill("col").join(","), ",");
  expect(wide.header).toHaveLength(50);
  expect(wide.rowCount).toBe(0);
});
