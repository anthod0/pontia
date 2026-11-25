export function hasControl(value: string): boolean {
  return Array.from(value).some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127);
}

export function maskSecret(value: string): string {
  const hidden = "*******";
  if (/[\r\n]/.test(value)) {
    const lines = value.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n");
    const pem = /^-----BEGIN ([A-Z0-9 ]{1,40})-----$/.exec(lines[0]);
    return pem
      ? `PEM(${pem[1]}, ${hidden}, ${lines.length} lines)`
      : `${hidden} (${lines.length} lines)`;
  }
  const points = Array.from(value);
  if (points.length <= 8) return hidden;
  const prefix = points.slice(0, points.length < 16 ? 2 : 4).join("");
  const suffix = points.slice(points.length < 16 ? -2 : -3).join("");
  return hasControl(prefix + suffix) ? hidden : prefix + hidden + suffix;
}

export function secretSizeValid(value: string): boolean {
  return value.length > 0 && new TextEncoder().encode(value).length <= 65536;
}
