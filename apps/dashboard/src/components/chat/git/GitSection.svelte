<script lang="ts">
  import type { Snippet } from 'svelte'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'

  let {
    kind,
    title,
    count,
    description,
    open,
    disabled = false,
    scrollRoot,
    actions,
    children,
    onchange,
  }: {
    kind: 'changes' | 'history'
    title: string
    count?: number
    description?: string
    open: boolean
    disabled?: boolean
    scrollRoot?: HTMLElement
    actions?: Snippet
    children?: Snippet
    onchange: (open: boolean) => void
  } = $props()

  let heading = $state<HTMLElement>()

  function activate() {
    if (disabled || !heading || !scrollRoot) return
    const normalTop = heading.offsetTop
    const above = normalTop < scrollRoot.scrollTop
    const below = normalTop > scrollRoot.scrollTop + scrollRoot.clientHeight - heading.offsetHeight
    const stuck = kind === 'changes' ? scrollRoot.scrollTop > 0 : above || below
    if (stuck) scrollRoot.scrollTo({ top: Math.max(0, normalTop), behavior: 'smooth' })
    else onchange(!open)
  }

  function keydown(event: KeyboardEvent) {
    if (disabled) return
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      activate()
    } else if (event.key === 'ArrowLeft' && open) {
      event.preventDefault()
      onchange(false)
    } else if (event.key === 'ArrowRight' && !open) {
      event.preventDefault()
      onchange(true)
    }
  }
</script>

<section class={kind === 'history' ? 'min-h-0' : ''} data-git-section={kind}>
  <div
    bind:this={heading}
    data-git-row={`section:${kind}`}
    role="button"
    tabindex={disabled ? -1 : 0}
    aria-disabled={disabled}
    aria-expanded={disabled ? undefined : open}
    class={[
      'z-20 grid h-7 w-full grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 bg-surface px-4 text-left text-xs outline-none focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset max-lg:h-10 max-lg:text-13',
      kind === 'changes' ? 'sticky top-0' : 'sticky top-0 bottom-[-12px] border-t max-lg:bottom-[-24px]',
      !disabled && 'cursor-pointer hover:bg-muted',
    ]}
    onclick={activate}
    onkeydown={keydown}
  >
    {#if !disabled}<CaretDownIcon class={['size-3 transition-transform', !open && '-rotate-90']} />{:else}<span></span>{/if}
    <div class="min-w-0 truncate font-semibold text-foreground">
      {title}
      {#if count !== undefined}<span class="ml-1 font-normal text-muted-foreground tabular-nums">{count}</span>{/if}
      {#if description}<span class="ml-2 font-normal text-muted-foreground">{description}</span>{/if}
    </div>
    {#if actions}<div role="presentation" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()}>{@render actions()}</div>{/if}
  </div>
  {#if open && children}{@render children()}{/if}
</section>
