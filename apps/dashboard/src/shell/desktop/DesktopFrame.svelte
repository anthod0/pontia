<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { provideTopBarAction } from '../../components/layout/topBarAction'
  import DownloadCenter from '../../components/layout/DownloadCenter.svelte'
  import SettingsShell from '../../components/settings/SettingsShell.svelte'
  import { isSettingsPath } from '../../components/settings/sections'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { provideFilePreviewNavigator } from '../../components/chat/filePreviewContext'
  import { filePreviewWidth } from '../../components/chat/filePreviewWidth'
  import { useSessionPanel } from '../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import SessionSidePanel from '../../components/chat/sessionPanel/SessionSidePanel.svelte'
  import FileDropZone from '../../components/chat/FileDropZone.svelte'

  let { children, sidebar, topbar }: {
    children: Snippet
    sidebar: Snippet
    topbar: Snippet
  } = $props()
  let currentPath = $state(dashboardRelativePath())
  const topBarAction = provideTopBarAction()
  const sessionPanel = useSessionPanel()!
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
    return path === '/new' || path === '/' || path === '/bot' || path.startsWith('/chat/') || path.startsWith('/bots/')
  }

  const settingsPath = $derived(isSettingsPath(currentPath))
  const chatPath = $derived(isChatPath(currentPath))
  const mainClass = $derived(settingsPath ? 'min-w-0 flex-1 bg-surface' : chatPath ? 'min-w-0 flex-1 bg-surface px-4 md:px-8' : 'min-w-0 flex-1 bg-surface p-4 md:p-6')

  const contentClass = $derived(settingsPath || chatPath ? 'min-w-0 w-full' : 'mx-auto min-w-0 w-full max-w-7xl')
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider class="desktop-shell file-preview-layout" style={$filePreviewWidth === null ? undefined : `--file-preview-saved-width: ${$filePreviewWidth}px`}>
  {@render sidebar()}
  <Sidebar.Inset class="bg-surface lg:mr-[var(--file-preview-width)]">
    <FileDropZone>
      <!-- On chat pages only the top bar buttons take pointer input, so the feed beneath still scrolls. -->
      <div class={['sticky z-10 flex items-center', chatPath ? 'pointer-events-none' : 'bg-surface']} style="top: var(--visual-viewport-top, 0px)">
        <div class="min-w-0 flex-1">
          {@render topbar()}
        </div>
        {@render $topBarAction?.()}
        <div class="shrink-0 md:pr-4"><DownloadCenter /></div>
      </div>
      <main class={mainClass}>
        <div class={contentClass}>
          {#if settingsPath}
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
