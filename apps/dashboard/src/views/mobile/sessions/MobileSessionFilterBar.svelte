<script lang="ts">
  import { t } from '$lib/i18n'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import type { SessionScope } from '../../../features/sessions/sessionList.svelte'
  let {
    scope,
    select,
    workspace,
    open,
  }: { scope: SessionScope; select: (s: SessionScope) => void; workspace: string; open: () => void } = $props()
  const filters = ['active', 'all', 'pinned', 'archived'] as const
  const chip =
    'mobile-glass flex min-h-11 shrink-0 items-center gap-1 rounded-full px-3 text-sm aria-pressed:bg-selected aria-pressed:text-primary'
</script>

<div class="sticky top-0 z-10 flex gap-1.5 overflow-x-auto py-1">
  {#each filters as f (f)}<button class={chip} aria-pressed={scope === f} onclick={() => select(f)}
      >{$t(`common.${f}`)}</button
    >{/each}
  <button class={chip} aria-pressed={!!workspace} onclick={open}
    ><FolderIcon />{workspace || $t('workspaces.all')}</button
  >
</div>
