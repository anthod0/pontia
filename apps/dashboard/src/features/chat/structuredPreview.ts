import { isAlias, isMap, isScalar, isSeq, parseAllDocuments, type ParsedNode } from "yaml";

export type TreeNode =
  | { kind: "object" | "array"; children: [string, TreeNode][] }
  | { kind: "string" | "number" | "boolean" | "null" | "alias"; text: string };

function scalar(value: unknown, source?: string): TreeNode {
  if (typeof value === "string") return { kind: "string", text: JSON.stringify(value) };
  if (typeof value === "boolean") return { kind: "boolean", text: String(value) };
  if (value === null) return { kind: "null", text: "null" };
  if (typeof value === "number" || typeof value === "bigint")
    return { kind: "number", text: source ?? String(value) };
  return { kind: "string", text: String(value) };
}

function jsonNode(_key: string, value: unknown, context?: { source?: string }): TreeNode {
  if (typeof value !== "object" || value === null) return scalar(value, context?.source);
  // JSON.parse visits children before their parent.
  return {
    kind: Array.isArray(value) ? "array" : "object",
    children: Object.entries(value),
  };
}

function yamlNode(node: ParsedNode | null, source: string): TreeNode {
  if (isMap(node)) {
    return {
      kind: "object",
      children: node.items.map(({ key, value }) => [
        isScalar(key) ? String(key.value) : source.slice(key.range[0], key.range[1]),
        yamlNode(value, source),
      ]),
    };
  }
  if (isSeq(node)) {
    return {
      kind: "array",
      children: node.items.map((item, i) => [String(i), yamlNode(item, source)]),
    };
  }
  if (isAlias(node)) return { kind: "alias", text: "*" + node.source };
  return scalar(node?.value ?? null, node?.source);
}

export function parseStructuredPreview(source: string, format: "json" | "yaml"): TreeNode {
  if (format === "json") return JSON.parse(source, jsonNode);
  const documents = parseAllDocuments(source, { intAsBigInt: true });
  const roots = documents.map((doc) => {
    if (doc.errors.length) throw doc.errors[0];
    return yamlNode(doc.contents, source);
  });
  return roots.length > 1
    ? { kind: "array", children: roots.map((root, index) => [String(index), root]) }
    : (roots[0] ?? scalar(null));
}
