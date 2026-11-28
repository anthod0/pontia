<script lang="ts">
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { cn } from '$lib/utils.js'
  import { dashboardClient } from '../../clients'
  import pi from '../../assets/client-icons/pi.svg?raw'
  import codex from '../../assets/client-icons/codex.svg?raw'
  import claude from '../../assets/client-icons/claude.svg?raw'
  import grok from '../../assets/client-icons/grok.svg?raw'
  import kiro from '../../assets/client-icons/kiro.svg?raw'
  import antigravity from '../../assets/client-icons/antigravity.svg?raw'

  interface Props {
    clientType: string | undefined
    clientKey?: string | null
    class?: string
  }

  let { clientType, clientKey, class: className }: Props = $props()

  const icons = new Map(Object.entries({ pi, codex, claude, grok, kiro, antigravity }))

  // Inline SVG ids are document-global, so prefix them per instance to keep mask and filter references local.
  const idPrefix = $props.id()

  const iconKey = $derived(dashboardClient(clientType)?.iconKey(clientKey ?? undefined))
  const svg = $derived(iconKey && icons.get(iconKey))

  const icon = $derived(svg?.replace(/\bid="([^"]+)"/g, `id="${idPrefix}-$1"`).replace(/url\(#([^)]+)\)/g, `url(#${idPrefix}-$1)`))
</script>

<span class={cn('inline-flex size-4 shrink-0 [&>svg]:size-full [&>svg]:max-h-full [&>svg]:max-w-full', className)} aria-hidden="true">
  {#if icon}{@html icon}{:else}<TerminalWindowIcon />{/if}
</span>
