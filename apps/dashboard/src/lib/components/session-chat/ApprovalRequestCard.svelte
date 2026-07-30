<script lang="ts">
  import { ShieldAlert } from '@lucide/svelte'
  import type { ApprovalRequestView } from '$lib/approvals'

  export let approval: ApprovalRequestView
</script>

<aside
  aria-label={`Approval required for ${approval.toolName}`}
  class="mx-4 mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-4 text-sm"
>
  <div class="flex items-start gap-3">
    <ShieldAlert class="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />
    <div class="min-w-0 space-y-2">
      <div>
        <p class="font-medium">Approval required</p>
        <p class="text-muted-foreground">
          Claude is waiting to use <span class="font-mono text-foreground">{approval.toolName}</span>.
        </p>
      </div>
      {#if approval.permissionSuggestions.length}
        <div>
          <p class="mb-1 font-medium">Always allow options</p>
          <ul class="space-y-1">
            {#each approval.permissionSuggestions as suggestion}
              <li class="overflow-x-auto rounded border bg-background/70 px-2 py-1">
                <code class="whitespace-pre-wrap break-words text-xs">{JSON.stringify(suggestion)}</code>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      <p class="text-xs text-muted-foreground">Waiting for a decision in Claude or Pontia.</p>
    </div>
  </div>
</aside>
