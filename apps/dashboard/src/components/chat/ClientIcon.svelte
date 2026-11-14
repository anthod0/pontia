<script lang="ts">
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { cn } from '$lib/utils.js'
  import pi from '../../assets/client-icons/pi.svg?raw'
  import codex from '../../assets/client-icons/codex.svg?raw'
  import claude from '../../assets/client-icons/claude.svg?raw'
  import grok from '../../assets/client-icons/grok.svg?raw'
  import opencode from '../../assets/client-icons/opencode.svg?raw'
  import opencodeDark from '../../assets/client-icons/opencode-dark.svg?raw'

  interface Props {
    clientType: string | undefined
    acpAgentId?: unknown
    class?: string
  }

  let { clientType, acpAgentId, class: className }: Props = $props()

  const clientIcons: Record<string, string> = { pi, codex, 'claude-agent': claude }
  const acpAgentIcons: Record<string, string> = { grok, opencode }

  const icon = $derived.by(() => {
    if (!clientType) return undefined
    if (clientType !== 'acp') return clientIcons[clientType]
    return typeof acpAgentId === 'string' ? acpAgentIcons[acpAgentId] : undefined
  })
  const darkIcon = $derived(clientType === 'acp' && acpAgentId === 'opencode' ? opencodeDark : undefined)
</script>

{#if icon}
  <span class={cn('inline-flex shrink-0 [&>svg]:size-full', darkIcon && 'dark:hidden', className)} aria-hidden="true">{@html icon}</span>
  {#if darkIcon}
    <span class={cn('hidden shrink-0 dark:inline-flex [&>svg]:size-full', className)} aria-hidden="true">{@html darkIcon}</span>
  {/if}
{:else}
  <TerminalWindowIcon class={className} aria-hidden="true" />
{/if}
