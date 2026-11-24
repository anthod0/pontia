<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import MessageComposer from '../../../components/chat/MessageComposer.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import BotChatSkeleton from '../../../components/bots/BotChatSkeleton.svelte'
  import { provideLocalhostPreview } from '../../../components/chat/localhostPreviewContext'
  import { provideFilePreviewNavigator } from '../../../components/chat/filePreviewContext'
  import BotQuoteBlock from '../../../components/bots/BotQuoteBlock.svelte'
  import DesktopBotPanel from './DesktopBotPanel.svelte'
  import type { BotChatController } from '../../../features/bots/botChat.svelte'
  import BotConversation from '../../../components/bots/BotConversation.svelte'
  let {chat}: {chat:BotChatController} = $props()
  const timeline = $derived(chat.timeline)
  let panel = $state<DesktopBotPanel>()
  provideLocalhostPreview(target => panel?.openPreview(target))
  provideFilePreviewNavigator({open: file => panel?.openFile(file)})
  // The feed stays usable after a failed refresh, so the error stays up until a refresh succeeds.
  $effect(() => {
    const message = timeline.bot ? timeline.refreshError : ''
    if (!message) return
    toast.error(message, {
      id: 'bot-refresh-error',
      duration: Number.POSITIVE_INFINITY,
      dismissible: false,
      important: true,
      action: { label: $t('common.retry'), onClick: event => { event.preventDefault(); timeline.refresh(true) } },
    })
    return () => toast.dismiss('bot-refresh-error')
  })

</script>
{#snippet refreshFailure()}
  <div class="flex items-center justify-center gap-2 text-sm" role="alert">
    {timeline.refreshError}
    <Button size="xs" variant="ghost" onclick={() => timeline.refresh(true)}>{$t('common.retry')}</Button>
  </div>
{/snippet}

{#if timeline.bot}
  <DesktopBotPanel bind:this={panel} bot={timeline.bot} count={timeline.openTodoCount} onjump={chat.jumpTo} onquote={chat.addQuote} onreply={chat.replyTodo} ontodochange={chat.changeTodo} />
  <!-- The feed scrolls beneath the transparent top bar, like the Session chat page. -->
  <section class="mx-auto -mt-12 flex h-dvh max-w-3xl flex-col" aria-label={timeline.bot.name}>
    <BotConversation {chat} oncontext={id=>panel?.viewContext(id)} ontodos={()=>panel?.viewTodos()} />
    <div class="shrink-0 px-1 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
      {#if timeline.status.kind === 'processing'}
        {#if !chat.typing}
          <p class="mb-2 flex items-start gap-2 px-2 text-sm leading-5 text-muted-foreground" role="status">
            <BotAvatar shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working class="size-5 shrink-0" />
            <span class="shimmer min-w-0">{$t('bots.processing')}</span>
          </p>
        {/if}
      {:else if timeline.status.kind === 'working'}
        <p class="mb-2 flex items-start gap-2 px-2 text-sm leading-5 text-muted-foreground" role="status">
          <BotAvatar shape={timeline.bot.avatar_shape} color={timeline.bot.avatar_color} working class="size-5 shrink-0" />
          <span class="shimmer line-clamp-2 min-w-0 [overflow-wrap:anywhere]" title={timeline.status.title}>{timeline.status.title}</span>
        </p>
      {/if}
      <div aria-live="polite">
        {#if chat.quote}<BotQuoteBlock quote={chat.quote} speaker={chat.quoteSpeaker(chat.quote)} image={chat.quoteImage(chat.quote)} onremove={() => chat.setQuote(null)} />{/if}
      </div>
      <MessageComposer bind:this={chat.composer} draftId={`bot:${chat.botId}`} bind:value={chat.draft} placeholder={$t('bots.message')} fullscreen
        submitDisabled={chat.quote !== null && !chat.draft.trim()}
        busy={chat.reloading} commands={[{ name: '/reload', description: $t('bots.reload'), run: chat.reload }]}
        onSubmit={chat.send} />
    </div>
  </section>
{:else if timeline.refreshError}
  {@render refreshFailure()}
{:else}
  <BotChatSkeleton />
{/if}
