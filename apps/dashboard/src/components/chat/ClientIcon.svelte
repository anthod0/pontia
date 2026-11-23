<script lang="ts">
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { cn } from '$lib/utils.js'
  import pi from '../../assets/client-icons/pi.svg?raw'
  import codex from '../../assets/client-icons/codex.svg?raw'
  import claude from '../../assets/client-icons/claude.svg?raw'
  import grok from '../../assets/client-icons/grok.svg?raw'
  import kiro from '../../assets/client-icons/kiro.svg?raw'

  interface Props {
    clientType: string | undefined
    acpAgentId?: unknown
    class?: string
  }

  let { clientType, acpAgentId, class: className }: Props = $props()

  const clientIcons: Record<string, string> = { pi, codex, 'claude-agent': claude }

  const acpIcons = new Map([['grok', grok], ['kiro', kiro]])

  const icon = $derived.by(() => {
    if (!clientType) return undefined
    if (clientType !== 'acp') return clientIcons[clientType]
    return typeof acpAgentId === 'string' ? acpIcons.get(acpAgentId) : undefined
  })
</script>

{#if icon}
  <span class={cn('inline-flex shrink-0 [&>svg]:size-full', className)} aria-hidden="true">{@html icon}</span>
{:else}
  <TerminalWindowIcon class={className} aria-hidden="true" />
{/if}
