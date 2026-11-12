import { englishOnlyPages, localeInfo } from "../src/lib/locales";
import { parseContentPath } from "../src/lib/content-path";

type Node = {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: Node[];
};

/**
 * Prefix site-relative links in translated content with the locale of the file, so a translation
 * keeps the same link targets as its English source.
 */
export function rehypeLocalizeLinks() {
  return (tree: Node, file: { filename?: string }) => {
    const content = file.filename && parseContentPath(file.filename);
    if (!content) return;
    const { prefix } = localeInfo[content.locale];
    if (!prefix) return;
    const visit = (node: Node) => {
      const href = node.properties?.href;
      if (
        node.tagName === "a" &&
        typeof href === "string" &&
        href.startsWith("/") &&
        !href.startsWith("//")
      ) {
        const pathname = href.split(/[?#]/, 1)[0];
        if (
          !englishOnlyPages.some((page) => pathname === page) &&
          pathname !== "/api" &&
          !pathname.startsWith("/api/")
        ) {
          node.properties!.href = `/${prefix}${href}`;
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}
