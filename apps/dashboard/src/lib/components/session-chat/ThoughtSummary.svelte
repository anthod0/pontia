<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import LightbulbIcon from 'phosphor-svelte/lib/LightbulbIcon'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { cn } from '$lib/utils.js'
  import ThoughtCommandStep from './ThoughtCommandStep.svelte'
  import ThoughtFallbackStep from './ThoughtFallbackStep.svelte'
  import ThoughtFileStep from './ThoughtFileStep.svelte'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    class?: string
  }

  type FileOperation = 'read' | 'write' | 'edit'

  interface GroupedFileSteps {
    id: string
    kind: 'file_group'
    operation: FileOperation
    files: string[]
  }

  type DisplayStep = SessionChatThoughtStep | GroupedFileSteps

  let { steps, active = false, class: className }: Props = $props()
  let open = $state(false)

  const visibleSteps = $derived(groupFileSteps(steps))

  function groupFileSteps(source: SessionChatThoughtStep[]): DisplayStep[] {
    const grouped: DisplayStep[] = []
    for (let index = 0; index < source.length;) {
      const step = source[index]!
      const operation = fileOperationFor(step)
      if (!operation) {
        grouped.push(step)
        index += 1
        continue
      }

      let end = index + 1
      while (end < source.length && fileOperationFor(source[end]!) === operation) end += 1
      const files = [...new Set(source.slice(index, end).map((item) => filePathFor(item)!))]
      grouped.push({
        id: `file-group:${step.id}:${source[end - 1]!.id}`,
        kind: 'file_group',
        operation,
        files,
      })
      index = end
    }
    return grouped
  }

  function fileOperationFor(step: SessionChatThoughtStep): FileOperation | null {
    if (step.kind !== 'tool_call') return null
    const type = step.managedToolUse?.input.type
    return type === 'read' || type === 'write' || type === 'edit' ? type : null
  }

  function filePathFor(step: SessionChatThoughtStep): string | null {
    const input = step.managedToolUse?.input
    return input?.type === 'read' || input?.type === 'write' || input?.type === 'edit' ? input.path : null
  }

  function hasConnectorAfter(index: number): boolean {
    return index < visibleSteps.length - 1 && visibleSteps[index + 1]?.kind !== 'assistant'
  }
</script>

{#if visibleSteps.length}
  <Collapsible.Root bind:open class={cn('not-prose min-w-0', className)}>
    <Collapsible.Trigger
      class="group inline-flex min-w-0 items-center gap-1.5 py-2.5 pr-3 text-sm leading-5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      aria-label={open ? 'Hide agent work steps' : 'Show agent work steps'}
    >
      <span class="truncate">{active ? 'Working' : 'Agent work'}</span>
      <CaretRightIcon class={cn('size-4 shrink-0 transition-transform', open && 'rotate-90')} aria-hidden="true" />
    </Collapsible.Trigger>

    <Collapsible.Content class="pb-2 pt-1">
      <div class="space-y-1">
        {#each visibleSteps as step, index (step.id)}
          {#if step.kind === 'assistant'}
            <div class="py-3">
              <Message.Response content={step.content} markdown streamId={step.id} />
            </div>
          {:else if step.kind === 'tool_call' && step.managedToolUse?.input.type === 'bash'}
            <ThoughtCommandStep title={step.title} command={step.content} connected={hasConnectorAfter(index)} />
          {:else if step.kind === 'file_group'}
            <ThoughtFileStep operation={step.operation} files={step.files} connected={hasConnectorAfter(index)} />
          {:else if step.kind === 'tool_call'}
            <ThoughtFallbackStep title={step.title} content={step.content} connected={hasConnectorAfter(index)} />
          {:else if step.kind === 'tool_result'}
            <ThoughtFallbackStep title={step.title} content={step.content} detailsLabel="result" error={step.status === 'error'} connected={hasConnectorAfter(index)} />
          {:else}
            <div class="relative flex min-w-0 gap-3 py-1.5">
              <span
                class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground"
                aria-label="Thinking"
              >
                <LightbulbIcon class="size-4" aria-hidden="true" />
              </span>
              {#if hasConnectorAfter(index)}
                <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
              {/if}
              <div class="min-w-0 flex-1 pt-0.5 text-sm leading-5">
                <span class="break-words font-medium text-foreground/75">{step.content}</span>
              </div>
            </div>
          {/if}
        {/each}
        {#if !visibleSteps.length}
          <p class="py-2 text-sm text-muted-foreground">Working…</p>
        {/if}
      </div>
    </Collapsible.Content>
  </Collapsible.Root>
{/if}
