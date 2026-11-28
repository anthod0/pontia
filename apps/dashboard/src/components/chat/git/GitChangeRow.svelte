<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '$lib/i18n'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import MinusIcon from 'phosphor-svelte/lib/MinusIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import type { GitChangedFile } from '../../../api/types'
  import GitActionIcon from './GitActionIcon.svelte'
  import GitFileRow from './GitFileRow.svelte'
  import { changeKey } from './gitChanges'

  let { file, label, selected, touch, busy, expanded, expand, onopen, onsheet, onstage, ondiscard }: {
    file: GitChangedFile; label: string; selected: boolean; touch: boolean; busy: boolean
    expanded: boolean; expand: (open: boolean) => void; onopen: () => void; onsheet: () => void; onstage: () => void; ondiscard: () => void
  } = $props()
  const staged = $derived(file.group === 'staged')
  const action = $derived(staged ? 'unstage' : 'stage')
  const discardable = $derived(!staged && !file.path.endsWith('/'))
  const width = $derived(discardable ? 136 : 68)
  const enabled = $derived(!busy && file.group !== 'conflicted')
  let row: HTMLLIElement
  let drag = $state<number | null>(null)
  const offset = $derived(touch && enabled ? drag ?? (expanded ? width : 0) : 0)
  let start: { x: number; y: number; offset: number } | null = null
  let timer: ReturnType<typeof setTimeout>
  let suppressClick = false

  function cancel() {
    clearTimeout(timer)
    start = null
    drag = null
  }
  onDestroy(cancel)
  function down(e: PointerEvent) {
    if (!touch || !enabled || !e.isPrimary || e.button !== 0) return
    suppressClick = false
    start = { x: e.clientX, y: e.clientY, offset }
    timer = setTimeout(() => { cancel(); suppressClick = true; expand(false); onsheet() }, 500)
  }
  function move(e: PointerEvent) {
    if (!start) return
    const dx = start.x - e.clientX
    const dy = e.clientY - start.y
    if (drag === null) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < 6) return
      clearTimeout(timer)
      if (Math.abs(dy) >= Math.abs(dx)) { cancel(); return }
      suppressClick = true
      const target = e.currentTarget as HTMLElement
      target.setPointerCapture(e.pointerId)
      expand(false)
    }
    drag = Math.min(width, Math.max(0, start.offset + dx))
  }
  function up(e: PointerEvent) {
    if (start && drag !== null && enabled) {
      if (start.x - e.clientX > row.clientWidth * 0.6) { expand(false); onstage() }
      else expand(drag > width / 2)
    }
    cancel()
  }
  function open() {
    if (suppressClick) { suppressClick = false; return }
    if (expanded) expand(false)
    else onopen()
  }
  $effect(() => {
    if (!touch || !enabled) return
    const button = row.querySelector('[data-git-row]')!
    button.setAttribute('aria-haspopup', 'dialog')
    return () => button.removeAttribute('aria-haspopup')
  })
  $effect(() => {
    if (!expanded) return
    const outside = (e: Event) => { if (!row.contains(e.target as Node)) expand(false) }
    const close = () => expand(false)
    document.addEventListener('pointerdown', outside, true)
    document.addEventListener('scroll', close, true)
    return () => { document.removeEventListener('pointerdown', outside, true); document.removeEventListener('scroll', close, true) }
  })
</script>
<li bind:this={row} class={['group/row relative', touch && 'overflow-hidden rounded-item']}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class={['touch-pan-y', offset && 'bg-muted', touch && 'select-none']} style:clip-path={offset ? `inset(0 ${offset}px 0 0)` : undefined}
    onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={cancel}
    oncontextmenu={(e) => { if (touch && enabled) { e.preventDefault(); cancel(); suppressClick = true; onsheet() } }}>
    <GitFileRow {file} rowKey={changeKey(file)} {label} {selected} onopen={open} />
  </div>
  {#if enabled && !touch}
    <div class="absolute top-[3px] right-1 hidden gap-0.5 rounded-full bg-muted pl-2 group-focus-within/row:flex group-hover/row:flex">
      {#if discardable}<GitActionIcon action="discard" label={$t('git.discard')} path={file.path} onclick={ondiscard} />{/if}
      <GitActionIcon {action} label={$t(`git.${action}`)} path={file.path} onclick={onstage} />
    </div>
  {:else if enabled}
    <button class="sr-only focus-visible:not-sr-only" aria-haspopup="dialog" aria-label={$t('git.fileActions', { path: file.path })} onclick={onsheet}>{$t('git.moreActions')}</button>
    {#if offset}
      <div class="absolute inset-y-0 right-0 flex overflow-hidden" style:width={`${offset}px`}>
        {#if discardable}<button class="flex w-[68px] shrink-0 flex-col items-center justify-center bg-destructive text-xs font-medium text-white" onclick={() => { expand(false); ondiscard() }}><ArrowCounterClockwiseIcon class="size-[18px]" />{$t('git.discardShort')}</button>{/if}
        <button class="flex w-[68px] shrink-0 flex-col items-center justify-center bg-primary-solid text-xs font-medium text-primary-foreground" onclick={() => { expand(false); onstage() }}>
          {#if staged}<MinusIcon class="size-[18px]" />{:else}<PlusIcon class="size-[18px]" />{/if}{$t(`git.${action}`)}
        </button>
      </div>
    {/if}
  {/if}
</li>
