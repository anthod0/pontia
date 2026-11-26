<script lang="ts">
  import { t } from '$lib/i18n'
  import { quoteSummary, quoteText, type BotQuote } from '$lib/botQuote'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { MediaQuery } from 'svelte/reactivity'
  import QuoteThumbnail from './QuoteThumbnail.svelte'

  let { quote, speaker, image = null, onremove }: {
    quote: BotQuote
    speaker: string
    /** The quoted file when it may be a previewable image. */
    image?: { sessionId: string; path: string } | null
    onremove: () => void
  } = $props()
  const text = $derived(quoteText(quote))
  const summary = $derived(quoteSummary(quote))
  const desktop = new MediaQuery('(min-width: 768px)')
</script>

<div
  class="relative flex min-w-0 items-center gap-1.5 pl-2.5 text-[13px] leading-5 text-muted-foreground before:absolute before:inset-y-0.5 before:left-0 before:w-0.5 before:rounded-[1px] before:bg-[color-mix(in_srgb,var(--muted-foreground)_45%,transparent)] sm:text-[12.5px]"
  title={desktop.current ? `${speaker}: ${text}` : undefined}
  data-bot-pending-quote
>
  <span class="max-w-[40%] shrink-0 truncate font-medium text-foreground">{speaker}</span>
  {#if image}<QuoteThumbnail sessionId={image.sessionId} path={image.path} class="size-4 rounded-sm ring-1 ring-foreground/10" />{/if}
  <span class="min-w-0 truncate">{summary}</span>
  <button
    type="button"
    class="relative ml-auto grid size-6 shrink-0 place-items-center rounded-full hover:bg-foreground/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none before:absolute before:-inset-2.5 before:content-[''] sm:size-5 sm:before:-inset-1.5"
    aria-label={$t('bots.removeQuote', { text })}
    onmousedown={event => event.preventDefault()}
    onclick={onremove}
  >
    <XIcon class="size-[13px] sm:size-3" />
  </button>
</div>
