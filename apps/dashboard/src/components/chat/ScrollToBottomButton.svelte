<script lang="ts">
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { t } from '$lib/i18n'
  import { cn } from '$lib/utils.js'

  const ANIMATION_MS = 200

  let { sentinel, root = null, onclick, onpinnedchange, class: className }: {
    /** Shown while this element is outside the scroll viewport. */
    sentinel: HTMLElement | null | undefined
    /** The scroll container, or null for the document viewport. */
    root?: Element | null
    onclick: () => void
    onpinnedchange?: (pinned: boolean) => void
    class?: string
  } = $props()

  let visible = $state(false)
  let rendered = $state(false)
  let hideTimer: ReturnType<typeof setTimeout> | undefined

  function setVisible(next: boolean) {
    clearTimeout(hideTimer)
    visible = next
    onpinnedchange?.(!next)
    if (next) {
      rendered = true
      return
    }
    hideTimer = setTimeout(() => {
      if (!visible) rendered = false
    }, ANIMATION_MS)
  }

  export function hide() {
    setVisible(false)
  }

  $effect(() => {
    if (!sentinel) return
    const observer = new IntersectionObserver(([entry]) => {
      if (entry) setVisible(!entry.isIntersecting)
    }, { root, threshold: 0.01 })
    observer.observe(sentinel)
    return () => observer.disconnect()
  })

  $effect(() => () => clearTimeout(hideTimer))
</script>

{#if rendered}
  <div class={cn('pointer-events-none', visible ? 'scroll-down-enter' : 'scroll-down-exit', className)}>
    <div class="mx-auto flex w-full max-w-[760px] justify-end">
      <Button
        data-mobile-toast-avoid
        type="button"
        variant="outline"
        size="icon"
        class="pointer-events-auto rounded-full bg-background p-0 shadow-none"
        aria-label={$t('chat.scrollBottom')}
        title={$t('chat.scrollBottom')}
        onclick={() => { onclick(); setVisible(false) }}
      >
        <CaretDownIcon class="size-4" />
      </Button>
    </div>
  </div>
{/if}

<style>
  .scroll-down-enter {
    animation: scroll-down-in 180ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .scroll-down-exit {
    animation: scroll-down-out 160ms cubic-bezier(0.4, 0, 1, 1) both;
  }

  @keyframes scroll-down-in {
    from {
      opacity: 0;
      translate: 0 0.75rem;
    }
    to {
      opacity: 1;
      translate: 0 0;
    }
  }

  @keyframes scroll-down-out {
    from {
      opacity: 1;
      translate: 0 0;
    }
    to {
      opacity: 0;
      translate: 0 0.75rem;
    }
  }
</style>
