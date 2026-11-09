<script lang="ts">
  import { useDeviceRuntime } from '../../device/context';
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
  import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button, type ButtonVariant } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import type { Component } from 'svelte'
  import type { WorkspaceAvailability, WorkspaceDirectoryEntryView, WorkspaceView } from '../../api/types'
  const device = useDeviceRuntime();
  const {
    createDeleteWorkspaceMutation,
    createRegisterWorkspaceMutation,
    createRenameWorkspaceMutation,
    createWorkspaceRootEntriesQuery,
    createWorkspaceRootsQuery,
    createWorkspacesQuery,
  } = device.workspaceQueries;

  interface ToolbarButton {
    label: string
    tooltip?: string
    icon: Component<{ class?: string, weight?: 'regular' | 'fill' }>
    iconWeight?: 'regular' | 'fill'
    variant?: ButtonVariant
    class?: string
    pressed?: boolean
    disabled?: boolean
    onclick: () => void
  }

  type BrowserLocation = { view: 'active' } | { view: 'unavailable' } | { view: 'directory', rootId: string, path: string }

  let rootId = $state('')
  let browsePath = $state('')
  let openedPath = $state('')
  const workspacesQuery = createWorkspacesQuery()
  const rootsQuery = createWorkspaceRootsQuery()
  const entriesQuery = createWorkspaceRootEntriesQuery(() => rootId, () => openedPath)
  const registerWorkspaceMutation = createRegisterWorkspaceMutation()
  const renameWorkspaceMutation = createRenameWorkspaceMutation()
  const deleteWorkspaceMutation = createDeleteWorkspaceMutation()
  let registeringPath = $state<string | null>(null)
  let actionError = $state<string | null>(null)
  let deletingWorkspaceId = $state<string | null>(null)
  let renameError = $state<string | null>(null)
  let renamingWorkspace = $state<WorkspaceView | null>(null)
  let renamingWorkspaceName = $state('')
  let renameWorkspaceDialogOpen = $state(false)
  let savingRename = $state(false)
  let showHiddenDirectories = $state(false)
  let view = $state<BrowserLocation['view']>('directory')
  let workspaces = $derived(workspacesQuery.data ?? [])
  let activeWorkspaces = $derived(workspaces.filter((workspace) => workspace.state === 'active'))
  let unavailableWorkspaces = $derived(activeWorkspaces.filter((workspace) => workspace.availability.status !== 'available'))
  let workspaceRoots = $derived(rootsQuery.data ?? [])
  let listing = $derived(entriesQuery.data ?? null)
  let selectedRoot = $derived(workspaceRoots.find((root) => root.root_id === rootId) ?? null)
  let currentWorkspace = $derived(listing ? workspaceForCanonicalPath(listing.canonical_path) : null)
  let visibleEntries = $derived(
    (listing?.entries ?? [])
      .filter((entry) => showHiddenDirectories || !entry.name.startsWith('.'))
      .sort((left, right) => Number(right.is_workspace) - Number(left.is_workspace) || left.name.localeCompare(right.name)),
  )
  let hiddenEntryCount = $derived((listing?.entries.length ?? 0) - visibleEntries.length)
  let breadcrumbs = $derived(pathBreadcrumbs(browsePath))
  let parentPath = $derived(browsePath ? browsePath.split('/').filter(Boolean).slice(0, -1).join('/') : null)
  let browserLoadError = $derived(rootsQuery.error ?? workspacesQuery.error)
  let workspacesLoading = $derived(workspacesQuery.isPending)
  let actionsDisabled = $derived(workspacesLoading || registeringPath !== null || deletingWorkspaceId !== null)
  $effect(() => {
    if (!rootId && workspaceRoots.length) rootId = workspaceRoots[0].root_id
  })
  $effect(() => {
    if (view === 'unavailable' && !unavailableWorkspaces.length) view = 'active'
  })
  $effect(() => {
    if (listing && listing.root_id === rootId && listing.path !== browsePath) browsePath = listing.path
  })
  $effect(() => {
    if (!renameWorkspaceDialogOpen && renamingWorkspace && !savingRename) {
      renamingWorkspace = null
      renamingWorkspaceName = ''
    }
  })

  async function retryDirectory(): Promise<void> {
    await entriesQuery.refetch()
  }

  async function refreshAll(): Promise<void> {
    await Promise.all([workspacesQuery.refetch(), rootsQuery.refetch()])
    if (rootId) await entriesQuery.refetch()
  }

  function pathBreadcrumbs(path: string): { label: string, path: string }[] {
    const segments = path.split('/').filter(Boolean)
    return [
      { label: selectedRoot?.label ?? 'Root', path: '' },
      ...segments.map((segment, index) => ({ label: segment, path: segments.slice(0, index + 1).join('/') })),
    ]
  }

  function navigate(location: BrowserLocation): void {
    view = location.view
    if (location.view !== 'directory') return
    rootId = location.rootId
    loadPath(location.path)
  }

  function openRoot(nextRootId: string): void {
    navigate({ view: 'directory', rootId: nextRootId, path: '' })
  }

  function openPath(path: string): void {
    navigate({ view: 'directory', rootId, path })
  }

  function workspaceLocation(workspace: WorkspaceView): BrowserLocation | null {
    const availability = workspace.availability
    return availability.status === 'available'
      ? { view: 'directory', rootId: availability.root_id, path: availability.path }
      : null
  }

  function workspaceAvailabilityLabel(availability: WorkspaceAvailability): string {
    if (availability.status === 'available') return 'Available'
    const labels = {
      outside_root: 'Outside root',
      missing_directory: 'Missing directory',
      not_directory: 'Not a directory',
      permission_denied: 'Permission denied',
      root_unavailable: 'Cannot check: root unavailable',
      check_failed: 'Cannot check directory',
    }
    return labels[availability.reason]
  }

  function loadPath(path: string): void {
    if (!rootId) return
    openedPath = path
    browsePath = path
  }

  function canonicalPathForEntry(entry: WorkspaceDirectoryEntryView): string | null {
    if (!selectedRoot?.canonical_path) return null
    const rootPath = selectedRoot.canonical_path.replace(/\/+$/, '')
    return entry.path.trim() ? `${rootPath}/${entry.path}` : rootPath
  }

  function workspaceForCanonicalPath(canonicalPath: string): WorkspaceView | null {
    return workspaces.find((workspace) => workspace.canonical_path === canonicalPath || workspace.display_path === canonicalPath) ?? null
  }

  function workspaceForEntry(entry: WorkspaceDirectoryEntryView): WorkspaceView | null {
    const canonicalPath = canonicalPathForEntry(entry)
    return canonicalPath ? workspaceForCanonicalPath(canonicalPath) : null
  }

  function workspaceName(path: string): string {
    const segments = path.split('/').filter(Boolean)
    return segments.at(-1) ?? selectedRoot?.label ?? 'Workspace'
  }

  async function togglePath(path: string, workspace: WorkspaceView | null): Promise<void> {
    if (workspace) {
      await deleteRegisteredWorkspace(workspace.workspace_id)
      return
    }
    if (!rootId || registeringPath !== null) return
    registeringPath = path
    actionError = null
    try {
      await registerWorkspaceMutation.mutateAsync({ root_id: rootId, path, name: workspaceName(path) })
    } catch (error) {
      actionError = errorMessage(error)
    } finally {
      registeringPath = null
    }
  }

  function startRenamingWorkspace(workspace: WorkspaceView): void {
    renameError = null
    renamingWorkspace = workspace
    renamingWorkspaceName = workspace.name ?? workspace.display_path
    renameWorkspaceDialogOpen = true
  }

  function cancelRenamingWorkspace(): void {
    renameWorkspaceDialogOpen = false
    renamingWorkspace = null
    renamingWorkspaceName = ''
  }

  async function confirmRenameWorkspace(): Promise<void> {
    if (!renamingWorkspace || savingRename) return
    savingRename = true
    renameError = null
    try {
      await renameWorkspaceMutation.mutateAsync({
        workspaceId: renamingWorkspace.workspace_id,
        input: { name: renamingWorkspaceName.trim() || null },
      })
      renameWorkspaceDialogOpen = false
      renamingWorkspace = null
      renamingWorkspaceName = ''
    } catch (error) {
      renameError = errorMessage(error)
    } finally {
      savingRename = false
    }
  }

  async function deleteRegisteredWorkspace(workspaceId: string): Promise<void> {
    if (deletingWorkspaceId) return
    deletingWorkspaceId = workspaceId
    actionError = null
    try {
      await deleteWorkspaceMutation.mutateAsync(workspaceId)
    } catch (error) {
      actionError = errorMessage(error)
    } finally {
      deletingWorkspaceId = null
    }
  }

  const sidebarItemClass = 'flex shrink-0 items-center md:w-full gap-2 rounded-md px-2 py-1.5 outline-none text-left whitespace-nowrap text-muted-foreground hover:bg-muted hover:text-foreground aria-[current=true]:bg-muted aria-[current=true]:font-medium aria-[current=true]:text-heading'

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error)
  }
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
      <Empty.Media variant="icon"><WarningCircleIcon class="size-4 text-destructive" /></Empty.Media>
      <Empty.Title>{title}</Empty.Title>
      <Empty.Description>{message}</Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button size="sm" variant="outline" onclick={onretry}><ArrowsClockwiseIcon class="size-4" /> Retry</Button>
    </Empty.Content>
  </Empty.Root>
{/snippet}

