<script lang="ts">
  import type { WorkspaceGitStatusView } from '../../api/types'
  import { gitBranchLabel, hasGitChangeCounts } from './sessionMetadata'

  interface Props {
    gitStatus: WorkspaceGitStatusView
  }

  let { gitStatus }: Props = $props()
</script>

<span>{gitBranchLabel(gitStatus)}</span>
{#if gitStatus.ahead}<span class="text-blue-600 dark:text-blue-400">↑{gitStatus.ahead}</span>{/if}
{#if gitStatus.behind}<span class="text-violet-600 dark:text-violet-400">↓{gitStatus.behind}</span>{/if}
{#if hasGitChangeCounts(gitStatus)}
  {#if gitStatus.staged_count}<span class="text-emerald-600 dark:text-emerald-400">+{gitStatus.staged_count}</span>{/if}
  {#if gitStatus.unstaged_count}<span class="text-amber-600 dark:text-amber-400">~{gitStatus.unstaged_count}</span>{/if}
  {#if gitStatus.untracked_count}<span class="text-cyan-600 dark:text-cyan-400">?{gitStatus.untracked_count}</span>{/if}
  {#if gitStatus.conflicted_count}<span class="text-destructive">!{gitStatus.conflicted_count}</span>{/if}
{/if}
