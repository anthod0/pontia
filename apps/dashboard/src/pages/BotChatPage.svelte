<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { BotTimeline } from '$lib/botTimeline.svelte'
  import BotMessageFeed from '../components/bots/BotMessageFeed.svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { MessageResponse } from '$lib/components/ai-elements/message/index.js'
  import SharedFileCard from '../components/chat/SharedFileCard.svelte'
  import MessageComposer from '../components/chat/MessageComposer.svelte'
  import { parseUploadMessage } from '$lib/uploads/message'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { useDeviceRuntime } from '../device/context'
  import { openBotChatStream } from '../services/botChatStream'
  import BotAvatar from '../components/bots/BotAvatar.svelte'

  let { botId }: { botId: string } = $props()
  const device = useDeviceRuntime()

  const timeline = new BotTimeline(device, untrack(() => botId))
  let draft = $state('')
  let typing = $state(false)
  let actionError = $state('')
  let retrying = $state<string[]>([])
  let feed = $state<BotMessageFeed>()

  onMount(() => {
    const stream = openBotChatStream(device, botId, update => {
      typing = update.type === 'typing'
      if (update.type === 'message') timeline.message(update.message)
      else if (update.type === 'changed' || update.type === 'reset') timeline.refresh(update.type === 'reset')
    }, () => { typing = false })
    const unsubscribeFiles = device.events.subscribeDashboardEvents(event => {
      if (event.kind === 'session_event' && event.event.file) timeline.refresh()
    })
    const resume = () => { if (!document.hidden) timeline.refresh(true) }
    document.addEventListener('visibilitychange', resume)
    window.addEventListener('online', resume)
    timeline.refresh()
    return () => {
      stream()
      unsubscribeFiles()
      timeline.destroy()
      document.removeEventListener('visibilitychange', resume)
      window.removeEventListener('online', resume)
    }
  })
  async function send(content: string): Promise<boolean> {
    const text = draft
    draft = ''
    actionError = ''
    feed?.followSend()
    try {
      timeline.message(await device.api.sendBotMessage(botId, content))
      return true
    } catch (failure) {
      draft = draft ? text + '\n\n' + draft : text
      actionError = failure instanceof Error ? failure.message : String(failure)
      timeline.refresh()
      return false
    }
  }

  async function retry(id: string) {
    retrying = [...retrying, id]
    actionError = ''
    try {
      timeline.message(await device.api.retryBotMessage(botId, id))
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure)
      timeline.refresh()
    } finally {
      retrying = retrying.filter((messageId) => messageId !== id)
    }
  }
</script>

{#snippet refreshFailure()}
  <div class="flex items-center justify-center gap-2 text-sm" role="alert">
    {timeline.refreshError}
    <Button size="xs" variant="ghost" onclick={() => timeline.refresh(true)}>{$t('common.retry')}</Button>
  </div>
{/snippet}

{#if timeline.bot}
  <!-- The feed scrolls beneath the transparent top bar, like the Session chat page. -->
  <section class="mx-auto -mt-14 flex h-dvh max-w-3xl flex-col md:-mt-12" aria-label={timeline.bot.name}>
    <BotMessageFeed bind:this={feed} {timeline} {typing}>
      {#snippet row(entry)}
        {#if 'content' in entry}
          {@const parts = entry.kind === 'user_input' ? parseUploadMessage(entry.content) : { text: entry.content, files: [] }}
          <div class={['flex', entry.kind === 'user_input' ? 'justify-end' : 'justify-start']}>
            <div class="max-w-[90%] min-w-0 space-y-2">
              {#if parts.files.length}
                <ul class="flex flex-wrap justify-end gap-2" data-user-message-uploads>
                  {#each parts.files as file}<li class="rounded-md border bg-background px-2 py-1 text-xs [overflow-wrap:anywhere]" title={file.path}>{$t('files.uploadedPrefix')} {file.name}</li>{/each}
                </ul>
              {/if}
              {#if parts.text}
                {#if entry.kind === 'bot_reply'}
                  <MessageResponse
                    content={parts.text}
                    markdown
                    sessionId={timeline.fileLinkSession?.session_id}
                    linkBase={timeline.fileLinkSession?.workspace ? `${timeline.fileLinkSession.workspace.replace(/\/+$/, '')}/` : null}
                    class="size-auto rounded-2xl bg-muted px-4 py-3 break-words [&_code]:bg-background [&_th]:bg-background [&_[data-code-block]]:bg-background"
                  />
                {:else}
                  <p class="rounded-2xl border border-border bg-background px-4 py-3 text-sm leading-6 whitespace-pre-wrap [overflow-wrap:anywhere]">{parts.text}</p>
                {/if}
              {/if}
              {#if entry.status === 'failed'}
                <div class="mt-1 flex items-center justify-end gap-2 text-xs text-destructive">
                  <span>{$t('bots.sendFailed')}</span>
                  <Button variant="ghost" size="xs" disabled={retrying.includes(entry.id)} onclick={() => void retry(entry.id)}>{$t('common.retry')}</Button>
                </div>
              {/if}
            </div>
          </div>
        {:else if entry.kind === 'shared_file'}
          <div class="w-fit max-w-[90%]">
            <SharedFileCard file={entry.file} sessionId={entry.session_id} downloadOnly />
          </div>
        {:else}
          <p class="flex items-center justify-center gap-2 text-center text-xs text-muted-foreground">
            <WarningCircleIcon class="size-4 shrink-0" />
            {entry.kind === 'session.exited' ? $t('bots.sessionExited') : $t('bots.executionFailed')}
          </p>
        {/if}
      {/snippet}
    </BotMessageFeed>
    <div class="shrink-0 px-1 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
      {#if timeline.status.kind === 'processing'}
        {#if !typing}
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
      {#if timeline.refreshError}{@render refreshFailure()}{/if}
      {#if actionError}<p role="alert" class="mb-2 text-sm text-destructive">{actionError}</p>{/if}
      <MessageComposer draftId={`bot:${botId}`} bind:value={draft} placeholder={$t('bots.message')} fullscreen onSubmit={send} />
    </div>
  </section>
{:else if timeline.refreshError}
  {@render refreshFailure()}
{:else}
  <p role="status" class="mx-auto max-w-3xl text-sm text-muted-foreground">{$t('common.loading')}</p>
{/if}