<div class="mx-auto flex w-full max-w-5xl flex-col overflow-hidden rounded-lg border bg-background text-sm">
  {#if browserLoadError}
    <div class="flex min-h-56 flex-col">
      {@render loadError('Workspace browser unavailable', browserLoadError.message, () => void refreshAll())}
    </div>
  {:else if workspacesLoading || rootsQuery.isPending}
    <div class="space-y-3 p-4" aria-label="Loading workspace browser">
      <Skeleton class="h-8 w-full" />
      <Skeleton class="h-72 w-full" />
    </div>
  {:else if !workspaceRoots.length}
    <Empty.Root class="min-h-56">
      <Empty.Header>
        <Empty.Media variant="icon"><FolderIcon class="size-4" /></Empty.Media>
        <Empty.Title>No workspace roots configured</Empty.Title>
        <Empty.Description>Run <code class="rounded-sm bg-muted px-1 py-0.5 text-foreground">pontia init</code> on the Pontia host, configure at least one workspace root, then restart Pontia.</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Tooltip.Provider delayDuration={300}>
      <div class="flex items-center gap-1 border-b px-2 py-1.5">
        {@render toolbarButton({ label: 'Open parent directory', disabled: view !== 'directory' || parentPath === null, onclick: () => openPath(parentPath ?? ''), icon: ArrowUpIcon })}
        <nav aria-label="Current path" class="min-w-0 flex-1">
          <ol class="flex min-w-0 items-center overflow-x-auto rounded-md bg-muted/60 px-1 py-0.5 whitespace-nowrap">
            {#if view !== 'directory'}
              {@const ViewIcon = view === 'active' ? StarIcon : WarningCircleIcon}
              <li>
                <span class="flex h-6 items-center gap-1.5 px-1.5 font-medium text-foreground" aria-current="page">
                  <ViewIcon class="size-3.5" aria-hidden="true" />
                  {view === 'active' ? 'Active' : 'Unavailable workspaces'}
                </span>
              </li>
            {:else}
              {#each breadcrumbs as crumb, index (crumb.path)}
                {#if index > 0}<li aria-hidden="true"><CaretRightIcon class="size-3 text-muted-foreground" /></li>{/if}
                <li>
                  <button
                    type="button"
                    class="flex h-6 items-center gap-1.5 rounded px-1.5 outline-none hover:bg-background aria-[current=page]:font-medium aria-[current=page]:text-foreground text-muted-foreground"
                    aria-current={index === breadcrumbs.length - 1 ? 'page' : undefined}
                    title={index === 0 ? (selectedRoot?.canonical_path ?? undefined) : undefined}
                    onclick={() => openPath(crumb.path)}
                  >
                    {#if index === 0}<HardDrivesIcon class="size-3.5" aria-hidden="true" />{/if}
                    {crumb.label}
                  </button>
                </li>
              {/each}
            {/if}
          </ol>
        </nav>
        {#if view === 'directory'}
          {@render toolbarButton({
            label: 'Show hidden',
            tooltip: showHiddenDirectories ? 'Hide hidden directories' : 'Show hidden directories',
            pressed: showHiddenDirectories,
            class: 'text-muted-foreground aria-pressed:text-foreground',
            onclick: () => { showHiddenDirectories = !showHiddenDirectories },
            icon: showHiddenDirectories ? EyeIcon : EyeSlashIcon,
          })}
        {/if}
        {@render toolbarButton({ label: 'Refresh', onclick: () => void refreshAll(), icon: ArrowsClockwiseIcon })}
        {#if view === 'directory' && listing}
          {@const name = currentWorkspace?.name ?? workspaceName(listing.path)}
          {@render toolbarButton({
            label: currentWorkspace ? `Deactivate ${name}` : `Activate ${name}`,
            tooltip: registeringPath === listing.path ? 'Activating…' : currentWorkspace ? 'Deactivate current directory' : 'Activate current directory',
            class: currentWorkspace ? 'text-primary hover:text-primary' : undefined,
            disabled: actionsDisabled,
            onclick: () => void togglePath(listing?.path ?? '', currentWorkspace),
            icon: StarIcon,
            iconWeight: currentWorkspace ? 'fill' : 'regular',
          })}
        {/if}
      </div>
    </Tooltip.Provider>

    {#if actionError}
      <div class="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-destructive" role="alert">
        <WarningCircleIcon class="size-3.5 shrink-0" aria-hidden="true" />
        <span class="min-w-0 flex-1 truncate" title={actionError}>{actionError}</span>
        <Button size="icon-xs" variant="ghost" class="text-muted-foreground" aria-label="Dismiss error" onclick={() => { actionError = null }}><XIcon /></Button>
      </div>
    {/if}

    <div class="flex min-h-0 flex-col md:h-[30rem] md:flex-row">
      <nav aria-label="Workspace browser locations" class="flex shrink-0 gap-1 overflow-x-auto border-b p-2 md:w-52 md:flex-col md:gap-3 md:border-r md:border-b-0">
        <button
          type="button"
          class={sidebarItemClass}
          aria-current={view === 'active'}
          onclick={() => navigate({ view: 'active' })}
        >
          <StarIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span class="truncate">Active</span>
          <span class="ml-auto pl-2 text-xs font-normal text-muted-foreground">{workspacesLoading ? '…' : activeWorkspaces.length}</span>
        </button>
        {#if unavailableWorkspaces.length}
          <button
            type="button"
            class="{sidebarItemClass} md:-mt-2"
            aria-current={view === 'unavailable'}
            onclick={() => navigate({ view: 'unavailable' })}
          >
            <WarningCircleIcon class="size-4 shrink-0 text-warning" aria-hidden="true" />
            <span class="truncate">Unavailable</span>
            <span class="ml-auto pl-2 text-xs font-normal text-muted-foreground">{unavailableWorkspaces.length}</span>
          </button>
        {/if}
        <div class="flex gap-1 md:flex-col">
          <p class="hidden px-2 text-xs font-medium text-muted-foreground md:block">Roots</p>
          <ul class="flex gap-1 md:flex-col" aria-label="Workspace roots">
            {#each workspaceRoots as root (root.root_id)}
              <li>
                <button
                  type="button"
                  class={sidebarItemClass}
                  aria-current={view === 'directory' && root.root_id === rootId}
                  title={root.canonical_path ?? root.label}
                  onclick={() => openRoot(root.root_id)}
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
        {#if view === 'active'}
          {#if workspacesLoading}
            <div class="space-y-1 p-2"><Skeleton class="h-10 w-full" /><Skeleton class="h-10 w-full" /></div>
          {:else if activeWorkspaces.length}
            <ul class="flex-1 overflow-auto p-1" aria-label="Active workspaces">
              {#each activeWorkspaces as workspace (workspace.workspace_id)}
                {@const location = workspaceLocation(workspace)}
                {@const title = workspace.name ?? workspace.display_path}
                <li class="group flex items-center gap-2 rounded-md pr-1 hover:bg-muted">
                  <button
                    type="button"
                    class="flex min-h-10 min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1 text-left outline-none disabled:cursor-default"
                    aria-label="Open workspace {title}"
                    title={location ? workspace.canonical_path : `${workspace.canonical_path}: ${workspaceAvailabilityLabel(workspace.availability)}`}
                    disabled={!location}
                    onclick={() => { if (location) navigate(location) }}
                  >
                    <FolderStarIcon weight="fill" class="size-4.5 shrink-0 text-primary" aria-hidden="true" />
                    <span class="flex min-w-0 flex-col">
                      <span class="truncate">{title}</span>
                      <span class="truncate text-xs text-muted-foreground">{workspace.canonical_path}</span>
                    </span>
                  </button>
                  <div class="flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100 group-has-focus-visible:opacity-100">
                    <Button size="icon-xs" variant="ghost" aria-label={`Rename ${title}`} title="Rename workspace" onclick={() => startRenamingWorkspace(workspace)}><PencilSimpleIcon /></Button>
                    <Button size="xs" variant="ghost" aria-label={`Deactivate ${title}`} title="Remove workspace registration" onclick={() => void deleteRegisteredWorkspace(workspace.workspace_id)} disabled={actionsDisabled}>Deactivate</Button>
                  </div>
                </li>
              {/each}
            </ul>
          {:else}
            <Empty.Root class="flex-1"><Empty.Header><Empty.Media variant="icon"><StarIcon class="size-4" /></Empty.Media><Empty.Title>No active workspaces</Empty.Title><Empty.Description>Activate a directory from a root to add it here.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        {:else if view === 'unavailable'}
          <ul class="flex-1 overflow-auto p-1" aria-label="Unavailable workspaces">
            {#each unavailableWorkspaces as workspace (workspace.workspace_id)}
              {@const title = workspace.name ?? workspace.display_path}
              <li class="group flex items-center gap-2 rounded-md px-2 py-1 hover:bg-muted">
                <WarningCircleIcon class="size-4.5 shrink-0 text-warning" aria-hidden="true" />
                <div class="flex min-h-8 min-w-0 flex-1 flex-col justify-center">
                  <span class="truncate">{title} <span class="text-xs text-muted-foreground">· {workspaceAvailabilityLabel(workspace.availability)}</span></span>
                  <span class="truncate text-xs text-muted-foreground" title={workspace.canonical_path}>{workspace.canonical_path}</span>
                </div>
                <Button
                  size="xs"
                  variant="ghost"
                  class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  aria-label={`Revoke ${title}`}
                  title="Remove workspace registration"
                  onclick={() => void deleteRegisteredWorkspace(workspace.workspace_id)}
                  disabled={actionsDisabled}
                >
                  Revoke
                </Button>
              </li>
            {/each}
          </ul>
        {:else}
          {#if listing?.warnings.length}<div class="px-3 pt-2 text-xs text-muted-foreground">{listing.warnings.join(' · ')}</div>{/if}
          {#if entriesQuery.isFetching && !listing}
            <div class="space-y-1 p-2"><Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" /><Skeleton class="h-8 w-full" /></div>
          {:else if entriesQuery.error}
            {@render loadError('Cannot open this directory', entriesQuery.error.message, () => void retryDirectory())}
          {:else if listing && visibleEntries.length}
            <ul class="flex-1 overflow-auto p-1" aria-label="Directories">
              {#each visibleEntries as entry (entry.path)}
                {@const entryWorkspace = workspaceForEntry(entry)}
                <li class="group flex items-center gap-2 rounded-md pr-1 hover:bg-muted">
                  <button
                    type="button"
                    class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-md px-2 text-left outline-none"
                    aria-label="Open directory {entry.name}"
                    onclick={() => openPath(entry.path)}
                  >
                    {#if entry.is_workspace}<FolderStarIcon weight="fill" class="size-4.5 shrink-0 text-primary" aria-hidden="true" />{:else}<FolderIcon weight="fill" class="size-4.5 shrink-0 text-muted-foreground/70" aria-hidden="true" />{/if}
                    <span class="truncate">{entry.name}</span>
                    {#if entryWorkspace}
                      {#if entryWorkspace.name && entryWorkspace.name !== entry.name}<span class="truncate text-xs text-muted-foreground">{entryWorkspace.name}</span>{/if}
                    {/if}
                  </button>
                  <div class="flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100 group-has-focus-visible:opacity-100">
                    {#if entryWorkspace}
                      <Button size="icon-xs" variant="ghost" aria-label={`Rename ${entryWorkspace.name ?? entry.name}`} title="Rename workspace" onclick={() => startRenamingWorkspace(entryWorkspace)}><PencilSimpleIcon /></Button>
                    {/if}
                    <Button
                      size="xs"
                      variant={entry.is_workspace ? 'ghost' : 'outline'}
                      aria-label={entry.is_workspace ? `Deactivate ${entry.name}` : `Activate ${entry.name}`}
                      title={entry.is_workspace ? 'Remove workspace registration' : 'Register as workspace'}
                      onclick={() => void togglePath(entry.path, entryWorkspace)}
                      disabled={actionsDisabled}
                    >
                      {registeringPath === entry.path ? 'Activating…' : entry.is_workspace ? 'Deactivate' : 'Activate'}
                    </Button>
                  </div>
                </li>
              {/each}
            </ul>
          {:else if listing}
            <Empty.Root class="flex-1"><Empty.Header><Empty.Media variant="icon"><FolderOpenIcon class="size-4" /></Empty.Media><Empty.Title>No subdirectories</Empty.Title><Empty.Description>{hiddenEntryCount ? `${hiddenEntryCount} hidden director${hiddenEntryCount === 1 ? 'y' : 'ies'}.` : 'This directory is empty.'}</Empty.Description></Empty.Header></Empty.Root>
          {:else}
            <Empty.Root class="flex-1"><Empty.Header><Empty.Title>No root opened</Empty.Title><Empty.Description>Select a workspace root to browse.</Empty.Description></Empty.Header></Empty.Root>
        {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<Dialog.Root bind:open={renameWorkspaceDialogOpen}>
  {#if renamingWorkspace}
    <Dialog.Content class="max-w-md">
      <form onsubmit={(event) => { event.preventDefault(); void confirmRenameWorkspace() }}>
        <Dialog.Header>
          <Dialog.Title>Confirm workspace rename</Dialog.Title>
          <Dialog.Description>Rename <span class="font-medium text-foreground">{renamingWorkspace.name ?? renamingWorkspace.display_path}</span>.</Dialog.Description>
        </Dialog.Header>
        <div class="mt-4 space-y-2">
          <Label for="rename-workspace-name">Display name</Label>
          <Input id="rename-workspace-name" bind:value={renamingWorkspaceName} placeholder={renamingWorkspace.display_path} />
          <p class="text-xs text-muted-foreground">Clear the name to display the workspace path.</p>
          {#if renameError}<p class="text-xs text-destructive" role="alert">{renameError}</p>{/if}
        </div>
        <Dialog.Footer class="mt-5">
          <Button type="button" variant="outline" onclick={cancelRenamingWorkspace} disabled={savingRename}>Cancel</Button>
          <Button type="submit" disabled={savingRename}>{savingRename ? 'Saving…' : 'Rename workspace'}</Button>
        </Dialog.Footer>
      </form>
    </Dialog.Content>
  {/if}
</Dialog.Root>
