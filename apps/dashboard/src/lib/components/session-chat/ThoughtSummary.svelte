<script lang="ts">
  import ThoughtSummaryCollapsed from './ThoughtSummaryCollapsed.svelte'
  import ThoughtSummarySheet from './ThoughtSummarySheet.svelte'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    class?: string
  }

  let { steps, active = false, class: className }: Props = $props()
  let sheetOpen = $state(false)
  const latestStep = $derived(steps.at(-1) ?? null)
</script>

<ThoughtSummaryCollapsed step={latestStep} stepCount={steps.length} {active} class={className} onOpen={() => (sheetOpen = true)} />
<ThoughtSummarySheet bind:open={sheetOpen} {steps} {active} />
