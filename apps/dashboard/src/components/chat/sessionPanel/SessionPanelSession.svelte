<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon'
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
  <div class="pointer-events-auto mr-3 flex h-12 shrink-0 items-center rounded-lg bg-popover px-0.5 ring-1 ring-foreground/10 max-md:mt-2 md:h-8 md:px-1">
    <Button variant="ghost" size="icon" class={['size-11 md:size-7', pressed && 'bg-muted text-heading']}
      aria-label={$t('nav.sidebar')} title={$t('nav.sidebar')} aria-pressed={pressed} onclick={() => panel?.toggle()}>
      <SidebarSimpleIcon class="size-5 -scale-x-100 md:size-4" />
    </Button>
  </div>
{/snippet}
