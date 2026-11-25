<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { t } from '$lib/i18n'
  import type { GitBranch } from '../../../api/types'

  let { branches, head, ref, onchoose }: { branches: GitBranch[]; head: string | null; ref: string | null; onchoose: (ref: string) => void } = $props()
  const local = $derived(branches.filter(branch => !branch.remote))
  const remote = $derived(branches.filter(branch => branch.remote))
  const options = $derived([
    ...(head === 'HEAD' ? [{ ref: 'HEAD', name: $t('git.detachedHead'), current: true, remote: false }] : []),
    ...local, ...remote,
  ])
  const selected = $derived(branches.find(branch => branch.ref === ref))
  const branchLabel = $derived(selected?.name ?? (ref === 'HEAD' ? $t('git.detachedHead') : $t('git.noBranch')))
</script>

<ResponsiveSelect value={ref ?? undefined} onValueChange={onchoose}
  label={$t('git.branchToShow', { branch: branchLabel })} title={$t('git.branchPickerHint')} triggerClass="h-6 min-w-0 max-w-full rounded-full border-0 bg-muted px-2 py-0 shadow-none data-[size=default]:h-6"
  align="start" contentClass="max-h-80 w-72" itemClass="py-1.5" {options} keyOf={branch => branch.ref} labelOf={branch => branch.name}
  groupOf={branch => branch.ref === 'HEAD' ? '' : $t(branch.remote ? 'git.remoteBranches' : 'git.localBranches')}>
  {#snippet trigger()}
    <GitBranchIcon class="size-4 text-muted-foreground" />
    <span class="min-w-0 flex-1 truncate text-left font-mono text-[13px]">{branchLabel}</span>
    {#if ref === head}<span class="shrink-0 text-xs text-muted-foreground">{$t('common.current')}</span>{/if}
  {/snippet}
  {#snippet option(branch)}
    <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{branch.name}</span>
    {#if branch.current}<Badge variant="outline" class="ml-2 shrink-0 font-normal">{$t('common.current')}</Badge>{/if}
  {/snippet}
</ResponsiveSelect>
