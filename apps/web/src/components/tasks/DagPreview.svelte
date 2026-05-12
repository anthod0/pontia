<script lang="ts">
  import type { TaskDagView, WorkItemRunView, WorkItemView } from '../../api/types';

  export let dag: TaskDagView;

  interface DagNode {
    item: WorkItemView;
    run: WorkItemRunView | undefined;
    layer: number;
    directDependencies: WorkItemView[];
    directDependents: WorkItemView[];
  }

  function latestRunFor(dagView: TaskDagView, workItemId: string): WorkItemRunView | undefined {
    return dagView.runs.filter((run) => run.work_item_id === workItemId).at(-1);
  }

  function buildNodes(dagView: TaskDagView): DagNode[] {
    const byId = new Map(dagView.work_items.map((item) => [item.work_item_id, item]));
    const incoming = new Map<string, Set<string>>();
    const outgoing = new Map<string, Set<string>>();

    for (const item of dagView.work_items) {
      incoming.set(item.work_item_id, new Set());
      outgoing.set(item.work_item_id, new Set());
    }

    for (const edge of dagView.edges) {
      if (!byId.has(edge.from_work_item_id) || !byId.has(edge.to_work_item_id)) continue;
      incoming.get(edge.to_work_item_id)?.add(edge.from_work_item_id);
      outgoing.get(edge.from_work_item_id)?.add(edge.to_work_item_id);
    }

    const remainingDependencies = new Map(
      [...incoming.entries()].map(([id, ids]) => [id, new Set(ids)]),
    );
    const layerById = new Map<string, number>();
    const queue = dagView.work_items
      .filter((item) => remainingDependencies.get(item.work_item_id)?.size === 0)
      .map((item) => item.work_item_id);

    for (const id of queue) layerById.set(id, 0);

    while (queue.length) {
      const id = queue.shift()!;
      const nextLayer = (layerById.get(id) ?? 0) + 1;
      for (const dependentId of outgoing.get(id) ?? []) {
        const remaining = remainingDependencies.get(dependentId);
        remaining?.delete(id);
        if (remaining?.size === 0) {
          layerById.set(dependentId, Math.max(layerById.get(dependentId) ?? 0, nextLayer));
          queue.push(dependentId);
        } else if (remaining) {
          layerById.set(dependentId, Math.max(layerById.get(dependentId) ?? 0, nextLayer));
        }
      }
    }

    return dagView.work_items
      .map((item, index) => ({
        item,
        run: latestRunFor(dagView, item.work_item_id),
        layer: layerById.get(item.work_item_id) ?? 0,
        directDependencies: [...(incoming.get(item.work_item_id) ?? [])]
          .map((id) => byId.get(id))
          .filter((dependency): dependency is WorkItemView => Boolean(dependency)),
        directDependents: [...(outgoing.get(item.work_item_id) ?? [])]
          .map((id) => byId.get(id))
          .filter((dependent): dependent is WorkItemView => Boolean(dependent)),
        index,
      }))
      .sort((left, right) => left.layer - right.layer || left.index - right.index);
  }

  function groupByLayer(nodes: DagNode[]): DagNode[][] {
    const groups: DagNode[][] = [];
    for (const node of nodes) {
      groups[node.layer] ??= [];
      groups[node.layer].push(node);
    }
    return groups.filter(Boolean);
  }

  function statusClass(state: string | undefined): string {
    if (state === 'completed') return 'is-completed';
    if (state === 'running') return 'is-running';
    if (state === 'failed') return 'is-failed';
    if (state === 'blocked') return 'is-blocked';
    if (state === 'ready') return 'is-ready';
    return 'is-pending';
  }

  $: nodes = buildNodes(dag);
  $: layers = groupByLayer(nodes);
</script>

<div class="metadata-grid compact-grid">
  <span>WorkItems</span><strong>{dag.summary.total_work_items}</strong>
  <span>Ready</span><strong>{dag.summary.ready_work_items}</strong>
  <span>Running</span><strong>{dag.summary.running_work_items}</strong>
  <span>Completed</span><strong>{dag.summary.completed_work_items}</strong>
  <span>Blocked</span><strong>{dag.summary.blocked_work_items}</strong>
  <span>Failed</span><strong>{dag.summary.failed_work_items}</strong>
  <span>Runs</span><strong>{dag.summary.total_runs}</strong>
  <span>Open signals</span><strong>{dag.summary.open_signals}</strong>
</div>

<div class="dag-preview" aria-label="DAG preview">
  {#each layers as layer, layerIndex}
    <section class="dag-layer" aria-label={`DAG layer ${layerIndex + 1}`}>
      <div class="dag-layer-label">Stage {layerIndex + 1}</div>
      <div class="dag-layer-items">
        {#each layer as node (node.item.work_item_id)}
          <article class="dag-node {statusClass(node.item.runtime?.current_state)}">
            <div class="row">
              <strong>{node.item.title}</strong>
              <span class="badge">{node.item.runtime?.current_state ?? 'unknown'}</span>
            </div>
            <p class="muted">{node.item.kind} · {node.item.execution_profile_id}{#if node.item.execution_profile_version}@{node.item.execution_profile_version}{/if}</p>
            {#if node.item.description}<p>{node.item.description}</p>{/if}
            <div class="dag-node-links">
              <span>Depends on</span>
              <strong>{node.directDependencies.length ? node.directDependencies.map((item) => item.title).join(', ') : 'none'}</strong>
              <span>Next</span>
              <strong>{node.directDependents.length ? node.directDependents.map((item) => item.title).join(', ') : 'none'}</strong>
              {#if node.run}
                <span>Latest run</span>
                <strong>{node.run.run_id} ({node.run.state})</strong>
              {/if}
            </div>
          </article>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .dag-preview { display: grid; gap: .75rem; margin-top: .75rem; overflow-x: auto; padding-bottom: .25rem; }
  .dag-layer { display: grid; grid-template-columns: 5.5rem minmax(0, 1fr); gap: .75rem; align-items: stretch; min-width: 32rem; }
  .dag-layer-label { display: grid; place-items: center; border: 1px dashed var(--border); border-radius: .65rem; color: var(--muted); font-size: .8rem; font-weight: 800; text-transform: uppercase; }
  .dag-layer-items { display: grid; grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr)); gap: .75rem; }
  .dag-node { border: 1px solid var(--border); border-left-width: .35rem; border-radius: .65rem; padding: .75rem; background: rgb(148 163 184 / .08); }
  .dag-node p { margin: .45rem 0 0; }
  .dag-node.is-completed { border-left-color: #16a34a; }
  .dag-node.is-running { border-left-color: var(--accent); }
  .dag-node.is-failed { border-left-color: var(--danger); }
  .dag-node.is-blocked { border-left-color: #f97316; }
  .dag-node.is-ready { border-left-color: #0ea5e9; }
  .dag-node.is-pending { border-left-color: var(--muted); }
  .badge { border-radius: 999px; padding: .15rem .45rem; background: rgb(79 70 229 / .14); color: var(--accent); font-size: .75rem; font-weight: 800; }
  .dag-node-links { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: .25rem .5rem; margin-top: .6rem; font-size: .82rem; }
  .dag-node-links span { color: var(--muted); }
  .dag-node-links strong { overflow-wrap: anywhere; }
</style>
