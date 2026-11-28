<script lang="ts">
  import * as Menu from '$lib/components/ui/dropdown-menu'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { buttonVariants } from '$lib/components/ui/button'
  import { t } from '$lib/i18n'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import ArrowDownIcon from 'phosphor-svelte/lib/ArrowDownIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import MinusIcon from 'phosphor-svelte/lib/MinusIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'

  let { touch, branch, upstream, ahead, behind, staged, unstaged, discardable, busy, onaction }: {
    touch: boolean; branch: string; upstream?: string | null; ahead: number; behind: number
    staged: boolean; unstaged: boolean; discardable: boolean; busy: boolean
    onaction: (action: 'pull' | 'push' | 'stageAll' | 'unstageAll' | 'discardAll') => void
  } = $props()
  let open = $state(false)
  const items = $derived([
    { key: 'pull', icon: ArrowDownIcon, disabled: !upstream, count: behind && `↓${behind}` },
    { key: 'push', icon: ArrowUpIcon, disabled: false, count: ahead && `↑${ahead}` },
    { key: 'stageAll', icon: PlusIcon, disabled: !unstaged },
    { key: 'unstageAll', icon: MinusIcon, disabled: !staged },
    { key: 'discardAll', icon: ArrowCounterClockwiseIcon, disabled: !discardable },
  ] as const)
  const trigger = buttonVariants({ variant: 'ghost', size: 'icon-circle-sm', class: '-mr-1.5 size-7 max-lg:size-10 touch-target:size-10 touch-target:min-h-0! touch-target:min-w-0!' })
</script>

{#snippet label(item: typeof items[number])}
  <item.icon class="size-4 touch-target:size-5" />{$t(`git.${item.key}`)}
  {#if 'count' in item && item.count}<span class="ml-auto font-mono text-xs text-muted-foreground">{item.count}</span>{/if}
{/snippet}
{#if touch}
  <Sheet.Root bind:open>
    <Sheet.Trigger class={trigger} aria-label={$t('git.moreActions')} disabled={busy}><DotsThreeIcon class="size-[22px]" /></Sheet.Trigger>
    <BottomSheet dismiss={() => open = false}>
      <Sheet.Title>{branch}</Sheet.Title><Sheet.Description class="mb-2 font-mono text-xs">{branch}{upstream ? ` → ${upstream}` : ''}</Sheet.Description>
      {#each items as item, i (item.key)}
        {#if i === 2 || i === 4}<hr class="my-1" />{/if}
        <button class={['flex h-12 w-full items-center gap-3 px-2 text-base disabled:opacity-50', item.key === 'discardAll' && 'text-destructive']} disabled={busy || item.disabled} onclick={() => { open = false; onaction(item.key) }}>
          {@render label(item)}
        </button>
      {/each}
    </BottomSheet>
  </Sheet.Root>
{:else}
  <Menu.Root>
    <Menu.Trigger class={trigger} aria-label={$t('git.moreActions')} disabled={busy}><DotsThreeIcon class="size-[18px]" /></Menu.Trigger>
    <Menu.Content align="end" class="min-w-44">
      {#each items as item, i (item.key)}
        {#if i === 2 || i === 4}<Menu.Separator />{/if}
        <Menu.Item disabled={busy || item.disabled} variant={item.key === 'discardAll' ? 'destructive' : 'default'} onSelect={() => onaction(item.key)}>{@render label(item)}</Menu.Item>
      {/each}
    </Menu.Content>
  </Menu.Root>
{/if}
