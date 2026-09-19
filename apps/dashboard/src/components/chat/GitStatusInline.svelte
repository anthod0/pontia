<script lang="ts">
  import type { WorkspaceGitStatusView } from '../../api/types'
  import { gitBranchLabel, hasGitChangeCounts } from './sessionMetadata'

  interface Props {
    gitStatus: WorkspaceGitStatusView
  }

  let { gitStatus }: Props = $props()
</script>

<span>{gitBranchLabel(gitStatus)}</span>
{#if gitStatus.ahead}<span class="text-primary ">↑{gitStatus.ahead}</span>{/if}
{#if gitStatus.behind}<span class="text-interrupted ">↓{gitStatus.behind}</span>{/if}
{#if hasGitChangeCounts(gitStatus)}
  {#if gitStatus.staged_count}<span class="text-success ">+{gitStatus.staged_count}</span>{/if}
  {#if gitStatus.unstaged_count}<span class="text-warning ">~{gitStatus.unstaged_count}</span>{/if}
  {#if gitStatus.untracked_count}<span class="text-aqua ">?{gitStatus.untracked_count}</span>{/if}
  {#if gitStatus.conflicted_count}<span class="text-destructive">!{gitStatus.conflicted_count}</span>{/if}
{/if}
