<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { untrack } from 'svelte'
  import { t, currentLocale } from '$lib/i18n'
  import { formatRelativeTime } from '$lib/format'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import type { DeviceRuntime } from '../../../device/runtime'
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import GitHistory from '../../../components/chat/git/GitHistory.svelte'
  import GitChangeList from '../../../components/chat/git/GitChangeList.svelte'
  import MobileGitCommitScreen from './MobileGitCommitScreen.svelte'
  import MobileGitDiffScreen from './MobileGitDiffScreen.svelte'
  let { device, workspaceId }: { device: DeviceRuntime; workspaceId: string } = $props()
  const queries = untrack(() => device.workspaceQueries)
  const changes = queries.createWorkspaceGitChangesQuery(
    () => workspaceId,
    () => true
  )
  const statuses = queries.createWorkspaceGitStatusesStore()
  const status = $derived($statuses[workspaceId])
  const git = useSessionPanel()!.git
  const stack = useMobileStack()!
  let history = $state<GitHistory>()
  function refresh() {
    void changes.refetch()
    void queries.refreshWorkspaceGitStatus(workspaceId).catch(error => toast.error(error.message))
    history?.refresh()
  }
</script>

{#snippet diff()}{#if git.diff}<MobileGitDiffScreen {device} {workspaceId} target={{ file: git.diff }} />{/if}{/snippet}
{#snippet commit()}{#if git.commit}<MobileGitCommitScreen {device} {workspaceId} sha={git.commit} />{/if}{/snippet}
<div class="flex h-full min-w-0 flex-col">
  <div class="flex min-h-11 items-center gap-2 text-xs">
    <span class="truncate font-mono"
      >{status?.branch ?? $t('git.noBranch')}{#if status?.ahead}
        ↑{status.ahead}{/if}</span
    >
    {#if changes.data}<span>{$t('git.changeCount', { count: changes.data.total })}</span><time
        class="text-muted-foreground"
        >{formatRelativeTime(new Date(changes.dataUpdatedAt).toISOString(), $currentLocale)}</time
      >{/if}
    <button class="mobile-icon ml-auto" aria-label={$t('common.refresh')} onclick={refresh}
      ><ArrowClockwiseIcon /></button
    >
  </div>
  <div class="my-2 flex rounded-full bg-muted p-1">
    {#each ['changes', 'history'] as const as tab}<button
        class="min-h-11 flex-1 rounded-full aria-pressed:bg-card"
        aria-pressed={git.view === tab}
        onclick={() => (git.view = tab)}>{$t(tab === 'changes' ? 'git.changesView' : 'git.historyView')}</button
      >{/each}
  </div>
  {#if git.view === 'history'}<GitHistory
      bind:this={history}
      {device}
      {workspaceId}
      {git}
      oncommit={(next) => {
        git.openCommit(next)
        stack.open({
          dismiss: () => (git.commit = null),
          title: $t('git.commitTitle', { sha: next.slice(0, 7) }),
          content: commit,
        })
      }}
    />
  {:else if changes.isError}<p role="alert">{changes.error.message}</p>
  {:else if !changes.data}<p role="status">{$t('common.loading')}</p>
  {:else if changes.data.files.length}<div class="min-h-0 overflow-y-auto">
      <GitChangeList
        files={changes.data.files}
        onopen={(next) => {
          git.diff = next
          stack.open({ dismiss: () => (git.diff = null), title: next.path, content: diff })
        }}
      />{#if changes.data.truncated}<p>
          {$t('git.listTruncated', { shown: changes.data.files.length, total: changes.data.total })}
        </p>{/if}
    </div>
  {:else}<p class="p-4 text-center">{$t(changes.data.repo_root ? 'git.noChanges' : 'git.notRepository')}</p>{/if}
</div>
