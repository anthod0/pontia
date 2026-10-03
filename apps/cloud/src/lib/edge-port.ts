import unsafePorts from "../../../../config/edge-unsafe-ports.json";

// https://fetch.spec.whatwg.org/#port-blocking (includes Workers' prohibited TCP 25).
export function isEdgePort(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 1 &&
    value <= 65535 &&
    !unsafePorts.includes(value)
  );
}

export function edgeAuthority(hostname: string, port: number): string {
  return port === 443 ? hostname : `${hostname}:${port}`;
}
