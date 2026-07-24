import type { TurnView } from '../../api/types';

export type SessionTurnTreeRelation = 'root' | 'linked' | 'unknown' | 'orphan' | 'cycle';

export interface SessionTurnTreeRow {
  turn: TurnView;
  depth: number;
  relation: SessionTurnTreeRelation;
  isCurrent: boolean;
  isCurrentBranch: boolean;
  childCount: number;
}

export function buildSessionTurnTree(
  turns: TurnView[],
  currentTurnId: string | null,
): SessionTurnTreeRow[] {
  const turnsById = new Map(turns.map((turn) => [turn.turn_id, turn]));
  const candidateParents = new Map<string, string>();

  for (const turn of turns) {
    if (turn.topology_status !== 'linked' || !turn.parent_turn_id) continue;
    if (turnsById.has(turn.parent_turn_id)) candidateParents.set(turn.turn_id, turn.parent_turn_id);
  }

  const cycleTurnIds = findCycleTurnIds(turns, candidateParents);
  const treeParents = new Map(
    [...candidateParents].filter(([turnId]) => !cycleTurnIds.has(turnId)),
  );
  const childrenByParent = new Map<string, TurnView[]>();
  for (const turn of turns) {
    const parentId = treeParents.get(turn.turn_id);
    if (!parentId) continue;
    const siblings = childrenByParent.get(parentId) ?? [];
    siblings.push(turn);
    childrenByParent.set(parentId, siblings);
  }
  for (const children of childrenByParent.values()) children.sort(compareTurns);

  const currentBranchIds = ancestorIds(currentTurnId, treeParents, turnsById);
  const roots = turns
    .filter((turn) => !treeParents.has(turn.turn_id))
    .sort(compareTurns);
  const rows: SessionTurnTreeRow[] = [];
  const visited = new Set<string>();

  function append(turn: TurnView, depth: number): void {
    if (visited.has(turn.turn_id)) return;
    visited.add(turn.turn_id);
    const children = childrenByParent.get(turn.turn_id) ?? [];
    rows.push({
      turn,
      depth,
      relation: relationForTurn(turn, turnsById, cycleTurnIds),
      isCurrent: turn.turn_id === currentTurnId,
      isCurrentBranch: currentBranchIds.has(turn.turn_id),
      childCount: children.length,
    });
    for (const child of children) append(child, depth + 1);
  }

  for (const root of roots) append(root, 0);
  for (const turn of turns.slice().sort(compareTurns)) append(turn, 0);
  return rows;
}

function relationForTurn(
  turn: TurnView,
  turnsById: Map<string, TurnView>,
  cycleTurnIds: Set<string>,
): SessionTurnTreeRelation {
  if (cycleTurnIds.has(turn.turn_id)) return 'cycle';
  if (turn.topology_status === 'unknown') return 'unknown';
  if (turn.topology_status === 'root') return 'root';
  if (!turn.parent_turn_id || !turnsById.has(turn.parent_turn_id)) return 'orphan';
  return 'linked';
}

function findCycleTurnIds(turns: TurnView[], parents: Map<string, string>): Set<string> {
  const cycleTurnIds = new Set<string>();
  for (const turn of turns) {
    const path: string[] = [];
    const pathIndex = new Map<string, number>();
    let currentId: string | undefined = turn.turn_id;
    while (currentId) {
      const repeatedAt = pathIndex.get(currentId);
      if (repeatedAt !== undefined) {
        for (const cycleId of path.slice(repeatedAt)) cycleTurnIds.add(cycleId);
        break;
      }
      pathIndex.set(currentId, path.length);
      path.push(currentId);
      currentId = parents.get(currentId);
    }
  }
  return cycleTurnIds;
}

function ancestorIds(
  currentTurnId: string | null,
  parents: Map<string, string>,
  turnsById: Map<string, TurnView>,
): Set<string> {
  const ancestors = new Set<string>();
  let currentId = currentTurnId && turnsById.has(currentTurnId) ? currentTurnId : null;
  while (currentId && !ancestors.has(currentId)) {
    ancestors.add(currentId);
    currentId = parents.get(currentId) ?? null;
  }
  return ancestors;
}

function compareTurns(left: TurnView, right: TurnView): number {
  const createdComparison = left.created_at.localeCompare(right.created_at);
  return createdComparison || left.turn_id.localeCompare(right.turn_id);
}
