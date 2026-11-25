<script lang="ts">
  import { t } from '$lib/i18n'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { cn } from '$lib/utils'
  import type { ClientStatusTone } from '../../features/settings/agentClients.svelte'
  import type { MessageKey } from '$lib/i18n'

  let { status, class: className }: { status: { tone: ClientStatusTone; label: MessageKey }; class?: string } =
    $props()
  const dots: Record<Exclude<ClientStatusTone, 'busy'>, string> = {
    error: 'bg-destructive',
    inactive: 'bg-inactive',
    warning: 'bg-warning',
    off: 'ring-[1.5px] ring-inactive ring-inset',
    success: 'bg-success',
  }
</script>

<span class={cn('flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground', className)}>
  {#if status.tone === 'busy'}<CircleNotchIcon class="size-3 shrink-0 animate-spin" />
  {:else}<span aria-hidden="true" class="size-2 shrink-0 rounded-full {dots[status.tone]}"></span>{/if}
  <span class="truncate">{$t(status.label)}</span>
</span>
