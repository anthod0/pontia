<script lang="ts">
  import { CircleDot, Hammer, Sparkles } from '@lucide/svelte'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    open?: boolean
  }

  let { steps, active = false, open = $bindable(false) }: Props = $props()

  function labelForStep(step: SessionChatThoughtStep): string {
    if (step.kind === 'thinking') return 'Thinking'
    return step.title || (step.kind === 'tool_call' ? 'Tool call' : 'Tool result')
  }

  function iconForStep(step: SessionChatThoughtStep): typeof Sparkles {
    if (step.kind === 'thinking') return Sparkles
    if (step.kind === 'tool_call') return Hammer
    return CircleDot
  }
</script>

<Sheet.Root bind:open>
  <Sheet.Content class="w-[92vw] gap-0 overflow-hidden p-0 data-[side=right]:sm:max-w-4xl sm:max-w-4xl">
    <Sheet.Header class="border-b px-6 py-4">
      <div class="flex flex-wrap items-start justify-between gap-3 pr-10">
        <div>
          <Sheet.Title>Thought details</Sheet.Title>
          <Sheet.Description>{steps.length} step{steps.length === 1 ? '' : 's'}{active ? ' · working' : ''}</Sheet.Description>
        </div>
        {#if active}<Badge variant="secondary">working</Badge>{/if}
      </div>
    </Sheet.Header>

    <div class="min-h-0 flex-1 space-y-3 overflow-auto px-6 py-4">
      {#if steps.length}
        {#each steps as step (step.id)}
          {@const Icon = iconForStep(step)}
          <article class="rounded-xl border bg-background/70 p-3">
            <div class="mb-2 flex min-w-0 items-center gap-2">
              <span class="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
                <Icon class="size-4" />
              </span>
              <span class="min-w-0 flex-1 truncate text-base font-semibold text-foreground">{labelForStep(step)}</span>
              {#if step.status}<Badge variant="secondary" class="text-sm font-medium">{step.status}</Badge>{/if}
            </div>
            <pre class="max-h-[45vh] overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted/35 p-3 text-xs leading-5 text-foreground/90">{step.content}</pre>
          </article>
        {/each}
      {:else}
        <p class="text-sm text-muted-foreground">Working…</p>
      {/if}
    </div>
  </Sheet.Content>
</Sheet.Root>
