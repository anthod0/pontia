<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { onDestroy, untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { Button } from '$lib/components/ui/button'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import FolderStarIcon from 'phosphor-svelte/lib/FolderStarIcon'
  import EyeIcon from 'phosphor-svelte/lib/EyeIcon'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import RenameWorkspaceDialog from '../../../components/workspaces/RenameWorkspaceDialog.svelte'
  import { createWorkspaceBrowser } from '../../../features/settings/workspaceBrowser.svelte'
  import type { WorkspaceView } from '../../../api/types'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'

  let { openFirstAvailableRoot = false }: { openFirstAvailableRoot?: boolean } = $props()
  const browser = createWorkspaceBrowser('active', untrack(() => openFirstAvailableRoot))
  const { action } = useMobileHeader()
  const stack = useMobileStack()!
  let revealed = $state<string | null>(null)
  let target = $state<WorkspaceView | null>(null)
  let menuOpen = $state(false)
  let timer: ReturnType<typeof setTimeout>
  let start: { x: number; y: number } | null = null
  let suppressClick = false
  function cancelGesture() {
    clearTimeout(timer)
    start = null
  }
  onDestroy(cancelGesture)
  function menu(workspace: WorkspaceView) {
    target = workspace
    menuOpen = true
  }
  function down(event: PointerEvent, workspace: WorkspaceView) {
    cancelGesture()
    suppressClick = false
    start = { x: event.clientX, y: event.clientY }
    timer = setTimeout(() => {
      suppressClick = true
      cancelGesture()
      menu(workspace)
    }, 500)
  }
  function move(event: PointerEvent, workspace: WorkspaceView) {
    if (!start) return
    if (Math.abs(event.clientY - start.y) > 12) cancelGesture()
    else if (start.x - event.clientX > 50) {
      suppressClick = true
      cancelGesture()
      revealed = workspace.workspace_id
    }
  }
  $effect(() => {
    action.set(refresh)
    return () => action.set(null)
  })
</script>

{#snippet refresh()}<Button
    size="icon"
    variant="ghost"
    aria-label={$t('common.refresh')}
    onclick={() => browser.refreshAll()}><ArrowClockwiseIcon /></Button
  >{/snippet}
{#if browser.browserLoadError}
  <p role="alert">{browser.browserLoadError.message}</p>
  <Button onclick={() => browser.refreshAll()}>{$t('common.retry')}</Button>
{:else if browser.workspacesLoading || browser.rootsQuery.isPending}
  <p role="status">{$t('workspaces.loadingBrowser')}</p>
{:else}
  {#if !browser.workspaceRoots.length}<p class="py-3">{$t('workspaces.noRoots')}<br />{$t('workspaces.runPrefix')} <code>pontia init</code> {$t('workspaces.initSuffix')}</p>{/if}
  <nav class="mb-3 flex gap-5 overflow-x-auto border-b" aria-label={$t('workspaces.browserLocations')}>
    <button
      class="location"
      aria-pressed={browser.view === 'active'}
      onclick={() => browser.navigate({ view: 'active' })}>{$t('common.active')}</button
    >
    <button
      class="location"
      aria-pressed={browser.view === 'unavailable'}
      onclick={() => browser.navigate({ view: 'unavailable' })}>{$t('workspaces.unavailable')}</button
    >
    {#each browser.workspaceRoots as root (root.root_id)}<button
        class="location"
        aria-pressed={browser.view === 'directory' && browser.rootId === root.root_id}
        onclick={() => browser.openRoot(root.root_id)}>{root.label}</button
      >{/each}
  </nav>
  {#if browser.actionError}<p role="alert" class="text-destructive">{browser.actionError}</p>{/if}
  {#if browser.view === 'active'}
    {#each browser.activeWorkspaces.filter((workspace) => workspace.availability.status === 'available') as workspace (workspace.workspace_id)}
      {@const location = browser.workspaceLocation(workspace)}
      <div class="relative overflow-hidden rounded-xl">
        <div inert={revealed !== workspace.workspace_id} class="absolute inset-y-0 right-0 flex w-40">
          <button
            class="flex-1 bg-muted"
            onclick={() => {
              revealed = null
              browser.startRenamingWorkspace(workspace)
            }}>{$t('workspaces.rename')}</button
          >
          <button
            class="flex-1 bg-destructive text-destructive-foreground"
            disabled={browser.actionsDisabled}
            onclick={() => {
              revealed = null
              void browser.deleteRegisteredWorkspace(workspace.workspace_id)
            }}>{$t('workspaces.deactivate')}</button
          >
        </div>
        <div
          class="relative flex items-center bg-surface transition-transform"
          style:transform={revealed === workspace.workspace_id ? 'translateX(-10rem)' : undefined}
        >
          <button
            class="mobile-row min-w-0 flex-1 touch-pan-y select-none"
            onpointerdown={(e) => down(e, workspace)}
            onpointermove={(e) => move(e, workspace)}
            onpointercancel={cancelGesture}
            onpointerup={cancelGesture}
            onpointerleave={cancelGesture}
            onclick={() => {
              if (!suppressClick) {
                if (revealed) revealed = null
                else if (location) browser.navigate(location)
              }
              suppressClick = false
            }}
          >
            <span class="min-w-0 text-left"
              ><strong class="block truncate">{workspace.name ?? workspace.display_path}</strong><small
                class="block truncate font-mono">{workspace.canonical_path}</small
              ></span
            >
          </button>
          <button
            class="mobile-icon"
            aria-label={workspace.name ?? workspace.display_path}
            onclick={() => menu(workspace)}><DotsThreeIcon /></button
          >
        </div>
      </div>
    {:else}<p class="py-4">{$t('workspaces.noActive')}</p>{/each}
  {:else if browser.view === 'unavailable'}
    {#each browser.unavailableWorkspaces as workspace (workspace.workspace_id)}
      <div class="mobile-row">
        <div class="min-w-0 flex-1">
          <p class="truncate">{workspace.name ?? workspace.display_path}</p>
          <small>{browser.workspaceAvailabilityLabel(workspace.availability)}</small>
        </div>
        <Button
          variant="secondary"
          class="h-11 rounded-xl px-3"
          disabled={browser.actionsDisabled}
          onclick={() => browser.deleteRegisteredWorkspace(workspace.workspace_id)}
          >{$t('workspaces.revoke')}</Button
        >
      </div>
    {:else}<p class="py-4">{$t('workspaces.noneUnavailable')}</p>{/each}
  {:else}
    <div class="rounded-2xl border bg-card px-3">
      <div class="flex items-center gap-2 border-b py-1">
        <button
          class="mobile-icon disabled:opacity-40"
          disabled={browser.parentPath === null}
          aria-label={$t('workspaces.parentDirectory')}
          onclick={() => browser.openPath(browser.parentPath ?? '')}><ArrowUpIcon /></button
        >
        <nav
          class="flex flex-1 overflow-x-auto whitespace-nowrap font-mono text-xs"
          aria-label={$t('workspaces.currentPath')}
        >
          {#each browser.breadcrumbs as crumb (crumb.path)}<button
              class="min-h-11 px-1"
              onclick={() => browser.openPath(crumb.path)}>{crumb.label}/</button
            >{/each}
        </nav>
        <button
          class="mobile-icon"
          aria-label={$t('workspaces.showHidden')}
          class:text-primary={browser.showHiddenDirectories}
          aria-pressed={browser.showHiddenDirectories}
          onclick={() => (browser.showHiddenDirectories = !browser.showHiddenDirectories)}
          ><EyeIcon /></button
        >
      </div>
      {#if browser.entriesQuery.isError}<p role="alert">{browser.entriesQuery.error.message}</p>
        <Button onclick={() => browser.retryDirectory()}>{$t('common.retry')}</Button>
      {:else if browser.listing}
        {#each browser.listing.warnings as warning (warning)}<p>{warning}</p>{/each}
        {#each browser.visibleEntries as entry (entry.path)}
          <div class="mobile-row py-1">
            <button class="flex min-h-11 min-w-0 flex-1 items-center gap-3 text-left" onclick={() => browser.openPath(entry.path)}
              >{#if entry.is_workspace}<FolderStarIcon class="shrink-0 text-primary" />{:else}<FolderIcon class="shrink-0 text-muted-foreground" />{/if}
              <span class="truncate">{entry.name}</span></button
            >
            <Button
              variant="secondary"
              class="h-11 rounded-xl px-3"
              disabled={browser.actionsDisabled}
              onclick={() => browser.togglePath(entry.path, browser.workspaceForEntry(entry))}
              >{$t(entry.is_workspace ? 'workspaces.deactivate' : 'workspaces.activate')}</Button
            >
          </div>
        {:else}<p class="py-4">{$t('workspaces.noSubdirectories')}</p>{/each}
        <div class="sticky bottom-0 flex items-center gap-2 rounded-b-2xl bg-card py-3">
          <span class="min-w-0 flex-1 truncate font-mono text-xs">{browser.listing.canonical_path}</span><Button
            disabled={browser.actionsDisabled}
            class="h-11 rounded-xl px-3"
            onclick={() => browser.listing && browser.togglePath(browser.listing.path, browser.currentWorkspace)}
            >{$t(browser.currentWorkspace ? 'workspaces.deactivateCurrent' : 'workspaces.activateCurrent')}</Button
          >
        </div>
      {:else}<p role="status">{$t('common.loading')}</p>{/if}
    </div>
  {/if}
{/if}
<Sheet.Root bind:open={menuOpen}
  ><BottomSheet dismiss={() => (menuOpen = false)} aria-describedby={undefined}>
    <Sheet.Title>{target?.name ?? target?.display_path}</Sheet.Title>
    {#if target}{@const workspace = target}
      <button class="mobile-row" onclick={() => stack.back(() => browser.startRenamingWorkspace(workspace))}
        >{$t('workspaces.rename')}</button
      >
      <button
        class="mobile-row text-destructive"
        disabled={browser.actionsDisabled}
        onclick={() => stack.back(() => void browser.deleteRegisteredWorkspace(workspace.workspace_id))}
        >{$t('workspaces.deactivate')}</button
      >
    {/if}
  </BottomSheet></Sheet.Root
>
<RenameWorkspaceDialog {browser} />

<style>
  .location {
    position: relative;
    flex: none;
    min-height: 44px;
    color: var(--muted-foreground);
    font-size: 15px;
    font-weight: 500;
  }
  .location[aria-pressed='true'] {
    color: var(--heading);
    font-weight: 600;
  }
  .location[aria-pressed='true']::after {
    position: absolute;
    inset: auto 0 0;
    height: 2px;
    background: var(--primary);
    content: '';
  }
</style>
