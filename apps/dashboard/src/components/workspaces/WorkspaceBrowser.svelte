<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, CornerUpLeft, Folder, FolderBookmark, FolderOpen, Pencil, RefreshCw } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import type { WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView, WorkspaceView } from '../../api/types'
  import { browseWorkspaceRoot, deleteWorkspace, loadWorkspaceRoots, loadWorkspaces, registerWorkspace, renameWorkspace, workspaceRoots, workspaces, workspacesError, workspacesLoading } from '../../stores/workspaces'

  let mounted = false
  let rootId = ''
  let browsePath = ''
  let listing: WorkspaceDirectoryListingView | null = null
  let rootsLoading = true
  let browserLoading = false
  let browserError: string | null = null
  let registeringPath: string | null = null
  let registerError: string | null = null
  let deletingWorkspaceId: string | null = null
  let deleteError: string | null = null
  let renameError: string | null = null
  let renamingWorkspace: WorkspaceView | null = null
  let renamingWorkspaceName = ''
  let renameWorkspaceDialogOpen = false
  let savingRename = false

  onMount(() => {
    mounted = true
    const controller = new AbortController()

    void initialize(controller.signal)
    return () => {
      mounted = false
      controller.abort()
    }
  })

  async function initialize(signal: AbortSignal): Promise<void> {
    rootsLoading = true
    try {
      await Promise.all([
        loadWorkspaces({ signal }),
        loadWorkspaceRoots({ signal }).then((roots) => {
          if (!rootId && roots.length) rootId = roots[0].root_id
        }),
      ])
      if (!signal.aborted && rootId) await openPath('', { signal })
    } catch (error) {
      if (!isAbortError(error)) browserError = errorMessage(error)
    } finally {
      if (!signal.aborted) rootsLoading = false
    }
  }

  $: selectedRoot = $workspaceRoots.find((root) => root.root_id === rootId) ?? null
  $: currentWorkspace = listing ? workspaceForCanonicalPath(listing.canonical_path) : null
  $: if (!renameWorkspaceDialogOpen && renamingWorkspace && !savingRename) {
    renamingWorkspace = null
    renamingWorkspaceName = ''
  }

  async function refreshAll(): Promise<void> {
    await Promise.all([loadWorkspaces(), loadWorkspaceRoots()])
    if (!rootId && $workspaceRoots.length) rootId = $workspaceRoots[0].root_id
    if (rootId) await openPath(browsePath)
  }

  async function openPath(path: string, options: { signal?: AbortSignal } = {}): Promise<void> {
    if (!rootId) return
    browserLoading = true
    browserError = null
    try {
      listing = await browseWorkspaceRoot(rootId, path, options)
      browsePath = listing.path
    } catch (error) {
      if (isAbortError(error)) return
      listing = null
      browserError = errorMessage(error)
    } finally {
      if (!options.signal?.aborted) browserLoading = false
    }
  }

  function canonicalPathForEntry(entry: WorkspaceDirectoryEntryView): string | null {
    if (!selectedRoot?.canonical_path) return null
    const rootPath = selectedRoot.canonical_path.replace(/\/+$/, '')
    return entry.path.trim() ? `${rootPath}/${entry.path}` : rootPath
  }

  function workspaceForCanonicalPath(canonicalPath: string): WorkspaceView | null {
    return $workspaces.find((workspace) => workspace.canonical_path === canonicalPath || workspace.display_path === canonicalPath) ?? null
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
    registerError = null
    try {
      await registerWorkspace({ root_id: rootId, path, name: workspaceName(path) })
      if (mounted) await openPath(browsePath)
    } catch (error) {
      registerError = errorMessage(error)
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
      await renameWorkspace(renamingWorkspace.workspace_id, { name: renamingWorkspaceName.trim() || null })
      renameWorkspaceDialogOpen = false
      renamingWorkspace = null
      renamingWorkspaceName = ''
      if (rootId) await openPath(browsePath)
    } catch (error) {
      renameError = errorMessage(error)
    } finally {
      savingRename = false
    }
  }

  async function deleteRegisteredWorkspace(workspaceId: string): Promise<void> {
    if (deletingWorkspaceId) return
    deletingWorkspaceId = workspaceId
    deleteError = null
    try {
      await deleteWorkspace(workspaceId)
      if (rootId) await openPath(browsePath)
    } catch (error) {
      deleteError = errorMessage(error)
    } finally {
      deletingWorkspaceId = null
    }
  }

  function isAbortError(error: unknown): boolean {
    return error instanceof DOMException && error.name === 'AbortError'
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error)
  }
