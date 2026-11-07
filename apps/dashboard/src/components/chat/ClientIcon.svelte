<script lang="ts">
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import type { SessionCreationOption } from '../../api/types'
  import { cn } from '$lib/utils.js'
  import pi from '../../assets/client-icons/pi.svg?raw'
  import codex from '../../assets/client-icons/codex.svg?raw'
  import claude from '../../assets/client-icons/claude.svg?raw'
  import grok from '../../assets/client-icons/grok.svg?raw'
  import kimi from '../../assets/client-icons/kimi.svg?raw'
  import opencode from '../../assets/client-icons/opencode.svg?raw'

  interface Props {
    option: SessionCreationOption | undefined
    class?: string
  }

  let { option, class: className }: Props = $props()

  const clientIcons: Record<string, string> = { pi, codex, 'claude-agent': claude }
  const acpAgentIcons: Record<string, string> = { grok, kimi, opencode }

  const icon = $derived.by(() => {
    if (!option) return undefined
    if (option.client_type !== 'acp') return clientIcons[option.client_type]
    const agentId = option.metadata.acp_agent_id
    return typeof agentId === 'string' ? acpAgentIcons[agentId] : undefined
  })
</script>

{#if icon}
  <span class={cn('inline-flex shrink-0 [&>svg]:size-full', className)} aria-hidden="true">{@html icon}</span>
{:else}
  <TerminalWindowIcon class={className} aria-hidden="true" />
{/if}
