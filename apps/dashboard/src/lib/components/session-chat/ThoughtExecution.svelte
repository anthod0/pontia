<script lang="ts">
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  let { step }: { step: SessionChatThoughtStep } = $props()
  const labels: Record<NonNullable<SessionChatThoughtStep['executionStatus']>, string> = {
    generated: 'Call generated', running: 'Running', ended: 'Ended · outcome unknown',
    succeeded: 'Succeeded', failed: 'Failed', rejected: 'Rejected', interrupted: 'Interrupted',
  }
</script>

<div class="mb-2 ml-9 min-w-0 space-y-2 text-sm">
  <span class="text-muted-foreground" role="status">{labels[step.executionStatus ?? 'generated']}</span>
  {#if step.processOutput && (!step.result || step.processOutput.text !== step.result.content)}
    <Collapsible.Root open={!step.result}>
      <Collapsible.Trigger class="text-muted-foreground">Output{step.processOutput.truncated ? ' · partial / truncated' : ''}</Collapsible.Trigger>
      <Collapsible.Content>
        <pre class="max-h-96 overflow-auto whitespace-pre-wrap break-words font-mono">{step.processOutput.text}</pre>
      </Collapsible.Content>
    </Collapsible.Root>
  {/if}
  {#if step.result}
    <Collapsible.Root open>
      <Collapsible.Trigger class="text-muted-foreground">{step.result.status === 'error' ? 'Error / result' : 'Result'}{step.result.truncated ? ' · truncated' : ''}</Collapsible.Trigger>
      <Collapsible.Content>
        <pre class="max-h-96 overflow-auto whitespace-pre-wrap break-words font-mono" class:text-destructive={step.result.status === 'error'}>{step.result.content}</pre>
      </Collapsible.Content>
    </Collapsible.Root>
  {/if}
</div>
