<script lang="ts">
  import { onMount } from 'svelte'
  import { CheckCircle2, Circle, CircleAlert, CornerUpLeft, FolderOpen, Pencil, RefreshCw, Trash2 } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { formatDateTime } from '../components/tasks/format'
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
  let savingRename = false
  let registerEntry: WorkspaceDirectoryEntryView | null = null
  let registerWorkspaceName = ''

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

  async function activateEntry(entry: WorkspaceDirectoryEntryView): Promise<void> {
    if (entry.is_workspace) {
      const workspace = workspaceForEntry(entry)
      if (!workspace) {
        deleteError = `Could not find registered workspace for ${entry.name}. Refresh and try again.`
        return
      }
      await deleteRegisteredWorkspace(workspace.workspace_id, workspace.name ?? workspace.display_path)
      return
    }
    registerError = null
    registerEntry = entry
    registerWorkspaceName = entry.name
  }

  async function confirmActivateEntry(): Promise<void> {
    if (!registerEntry || !rootId || registering) return
    registering = true
    registerError = null
    try {
      await registerWorkspace({ root_id: rootId, path: registerEntry.path, name: registerWorkspaceName.trim() || null })
      registerEntry = null
      registerWorkspaceName = ''
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
  }

  async function confirmRenameWorkspace(): Promise<void> {
    if (!renamingWorkspace || savingRename) return
    savingRename = true
    renameError = null
    try {
      await renameWorkspace(renamingWorkspace.workspace_id, { name: renamingWorkspaceName.trim() || null })
      renamingWorkspace = null
      renamingWorkspaceName = ''
      if (rootId) await openPath(browsePath)
    } catch (error) {
      renameError = error instanceof Error ? error.message : String(error)
    } finally {
      savingRename = false
    }
  }

  async function deleteRegisteredWorkspace(workspaceId: string, label: string): Promise<void> {
    if (deletingWorkspaceId || !confirm(`Delete workspace "${label}" from pilotfy? Files on disk will not be deleted.`)) return
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

  <div class="grid gap-6 xl:grid-cols-[minmax(0,1.1fr)_minmax(0,0.9fr)] xl:items-start">
  <div class="xl:order-2">
  <Card.Root>
    <Card.Header><Card.Title>Active workspaces</Card.Title><Card.Description>{$workspaces.length} available for DAG task creation.</Card.Description></Card.Header>
    <Card.Content>
      {#if $workspacesLoading}
        <div class="grid gap-3 md:grid-cols-2"><Skeleton class="h-20 w-full" /><Skeleton class="h-20 w-full" /></div>
      {:else if !$workspaces.length}
        <Empty.Root><Empty.Header><Empty.Title>No workspaces</Empty.Title><Empty.Description>Use Active in the browser below to register a directory.</Empty.Description></Empty.Header></Empty.Root>
      {:else}
        <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-1 2xl:grid-cols-2">
          {#each $workspaces as workspace}
            {@const workspaceLabel = workspace.name ?? workspace.display_path}
            <div class="rounded-xl border bg-card p-3 text-sm shadow-sm transition hover:-translate-y-0.5 hover:border-primary/20 hover:shadow-md">
              <div class="flex items-start gap-3">
                <div class="workspace-folder-preview" aria-hidden="true">
                  <div class="workspace-folder-tab"></div>
                  <div class="workspace-folder-body">
                    <FolderOpen class="size-5" />
                  </div>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="font-medium">{workspaceLabel}</div>
                  <div class="truncate text-muted-foreground" title={workspace.canonical_path}>{workspace.canonical_path}</div>
                  <div class="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground"><Badge variant="secondary">{workspace.state}</Badge><span>Updated {formatDateTime(workspace.updated_at)}</span></div>
                </div>
                <div class="flex shrink-0 gap-2">
                  <Button
                    size="icon-sm"
                    variant="outline"
                    aria-label={`Rename ${workspaceLabel}`}
                    title="Rename workspace"
                    onclick={() => startRenamingWorkspace(workspace)}
                  >
                    <Pencil class="size-4" />
                  </Button>
                  <Button
                    size="icon-sm"
                    variant="outline"
                    aria-label={deletingWorkspaceId === workspace.workspace_id ? `Deleting ${workspaceLabel}` : `Delete ${workspaceLabel}`}
                    title={deletingWorkspaceId === workspace.workspace_id ? 'Deleting…' : 'Delete workspace'}
                    onclick={() => void deleteRegisteredWorkspace(workspace.workspace_id, workspaceLabel)}
                    disabled={deletingWorkspaceId === workspace.workspace_id}
                  >
                    <Trash2 class="size-4" />
                  </Button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
  </div>

  <div class="xl:order-1">
  <Card.Root>
    <Card.Header>
      <Card.Title class="flex items-center gap-2"><FolderOpen class="size-5" /> Root browser</Card.Title>
      <Card.Description>Select a root, browse directories, then use Active to register or remove workspaces.</Card.Description>
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

      {#if selectedRoot}
        <p class="text-xs text-muted-foreground">Root state: {selectedRoot.state} · {selectedRoot.canonical_path ?? 'virtual root'}</p>
      {/if}

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
          <div class="max-h-[28rem] overflow-auto">
            <Table.Root>
              <Table.Header><Table.Row><Table.Head>Directory</Table.Head><Table.Head class="text-right">Action</Table.Head></Table.Row></Table.Header>
              <Table.Body>
                {#each listing.entries as entry}
                  <Table.Row>
                    <Table.Cell class="font-medium">
                      <button
                        type="button"
                        class="flex min-w-0 cursor-pointer items-center gap-2 text-left hover:underline"
                        aria-label="Open directory {entry.name}"
                        title="Open directory"
                        onclick={() => void openPath(entry.path)}
                      >
                        <FolderOpen class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                        <span class="truncate">{entry.name}/</span>
                      </button>
                    </Table.Cell>
                    <Table.Cell class="text-right">
                      <Button size="sm" variant={entry.is_workspace ? 'secondary' : 'outline'} aria-label={entry.is_workspace ? `Deactivate ${entry.name}` : `Activate ${entry.name}`} title={entry.is_workspace ? 'Delete workspace registration' : 'Register as workspace'} onclick={() => void activateEntry(entry)}>
                        Active
                        {#if entry.is_workspace}
                          <CheckCircle2 class="size-4 text-primary" />
                        {:else}
                          <Circle class="size-4 text-muted-foreground" />
                        {/if}
                      </Button>
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
  </div>
  </div>
</section>

{#if renamingWorkspace}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm" role="presentation">
    <form class="w-full max-w-md rounded-xl border bg-card p-5 shadow-xl" onsubmit={(event) => { event.preventDefault(); void confirmRenameWorkspace() }}>
      <div class="space-y-2">
        <h3 class="text-lg font-semibold">Confirm workspace rename</h3>
        <p class="text-sm text-muted-foreground">Rename <span class="font-medium text-foreground">{renamingWorkspace.name ?? renamingWorkspace.display_path}</span>.</p>
      </div>
      <div class="mt-4 space-y-2">
        <Label for="rename-workspace-name">Display name</Label>
        <Input id="rename-workspace-name" bind:value={renamingWorkspaceName} placeholder={renamingWorkspace.display_path} />
        <p class="text-xs text-muted-foreground">Clear the name to display the workspace path.</p>
      </div>
      <div class="mt-5 flex justify-end gap-2">
        <Button type="button" variant="outline" onclick={() => { renamingWorkspace = null; renamingWorkspaceName = '' }} disabled={savingRename}>Cancel</Button>
        <Button type="submit" disabled={savingRename}>{savingRename ? 'Saving…' : 'Rename workspace'}</Button>
      </div>
    </form>
  </div>
{/if}

{#if registerEntry}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm" role="presentation">
    <form class="w-full max-w-md rounded-xl border bg-card p-5 shadow-xl" onsubmit={(event) => { event.preventDefault(); void confirmActivateEntry() }}>
      <div class="space-y-2">
        <h3 class="text-lg font-semibold">Confirm workspace registration</h3>
        <p class="text-sm text-muted-foreground">Register <span class="font-medium text-foreground">{registerEntry.name}</span> as an execution workspace.</p>
      </div>
      <div class="mt-4 space-y-2">
        <Label for="register-entry-name">Display name</Label>
        <Input id="register-entry-name" bind:value={registerWorkspaceName} placeholder={registerEntry.name} />
        <p class="text-xs text-muted-foreground">Edit the name, or clear it to keep the default registration behavior.</p>
      </div>
      <div class="mt-5 flex justify-end gap-2">
        <Button type="button" variant="outline" onclick={() => { registerEntry = null; registerWorkspaceName = '' }} disabled={registering}>Cancel</Button>
        <Button type="submit" disabled={registering}>{registering ? 'Registering…' : 'Register workspace'}</Button>
      </div>
    </form>
  </div>
{/if}

<style>
  .workspace-folder-preview {
    position: relative;
    width: 4.25rem;
    height: 3.25rem;
    flex: none;
    filter: drop-shadow(0 0.45rem 0.6rem oklch(0 0 0 / 12%));
  }

  .workspace-folder-tab {
    position: absolute;
    left: 0.35rem;
    top: 0.15rem;
    width: 2rem;
    height: 0.9rem;
    border: 1px solid color-mix(in oklch, var(--border), var(--foreground) 6%);
    border-bottom: 0;
    border-radius: 0.5rem 0.5rem 0 0;
    background: linear-gradient(135deg, color-mix(in oklch, var(--muted), var(--primary) 14%), var(--muted));
  }

  .workspace-folder-body {
    position: absolute;
    inset: 0.8rem 0 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border: 1px solid color-mix(in oklch, var(--border), var(--foreground) 8%);
    border-radius: 0.55rem 0.75rem 0.75rem 0.75rem;
    background:
      radial-gradient(circle at 78% 18%, color-mix(in oklch, var(--primary), transparent 82%), transparent 35%),
      linear-gradient(145deg, color-mix(in oklch, var(--muted), var(--background) 14%), color-mix(in oklch, var(--muted), var(--primary) 10%));
    color: color-mix(in oklch, var(--muted-foreground), var(--foreground) 18%);
  }

  .workspace-folder-body::after {
    position: absolute;
    right: -0.6rem;
    bottom: -0.45rem;
    width: 3.1rem;
    height: 1.45rem;
    content: '';
    border-radius: 999px;
    background: color-mix(in oklch, var(--background), transparent 62%);
    transform: rotate(-16deg);
  }

  .workspace-folder-body :global(svg) {
    position: relative;
    z-index: 1;
  }
</style>
