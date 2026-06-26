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
  import type { WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView, WorkspaceView } from '../api/types'
  import { browseWorkspaceRoot, deleteWorkspace, loadWorkspaceRoots, loadWorkspaces, registerWorkspace, renameWorkspace, workspaceRoots, workspaces, workspacesError, workspacesLoading } from '../stores/workspaces'

  let rootId = ''
  let browsePath = ''
  let listing: WorkspaceDirectoryListingView | null = null
  let browserLoading = false
  let browserError: string | null = null
  let registering = false
  let registerError: string | null = null
  let deletingWorkspaceId: string | null = null
  let deleteError: string | null = null
  let renameError: string | null = null
  let renamingWorkspace: WorkspaceView | null = null
  let renamingWorkspaceName = ''
  let renameWorkspaceDialogOpen = false
  let outsideRootWorkspacesDialogOpen = false
  let savingRename = false

  onMount(() => {
    const controller = new AbortController()

    void (async () => {
      await Promise.all([loadWorkspaces({ signal: controller.signal }), loadWorkspaceRoots({ signal: controller.signal }).then((roots) => {
        if (!rootId && roots.length) rootId = roots[0].root_id
      })])
      if (!controller.signal.aborted && rootId) await openPath('', { signal: controller.signal })
    })()

    return () => controller.abort()
  })

  $: selectedRoot = $workspaceRoots.find((root) => root.root_id === rootId) ?? null
  $: configuredRootPaths = $workspaceRoots
    .map((root) => root.canonical_path)
    .filter((path): path is string => Boolean(path?.trim()))
    .map(normalizeAbsolutePath)
  $: outsideRootWorkspaces = $workspaces.filter((workspace) => workspace.state === 'active' && !isPathInsideAnyRoot(workspace.canonical_path, configuredRootPaths))
  $: if (!renameWorkspaceDialogOpen && renamingWorkspace && !savingRename) {
    renamingWorkspace = null
    renamingWorkspaceName = ''
  }

  async function refreshAll(): Promise<void> {
    await Promise.all([loadWorkspaces(), loadWorkspaceRoots()])
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
      if (error instanceof DOMException && error.name === 'AbortError') return
      listing = null
      browserError = error instanceof Error ? error.message : String(error)
    } finally {
      browserLoading = false
    }
  }

  function canonicalPathForEntry(entry: WorkspaceDirectoryEntryView): string | null {
    if (!selectedRoot?.canonical_path) return null
    const rootPath = selectedRoot.canonical_path.replace(/\/+$/, '')
    return entry.path.trim() ? `${rootPath}/${entry.path}` : rootPath
  }

  function workspaceForEntry(entry: WorkspaceDirectoryEntryView): WorkspaceView | null {
    const canonicalPath = canonicalPathForEntry(entry)
    if (!canonicalPath) return null
    return $workspaces.find((workspace) => workspace.canonical_path === canonicalPath || workspace.display_path === canonicalPath) ?? null
  }

  function normalizeAbsolutePath(path: string): string {
    const trimmed = path.trim()
    if (trimmed === '/') return '/'
    return trimmed.replace(/\/+$/, '')
  }

  function isPathInsideRoot(path: string, rootPath: string): boolean {
    const normalizedPath = normalizeAbsolutePath(path)
    const normalizedRoot = normalizeAbsolutePath(rootPath)
    return normalizedPath === normalizedRoot || normalizedPath.startsWith(`${normalizedRoot}/`)
  }

  function isPathInsideAnyRoot(path: string, rootPaths: string[]): boolean {
    return rootPaths.some((rootPath) => isPathInsideRoot(path, rootPath))
  }

  async function activateEntry(entry: WorkspaceDirectoryEntryView): Promise<void> {
    if (entry.is_workspace) {
      const workspace = workspaceForEntry(entry)
      if (!workspace) {
        deleteError = `Could not find registered workspace for ${entry.name}. Refresh and try again.`
        return
      }
      await deleteRegisteredWorkspace(workspace.workspace_id)
      return
    }
    if (!rootId || registering) return
    registering = true
    registerError = null
    try {
      await registerWorkspace({ root_id: rootId, path: entry.path, name: entry.name })
      if (rootId) await openPath(browsePath)
    } catch (error) {
      registerError = error instanceof Error ? error.message : String(error)
    } finally {
      registering = false
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
      renameError = error instanceof Error ? error.message : String(error)
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
      deleteError = error instanceof Error ? error.message : String(error)
    } finally {
      deletingWorkspaceId = null
    }
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="text-3xl font-semibold tracking-tight">Workspaces</h2>
      <p class="max-w-3xl text-muted-foreground">Browse configured roots and register execution workspaces through the External API.</p>
    </div>
    <Button variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  {#if $workspacesError || browserError || registerError || renameError || deleteError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Workspace error</Alert.Title>
      <Alert.Description>{deleteError ?? renameError ?? registerError ?? browserError ?? $workspacesError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if outsideRootWorkspaces.length}
    <Alert.Root>
      <CircleAlert class="size-4" />
      <Alert.Title>{outsideRootWorkspaces.length} active workspace{outsideRootWorkspaces.length === 1 ? '' : 's'} outside configured roots</Alert.Title>
      <Alert.Description>
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <span>These active workspaces are registered but are not under any configured workspace root.</span>
          <Button size="sm" variant="outline" onclick={() => { outsideRootWorkspacesDialogOpen = true }}>Review outside-root workspaces</Button>
        </div>
      </Alert.Description>
    </Alert.Root>
  {/if}

  <Card.Root class="mx-auto max-w-5xl">
    <Card.Header>
      <Card.Title class="flex items-center gap-2"><FolderOpen class="size-5" /> Browser</Card.Title>
      <Card.Description>Select a root and browse directories. Active workspaces stay pinned at the top of the browser.</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-4">
      <div class="grid gap-3 md:grid-cols-[220px_1fr_auto] md:items-end">
        <div class="space-y-2">
          <Label for="workspace-root">Root</Label>
          <select id="workspace-root" bind:value={rootId} onchange={() => void openPath('')} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
            {#each $workspaceRoots as root}
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
        {#if selectedRoot}
          <p>Root state: {selectedRoot.state} · {selectedRoot.canonical_path ?? 'virtual root'}</p>
        {/if}
        {#if $workspacesLoading}
          <p>Loading active workspaces…</p>
        {:else}
          <p>{$workspaces.length} active workspace{$workspaces.length === 1 ? '' : 's'}</p>
        {/if}
      </div>

      {#if browserLoading}
        <div class="space-y-2"><Skeleton class="h-9 w-full" /><Skeleton class="h-9 w-full" /><Skeleton class="h-9 w-full" /></div>
      {:else if listing}
        <div class="rounded-lg border">
          <div class="flex flex-wrap items-center justify-between gap-2 border-b p-3 text-sm">
            <span class="font-medium">{listing.canonical_path}</span>
            {#if listing.parent_path !== null}
              <Button size="icon-sm" variant="ghost" aria-label="Open parent directory" title="Open parent directory" onclick={() => void openPath(listing?.parent_path ?? '')}>
                <CornerUpLeft class="size-4" />
              </Button>
            {/if}
          </div>
          {#if listing.warnings.length}
            <div class="border-b bg-muted/40 p-3 text-xs text-muted-foreground">{listing.warnings.join(' · ')}</div>
          {/if}
          <div class="max-h-[32rem] overflow-auto">
            <Table.Root>
              <Table.Header><Table.Row><Table.Head>Directory</Table.Head><Table.Head class="text-right">Action</Table.Head></Table.Row></Table.Header>
              <Table.Body>
                {#each [...listing.entries].sort((left, right) => Number(right.is_workspace) - Number(left.is_workspace) || left.name.localeCompare(right.name)) as entry}
                  {@const entryWorkspace = workspaceForEntry(entry)}
                  <Table.Row>
                    <Table.Cell class="font-medium">
                      <button
                        type="button"
                        class="flex min-w-0 cursor-pointer items-center gap-2 text-left hover:underline"
                        aria-label="Open directory {entry.name}"
                        title="Open directory"
                        onclick={() => void openPath(entry.path)}
                      >
                        {#if entry.is_workspace}
                          <FolderBookmark class="size-4 shrink-0 text-foreground/80" aria-hidden="true" />
                        {:else}
                          <Folder class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                        {/if}
                        <span class="truncate">{entry.name}/</span>
                      </button>
                    </Table.Cell>
                    <Table.Cell class="text-right">
                      <div class="flex justify-end gap-2">
                        {#if entryWorkspace}
                          <Button
                            size="icon-sm"
                            variant="outline"
                            aria-label={`Rename ${entryWorkspace.name ?? entry.name}`}
                            title="Rename workspace"
                            onclick={() => startRenamingWorkspace(entryWorkspace)}
                          >
                            <Pencil class="size-4" />
                          </Button>
                        {/if}
                        <Button
                          size="sm"
                          variant={entry.is_workspace ? 'secondary' : 'outline'}
                          aria-label={entry.is_workspace ? `Deactivate ${entry.name}` : `Activate ${entry.name}`}
                          title={entry.is_workspace ? 'Remove workspace registration' : 'Register as workspace'}
                          onclick={() => void activateEntry(entry)}
                          disabled={registering || (!!entryWorkspace && deletingWorkspaceId === entryWorkspace.workspace_id)}
                        >
                          {entry.is_workspace ? 'Deactivate' : 'Activate'}
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
    </Card.Content>
  </Card.Root>
</section>

<Dialog.Root bind:open={outsideRootWorkspacesDialogOpen}>
  <Dialog.Content class="max-w-3xl">
    <Dialog.Header>
      <Dialog.Title>Outside-root active workspaces</Dialog.Title>
      <Dialog.Description>
        Revoke workspace registrations that are not covered by configured roots.
      </Dialog.Description>
    </Dialog.Header>
    <div class="mt-4 max-h-[28rem] overflow-auto rounded-lg border">
      <Table.Root>
        <Table.Header>
          <Table.Row>
            <Table.Head>Workspace</Table.Head>
            <Table.Head>Path</Table.Head>
            <Table.Head class="text-right">Action</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each outsideRootWorkspaces as workspace (workspace.workspace_id)}
            <Table.Row>
              <Table.Cell class="font-medium">{workspace.name ?? workspace.display_path}</Table.Cell>
              <Table.Cell class="max-w-[24rem] truncate text-muted-foreground" title={workspace.canonical_path}>{workspace.canonical_path}</Table.Cell>
              <Table.Cell class="text-right">
                <Button
                  size="sm"
                  variant="outline"
                  aria-label={`Revoke ${workspace.name ?? workspace.display_path}`}
                  onclick={() => void deleteRegisteredWorkspace(workspace.workspace_id)}
                  disabled={deletingWorkspaceId === workspace.workspace_id}
                >
                  Revoke
                </Button>
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    </div>
    <Dialog.Footer class="mt-5">
      <Button type="button" variant="outline" onclick={() => { outsideRootWorkspacesDialogOpen = false }}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={renameWorkspaceDialogOpen}>
  {#if renamingWorkspace}
    <Dialog.Content class="max-w-md">
      <form onsubmit={(event) => { event.preventDefault(); void confirmRenameWorkspace() }}>
        <Dialog.Header>
          <Dialog.Title>Confirm workspace rename</Dialog.Title>
          <Dialog.Description>
            Rename <span class="font-medium text-foreground">{renamingWorkspace.name ?? renamingWorkspace.display_path}</span>.
          </Dialog.Description>
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

