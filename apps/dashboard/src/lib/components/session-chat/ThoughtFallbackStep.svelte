<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import WrenchIcon from 'phosphor-svelte/lib/WrenchIcon'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { cn } from '$lib/utils.js'

  interface Props {
    title: string
    content: string
    detailsLabel?: 'parameters' | 'result'
    error?: boolean
    connected?: boolean
  }

  let { title, content, detailsLabel = 'parameters', error = false, connected = false }: Props = $props()
  let open = $state(false)
  const parameters = $derived(formatParameters(title, content))

  function formatParameters(toolName: string, preview: string): string {
    const value = preview.startsWith(`${toolName} `) ? preview.slice(toolName.length + 1) : preview
    try {
      return JSON.stringify(JSON.parse(value), null, 2)
    } catch {
      return value
    }
  }
</script>

<Collapsible.Root bind:open class="relative min-w-0">
  <Collapsible.Trigger
    class="group/fallback-step flex w-full min-w-0 gap-3 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={open ? `Hide ${title} ${detailsLabel}` : `Show ${title} ${detailsLabel}`}
  >
    <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
      <WrenchIcon class="size-4 group-hover/fallback-step:hidden" aria-hidden="true" />
      <CaretRightIcon class={cn('hidden size-4 transition-transform group-hover/fallback-step:block', open && 'rotate-90')} aria-hidden="true" />
    </span>
    <span class={cn("min-w-0 flex-1 break-words pt-0.5 text-sm font-medium leading-5", error ? "text-destructive" : "text-foreground/75")}>{title}{detailsLabel === 'result' ? ' · Result' : ''}</span>
  </Collapsible.Trigger>

  {#if connected}
    <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
  {/if}

  <Collapsible.Content>
    <pre class="mb-3 ml-9 max-w-[calc(100%_-_2.25rem)] overflow-x-auto whitespace-pre-wrap [overflow-wrap:anywhere] px-2 py-1 font-mono text-sm leading-5 text-muted-foreground">{parameters}</pre>
  </Collapsible.Content>
</Collapsible.Root>
