import type { WorkflowNodeView } from '../../api/types';

export interface WorkflowPhaseTab {
  ordinal: number;
  name: string;
  nodes: WorkflowNodeView[];
  submittedCount: number;
  current: boolean;
}

export function groupWorkflowPhases(nodes: WorkflowNodeView[], currentNodeId: string | null): WorkflowPhaseTab[] {
  const phases: WorkflowPhaseTab[] = [];
  for (const node of nodes) {
    let phase = phases.at(-1);
    if (!phase || phase.name !== node.phase) {
      phase = {
        ordinal: phases.length + 1,
        name: node.phase,
        nodes: [],
        submittedCount: 0,
        current: false,
      };
      phases.push(phase);
    }
    phase.nodes.push(node);
    if (node.submitted_at) phase.submittedCount += 1;
    if (node.node_id === currentNodeId) phase.current = true;
  }
  return phases;
}

export function selectedPhaseOrdinal(raw: string | null, phases: WorkflowPhaseTab[]): number | null {
  if (raw === null || !/^\d+$/.test(raw)) return null;
  const ordinal = Number(raw);
  return phases.some((phase) => phase.ordinal === ordinal) ? ordinal : null;
}
