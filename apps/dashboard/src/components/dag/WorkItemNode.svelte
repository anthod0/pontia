<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { WorkItemFlowNode } from './dagGraph'

  let { data, selected }: NodeProps<WorkItemFlowNode> = $props()

  const stateClasses: Record<string, string> = {
    completed: 'border-emerald-500/70 bg-emerald-500/10',
    running: 'border-sky-500/70 bg-sky-500/10',
    ready: 'border-amber-500/70 bg-amber-500/10',
    blocked: 'border-orange-500/70 bg-orange-500/10',
    failed: 'border-destructive/70 bg-destructive/10',
  }

  let stateClass = $derived(stateClasses[data.state] ?? 'border-border bg-card')
</script>

<div class={`h-[118px] w-[260px] overflow-hidden rounded-xl border p-3 text-card-foreground shadow-sm ${stateClass} ${selected ? 'ring-2 ring-ring' : ''}`}>
  <Handle type="target" position={Position.Left} class="!size-2 !border-border !bg-background" />
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="truncate text-sm font-semibold" title={data.label}>{data.label}</div>
      <div class="mt-1 flex min-w-0 flex-wrap gap-1.5">
        <Badge variant="secondary" class="max-w-[10rem] truncate" title={data.kind}>{data.kind}</Badge>
        <Badge variant="outline" class="max-w-[8rem] truncate" title={data.state}>{data.state}</Badge>
      </div>
    </div>
    <span class="shrink-0 rounded-md bg-background/70 px-1.5 py-0.5 text-xs text-muted-foreground">P{data.priority}</span>
  </div>
  <p class="mt-2 line-clamp-2 break-words text-xs text-muted-foreground" title={data.description}>{data.description || 'No description'}</p>
  <Handle type="source" position={Position.Right} class="!size-2 !border-border !bg-background" />
</div>
