<script lang="ts">
  import { t } from '$lib/i18n'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import PaperPlaneTiltIcon from 'phosphor-svelte/lib/PaperPlaneTiltIcon'
  import { botBubbleClass } from '../../../components/bots/messageBubble'
</script>

<!-- Matches MobileBotChat: the feed runs under the floating header and ends at the pill composer. -->
<div class="mobile-bot-chat flex h-full min-h-0 flex-col">
  <div data-bot-conversation-skeleton role="status" aria-label={$t('chat.loadingConversation')}
    class="flex min-h-0 flex-1 flex-col justify-end gap-5 overflow-hidden pt-[var(--mobile-header-h,64px)] pb-5">
    <!-- The bubbles themselves are the placeholders: a one-line user message followed by a three-line Bot reply. -->
    <div class="flex justify-end">
      <div class={['h-12 w-2/5 max-w-[86%] animate-pulse', botBubbleClass]}></div>
    </div>
    <div class="flex justify-start">
      <div class={['h-24 w-4/5 max-w-[86%] animate-pulse', botBubbleClass]}></div>
    </div>
  </div>
  <!-- Decorative placeholder matching the mobile composer; the conversation skeleton announces the loading state. -->
  <div class="mobile-glass flex shrink-0 items-end rounded-3xl p-1" aria-hidden="true" inert>
    <span class="mobile-icon text-muted-foreground"><PlusIcon class="size-4" /></span>
    <span class="min-w-0 flex-1 truncate px-1 py-2.5 text-base leading-6 text-muted-foreground">{$t('bots.message')}</span>
    <span class="mobile-icon bg-primary text-primary-foreground opacity-40"><PaperPlaneTiltIcon class="size-4" /></span>
  </div>
</div>
