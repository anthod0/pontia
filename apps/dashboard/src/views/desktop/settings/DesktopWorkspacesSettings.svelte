<script lang="ts">
  import { untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import EyeIcon from 'phosphor-svelte/lib/EyeIcon'
  import EyeSlashIcon from 'phosphor-svelte/lib/EyeSlashIcon'
  import HardDrivesIcon from 'phosphor-svelte/lib/HardDrivesIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import FolderStarIcon from 'phosphor-svelte/lib/FolderStarIcon'
  import FolderOpenIcon from 'phosphor-svelte/lib/FolderOpenIcon'
  import StarIcon from 'phosphor-svelte/lib/StarIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button, type ButtonVariant } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import type { Component } from 'svelte'
  import RenameWorkspaceDialog from '../../../components/workspaces/RenameWorkspaceDialog.svelte'
  import { createWorkspaceBrowser } from '../../../features/settings/workspaceBrowser.svelte'
  let { openFirstAvailableRoot = false }: { openFirstAvailableRoot?: boolean } = $props()
  const browser = createWorkspaceBrowser('directory', untrack(() => openFirstAvailableRoot))
  interface ToolbarButton {
    label: string
    tooltip?: string
    icon: Component<{ class?: string; weight?: 'regular' | 'fill' }>
    iconWeight?: 'regular' | 'fill'
    variant?: ButtonVariant
    class?: string
    pressed?: boolean
    disabled?: boolean
    onclick: () => void
  }

  const sidebarItemClass =
    'flex shrink-0 items-center w-full gap-2 rounded-item px-2 py-1.5 outline-none text-left whitespace-nowrap text-muted-foreground hover:bg-muted hover:text-foreground aria-[current=true]:bg-muted aria-[current=true]:font-medium aria-[current=true]:text-heading'
</script>

{#snippet toolbarButton(button: ToolbarButton)}
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          size="icon-sm"
          variant={button.variant ?? 'ghost'}
          class={button.class}
          aria-label={button.label}
          aria-pressed={button.pressed}
          disabled={button.disabled}
          onclick={button.onclick}
        >
          <button.icon class="size-4" weight={button.iconWeight} />
        </Button>
      {/snippet}
    </Tooltip.Trigger>
    <Tooltip.Content>{button.tooltip ?? button.label}</Tooltip.Content>
  </Tooltip.Root>
{/snippet}

{#snippet loadError(title: string, message: string, onretry: () => void)}
  <Empty.Root class="flex-1" role="alert">
    <Empty.Header>
      <Empty.Media variant="icon"><WarningCircleIcon class="text-destructive" /></Empty.Media>
      <Empty.Title>{title}</Empty.Title>
      <Empty.Description>{message}</Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button size="sm" variant="outline" onclick={onretry}
        ><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button
      >
    </Empty.Content>
  </Empty.Root>
{/snippet}

<div
  class="mx-auto flex w-full max-w-5xl flex-col overflow-hidden rounded-panel border bg-background text-sm shadow-sm"
>
  {#if browser.browserLoadError}
    <div class="flex min-h-56 flex-col">
      {@render loadError(
        $t('workspaces.browserUnavailable'),
        browser.browserLoadError.message,
        () => void browser.refreshAll(),
      )}
    </div>
  {:else if browser.workspacesLoading || browser.rootsQuery.isPending}
    <div class="space-y-3 p-4" aria-label={$t('workspaces.loadingBrowser')}>
      <Skeleton class="h-8 w-full" />
      <Skeleton class="h-72 w-full" />
    </div>
  {:else if !browser.workspaceRoots.length}
    <Empty.Root class="min-h-56">
      <Empty.Header>
        <Empty.Media variant="icon"><FolderIcon /></Empty.Media>
        <Empty.Title>{$t('workspaces.noRoots')}</Empty.Title>
        <Empty.Description
          >{$t('workspaces.runPrefix')}
          <code class="rounded-tooltip bg-muted px-1 py-0.5 text-foreground">pontia init</code>
          {$t('workspaces.initSuffix')}</Empty.Description
        >
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Tooltip.Provider delayDuration={300}>
      <div class="flex items-center gap-1 border-b px-2 py-1.5">
        {@render toolbarButton({
          label: $t('workspaces.parentDirectory'),
          disabled: browser.view !== 'directory' || browser.parentPath === null,
          onclick: () => browser.openPath(browser.parentPath ?? ''),
          icon: ArrowUpIcon,
        })}
        <nav aria-label={$t('workspaces.currentPath')} class="min-w-0 flex-1">
          <ol
            class="flex min-w-0 items-center overflow-x-auto rounded-item bg-muted/60 px-1 py-0.5 whitespace-nowrap"
          >
            {#if browser.view !== 'directory'}
              {@const ViewIcon = browser.view === 'active' ? StarIcon : WarningCircleIcon}
              <li>
                <span class="flex h-6 items-center gap-1.5 px-1.5 font-medium text-foreground" aria-current="page">
                  <ViewIcon class="size-3.5" aria-hidden="true" />
                  {browser.view === 'active' ? $t('common.active') : $t('workspaces.unavailable')}
                </span>
              </li>
            {:else}
              {#each browser.breadcrumbs as crumb, index (crumb.path)}
                {#if index > 0}<li aria-hidden="true">
                    <CaretRightIcon class="size-3 text-muted-foreground" />
                  </li>{/if}
                <li>
                  <button
                    type="button"
                    class="flex h-6 items-center gap-1.5 rounded-tooltip px-1.5 outline-none hover:bg-background aria-[current=page]:font-medium aria-[current=page]:text-foreground text-muted-foreground"
                    aria-current={index === browser.breadcrumbs.length - 1 ? 'page' : undefined}
                    title={index === 0 ? (browser.selectedRoot?.canonical_path ?? undefined) : undefined}
                    onclick={() => browser.openPath(crumb.path)}
                  >
                    {#if index === 0}<HardDrivesIcon class="size-3.5" aria-hidden="true" />{/if}
                    {crumb.label}
                  </button>
                </li>
              {/each}
            {/if}
          </ol>
        </nav>
        {#if browser.view === 'directory'}
          {@render toolbarButton({
            label: $t('workspaces.showHidden'),
            tooltip: browser.showHiddenDirectories
              ? $t('workspaces.hideHiddenDirectories')
              : $t('workspaces.showHiddenDirectories'),
            pressed: browser.showHiddenDirectories,
            class: 'text-muted-foreground aria-pressed:text-foreground',
            onclick: () => {
              browser.showHiddenDirectories = !browser.showHiddenDirectories
            },
            icon: browser.showHiddenDirectories ? EyeIcon : EyeSlashIcon,
          })}
        {/if}
        {@render toolbarButton({
          label: $t('common.refresh'),
          onclick: () => void browser.refreshAll(),
          icon: ArrowClockwiseIcon,
        })}
        {#if browser.view === 'directory' && browser.listing}
          {@const name = browser.currentWorkspace?.name ?? browser.workspaceName(browser.listing.path)}
          {@render toolbarButton({
            label: browser.currentWorkspace
              ? $t('workspaces.deactivateNamed', { name: name })
              : $t('workspaces.activateNamed', { name: name }),
            tooltip:
              browser.registeringPath === browser.listing.path
                ? $t('workspaces.activating')
                : browser.currentWorkspace
                  ? $t('workspaces.deactivateCurrent')
                  : $t('workspaces.activateCurrent'),
            class: browser.currentWorkspace ? 'text-primary hover:text-primary' : undefined,
            disabled: browser.actionsDisabled,
            onclick: () => void browser.togglePath(browser.listing?.path ?? '', browser.currentWorkspace),
            icon: StarIcon,
            iconWeight: browser.currentWorkspace ? 'fill' : 'regular',
          })}
        {/if}
      </div>
    </Tooltip.Provider>

    {#if browser.actionError}
      <div class="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-destructive" role="alert">
        <WarningCircleIcon class="size-3.5 shrink-0" aria-hidden="true" />
        <span class="min-w-0 flex-1 truncate" title={browser.actionError}>{browser.actionError}</span>
        <Button
          size="icon-xs"
          variant="ghost"
          class="text-muted-foreground"
          aria-label={$t('common.dismissError')}
          onclick={() => {
            browser.actionError = null
          }}><XIcon /></Button
        >
      </div>
    {/if}

    <div class="flex min-h-0 h-[30rem] flex-row">
      <nav
        aria-label={$t('workspaces.browserLocations')}
        class="flex shrink-0 gap-1 overflow-x-auto border-b p-2 w-52 flex-col gap-3 border-r border-b-0"
      >
        <button
          type="button"
          class={sidebarItemClass}
          aria-current={browser.view === 'active'}
          onclick={() => browser.navigate({ view: 'active' })}
        >
          <StarIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span class="truncate">{$t('common.active')}</span>
          <span class="ml-auto pl-2 text-xs font-normal text-muted-foreground"
            >{browser.workspacesLoading ? '…' : browser.activeWorkspaces.length}</span
          >
        </button>
        {#if browser.unavailableWorkspaces.length}
          <button
            type="button"
            class="{sidebarItemClass} -mt-2"
            aria-current={browser.view === 'unavailable'}
            onclick={() => browser.navigate({ view: 'unavailable' })}
          >
            <WarningCircleIcon class="size-4 shrink-0 text-warning" aria-hidden="true" />
            <span class="truncate">{$t('common.unavailable')}</span>
            <span class="ml-auto pl-2 text-xs font-normal text-muted-foreground"
              >{browser.unavailableWorkspaces.length}</span
            >
          </button>
        {/if}
        <div class="flex gap-1 flex-col">
          <p class="px-2 text-xs font-medium text-muted-foreground">{$t('workspaces.roots')}</p>
          <ul class="flex gap-1 flex-col" aria-label={$t('workspaces.workspaceRoots')}>
            {#each browser.workspaceRoots as root (root.root_id)}
              <li>
                <button
                  type="button"
                  class={sidebarItemClass}
                  aria-current={browser.view === 'directory' && root.root_id === browser.rootId}
                  title={root.canonical_path ?? root.label}
                  onclick={() => browser.openRoot(root.root_id)}
                >
                  <HardDrivesIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                  <span class="truncate">{root.label}</span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      </nav>

      <div class="flex min-h-72 min-w-0 flex-1 flex-col">
        {#if browser.view === 'active'}
          {#if browser.workspacesLoading}
            <div class="space-y-1 p-2"><Skeleton class="h-10 w-full" /><Skeleton class="h-10 w-full" /></div>
          {:else if browser.activeWorkspaces.length}
            <ul class="flex-1 overflow-auto p-1" aria-label={$t('workspaces.active')}>
              {#each browser.activeWorkspaces as workspace (workspace.workspace_id)}
                {@const location = browser.workspaceLocation(workspace)}
                {@const title = workspace.name ?? workspace.display_path}
                <li class="group flex items-center gap-2 rounded-item pr-1 hover:bg-muted">
                  <button
                    type="button"
                    class="flex min-h-10 min-w-0 flex-1 items-center gap-2 rounded-item px-2 py-1 text-left outline-none disabled:cursor-default"
                    aria-label={$t('workspaces.openNamed', { name: title })}
                    title={location
                      ? workspace.canonical_path
                      : `${workspace.canonical_path}: ${browser.workspaceAvailabilityLabel(workspace.availability)}`}
                    disabled={!location}
                    onclick={() => {
                      if (location) browser.navigate(location)
                    }}
                  >
                    <FolderStarIcon weight="fill" class="size-4.5 shrink-0 text-primary" aria-hidden="true" />
                    <span class="flex min-w-0 flex-col">
                      <span class="truncate">{title}</span>
                      <span class="truncate text-xs text-muted-foreground">{workspace.canonical_path}</span>
                    </span>
                  </button>
                  <div
                    class="flex shrink-0 items-center gap-1 workspace-actions touch-target:gap-2 touch-target:[&_button]:min-h-11 touch-target:[&_button]:min-w-11"
                  >
                    <Button
                      size="icon-xs"
                      variant="ghost"
                      aria-label={$t('workspaces.renameNamed', { name: title })}
                      title={$t('workspaces.rename')}
                      onclick={() => browser.startRenamingWorkspace(workspace)}><PencilSimpleIcon /></Button
                    >
                    <Button
                      size="xs"
                      variant="ghost"
                      aria-label={$t('workspaces.deactivateNamed', { name: title })}
                      title={$t('workspaces.removeRegistration')}
                      onclick={() => void browser.deleteRegisteredWorkspace(workspace.workspace_id)}
                      disabled={browser.actionsDisabled}>{$t('workspaces.deactivate')}</Button
                    >
                  </div>
                </li>
              {/each}
            </ul>
          {:else}
            <Empty.Root class="flex-1"
              ><Empty.Header
                ><Empty.Media variant="icon"><StarIcon /></Empty.Media><Empty.Title
                  >{$t('workspaces.noActive')}</Empty.Title
                ><Empty.Description>{$t('workspaces.activateHint')}</Empty.Description></Empty.Header
              ></Empty.Root
            >
          {/if}
        {:else if browser.view === 'unavailable'}
          <ul class="flex-1 overflow-auto p-1" aria-label={$t('workspaces.unavailable')}>
            {#each browser.unavailableWorkspaces as workspace (workspace.workspace_id)}
              {@const title = workspace.name ?? workspace.display_path}
              <li class="group flex items-center gap-2 rounded-item px-2 py-1 hover:bg-muted">
                <WarningCircleIcon class="size-4.5 shrink-0 text-warning" aria-hidden="true" />
                <div class="flex min-h-8 min-w-0 flex-1 flex-col justify-center">
                  <span class="truncate"
                    >{title}
                    <span class="text-xs text-muted-foreground"
                      >· {browser.workspaceAvailabilityLabel(workspace.availability)}</span
                    ></span
                  >
                  <span class="truncate text-xs text-muted-foreground" title={workspace.canonical_path}
                    >{workspace.canonical_path}</span
                  >
                </div>
                <Button
                  size="xs"
                  variant="ghost"
                  class="workspace-actions touch-target:min-h-11 touch-target:min-w-11"
                  aria-label={$t('workspaces.revokeNamed', { name: title })}
                  title={$t('workspaces.removeRegistration')}
                  onclick={() => void browser.deleteRegisteredWorkspace(workspace.workspace_id)}
                  disabled={browser.actionsDisabled}
                >
                  {$t('workspaces.revoke')}
                </Button>
              </li>
            {/each}
          </ul>
        {:else}
          {#if browser.listing?.warnings.length}<div class="px-3 pt-2 text-xs text-muted-foreground">
              {browser.listing.warnings.join(' · ')}
            </div>{/if}
          {#if browser.entriesQuery.isFetching && !browser.listing}
            <div class="space-y-1 p-2">
              <Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" />
            </div>
          {:else if browser.entriesQuery.error}
            {@render loadError(
              $t('workspaces.cannotOpen'),
              browser.entriesQuery.error.message,
              () => void browser.retryDirectory(),
            )}
          {:else if browser.listing && browser.visibleEntries.length}
            <ul class="flex-1 overflow-auto p-1" aria-label={$t('workspaces.directories')}>
              {#each browser.visibleEntries as entry (entry.path)}
                {@const entryWorkspace = browser.workspaceForEntry(entry)}
                <li class="group flex items-center gap-2 rounded-item pr-1 hover:bg-muted">
                  <button
                    type="button"
                    class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-item px-2 text-left outline-none"
                    aria-label={$t('workspaces.openDirectoryNamed', { name: entry.name })}
                    onclick={() => browser.openPath(entry.path)}
                  >
                    {#if entry.is_workspace}<FolderStarIcon
                        weight="fill"
                        class="size-4.5 shrink-0 text-primary"
                        aria-hidden="true"
                      />{:else}<FolderIcon
                        weight="fill"
                        class="size-4.5 shrink-0 text-muted-foreground/70"
                        aria-hidden="true"
                      />{/if}
                    <span class="truncate">{entry.name}</span>
                    {#if entryWorkspace}
                      {#if entryWorkspace.name && entryWorkspace.name !== entry.name}<span
                          class="truncate text-xs text-muted-foreground">{entryWorkspace.name}</span
                        >{/if}
                    {/if}
                  </button>
                  <div
                    class="flex shrink-0 items-center gap-1 workspace-actions touch-target:gap-2 touch-target:[&_button]:min-h-11 touch-target:[&_button]:min-w-11"
                  >
                    {#if entryWorkspace}
                      <Button
                        size="icon-xs"
                        variant="ghost"
                        aria-label={$t('workspaces.renameNamed', { name: entryWorkspace.name ?? entry.name })}
                        title={$t('workspaces.rename')}
                        onclick={() => browser.startRenamingWorkspace(entryWorkspace)}><PencilSimpleIcon /></Button
                      >
                    {/if}
                    <Button
                      size="xs"
                      variant={entry.is_workspace ? 'ghost' : 'outline'}
                      aria-label={entry.is_workspace
                        ? $t('workspaces.deactivateNamed', { name: entry.name })
                        : $t('workspaces.activateNamed', { name: entry.name })}
                      title={entry.is_workspace ? $t('workspaces.removeRegistration') : $t('workspaces.register')}
                      onclick={() => void browser.togglePath(entry.path, entryWorkspace)}
                      disabled={browser.actionsDisabled}
                    >
                      {browser.registeringPath === entry.path
                        ? $t('workspaces.activating')
                        : entry.is_workspace
                          ? $t('workspaces.deactivate')
                          : $t('workspaces.activate')}
                    </Button>
                  </div>
                </li>
              {/each}
            </ul>
          {:else if browser.listing}
            <Empty.Root class="flex-1"
              ><Empty.Header
                ><Empty.Media variant="icon"><FolderOpenIcon /></Empty.Media><Empty.Title
                  >{$t('workspaces.noSubdirectories')}</Empty.Title
                ><Empty.Description
                  >{browser.hiddenEntryCount
                    ? $t('workspaces.hiddenCount', { count: browser.hiddenEntryCount })
                    : $t('workspaces.emptyDirectory')}</Empty.Description
                ></Empty.Header
              ></Empty.Root
            >
          {:else}
            <Empty.Root class="flex-1"
              ><Empty.Header
                ><Empty.Title>{$t('workspaces.noRootOpen')}</Empty.Title><Empty.Description
                  >{$t('workspaces.selectRoot')}</Empty.Description
                ></Empty.Header
              ></Empty.Root
            >
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<RenameWorkspaceDialog {browser} />

<style>
  @media (width >= 48rem) and (hover: hover) and (pointer: fine) {
    li:not(:hover):not(:focus-within) :global(.workspace-actions) {
      opacity: 0;
      pointer-events: none;
    }
  }
</style>
