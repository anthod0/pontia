<script lang="ts">
  import { untrack, type Snippet } from 'svelte'
  import { page } from '$app/state'
  import { afterNavigate } from '$app/navigation'
  import { readable } from 'svelte/store'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { provideSessionPanel } from '../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { provideFilePreviewNavigator } from '../../components/chat/filePreviewContext'
  import SessionSidePanel from '../../components/chat/sessionPanel/SessionSidePanel.svelte'
  import { t } from '$lib/i18n'
  import { dashboardRelativePath, navigate } from '$lib/navigation'
  import { keyboardOpen } from '$lib/visualViewport'
  import { chromeMetrics } from '$lib/mobile/chromeMetrics'
  import { edgeSwipeBack } from '$lib/mobile/gestures'
  import { provideMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createBotDirectory } from '../../features/bots/botDirectory.svelte'
  import { provideMobileDirectory } from '../../features/bots/mobileDirectory'
  import { provideTopBarAction } from '../../components/layout/topBarAction'
  import { provideSettingsDetail } from '../../components/settings/settingsDetail'
  import { settingsSections } from '../../components/settings/sections'
  import type { DeviceRuntime } from '../../device/runtime'
  import { useDashboardHost } from '../../device/hostContext'
  import MobileConnectionBanner from './MobileConnectionBanner.svelte'
  import MobileDeviceSheet from './MobileDeviceSheet.svelte'
  import MobileHeader from './MobileHeader.svelte'
  import MobileOverlayHost from './MobileOverlayHost.svelte'
  import MobileTabBar from './MobileTabBar.svelte'
  import { tabs, parentTab } from './tabs'

  let { device, children }: { device?: DeviceRuntime; children: Snippet } = $props()
  const host = useDashboardHost()
  const directory = provideMobileDirectory(untrack(() => device ? createBotDirectory(device) : null))
  const stack = provideMobileStack()
  const sessionPanel = provideSessionPanel()
  provideFilePreviewNavigator({open: target => sessionPanel.openFile(target)})
  const sessionDetail = $derived(device?.sessions.sessionDetail ?? readable(null))
  const sessionTitle = $derived($sessionDetail && $sessionDetail.session.session_id === page.params.sessionId ? sessionChatTitle($sessionDetail.session,$t) : undefined)
  const action = provideTopBarAction()
  const detail = provideSettingsDetail()
  let scroller: HTMLElement
  let scrolled = $state(false)
  let switchOpen = $state(false)
  const path = $derived(dashboardRelativePath(page.url.pathname))
  const tab = $derived(tabs.find(tab => tab.path === path))
  const hasTabs = $derived(!!tab && !$keyboardOpen && !stack.layers.length)
  const section = $derived(settingsSections.find(section => section.path === path))
  const bot = $derived(directory?.bots?.find(bot => bot.bot_id === page.params.botId))
  const title = $derived($detail?.title ?? bot?.name ?? sessionTitle ?? $t(tab?.label ?? section?.label ?? (path === '/new' ? 'nav.newChat' : path.startsWith('/chat/') ? 'sessions.title' : 'mobile.chats')))
  afterNavigate(() => {
    scroller?.scrollTo({top: 0})
  })
  function back() {
    if (stack.layers.length) stack.back()
    else if (page.state.mobileBack) history.back()
    else void navigate(parentTab(path), undefined, {scope: host.routeScope, replaceState: true})
  }
  $effect(() => {
    const id = page.state.mobileLayer
    untrack(() => stack.reconcile(id))
  })
  const showingDetail = $derived(!!$detail)
  $effect(() => {
    if (!showingDetail) return
    const id = untrack(() => stack.open({dismiss: () => $detail?.back()}))
    return () => stack.release(id)
  })
</script>

<div class="mobile-shell" data-tabs={hasTabs} data-keyboard={$keyboardOpen}>
  <div inert={stack.layers.some(layer => layer.content)} class="flex h-full flex-col" use:edgeSwipeBack={!tab && path !== '/new' && !stack.layers.length ? back : undefined}>
    <div class="mobile-chrome" data-scrolled={scrolled} use:chromeMetrics={'--mobile-header-h'}>
      <MobileHeader {title} device={tab ? host.activeDevice : null} back={tab || path === '/' ? undefined : back}
        modal={path === '/new'} disabled={$detail?.disabled} backLabel={$detail && section ? $t(section.label) : undefined}
        switchDevice={tab && host.canSwitchDevice ? () => switchOpen = true : undefined} action={$action} />
      <MobileConnectionBanner />
    </div>
    <main bind:this={scroller} onscroll={() => scrolled = scroller.scrollTop > 0} class="mobile-body" class:mobile-root={!!tab}>{@render children()}</main>
    {#if hasTabs}
      <MobileTabBar {path} scope={host.routeScope} hasOpenTodos={directory?.hasOpenTodos ?? false}
        scrollTop={() => scroller.scrollTo({top: 0, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth'})} />
    {/if}
  </div>
  {#if sessionPanel.open && sessionPanel.source}
    <SessionSidePanel panel={sessionPanel} source={sessionPanel.source} />
  {/if}
  <MobileOverlayHost {stack} />
  {#if host.canSwitchDevice}<MobileDeviceSheet bind:open={switchOpen} />{/if}
</div>
