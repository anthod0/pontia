// Every Dashboard read and write of localStorage and sessionStorage goes through this module.
//
// Key names: new keys use `pontia.<camelCaseName>`, the form most existing keys use.
// A key kept per device or per Bot appends its scope with `scopedKey`.
// Existing keys keep their names and value formats so saved data stays readable.

type Options = {
  area?: "local" | "session";
  /** Throws when storage is unavailable or full, for data whose loss would lose user work. */
  required?: boolean;
};

function storage(area: Options["area"] = "local"): Storage {
  return area === "local" ? window.localStorage : window.sessionStorage;
}

/** `name:scope` for one scope part, `name:["a","b"]` for several. */
export function scopedKey(name: string, ...scope: [string, ...string[]]): string {
  return `${name}:${scope.length === 1 ? scope[0] : JSON.stringify(scope)}`;
}

/** The saved string, or null when the key is missing or storage is unavailable. */
export function readStored(key: string, { area, required = false }: Options = {}): string | null {
  try {
    return storage(area).getItem(key);
  } catch (error) {
    if (required) throw error;
    return null;
  }
}

/** The localStorage value read by `parse`, or null when it is missing, unreadable or unavailable. */
export function readStoredWith<T>(key: string, parse: (saved: string) => T | undefined): T | null {
  const saved = readStored(key);
  if (saved === null) return null;
  try {
    return parse(saved) ?? null;
  } catch {
    return null;
  }
}

/** Saves `value`, or removes the key when it is null. Unavailable or full storage is ignored. */
export function writeStored(
  key: string,
  value: string | null,
  { area, required = false }: Options = {},
): void {
  try {
    if (value === null) storage(area).removeItem(key);
    else storage(area).setItem(key, value);
  } catch (error) {
    if (required) throw error;
  }
}
