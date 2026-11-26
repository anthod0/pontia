export type MarkdownFrontmatter = {
  frontmatter: string | null;
  body: string;
};

const openingDelimiter = /^---\r?\n/;
const closingDelimiter = /^(?:---|\.\.\.)(?:\r?\n|$)/gm;

export function parseMarkdownFrontmatter(source: string): MarkdownFrontmatter {
  const opening = openingDelimiter.exec(source);
  if (!opening) return { frontmatter: null, body: source };

  closingDelimiter.lastIndex = opening[0].length;
  const closing = closingDelimiter.exec(source);
  if (!closing) return { frontmatter: null, body: source };

  const frontmatterEnd = closing.index;
  const bodyStart = closing.index + closing[0].length;
  return {
    frontmatter: source.slice(opening[0].length, frontmatterEnd).replace(/\r?\n$/, ""),
    body: source.slice(bodyStart),
  };
}
