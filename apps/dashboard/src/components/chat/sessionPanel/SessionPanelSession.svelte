<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { useTopBarAction } from '../../layout/topBarAction'
  import { useDeviceRuntime } from '../../../device/context'
  import { useSessionPanel } from './sessionPanelContext.svelte'

  /** Connects the session page to the side panel: shows its Session there and adds the top bar button. */
  let { sessionId, workspaceId }: { sessionId: string; workspaceId: string | null } = $props()

  const device = useDeviceRuntime()
  const panel = useSessionPanel()
  const action = useTopBarAction()
  const pressed = $derived(!!panel?.open && panel.source?.sessionId === sessionId)

  $effect(() => panel?.attach({ device, sessionId, workspaceId }))

  onMount(() => {
    action?.set(sidebarButton)
    return () => action?.set(null)
  })
</script>

{#snippet sidebarButton()}
  <Button variant="ghost" size="icon-circle" class={['pointer-events-auto mr-3 size-12 md:size-8 p-0 ring-1 ring-foreground/10', pressed ? 'bg-muted text-heading' : 'bg-popover']}
    aria-label={$t('nav.sidebar')} title={$t('nav.sidebar')} aria-pressed={pressed} onclick={() => panel?.toggle()}>
    {#if pressed}<CaretRightIcon class="size-5 md:size-4" />{:else}<CaretLeftIcon class="size-5 md:size-4" />{/if}
  </Button>
{/snippet}
