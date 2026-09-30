export const PUBLIC_DASHBOARD_ORIGIN = "https://app.pontia.dev";

const RELATIVE_BASE = "https://cloud.invalid";

export function normalizeLoginReturnTo(value: string, cloudOrigin = RELATIVE_BASE): string | null {
  let destination: URL;
  try {
    destination = new URL(value, cloudOrigin);
  } catch {
    return null;
  }
  if (destination.username || destination.password) return null;
  if (destination.origin === cloudOrigin && destination.pathname === "/device") {
    return `${destination.pathname}${destination.search}`;
  }
  if (destination.origin === PUBLIC_DASHBOARD_ORIGIN) {
    return `${destination.origin}${destination.pathname}${destination.search}${destination.hash}`;
  }
  return null;
}

export function isValidLoginReturnTo(value: unknown): value is string | undefined {
  return (
    value === undefined || (typeof value === "string" && normalizeLoginReturnTo(value) === value)
  );
}
