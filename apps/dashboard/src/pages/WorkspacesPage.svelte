<script lang="ts">
  import { CircleAlert } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import WorkspaceBrowser from '../components/workspaces/WorkspaceBrowser.svelte'
  import type { WorkspaceRootView, WorkspaceView } from '../api/types'
  import { browseWorkspaceRoot, deleteWorkspace, workspaceRoots, workspaces } from '../stores/workspaces'

  type WorkspaceAvailabilityProblem = {
    workspace: WorkspaceView
    reason: 'outside_root' | 'directory_unavailable'
  }

  type AvailableWorkspaceRoot = {
    root_id: string
    canonical_path: string
  }

  let unavailableWorkspacesDialogOpen = false
  let workspaceAvailabilityProblems: WorkspaceAvailabilityProblem[] = []
  let workspaceAvailabilityCheckKey = ''
  let workspaceAvailabilityCheckGeneration = 0
  let deletingWorkspaceId: string | null = null
  let deleteError: string | null = null

  $: availableWorkspaceRoots = activeWorkspaceRoots($workspaceRoots)
  $: nextWorkspaceAvailabilityCheckKey = JSON.stringify({
    roots: availableWorkspaceRoots.map((root) => [root.root_id, root.canonical_path]),
    workspaces: $workspaces.filter((workspace) => workspace.state === 'active').map((workspace) => [workspace.workspace_id, workspace.canonical_path]),
  })
  $: if (nextWorkspaceAvailabilityCheckKey !== workspaceAvailabilityCheckKey) {
    workspaceAvailabilityCheckKey = nextWorkspaceAvailabilityCheckKey
    void refreshWorkspaceAvailability($workspaces, availableWorkspaceRoots)
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

  function activeWorkspaceRoots(roots: WorkspaceRootView[]): AvailableWorkspaceRoot[] {
    return roots
      .filter((root) => root.state === 'available' && Boolean(root.canonical_path?.trim()))
      .map((root) => ({ root_id: root.root_id, canonical_path: normalizeAbsolutePath(root.canonical_path!) }))
  }

  function rootContainingPath(path: string, roots: AvailableWorkspaceRoot[]): AvailableWorkspaceRoot | null {
    return roots
      .filter((root) => isPathInsideRoot(path, root.canonical_path))
      .sort((left, right) => right.canonical_path.length - left.canonical_path.length)[0] ?? null
  }

  function relativePathInsideRoot(path: string, rootPath: string): string {
    const normalizedPath = normalizeAbsolutePath(path)
    const normalizedRoot = normalizeAbsolutePath(rootPath)
    if (normalizedPath === normalizedRoot) return ''
    return normalizedPath.slice(normalizedRoot.length + 1)
  }

  function workspaceProblemReasonLabel(reason: WorkspaceAvailabilityProblem['reason']): string {
    return reason === 'outside_root' ? 'Outside root' : 'Missing directory'
  }

  async function refreshWorkspaceAvailability(workspaceViews: WorkspaceView[], roots: AvailableWorkspaceRoot[]): Promise<void> {
    const generation = ++workspaceAvailabilityCheckGeneration
    const problems: WorkspaceAvailabilityProblem[] = []
    for (const workspace of workspaceViews.filter((item) => item.state === 'active')) {
      const root = rootContainingPath(workspace.canonical_path, roots)
      if (!root) {
        problems.push({ workspace, reason: 'outside_root' })
        continue
      }
      try {
        await browseWorkspaceRoot(root.root_id, relativePathInsideRoot(workspace.canonical_path, root.canonical_path))
      } catch (_) {
        problems.push({ workspace, reason: 'directory_unavailable' })
      }
    }
    if (generation === workspaceAvailabilityCheckGeneration) workspaceAvailabilityProblems = problems
  }

  async function deleteUnavailableWorkspace(workspaceId: string): Promise<void> {
    if (deletingWorkspaceId) return
    deletingWorkspaceId = workspaceId
    deleteError = null
    try {
      await deleteWorkspace(workspaceId)
    } catch (error) {
      deleteError = error instanceof Error ? error.message : String(error)
    } finally {
      deletingWorkspaceId = null
    }
  }
</script>

<section class="space-y-6">
  <div class="space-y-2">
    <h2 class="text-3xl font-semibold tracking-tight">Workspaces</h2>
    <p class="max-w-3xl text-muted-foreground">Browse configured roots and register execution workspaces through the External API.</p>
  </div>

  {#if deleteError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Workspace error</Alert.Title>
      <Alert.Description>{deleteError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if workspaceAvailabilityProblems.length}
    <Alert.Root>
      <CircleAlert class="size-4" />
      <Alert.Title>{workspaceAvailabilityProblems.length} unavailable active workspace{workspaceAvailabilityProblems.length === 1 ? '' : 's'}</Alert.Title>
      <Alert.Description>
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <span>Some active workspaces are outside roots or point to missing directories.</span>
          <Button size="sm" variant="outline" onclick={() => { unavailableWorkspacesDialogOpen = true }}>Review</Button>
        </div>
      </Alert.Description>
    </Alert.Root>
  {/if}

  <WorkspaceBrowser />
</section>

<Dialog.Root bind:open={unavailableWorkspacesDialogOpen}>
  <Dialog.Content class="max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Unavailable active workspaces</Dialog.Title>
      <Dialog.Description>Revoke workspace registrations that are not reachable from configured roots.</Dialog.Description>
    </Dialog.Header>
    <div class="mt-4 max-h-[28rem] space-y-2 overflow-auto pr-1">
      {#each workspaceAvailabilityProblems as problem (problem.workspace.workspace_id)}
        {@const workspace = problem.workspace}
        <div class="flex items-center justify-between gap-3 rounded-lg border bg-card p-3">
          <div class="min-w-0 space-y-1">
            <div class="flex min-w-0 flex-wrap items-center gap-2">
              <span class="truncate font-medium">{workspace.name ?? workspace.display_path}</span>
              <Badge variant="secondary">{workspaceProblemReasonLabel(problem.reason)}</Badge>
            </div>
            <p class="truncate text-xs text-muted-foreground" title={workspace.canonical_path}>{workspace.canonical_path}</p>
          </div>
          <Button
            size="sm"
            variant="outline"
            aria-label={`Revoke ${workspace.name ?? workspace.display_path}`}
            onclick={() => void deleteUnavailableWorkspace(workspace.workspace_id)}
            disabled={deletingWorkspaceId === workspace.workspace_id}
          >
            Revoke
          </Button>
        </div>
      {/each}
    </div>
    <Dialog.Footer class="mt-5">
      <Button type="button" variant="outline" onclick={() => { unavailableWorkspacesDialogOpen = false }}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
