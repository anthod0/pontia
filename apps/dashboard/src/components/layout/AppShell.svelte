<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'
  import { provideTopBarAction } from './topBarAction'
  import DownloadCenter from './DownloadCenter.svelte'
  import ChatShortcuts from '../chat/ChatShortcuts.svelte'
  import ConnectionNotice from './ConnectionNotice.svelte'
  import SettingsShell from '../settings/SettingsShell.svelte'
  import { isSettingsPath } from '../settings/sections'
  import { provideSettingsDetail } from '../settings/settingsDetail'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { installVisualViewportCssVars } from '$lib/visualViewport'
  import { provideFilePreviewNavigator } from '../chat/filePreviewContext'
  import { filePreviewWidth } from '../chat/filePreviewWidth'
  import { provideSessionPanel } from '../chat/sessionPanel/sessionPanelContext.svelte'
  import SessionSidePanel from '../chat/sessionPanel/SessionSidePanel.svelte'
  import FileDropZone from '../chat/FileDropZone.svelte'

  let { children, sidebar, topbar, settingsEnabled = true }: {
    children: Snippet
    sidebar?: Snippet
    topbar?: Snippet
    settingsEnabled?: boolean
  } = $props()
  let currentPath = $state(dashboardRelativePath())
  const topBarAction = provideTopBarAction()
  provideSettingsDetail()
  const sessionPanel = provideSessionPanel()
  // Local file links open in the session side panel. The Bot chat page opens them in its own panel.
  provideFilePreviewNavigator({ open: target => sessionPanel.openFile(target) })
  // The session page registers its Session; other pages show no session side panel.
  const sessionSource = $derived(sessionPanel.open ? sessionPanel.source : null)

  function updatePath(): void {
    currentPath = dashboardRelativePath()
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/' || path === '/bot' || path.startsWith('/chat/') || path.startsWith('/bots/')
  }

  const settingsPath = $derived(isSettingsPath(currentPath))
  const chatPath = $derived(isChatPath(currentPath))
  const mainClass = $derived(settingsPath ? 'min-w-0 flex-1 bg-surface' : chatPath ? 'min-w-0 flex-1 bg-surface px-4 md:px-8' : 'min-w-0 flex-1 bg-surface p-4 md:p-6')

  let uninstallVisualViewportCssVars: (() => void) | null = null

  onMount(() => {
    uninstallVisualViewportCssVars = installVisualViewportCssVars()
  })

  onDestroy(() => {
    uninstallVisualViewportCssVars?.()
  })
  const contentClass = $derived(settingsPath || chatPath ? 'min-w-0 w-full' : 'mx-auto min-w-0 w-full max-w-7xl')
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider class="file-preview-layout" style={$filePreviewWidth === null ? undefined : `--file-preview-saved-width: ${$filePreviewWidth}px`}>
  {#if sidebar}
    {@render sidebar()}
  {:else}
    <ChatShortcuts />
    <ConnectionNotice />
    <AppSidebar />
  {/if}
  <Sidebar.Inset class="bg-surface lg:mr-[var(--file-preview-width)]">
    <FileDropZone>
      <!-- On chat pages only the top bar buttons take pointer input, so the feed beneath still scrolls. -->
      <div class={['sticky z-10 flex items-center', chatPath ? 'pointer-events-none' : 'bg-surface']} style="top: var(--visual-viewport-top, 0px)">
        <div class="min-w-0 flex-1">
          {#if topbar}
            {@render topbar()}
          {:else}
            <TopBar />
          {/if}
        </div>
        {@render $topBarAction?.()}
        <div class="shrink-0 md:pr-4"><DownloadCenter /></div>
      </div>
      <main class={mainClass}>
        <div class={contentClass}>
          {#if settingsPath && settingsEnabled}
            <SettingsShell>
              {@render children()}
            </SettingsShell>
          {:else}
            {@render children()}
          {/if}
        </div>
      </main>
    </FileDropZone>
  </Sidebar.Inset>
  {#if sessionSource}
    {#key sessionSource.device}
      <SessionSidePanel panel={sessionPanel} source={sessionSource} />
    {/key}
  {/if}
</Sidebar.Provider>

<style>
  :global(.file-preview-layout) {
    --file-preview-width: 0px;
  }
  @media (min-width: 1024px) {
    :global(.file-preview-layout:has([data-session-panel], [data-bot-chat-panel])) {
      /* Keep in sync with FILE_PREVIEW_MIN_WIDTH and FILE_PREVIEW_RESERVED_WIDTH. */
      --file-preview-width: clamp(
        320px,
        var(--file-preview-saved-width, clamp(360px, 40vw, 640px)),
        calc(100vw - 480px)
      );
    }
  }
</style>
