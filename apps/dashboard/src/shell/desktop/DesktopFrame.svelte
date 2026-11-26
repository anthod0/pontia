<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { provideTopBarAction } from '../../components/layout/topBarAction'
  import DownloadCenter from '../../components/layout/DownloadCenter.svelte'
  import SettingsShell from '../../components/settings/SettingsShell.svelte'
  import { isSettingsPath } from '../../components/settings/sections'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { provideFilePreviewNavigator } from '../../components/chat/filePreviewContext'
  import { CHAT_MIN_WIDTH, FILE_PREVIEW_MIN_WIDTH, filePreviewWidth } from '../../components/chat/filePreviewWidth'
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
  let contentInset = $state<HTMLElement | null>(null)

  $effect(() => {
    if (!contentInset) return
    const inset = contentInset
    const root = document.documentElement
    const update = () => {
      const bounds = inset.getBoundingClientRect()
      root.style.setProperty('--desktop-content-left', `${bounds.left}px`)
      root.style.setProperty('--desktop-content-right', `${window.innerWidth - bounds.right}px`)
    }
    update()
    const observer = new ResizeObserver(update)
    observer.observe(inset)
    window.addEventListener('resize', update)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', update)
      root.style.removeProperty('--desktop-content-left')
      root.style.removeProperty('--desktop-content-right')
    }
  })
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider class="desktop-shell">
  {@render sidebar()}
  <div class="file-preview-layout flex min-w-0 flex-1"
    style:--chat-min-width={`${CHAT_MIN_WIDTH}px`}
    style:--file-preview-min-width={`${FILE_PREVIEW_MIN_WIDTH}px`}
    style:--file-preview-saved-width={$filePreviewWidth === null ? undefined : `${$filePreviewWidth}px`}>
    <Sidebar.Inset bind:ref={contentInset} class="bg-surface lg:mr-[var(--file-preview-width)]">
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
    {#if sessionPanel.source}
      {#key sessionPanel.source.device}
        <SessionSidePanel panel={sessionPanel} source={sessionPanel.source} />
      {/key}
    {/if}
  </div>
</Sidebar.Provider>

<style>
  @property --file-preview-open {
    syntax: '<number>';
    inherits: true;
    initial-value: 0;
  }

  @media (min-width: 1024px) {
    :global(.file-preview-layout) {
      container-type: inline-size;
      --file-preview-target-width: min(
        calc(100cqw - var(--chat-min-width)),
        max(var(--file-preview-min-width), var(--file-preview-saved-width, clamp(360px, 40vw, 640px)))
      );
      --file-preview-width: calc(var(--file-preview-open) * var(--file-preview-target-width));
    }
    :global(.file-preview-layout:has([data-docked-panel-open])) {
      --file-preview-open: 1;
    }
    :global(.file-preview-layout:has([data-docked-panel-fullscreen]) > [data-slot="sidebar-inset"]) {
      visibility: hidden;
    }
    /* The Bot panel is mounted inside the chat inset. */
    :global(.file-preview-layout [data-docked-panel-fullscreen]) {
      visibility: visible;
      --file-preview-target-width: 100cqw;
      width: var(--file-preview-target-width);
    }
  }
  @media (min-width: 1024px) and (prefers-reduced-motion: no-preference) {
    :global(.file-preview-layout) {
      transition: --file-preview-open 200ms ease-out;
    }
  }
</style>
