<script lang="ts">
  import { ShieldAlert } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as ButtonGroup from '$lib/components/ui/button-group/index.js'
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
  class="mx-4 mb-3 rounded-lg border border-amber-500/40 bg-white p-4 text-sm text-zinc-950"
>
  <div class="flex items-start gap-3">
    <ShieldAlert class="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />
    <div class="min-w-0 space-y-2">
      <div>
        <p class="font-medium">Approval required</p>
        <p class="text-zinc-600">
          Claude is waiting to use <span class="font-mono text-zinc-950">{approval.toolName}</span>.
        </p>
      </div>
      {#if approval.permissionSuggestions.length}
        <div>
          <p class="mb-1 font-medium">Always allow options</p>
          <ul class="space-y-1">
            {#each approval.permissionSuggestions as suggestion}
              <li class="space-y-2 overflow-x-auto rounded border border-zinc-200 bg-zinc-50 px-2 py-2">
                <code class="block whitespace-pre-wrap break-words text-xs">{JSON.stringify(suggestion)}</code>
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  class="border-zinc-300 bg-white text-zinc-950 hover:bg-zinc-100 hover:text-zinc-950 dark:border-zinc-300 dark:bg-white dark:hover:bg-zinc-100"
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
      <ButtonGroup.Root class="flex flex-wrap" aria-label="Current request decisions">
        <Button
          type="button"
          size="sm"
          variant="outline"
          class="border-zinc-300 bg-white text-zinc-950 hover:bg-zinc-100 hover:text-zinc-950 dark:border-zinc-300 dark:bg-white dark:hover:bg-zinc-100"
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
      </ButtonGroup.Root>
      {#if error}
        <p role="alert" class="text-xs text-destructive">{error}</p>
      {/if}
    </div>
  </div>
</aside>
