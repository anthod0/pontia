type Node = {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: Node[];
};

/** Wrap every table in a horizontally scrollable container so wide tables never widen the page. */
export function rehypeTableScroll() {
  const wrap = (node: Node) => {
    if (!node.children) return;
    node.children = node.children.map((child) => {
      if (child.type === "element" && child.tagName === "table") {
        return {
          type: "element",
          tagName: "div",
          properties: { className: ["table-scroll"] },
          children: [child],
        };
      }
      wrap(child);
      return child;
    });
  };
  return (tree: Node) => wrap(tree);
}
