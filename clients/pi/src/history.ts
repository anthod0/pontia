import { RpcError } from "./rpc-error.js";

const PAGE_BYTES = 1024 * 1024;
export const MAX_HISTORY_ENTRIES = 100_000;
export interface HistoryContext {
  generation: number;
  sessionManager: {
    getSessionId(): string;
    getEntries(): unknown[];
    getLeafId(): string | null;
  };
}
interface Snapshot {
  session_id: string;
  generation: number;
  entry_count: number;
  upper_entry_id: string | null;
  leaf_id: string | null;
}
const invalid = (message: string): never => {
  throw new RpcError(-32602, message);
};
const token = (value: unknown): string => Buffer.from(JSON.stringify(value)).toString("base64url");
function decode(value: unknown): Record<string, unknown> {
  if (typeof value !== "string" || value.length > 4096)
    return invalid("Invalid history continuation");
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString());
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || token(parsed) !== value)
      return invalid("Invalid history continuation");
    return parsed;
  } catch {
    return invalid("Invalid history continuation");
  }
}
const identity = (value: unknown): value is string =>
  typeof value === "string" && !!value.trim() && value.length <= 512;

/** Reads the current canonical manager; tokens contain identities, never file positions. */
export function readHistory(context: HistoryContext, params: Record<string, unknown>): object {
  if (Object.keys(params).some((key) => !["page_size", "snapshot", "continuation"].includes(key)))
    return invalid("Unknown history parameter");
  const size = params.page_size === undefined ? 64 : params.page_size;
  if (!Number.isInteger(size) || (size as number) < 1 || (size as number) > 128)
    return invalid("Invalid history page size");
  const manager = context.sessionManager;
  const sessionId = manager.getSessionId();
  if (!identity(sessionId)) return invalid("Invalid native Session identity");
  const entries = manager.getEntries() as Record<string, unknown>[];
  if (!Array.isArray(entries) || entries.length > MAX_HISTORY_ENTRIES)
    return invalid("History snapshot exceeds entry limit");
  let snapshot: Snapshot;
  if (params.snapshot === undefined) {
    if (params.continuation !== undefined) return invalid("Continuation requires snapshot");
    snapshot = {
      session_id: sessionId,
      generation: context.generation,
      entry_count: entries.length,
      upper_entry_id: entries.length ? (entries[entries.length - 1].id as string) : null,
      leaf_id: manager.getLeafId(),
    };
  } else {
    const parsed = decode(params.snapshot);
    if (
      Object.keys(parsed).sort().join() !==
        "entry_count,generation,leaf_id,session_id,upper_entry_id" ||
      !Number.isInteger(parsed.entry_count) ||
      (parsed.entry_count as number) < 0 ||
      (parsed.entry_count as number) > MAX_HISTORY_ENTRIES ||
      parsed.session_id !== sessionId ||
      parsed.generation !== context.generation ||
      !(parsed.upper_entry_id === null || identity(parsed.upper_entry_id)) ||
      !(parsed.leaf_id === null || identity(parsed.leaf_id))
    )
      return invalid("History snapshot identity changed");
    snapshot = parsed as unknown as Snapshot;
  }
  const upper =
    snapshot.upper_entry_id === null
      ? -1
      : entries.findIndex((entry) => entry?.id === snapshot.upper_entry_id);
  if (
    snapshot.upper_entry_id !== null &&
    entries.filter((entry) => entry?.id === snapshot.upper_entry_id).length !== 1
  )
    return invalid("Duplicate history upper identity");
  if (snapshot.upper_entry_id !== null && upper < 0) return invalid("Invalid history upper bound");
  if (upper + 1 !== snapshot.entry_count) return invalid("History snapshot entry count changed");
  const ids = new Set<string>();
  for (let index = 0; index <= upper; index++) {
    const entry = entries[index];
    if (
      !entry ||
      !identity(entry.id) ||
      ids.has(entry.id) ||
      !identity(entry.type) ||
      !identity(entry.timestamp) ||
      !(entry.parentId === null || identity(entry.parentId)) ||
      (entry.parentId !== null && !ids.has(entry.parentId as string))
    )
      return invalid("Invalid history entry identity or parent");
    ids.add(entry.id);
  }
  if (snapshot.leaf_id !== null && !ids.has(snapshot.leaf_id))
    return invalid("History leaf is outside snapshot");
  const snapshotToken = token(snapshot);
  let start = 0;
  if (params.continuation !== undefined) {
    const continuation = decode(params.continuation);
    if (
      Object.keys(continuation).sort().join() !== "after,snapshot" ||
      continuation.snapshot !== snapshotToken ||
      !identity(continuation.after)
    )
      return invalid("Invalid history continuation");
    const after = entries.findIndex((entry) => entry?.id === continuation.after);
    if (after < 0 || after >= upper) return invalid("History continuation outside snapshot");
    start = after + 1;
  } else if (params.snapshot !== undefined) return invalid("Snapshot requires continuation");
  const page: unknown[] = [];
  let bytes = 0;
  for (let index = start; index <= upper && page.length < (size as number); index++) {
    const entry = entries[index];
    const encoded = JSON.stringify(entry);
    const length = Buffer.byteLength(encoded);
    if (length > PAGE_BYTES - 8192) return invalid("History entry exceeds response budget");
    if (bytes + length > PAGE_BYTES - 8192) break;
    page.push(entry);
    bytes += length + 1;
  }
  const end = start + page.length;
  const continuation =
    end <= upper ? token({ snapshot: snapshotToken, after: entries[end - 1].id }) : null;
  return {
    session_id: sessionId,
    snapshot: snapshotToken,
    entry_count: snapshot.entry_count,
    upper_entry_id: snapshot.upper_entry_id,
    leaf_id: snapshot.leaf_id,
    entries: page,
    continuation,
  };
}
