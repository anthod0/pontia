<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { highlightMarkdownCode } from '$lib/components/ai-elements/message/markdownHighlighter'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { cn } from '$lib/utils.js'

  interface Props {
    title: string
    command: string
    connected?: boolean
  }

  let { title, command, connected = false }: Props = $props()
  let open = $state(false)
  const highlightedCommand = $derived(highlightMarkdownCode(command, 'bash'))
</script>

<Collapsible.Root bind:open class="relative min-w-0">
  <Collapsible.Trigger
    class="group/command flex w-full min-w-0 gap-3 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={open ? `Hide ${title} details` : `Show ${title} details`}
  >
    <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
      <TerminalWindowIcon class="size-4 group-hover/command:hidden" aria-hidden="true" />
      <CaretRightIcon class={cn('hidden size-4 transition-transform group-hover/command:block', open && 'rotate-90')} aria-hidden="true" />
    </span>
    <span class="min-w-0 flex-1 pt-0.5 text-sm font-medium leading-5 text-foreground/75">{title}</span>
  </Collapsible.Trigger>

  {#if connected}
    <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
  {/if}

  <Collapsible.Content>
    <div class="mb-3 ml-9 max-w-[calc(100%_-_2.25rem)] overflow-x-auto">
      <div class="command-code min-w-full font-mono text-xs leading-relaxed">{@html highlightedCommand}</div>
    </div>
  </Collapsible.Content>
</Collapsible.Root>

<style>
  .command-code :global(pre.shiki) {
    width: 100%;
    margin: 0;
    padding: 0.5rem;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    background-color: transparent !important;
  }

  .command-code :global(pre.shiki code) {
    display: block;
    width: 100%;
    padding: 0;
    white-space: inherit;
    background: transparent;
  }
</style>
