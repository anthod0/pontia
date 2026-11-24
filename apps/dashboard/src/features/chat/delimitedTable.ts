const MAX_ROWS = 200;
export const MAX_COLUMNS = 50;

export function parseDelimitedTable(text: string, delimiter: "," | "\t") {
  let header: string[] = [];
  const rows: string[][] = [];
  let recordCount = 0;
  let columnCount = 0;
  let offset = text.startsWith("\uFEFF") ? 1 : 0;

  while (offset < text.length) {
    const row: string[] = [];
    let columns = 0;
    while (true) {
      const quoted = text[offset] === '"';
      const start = quoted ? ++offset : offset;
      if (quoted) {
        while (offset < text.length) {
          if (text[offset++] !== '"') continue;
          if (text[offset] !== '"') break;
          offset++;
        }
      }
      const end = quoted && offset > start && text[offset - 1] === '"' ? offset - 1 : offset;
      while (
        offset < text.length &&
        text[offset] !== delimiter &&
        text[offset] !== "\r" &&
        text[offset] !== "\n"
      )
        offset++;
      if (recordCount <= MAX_ROWS && columns < MAX_COLUMNS) {
        row.push(quoted ? text.slice(start, end).replaceAll('""', '"') : text.slice(start, offset));
      }
      columns++;
      if (text[offset] !== delimiter) break;
      offset++;
    }
    if (recordCount === 0) header = row;
    else if (recordCount <= MAX_ROWS) rows.push(row);
    recordCount++;
    columnCount = Math.max(columnCount, columns);
    if (text[offset] === "\r") offset++;
    if (text[offset] === "\n") offset++;
  }
  return { header, rows, rowCount: Math.max(0, recordCount - 1), columnCount };
}
