<script lang="ts">
  import ThoughtSummaryCollapsed from './ThoughtSummaryCollapsed.svelte'
  import ThoughtSummaryIdle from './ThoughtSummaryIdle.svelte'
  import ThoughtSummarySheet from './ThoughtSummarySheet.svelte'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    workedDurationMs?: number
    showStepCountFallback?: boolean
    class?: string
  }

  let { steps, active = false, workedDurationMs, showStepCountFallback = false, class: className }: Props = $props()
  let sheetOpen = $state(false)
</script>

{#if active}
  <ThoughtSummaryCollapsed {steps} {active} class={className} onOpen={() => (sheetOpen = true)} />
{:else if workedDurationMs !== undefined}
  <ThoughtSummaryIdle durationMs={workedDurationMs} class={className} onOpen={() => (sheetOpen = true)} />
{:else if showStepCountFallback}
  <ThoughtSummaryIdle count={steps.length} class={className} onOpen={() => (sheetOpen = true)} />
{/if}
<ThoughtSummarySheet bind:open={sheetOpen} {steps} {active} />
