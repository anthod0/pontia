<script lang="ts">
  import { Background, BackgroundVariant, Controls, MiniMap, SvelteFlow, type NodeTypes } from '@xyflow/svelte'
  import type { JsonObject } from '../../api/types'
  import WorkItemNode from './WorkItemNode.svelte'
  import { buildDraftDagFlow, type WorkItemFlowEdge, type WorkItemFlowNode } from './dagGraph'

  let { workItems, edges }: { workItems: JsonObject[]; edges: JsonObject[] } = $props()

  const nodeTypes: NodeTypes = { workItem: WorkItemNode }

  let nodes = $state.raw<WorkItemFlowNode[]>([])
  let flowEdges = $state.raw<WorkItemFlowEdge[]>([])
  $effect(() => {
    const flow = buildDraftDagFlow({ workItems, edges })
    nodes = flow.nodes
    flowEdges = flow.edges
  })
</script>

<div class="h-[34rem] overflow-hidden rounded-lg border bg-background">
  <SvelteFlow
    bind:nodes
    bind:edges={flowEdges}
    {nodeTypes}
    fitView
    nodesDraggable={false}
    nodesConnectable={false}
    elementsSelectable
    minZoom={0.2}
    maxZoom={1.5}
    proOptions={{ hideAttribution: true }}
  >
    <Background variant={BackgroundVariant.Dots} />
    <Controls />
    <MiniMap pannable zoomable />
  </SvelteFlow>
</div>
