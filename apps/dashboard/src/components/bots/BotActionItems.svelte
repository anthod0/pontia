<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'

  type Action = {
    disabled?: boolean
    destructive?: boolean
    select: () => void
  }

  let {
    edit,
    reload,
    remove,
    reloading = false,
    item,
    separator,
  }: {
    edit: () => void
    reload: () => void
    remove: () => void
    reloading?: boolean
    item: Snippet<[Action, Snippet]>
    separator: Snippet
  } = $props()
</script>

{#snippet editContent()}<PencilSimpleIcon />{$t('bots.edit')}{/snippet}
{#snippet reloadContent()}<ArrowClockwiseIcon />{$t('bots.reload')}{/snippet}
{#snippet deleteContent()}<TrashIcon />{$t('bots.delete')}{/snippet}

{@render item({ select: edit }, editContent)}
{@render item({ select: reload, disabled: reloading }, reloadContent)}
{@render separator()}
{@render item({ select: remove, destructive: true }, deleteContent)}
