<script lang="ts">
  import { ShieldAlert } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import type { ApprovalDecisionInput, ApprovalRequestView } from '$lib/approvals'

  export let approval: ApprovalRequestView
  export let onDecision: (decision: ApprovalDecisionInput) => Promise<void>

  let submitting = false
  let delivered = false
  let error: string | null = null

  async function submit(decision: ApprovalDecisionInput): Promise<void> {
    if (submitting || delivered) return
    submitting = true
    error = null
    try {
      await onDecision(decision)
      delivered = true
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Could not deliver the approval decision.'
    } finally {
      submitting = false
    }
  }
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
              <li class="space-y-2 overflow-x-auto rounded border bg-background/70 px-2 py-2">
                <code class="block whitespace-pre-wrap break-words text-xs">{JSON.stringify(suggestion)}</code>
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  disabled={submitting || delivered}
                  onclick={() => void submit({ decision: 'always_allow', permission_suggestion: suggestion })}
                >
                  Always Allow
                </Button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      <div class="flex flex-wrap gap-2">
        <Button
          type="button"
          size="sm"
          disabled={submitting || delivered}
          onclick={() => void submit({ decision: 'accept_once' })}
        >
          Accept Once
        </Button>
        <Button
          type="button"
          size="sm"
          variant="destructive"
          disabled={submitting || delivered}
          onclick={() => void submit({ decision: 'reject' })}
        >
          Reject
        </Button>
      </div>
      {#if error}
        <p role="alert" class="text-xs text-destructive">{error}</p>
      {:else if delivered}
        <p class="text-xs text-muted-foreground">Decision delivered. Waiting for Claude to confirm the final result.</p>
      {:else}
        <p class="text-xs text-muted-foreground">Waiting for a decision in Claude or Pontia.</p>
      {/if}
    </div>
  </div>
</aside>
