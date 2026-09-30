const deviceHandlePattern = /^[a-z][a-z0-9_-]{3,47}$/;

export const reservedDeviceHandles: ReadonlySet<string> = new Set([
  "about",
  "account",
  "accounts",
  "agent-profiles",
  "admin",
  "api",
  "assets",
  "auth",
  "authorization",
  "authorize",
  "billing",
  "callback",
  "callbacks",
  "chat",
  "connect",
  "contact",
  "dashboard",
  "device",
  "devices",
  "docs",
  "download",
  "downloads",
  "edge",
  "edges",
  "health",
  "healthz",
  "help",
  "install",
  "invite",
  "invites",
  "legal",
  "login",
  "logout",
  "oauth",
  "pricing",
  "privacy",
  "profile",
  "profiles",
  "register",
  "security",
  "session",
  "sessions",
  "settings",
  "signin",
  "signout",
  "signup",
  "static",
  "status",
  "support",
  "system",
  "terms",
  "user",
  "users",
  "verify",
  "webhook",
  "webhooks",
  "workflow",
  "workflows",
  "workspace",
  "workspaces",
]);

export function isValidDeviceHandle(value: string): boolean {
  return deviceHandlePattern.test(value) && !reservedDeviceHandles.has(value);
}

function normalizedBase(name: string): string {
  const normalized = name
    .normalize("NFKD")
    .replace(/\p{Mark}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^[-_]+|[-_]+$/g, "")
    .slice(0, 48);
  return deviceHandlePattern.test(normalized) ? normalized : "device";
}

export function deviceHandleCandidates(name: string, deviceId: string): string[] {
  const base = normalizedBase(name);
  const uuid = deviceId.replaceAll("-", "");
  const candidates = isValidDeviceHandle(base) ? [base] : [];
  for (let suffixLength = 8; suffixLength <= uuid.length; suffixLength += 4) {
    const suffix = uuid.slice(0, suffixLength);
    const prefix = base.slice(0, 48 - suffix.length - 1).replace(/[-_]+$/g, "");
    candidates.push(`${prefix}-${suffix}`);
  }
  return candidates;
}
