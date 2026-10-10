export function revisionSelection(raw: string | null, current: number): number | null {
  if (raw === null) return current;
  if (!/^[1-9]\d*$/.test(raw)) return null;
  const revision = Number(raw);
  return Number.isSafeInteger(revision) && revision <= current ? revision : null;
}

// Storage starts at 1 and resolve_patch increments in the same transaction only
// for applied graph changes. Rejected/blocked patches do not create revisions.
export function workflowRevisions(current: number): number[] {
  return Array.from({ length: current }, (_, index) => current - index);
}
