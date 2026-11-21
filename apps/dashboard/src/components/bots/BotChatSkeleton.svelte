<script lang="ts">
  import { t } from '$lib/i18n'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { botBubbleClass, userBubbleClass } from './messageBubble'

  // Bubble width and text line widths, oldest first, alternating user and Bot.
  const messages = [
    { user: true, width: 'w-2/5', lines: ['w-full'] },
    { user: false, width: 'w-4/5', lines: ['w-full', 'w-11/12', 'w-3/5'] },
    { user: true, width: 'w-3/5', lines: ['w-full', 'w-2/3'] },
    { user: false, width: 'w-2/3', lines: ['w-full', 'w-4/5'] },
  ]
</script>

<!-- Matches the Bot chat layout: the feed starts below the transparent top bar and the compact composer sits at the bottom. -->
<div class="mx-auto -mt-14 flex h-dvh max-w-3xl flex-col md:-mt-12">
  <!-- Same insets, row gap and bubble surfaces as BotMessageFeed and its rows. -->
  <div data-bot-conversation-skeleton role="status" aria-label={$t('chat.loadingConversation')}
    class="min-h-0 flex-1 space-y-5 overflow-hidden px-1 pt-[calc(3.5rem+20px)] md:pt-[calc(3rem+20px)]">
    {#each messages as message}
      <div class={['flex', message.user ? 'justify-end' : 'justify-start']}>
        <div class={['max-w-[90%] text-sm', message.width, message.user ? `${userBubbleClass} leading-6` : `${botBubbleClass} leading-[1.85]`]}>
          {#each message.lines as line}
            <div class="flex h-lh items-center"><Skeleton class={['h-2.5', line, !message.user && 'bg-background']} /></div>
          {/each}
        </div>
      </div>
    {/each}
  </div>
  <div class="shrink-0 px-1 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
    <!-- Decorative placeholder matching the compact composer; the conversation skeleton announces the loading state. -->
    <div class="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center rounded-lg border bg-background" aria-hidden="true" inert>
      <div class="py-1 pl-1">
        <span class="flex size-8 items-center justify-center text-muted-foreground opacity-50"><PlusIcon class="size-4" /></span>
      </div>
      <div class="truncate py-2 text-base text-muted-foreground sm:text-[13.5px]">{$t('bots.message')}</div>
      <div class="p-1">
        <span class="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-semibold text-primary-foreground opacity-50 max-sm:w-7 max-sm:justify-center max-sm:px-0">
          <span class="max-sm:sr-only">{$t('common.send')}</span>
          <ArrowUpIcon class="size-3.5" />
        </span>
      </div>
    </div>
  </div>
</div>
