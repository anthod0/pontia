import { englishOnlyPages, localePrefixes } from "../src/lib/locales";

type Node = {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: Node[];
};

const contentLocale = /\/src\/content\/(?:docs|blog)\/([^/]+)\//;

/**
 * Prefix site-relative links in translated content with the locale of the file, so a translation
 * keeps the same link targets as its English source.
 */
export function rehypeLocalizeLinks() {
  return (tree: Node, file: { filename?: string }) => {
    const locale = file.filename?.replaceAll("\\", "/").match(contentLocale)?.[1];
    if (!locale || locale === "en") return;
    const prefix = `/${localePrefixes[locale as keyof typeof localePrefixes]}`;
    const visit = (node: Node) => {
      const href = node.properties?.href;
      if (
        node.tagName === "a" &&
        typeof href === "string" &&
        href.startsWith("/") &&
        !href.startsWith("//") &&
        !englishOnlyPages.some((page) => href === page || href.startsWith(`${page}#`))
      ) {
        node.properties!.href = `${prefix}${href}`;
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}
