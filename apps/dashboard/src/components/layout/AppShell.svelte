<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'
  import DownloadCenter from './DownloadCenter.svelte'
  import ChatShortcuts from '../chat/ChatShortcuts.svelte'
  import ConnectionNotice from './ConnectionNotice.svelte'
  import SettingsShell from '../settings/SettingsShell.svelte'
  import { isSettingsPath } from '../settings/sections'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { installVisualViewportCssVars } from '$lib/visualViewport'
  import { provideFilePreview } from '../chat/filePreviewContext'
  import FilePreviewPanel from '../chat/FilePreviewPanel.svelte'
  import { filePreviewWidth } from '../chat/filePreviewWidth'

  let { children, sidebar, topbar, settingsEnabled = true }: {
    children: Snippet
    sidebar?: Snippet
    topbar?: Snippet
    settingsEnabled?: boolean
  } = $props()
  let currentPath = $state(dashboardRelativePath())
  let currentLocation = location.pathname
  const filePreview = provideFilePreview()

  function updatePath(): void {
    if (currentLocation !== location.pathname) filePreview.set(null)
    currentLocation = location.pathname
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

<Sidebar.Provider class="file-preview-layout" data-preview-open={$filePreview ? 'true' : 'false'} style={$filePreviewWidth === null ? undefined : `--file-preview-saved-width: ${$filePreviewWidth}px`}>
  {#if sidebar}
    {@render sidebar()}
  {:else}
    <ChatShortcuts />
    <ConnectionNotice />
    <AppSidebar />
  {/if}
  <Sidebar.Inset class="bg-surface lg:mr-[var(--file-preview-width)]">
    <!-- On chat pages only the top bar buttons take pointer input, so the feed beneath still scrolls. -->
    <div class={['sticky z-10 flex items-center', chatPath ? 'pointer-events-none' : 'bg-surface']} style="top: var(--visual-viewport-top, 0px)">
      <div class="min-w-0 flex-1">
        {#if topbar}
          {@render topbar()}
        {:else}
          <TopBar />
        {/if}
      </div>
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
  </Sidebar.Inset>
  {#if $filePreview}
    {#key $filePreview}
      <FilePreviewPanel target={$filePreview} onclose={() => filePreview.set(null)} />
    {/key}
  {/if}
</Sidebar.Provider>

<style>
  :global(.file-preview-layout) {
    --file-preview-width: 0px;
  }
  @media (min-width: 1024px) {
    :global(.file-preview-layout[data-preview-open='true']),
    :global(.file-preview-layout:has([data-bot-chat-panel])) {
      /* Keep in sync with FILE_PREVIEW_MIN_WIDTH and FILE_PREVIEW_RESERVED_WIDTH. */
      --file-preview-width: clamp(
        320px,
        var(--file-preview-saved-width, clamp(360px, 40vw, 640px)),
        calc(100vw - 480px)
      );
    }
  }
</style>
