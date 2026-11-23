<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { useTopBarAction } from '../../layout/topBarAction'
  import { useDeviceRuntime } from '../../../device/context'
  import type { WorkspaceGitStatusView } from '../../../api/types'
  import { useGitPanel } from './gitPanelContext.svelte'
  import { changeCountLabel, gitStatusChangeCount } from './gitChanges'

  /** Connects the session page to the Git panel: shows its Workspace there and adds the top bar button. */
  let { workspaceId, gitStatus }: { workspaceId: string; gitStatus: WorkspaceGitStatusView | undefined } = $props()

  const device = useDeviceRuntime()
  const panel = useGitPanel()
  const action = useTopBarAction()
  const count = $derived(gitStatusChangeCount(gitStatus))
  const conflicted = $derived((gitStatus?.conflicted_count ?? 0) > 0)
  const notRepository = $derived(gitStatus?.state === 'error' && !gitStatus.repo_root)
  const pressed = $derived(!!panel?.open && panel.source?.workspaceId === workspaceId)

  $effect(() => panel?.attach({ device, workspaceId }))

  onMount(() => {
    action?.set(gitButton)
    return () => action?.set(null)
  })
</script>

{#snippet gitButton()}
  <div class="pointer-events-auto mr-3 flex h-12 shrink-0 items-center rounded-lg bg-popover px-0.5 ring-1 ring-foreground/10 max-md:mt-2 md:h-8 md:px-1">
    <Button variant="ghost" size="icon" class={['relative size-11 md:size-7', pressed && 'bg-muted text-heading', notRepository && 'text-muted-foreground/60']}
      aria-label={count ? $t('git.panelWithCount', { count }) : $t('git.panel')} title={`${$t('git.panel')} (Alt G)`} aria-pressed={pressed}
      onclick={() => panel?.toggle()}>
      <GitBranchIcon class="size-5 md:size-4" />
      {#if count > 0}
        <Badge data-testid="git-change-count" class={['pointer-events-none absolute top-0.5 right-0.5 h-4 min-w-4 rounded-full px-1 text-[0.625rem] tabular-nums md:-top-1 md:-right-1', conflicted && 'bg-destructive text-primary-foreground']}>{changeCountLabel(count)}</Badge>
      {/if}
    </Button>
  </div>
{/snippet}
