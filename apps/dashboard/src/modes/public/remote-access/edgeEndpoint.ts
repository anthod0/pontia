// https://fetch.spec.whatwg.org/#port-blocking, plus ports the control-plane runtime cannot reach.
const UNSAFE_PORTS = new Set([
  1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
  103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
  512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
  995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
  6697, 10080,
]);
const LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const TOP_LEVEL_LABEL = /^(?:[a-z]{2,63}|xn--[a-z0-9-]{1,59}[a-z0-9])$/;
// Local, private, and special-use names (RFC 6761, RFC 6762, RFC 8375, RFC 9476).
const RESERVED_TOP_LEVEL_LABELS = new Set([
  "localhost",
  "local",
  "internal",
  "arpa",
  "localdomain",
  "invalid",
  "onion",
  "alt",
]);

export function isEdgePort(value: number): boolean {
  return Number.isInteger(value) && value >= 1 && value <= 65535 && !UNSAFE_PORTS.has(value);
}

export function isPublicHostname(value: string): boolean {
  const labels = value.split(".");
  const topLevel = labels.at(-1) ?? "";
  return (
    value.length <= 253 &&
    labels.length >= 2 &&
    labels.every((label) => LABEL.test(label)) &&
    TOP_LEVEL_LABEL.test(topLevel) &&
    !RESERVED_TOP_LEVEL_LABELS.has(topLevel)
  );
}