</script>

<div class="space-y-4">
  {#if $workspacesError || browserError || registerError || renameError || deleteError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Workspace error</Alert.Title>
      <Alert.Description>{deleteError ?? renameError ?? registerError ?? browserError ?? $workspacesError}</Alert.Description>
    </Alert.Root>
  {/if}

  <Card.Root class="mx-auto max-w-5xl">
    <Card.Header>
      <div class="flex items-start justify-between gap-3">
        <div>
          <Card.Title class="flex items-center gap-2"><FolderOpen class="size-5" /> Browser</Card.Title>
          <Card.Description class="mt-1">Select a root and browse directories. Active workspaces stay pinned at the top of the browser.</Card.Description>
        </div>
        <Button size="sm" variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
      </div>
    </Card.Header>
    <Card.Content class="space-y-4">
      {#if rootsLoading}
        <div class="space-y-3" aria-label="Loading workspace browser">
          <Skeleton class="h-9 w-full" />
          <Skeleton class="h-44 w-full" />
        </div>
      {:else if !$workspaceRoots.length}
        <Empty.Root class="min-h-56 border">
          <Empty.Header>
            <Empty.Media variant="icon"><Folder class="size-4" /></Empty.Media>
            <Empty.Title>No workspace roots configured</Empty.Title>
            <Empty.Description>Run <code class="rounded bg-muted px-1 py-0.5 text-foreground">pontia init</code> on the Pontia host, configure at least one workspace root, then restart Pontia.</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {:else}
        <div class="grid gap-3 md:grid-cols-[220px_1fr_auto] md:items-end">
          <div class="space-y-2">
            <Label for="workspace-root">Root</Label>
            <select id="workspace-root" bind:value={rootId} onchange={() => void openPath('')} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
              {#each $workspaceRoots as root (root.root_id)}
                <option value={root.root_id}>{root.label}</option>
              {/each}
            </select>
          </div>
          <div class="space-y-2">
            <Label for="browse-path">Path</Label>
            <Input id="browse-path" bind:value={browsePath} placeholder="Relative path inside root" />
          </div>
          <Button variant="outline" onclick={() => void openPath(browsePath)} disabled={!rootId || browserLoading}>Open</Button>
        </div>

        <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          {#if selectedRoot}<p>Root state: {selectedRoot.state} · {selectedRoot.canonical_path ?? 'virtual root'}</p>{/if}
          {#if $workspacesLoading}<p>Loading active workspaces…</p>{:else}<p>{$workspaces.length} active workspace{$workspaces.length === 1 ? '' : 's'}</p>{/if}
        </div>

        {#if browserLoading}
          <div class="space-y-2"><Skeleton class="h-9 w-full" /><Skeleton class="h-9 w-full" /><Skeleton class="h-9 w-full" /></div>
        {:else if listing}
          <div class="rounded-lg border">
            <div class="flex flex-wrap items-center justify-between gap-2 border-b p-3 text-sm">
              <div class="flex min-w-0 items-center gap-2">
                {#if listing.parent_path !== null}
                  <Button size="icon-sm" variant="ghost" aria-label="Open parent directory" title="Open parent directory" onclick={() => void openPath(listing?.parent_path ?? '')}><CornerUpLeft class="size-4" /></Button>
                {/if}
                <span class="truncate font-medium" title={listing.canonical_path}>{listing.canonical_path}</span>
              </div>
              <Button
                size="sm"
                variant={currentWorkspace ? 'secondary' : 'outline'}
                aria-label={currentWorkspace ? `Deactivate ${currentWorkspace.name ?? workspaceName(listing.path)}` : `Activate ${workspaceName(listing.path)}`}
                onclick={() => void togglePath(listing?.path ?? '', currentWorkspace)}
                disabled={registeringPath !== null || deletingWorkspaceId !== null}
              >
                {registeringPath === listing.path ? 'Activating…' : currentWorkspace ? 'Deactivate' : 'Activate current directory'}
              </Button>
            </div>
            {#if listing.warnings.length}<div class="border-b bg-muted/40 p-3 text-xs text-muted-foreground">{listing.warnings.join(' · ')}</div>{/if}
            <div class="max-h-[32rem] overflow-auto">
              <Table.Root>
                <Table.Header><Table.Row><Table.Head>Directory</Table.Head><Table.Head class="text-right">Action</Table.Head></Table.Row></Table.Header>
                <Table.Body>
                  {#each [...listing.entries].sort((left, right) => Number(right.is_workspace) - Number(left.is_workspace) || left.name.localeCompare(right.name)) as entry (entry.path)}
                    {@const entryWorkspace = workspaceForEntry(entry)}
                    <Table.Row>
                      <Table.Cell class="font-medium">
                        <button type="button" class="flex min-w-0 cursor-pointer items-center gap-2 text-left hover:underline" aria-label="Open directory {entry.name}" title="Open directory" onclick={() => void openPath(entry.path)}>
                          {#if entry.is_workspace}<FolderBookmark class="size-4 shrink-0 text-foreground/80" aria-hidden="true" />{:else}<Folder class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />{/if}
                          <span class="truncate">{entry.name}/</span>
                        </button>
                      </Table.Cell>
                      <Table.Cell class="text-right">
                        <div class="flex justify-end gap-2">
                          {#if entryWorkspace}
                            <Button size="icon-sm" variant="outline" aria-label={`Rename ${entryWorkspace.name ?? entry.name}`} title="Rename workspace" onclick={() => startRenamingWorkspace(entryWorkspace)}><Pencil class="size-4" /></Button>
                          {/if}
                          <Button
                            size="sm"
                            variant={entry.is_workspace ? 'secondary' : 'outline'}
                            aria-label={entry.is_workspace ? `Deactivate ${entry.name}` : `Activate ${entry.name}`}
                            title={entry.is_workspace ? 'Remove workspace registration' : 'Register as workspace'}
                            onclick={() => void togglePath(entry.path, entryWorkspace)}
                            disabled={registeringPath !== null || deletingWorkspaceId !== null}
                          >
                            {registeringPath === entry.path ? 'Activating…' : entry.is_workspace ? 'Deactivate' : 'Activate'}
                          </Button>
                        </div>
                      </Table.Cell>
                    </Table.Row>
                  {/each}
                </Table.Body>
              </Table.Root>
            </div>
          </div>
        {:else}
          <Empty.Root><Empty.Header><Empty.Title>No root opened</Empty.Title><Empty.Description>Select a workspace root to browse.</Empty.Description></Empty.Header></Empty.Root>
        {/if}
      {/if}
    </Card.Content>
  </Card.Root>
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
        </div>
        <Dialog.Footer class="mt-5">
          <Button type="button" variant="outline" onclick={cancelRenamingWorkspace} disabled={savingRename}>Cancel</Button>
          <Button type="submit" disabled={savingRename}>{savingRename ? 'Saving…' : 'Rename workspace'}</Button>
        </Dialog.Footer>
      </form>
    </Dialog.Content>
  {/if}
</Dialog.Root>
