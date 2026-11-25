<script lang="ts">
  import { t } from '$lib/i18n'
  import { entryQuote, quoteSummary, type BotQuote } from '$lib/botQuote'
  import type { BotMessage } from '../../../api/types'
  import MobileBotSheet from './MobileBotSheet.svelte'
  let {
    open = $bindable(false),
    message,
    onsubmit,
  }: { open: boolean; message: BotMessage; onsubmit: (text: string, quote: BotQuote) => Promise<void> } = $props()
  let text = $state('')
  let sending = $state(false)
  let error = $state('')
  const quote = $derived(entryQuote(message)!)
  async function send() {
    if (sending || !text.trim()) return
    sending = true
    try {
      await onsubmit(text.trim(), quote)
      open = false
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      sending = false
    }
  }
</script>

<MobileBotSheet bind:open title={$t('bots.reply')}>
  <p class="line-clamp-3 rounded-xl bg-muted px-4 py-3">{quoteSummary(quote)}</p>
  <textarea
    {@attach (node) => node.focus()}
    bind:value={text}
    readonly={sending}
    aria-label={$t('bots.message')}
    class="my-2 w-full rounded-xl border px-4 py-3 text-base"></textarea>
  {#if error}<p role="alert">{error}</p>{/if}
  <button
    class="min-h-11 rounded-full bg-primary px-5 text-primary-foreground disabled:opacity-50"
    disabled={sending || !text.trim()}
    onclick={send}>{$t('common.send')}</button
  >
</MobileBotSheet>
