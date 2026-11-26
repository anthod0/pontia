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

function isPublicIpv4(value: string): boolean {
  const parts = value.split(".");
  if (parts.length !== 4 || parts.some((part) => !/^(?:0|[1-9][0-9]{0,2})$/.test(part))) {
    return false;
  }
  const [a, b, c, d] = parts.map(Number);
  if ([a, b, c, d].some((part) => part > 255)) return false;
  return !(
    a === 0 ||
    a === 10 ||
    a === 127 ||
    a >= 224 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 0 && c === 0 && d !== 9 && d !== 10) ||
    (a === 192 && b === 0 && c === 2) ||
    (a === 192 && b === 88 && c === 99) ||
    (a === 192 && b === 168) ||
    (a === 198 && (b === 18 || b === 19)) ||
    (a === 198 && b === 51 && c === 100) ||
    (a === 203 && b === 0 && c === 113)
  );
}

function ipv6Segments(value: string): number[] | null {
  if (!/^[0-9a-f:]+$/.test(value) || value.split("::").length > 2) return null;
  const [head, tail] = value.includes("::") ? value.split("::") : [value, undefined];
  const left = head === "" ? [] : head.split(":");
  const right = tail === undefined || tail === "" ? [] : tail.split(":");
  const missing = 8 - left.length - right.length;
  if (tail === undefined ? missing !== 0 : missing < 1) return null;
  const groups = [...left, ...Array<string>(tail === undefined ? 0 : missing).fill("0"), ...right];
  if (groups.some((group) => !/^[0-9a-f]{1,4}$/.test(group))) return null;
  return groups.map((group) => Number.parseInt(group, 16));
}

function isPublicIpv6(value: string): boolean {
  const segments = ipv6Segments(value);
  if (!segments) return false;
  const [a, b] = segments;
  return (
    (a & 0xe000) === 0x2000 &&
    !(a === 0x2001 && b < 0x0200) &&
    !(a === 0x2001 && b === 0x0db8) &&
    a !== 0x2002 &&
    !(a === 0x3fff && b < 0x1000)
  );
}

export function isPublicEdgeHost(value: string): boolean {
  return isPublicHostname(value) || isPublicIpv4(value) || isPublicIpv6(value);
}
